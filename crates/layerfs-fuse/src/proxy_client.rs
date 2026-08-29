use crate::protocol::{read_response, write_request, ClosedCreate, Request, Response};
use crate::{Attr, FilesystemPort, Kind, NodeId, PortError, PortResult};
use std::collections::{BTreeMap, HashMap};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};

const CONNECTIONS: usize = 1;
const MAX_PENDING_UNLINKS: usize = 16_384;

pub struct ProxyClient {
    streams: Vec<Mutex<TcpStream>>,
    next: AtomicUsize,
    cache: Mutex<Cache>,
    reservation: Mutex<Reservation>,
    gate: RwLock<()>,
    paused: AtomicBool,
    pending: AtomicU64,
}

#[derive(Default)]
struct Cache {
    attrs: HashMap<NodeId, Attr>,
    directories: HashMap<NodeId, BTreeMap<Vec<u8>, (NodeId, Kind)>>,
    pending_creates: HashMap<NodeId, PendingCreate>,
    pending_closed: Vec<ClosedCreate>,
    pending_closed_bytes: usize,
    pending_unlinks: Vec<(NodeId, Vec<u8>)>,
}

struct PendingCreate {
    parent: NodeId,
    name: Vec<u8>,
    mode: u32,
    mtime: Option<(i64, u32)>,
    zero_len: u64,
    writes: Vec<(u64, Vec<u8>)>,
    bytes: usize,
}

#[derive(Default)]
struct Reservation {
    next: u64,
    end: u64,
}

impl ProxyClient {
    pub fn connect(endpoint: impl ToSocketAddrs, capability: [u8; 32]) -> std::io::Result<Self> {
        let address = endpoint
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::other("LayerFS endpoint"))?;
        let mut streams = Vec::with_capacity(CONNECTIONS);
        for _ in 0..CONNECTIONS {
            use std::io::{Read, Write};
            let mut stream = TcpStream::connect(address)?;
            stream.set_nodelay(true)?;
            stream.write_all(&capability)?;
            let mut accepted = [0];
            stream.read_exact(&mut accepted)?;
            if accepted != [1] {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "LayerFS capability",
                ));
            }
            streams.push(Mutex::new(stream));
        }
        let client = Self {
            streams,
            next: AtomicUsize::new(0),
            cache: Mutex::new(Cache::default()),
            reservation: Mutex::new(Reservation::default()),
            gate: RwLock::new(()),
            paused: AtomicBool::new(false),
            pending: AtomicU64::new(0),
        };
        client
            .refill_reservation()
            .map_err(|_| std::io::Error::other("LayerFS node reservation"))?;
        let entries = match client
            .exchange(Request::Readdir(crate::ROOT))
            .map_err(|_| std::io::Error::other("LayerFS root snapshot"))?
        {
            Response::Entries(entries) => entries,
            _ => return Err(std::io::Error::other("LayerFS root snapshot")),
        };
        client
            .remember_directory(crate::ROOT, &entries)
            .map_err(|_| std::io::Error::other("LayerFS root snapshot"))?;
        Ok(client)
    }

    fn exchange(&self, request: Request) -> PortResult<Response> {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.streams.len();
        self.exchange_at(index, request)
    }

    fn exchange_at(&self, index: usize, request: Request) -> PortResult<Response> {
        let _gate = self.gate.read().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.raw_exchange_at(index, request)
    }

    fn raw_exchange_at(&self, index: usize, request: Request) -> PortResult<Response> {
        let mut stream = self.streams[index].lock().map_err(|_| PortError::Io)?;
        write_request(&mut *stream, &request).map_err(|_| PortError::Io)?;
        match read_response(&mut *stream).map_err(|_| PortError::Io)? {
            Response::Error(error) => Err(error),
            response => Ok(response),
        }
    }

    fn send_at(&self, index: usize, request: Request) -> PortResult<()> {
        let _gate = self.gate.read().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.raw_send_at(index, request)
    }

    fn raw_send_at(&self, index: usize, request: Request) -> PortResult<()> {
        let mut stream = self.streams[index].lock().map_err(|_| PortError::Io)?;
        write_request(&mut *stream, &request).map_err(|_| PortError::Io)?;
        self.pending.fetch_add(1, Ordering::Release);
        Ok(())
    }

    fn node_stream(&self, node: NodeId) -> usize {
        node.0 as usize % self.streams.len()
    }

    #[doc(hidden)]
    pub fn barrier(&self) -> PortResult<()> {
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.flush_pending_locked()?;
        self.barrier_locked()
    }

    #[doc(hidden)]
    pub fn pause(&self) -> PortResult<()> {
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if !self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .pending_creates
            .is_empty()
        {
            return Err(PortError::Busy);
        }
        self.flush_pending_unlinks_locked()?;
        self.paused.store(true, Ordering::Release);
        self.barrier_locked()
    }

    #[doc(hidden)]
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
    }

    fn barrier_locked(&self) -> PortResult<()> {
        if self.pending.load(Ordering::Acquire) == 0 {
            return Ok(());
        }
        for index in 0..self.streams.len() {
            unit(self.raw_exchange_at(index, Request::Fence)?)?;
        }
        self.pending.store(0, Ordering::Release);
        Ok(())
    }
}

impl FilesystemPort for ProxyClient {
    fn lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        if let Some(cached) = self.cached_lookup(parent, name)? {
            return cached;
        }
        match self.exchange(Request::Lookup(parent, name.to_vec())) {
            Ok(response) => {
                let attr = attr(response)?;
                self.remember(parent, name, attr)?;
                Ok(attr)
            }
            Err(PortError::NotFound) => {
                if !self.cached_directory(parent)? {
                    if let Response::Entries(entries) = self.exchange(Request::Readdir(parent))? {
                        self.remember_directory(parent, &entries)?;
                    }
                }
                Err(PortError::NotFound)
            }
            Err(error) => Err(error),
        }
    }

    fn attr(&self, node: NodeId) -> PortResult<Attr> {
        if let Some(attr) = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .attrs
            .get(&node)
            .copied()
        {
            return Ok(attr);
        }
        let attr = attr(self.exchange_at(self.node_stream(node), Request::Attr(node))?)?;
        self.cache
            .lock()
            .map_err(|_| PortError::Io)?
            .attrs
            .insert(node, attr);
        Ok(attr)
    }

    fn readlink(&self, node: NodeId) -> PortResult<Vec<u8>> {
        bytes(self.exchange_at(self.node_stream(node), Request::Readlink(node))?)
    }

    fn readdir(&self, node: NodeId) -> PortResult<Vec<(NodeId, Kind, Vec<u8>)>> {
        self.barrier()?;
        match self.exchange(Request::Readdir(node))? {
            Response::Entries(entries) => {
                self.remember_directory(node, &entries)?;
                Ok(entries)
            }
            _ => Err(PortError::Io),
        }
    }

    fn create_file(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.flush_unlink_for(parent, name)?;
        let attr = attr(self.exchange(Request::CreateFile(parent, name.to_vec(), mode))?)?;
        self.remember(parent, name, attr)?;
        Ok(attr)
    }

    fn create_file_open(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.flush_unlink_for(parent, name)?;
        let reservable = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .get(&parent)
            .is_some_and(|entries| !entries.contains_key(name));
        if reservable {
            let node = self.reserved_node()?;
            let attr = Attr {
                node,
                size: 0,
                kind: Kind::File,
                mode: mode & 0o777,
                links: 1,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            };
            let _gate = self.gate.read().map_err(|_| PortError::Io)?;
            if self.paused.load(Ordering::Acquire) {
                return Err(PortError::Busy);
            }
            self.cache
                .lock()
                .map_err(|_| PortError::Io)?
                .pending_creates
                .insert(
                    node,
                    PendingCreate {
                        parent,
                        name: name.to_vec(),
                        mode,
                        mtime: None,
                        zero_len: 0,
                        writes: Vec::new(),
                        bytes: 0,
                    },
                );
            self.remember(parent, name, attr)?;
            return Ok(attr);
        }
        let attr = attr(self.exchange(Request::CreateFileOpen(parent, name.to_vec(), mode))?)?;
        self.remember(parent, name, attr)?;
        Ok(attr)
    }

    fn mkdir(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.flush_unlink_for(parent, name)?;
        let attr = attr(self.exchange(Request::Mkdir(parent, name.to_vec(), mode))?)?;
        self.remember(parent, name, attr)?;
        self.cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .insert(attr.node, BTreeMap::new());
        Ok(attr)
    }

    fn symlink(&self, parent: NodeId, name: &[u8], target: Vec<u8>) -> PortResult<Attr> {
        self.flush_unlink_for(parent, name)?;
        let attr = attr(self.exchange(Request::Symlink(parent, name.to_vec(), target))?)?;
        self.remember(parent, name, attr)?;
        Ok(attr)
    }

    fn link(&self, node: NodeId, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        self.flush_pending_create(node)?;
        self.flush_unlink_for(parent, name)?;
        let attr = attr(self.exchange(Request::Link(node, parent, name.to_vec()))?)?;
        self.remember(parent, name, attr)?;
        Ok(attr)
    }

    fn unlink(&self, parent: NodeId, name: &[u8], directory: bool) -> PortResult<()> {
        let pending = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .get(&parent)
            .and_then(|entries| entries.get(name))
            .map(|(node, _)| *node);
        if let Some(node) = pending {
            self.flush_pending_create(node)?;
        }
        if directory {
            self.barrier()?;
            unit(self.exchange(Request::Unlink(parent, name.to_vec(), true))?)?;
        } else {
            self.queue_unlink(parent, name)?;
        }
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        let removed = cache
            .directories
            .get_mut(&parent)
            .and_then(|entries| entries.remove(name));
        if !directory {
            if let Some(attr) = removed.and_then(|(node, _)| cache.attrs.get_mut(&node)) {
                attr.links = attr.links.saturating_sub(1);
            }
        }
        Ok(())
    }

    fn rename(
        &self,
        parent: NodeId,
        name: &[u8],
        new_parent: NodeId,
        new_name: &[u8],
        no_replace: bool,
    ) -> PortResult<()> {
        let moved = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .get(&parent)
            .and_then(|entries| entries.get(name))
            .map(|(node, _)| *node);
        self.flush_unlink_for(new_parent, new_name)?;
        if let Some(node) = moved {
            let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
            let target_exists = cache
                .directories
                .get(&new_parent)
                .is_some_and(|entries| entries.contains_key(new_name));
            if target_exists && no_replace {
                return Err(PortError::Exists);
            }
            if !target_exists && cache.pending_creates.contains_key(&node) {
                let pending = cache
                    .pending_creates
                    .get_mut(&node)
                    .expect("pending create");
                pending.parent = new_parent;
                pending.name = new_name.to_vec();
                let moved = cache
                    .directories
                    .get_mut(&parent)
                    .and_then(|entries| entries.remove(name));
                if let (Some(moved), Some(entries)) =
                    (moved, cache.directories.get_mut(&new_parent))
                {
                    entries.insert(new_name.to_vec(), moved);
                }
                return Ok(());
            }
            drop(cache);
            self.flush_pending_create(node)?;
        }
        unit(self.exchange(Request::Rename(
            parent,
            name.to_vec(),
            new_parent,
            new_name.to_vec(),
            no_replace,
        ))?)?;
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        let moved = cache
            .directories
            .get_mut(&parent)
            .and_then(|entries| entries.remove(name));
        match moved {
            Some(moved) => {
                if let Some(entries) = cache.directories.get_mut(&new_parent) {
                    entries.insert(new_name.to_vec(), moved);
                }
            }
            None => {
                cache.directories.remove(&new_parent);
            }
        }
        Ok(())
    }

    fn pin(&self, node: NodeId, truncate: bool, writable: bool) -> PortResult<()> {
        self.flush_pending_create(node)?;
        if !truncate && !writable {
            return self.send_at(self.node_stream(node), Request::PinRead(node));
        }
        unit(self.exchange(Request::Pin(node, truncate, writable))?)
    }

    fn unpin(&self, node: NodeId, writable: bool) -> PortResult<()> {
        let _gate = self.gate.read().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        let pending = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .pending_creates
            .remove(&node);
        if let Some(pending) = pending {
            if pending.zero_len != 0 {
                let stream = self.node_stream(node);
                self.send_pending_create(stream, node, pending)?;
                return self.raw_send_at(stream, Request::Unpin(node, writable));
            }
            let mut batches = Vec::new();
            {
                let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
                if !cache.pending_closed.is_empty()
                    && cache.pending_closed_bytes + pending.bytes > 1024 * 1024
                {
                    batches.push(std::mem::take(&mut cache.pending_closed));
                    cache.pending_closed_bytes = 0;
                }
                cache.pending_closed_bytes += pending.bytes;
                cache.pending_closed.push((
                    pending.parent,
                    pending.name,
                    pending.mode,
                    node,
                    pending.writes,
                    pending.mtime,
                ));
                if cache.pending_closed.len() >= 128 || cache.pending_closed_bytes >= 1024 * 1024 {
                    batches.push(std::mem::take(&mut cache.pending_closed));
                    cache.pending_closed_bytes = 0;
                }
            }
            for batch in batches {
                self.raw_send_at(0, Request::CreateFilesClosedReserved(batch))?;
            }
            return Ok(());
        }
        self.raw_send_at(self.node_stream(node), Request::Unpin(node, writable))
    }

    fn read(&self, node: NodeId, offset: u64, size: usize) -> PortResult<Vec<u8>> {
        self.flush_pending_create(node)?;
        bytes(self.exchange_at(self.node_stream(node), Request::Read(node, offset, size))?)
    }

    fn write(&self, node: NodeId, offset: u64, value: &[u8]) -> PortResult<usize> {
        if is_zero(value) {
            let end = offset
                .checked_add(value.len() as u64)
                .ok_or(PortError::Invalid)?;
            {
                let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
                let retained = cache
                    .pending_creates
                    .get_mut(&node)
                    .filter(|pending| pending.writes.is_empty())
                    .map(|pending| pending.zero_len = pending.zero_len.max(end))
                    .is_some();
                if retained {
                    if let Some(attr) = cache.attrs.get_mut(&node) {
                        attr.size = attr.size.max(end);
                    }
                    return Ok(value.len());
                }
            }
            self.flush_pending_create(node)?;
            self.send_at(
                self.node_stream(node),
                Request::WriteZero(
                    node,
                    offset,
                    value.len().try_into().map_err(|_| PortError::Invalid)?,
                ),
            )?;
            if let Some(attr) = self
                .cache
                .lock()
                .map_err(|_| PortError::Io)?
                .attrs
                .get_mut(&node)
            {
                attr.size = attr.size.max(offset.saturating_add(value.len() as u64));
            }
            return Ok(value.len());
        }
        {
            let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
            if let Some(pending) = cache.pending_creates.get_mut(&node) {
                if pending.writes.len() < 128 && pending.bytes + value.len() <= 1024 * 1024 {
                    pending.writes.push((offset, value.to_vec()));
                    pending.bytes += value.len();
                    if let Some(attr) = cache.attrs.get_mut(&node) {
                        attr.size = attr.size.max(offset.saturating_add(value.len() as u64));
                    }
                    return Ok(value.len());
                }
            }
        }
        self.flush_pending_create(node)?;
        self.send_at(
            self.node_stream(node),
            Request::Write(node, offset, value.to_vec()),
        )?;
        if let Some(attr) = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .attrs
            .get_mut(&node)
        {
            attr.size = attr.size.max(offset.saturating_add(value.len() as u64));
        }
        Ok(value.len())
    }

    fn truncate(&self, node: NodeId, size: u64) -> PortResult<()> {
        if size == 0 {
            let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
            if let Some(pending) = cache.pending_creates.get_mut(&node) {
                pending.zero_len = 0;
                pending.writes.clear();
                pending.bytes = 0;
                if let Some(attr) = cache.attrs.get_mut(&node) {
                    attr.size = 0;
                }
                return Ok(());
            }
        }
        self.flush_pending_create(node)?;
        unit(self.exchange_at(self.node_stream(node), Request::Truncate(node, size))?)?;
        if let Some(attr) = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .attrs
            .get_mut(&node)
        {
            attr.size = size;
        }
        Ok(())
    }

    fn chmod(&self, node: NodeId, mode: u32) -> PortResult<()> {
        {
            let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
            if let Some(pending) = cache.pending_creates.get_mut(&node) {
                pending.mode = mode;
                if let Some(attr) = cache.attrs.get_mut(&node) {
                    attr.mode = mode;
                }
                return Ok(());
            }
        }
        self.flush_pending_create(node)?;
        unit(self.exchange_at(self.node_stream(node), Request::Chmod(node, mode))?)?;
        if let Some(attr) = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .attrs
            .get_mut(&node)
        {
            attr.mode = mode;
        }
        Ok(())
    }

    fn set_mtime(&self, node: NodeId, seconds: i64, nanos: u32) -> PortResult<()> {
        {
            let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
            if let Some(pending) = cache.pending_creates.get_mut(&node) {
                pending.mtime = Some((seconds, nanos));
                if let Some(attr) = cache.attrs.get_mut(&node) {
                    attr.mtime_seconds = seconds;
                    attr.mtime_nanoseconds = nanos;
                }
                return Ok(());
            }
        }
        self.flush_pending_create(node)?;
        unit(self.exchange_at(
            self.node_stream(node),
            Request::SetMtime(node, seconds, nanos),
        )?)?;
        if let Some(attr) = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .attrs
            .get_mut(&node)
        {
            attr.mtime_seconds = seconds;
            attr.mtime_nanoseconds = nanos;
        }
        Ok(())
    }

    fn fsync(&self, node: Option<NodeId>) -> PortResult<()> {
        if let Some(node) = node {
            self.flush_pending_create(node)?;
        }
        self.barrier()?;
        match node {
            Some(node) => {
                unit(self.exchange_at(self.node_stream(node), Request::Fsync(Some(node)))?)
            }
            None => unit(self.exchange(Request::Fsync(None))?),
        }
    }
}

impl ProxyClient {
    fn flush_pending_create(&self, node: NodeId) -> PortResult<()> {
        if self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .pending_closed
            .iter()
            .any(|entry| entry.3 == node)
        {
            let _gate = self.gate.write().map_err(|_| PortError::Io)?;
            if self.paused.load(Ordering::Acquire) {
                return Err(PortError::Busy);
            }
            self.flush_pending_locked()?;
        }
        let _gate = self.gate.read().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        let Some(pending) = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .pending_creates
            .remove(&node)
        else {
            return Ok(());
        };
        let stream = self.node_stream(node);
        self.send_pending_create(stream, node, pending)
    }

    fn flush_pending_locked(&self) -> PortResult<()> {
        let (creates, closed, unlinks) = {
            let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
            cache.pending_closed_bytes = 0;
            (
                cache.pending_creates.drain().collect::<Vec<_>>(),
                std::mem::take(&mut cache.pending_closed),
                std::mem::take(&mut cache.pending_unlinks),
            )
        };
        for (node, pending) in creates {
            let stream = self.node_stream(node);
            self.send_pending_create(stream, node, pending)?;
        }
        if !closed.is_empty() {
            self.raw_send_at(0, Request::CreateFilesClosedReserved(closed))?;
        }
        for entries in unlinks.chunks(512) {
            self.raw_send_at(0, Request::UnlinkBatch(entries.to_vec()))?;
        }
        Ok(())
    }

    fn send_pending_create(
        &self,
        stream: usize,
        node: NodeId,
        pending: PendingCreate,
    ) -> PortResult<()> {
        self.raw_send_at(
            stream,
            Request::CreateFileOpenReserved(pending.parent, pending.name, pending.mode, node),
        )?;
        let mut zero_offset = 0;
        while zero_offset < pending.zero_len {
            let len = (pending.zero_len - zero_offset).min(16 * 1024 * 1024) as u32;
            self.raw_send_at(stream, Request::WriteZero(node, zero_offset, len))?;
            zero_offset += u64::from(len);
        }
        for (offset, bytes) in pending.writes {
            self.raw_send_at(stream, Request::Write(node, offset, bytes))?;
        }
        if let Some((seconds, nanos)) = pending.mtime {
            unit(self.raw_exchange_at(stream, Request::SetMtime(node, seconds, nanos))?)?;
        }
        Ok(())
    }

    fn flush_pending_unlinks_locked(&self) -> PortResult<()> {
        let (closed, entries) = {
            let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
            cache.pending_closed_bytes = 0;
            (
                std::mem::take(&mut cache.pending_closed),
                std::mem::take(&mut cache.pending_unlinks),
            )
        };
        if !closed.is_empty() {
            self.raw_send_at(0, Request::CreateFilesClosedReserved(closed))?;
        }
        for entries in entries.chunks(512) {
            self.raw_send_at(0, Request::UnlinkBatch(entries.to_vec()))?;
        }
        Ok(())
    }

    fn queue_unlink(&self, parent: NodeId, name: &[u8]) -> PortResult<()> {
        loop {
            {
                let _gate = self.gate.read().map_err(|_| PortError::Io)?;
                if self.paused.load(Ordering::Acquire) {
                    return Err(PortError::Busy);
                }
                let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
                if cache.pending_unlinks.len() < MAX_PENDING_UNLINKS {
                    cache.pending_unlinks.push((parent, name.to_vec()));
                    return Ok(());
                }
            }
            self.barrier()?;
        }
    }

    fn flush_unlink_for(&self, parent: NodeId, name: &[u8]) -> PortResult<()> {
        let pending = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .pending_unlinks
            .iter()
            .any(|(candidate_parent, candidate_name)| {
                *candidate_parent == parent && candidate_name == name
            });
        if pending {
            self.barrier()?;
        }
        Ok(())
    }

    fn refill_reservation(&self) -> PortResult<()> {
        let start = match self.exchange(Request::ReserveNodes(65_536))? {
            Response::Node(node) => node.0,
            _ => return Err(PortError::Io),
        };
        let mut reservation = self.reservation.lock().map_err(|_| PortError::Io)?;
        reservation.next = start;
        reservation.end = start + 65_536;
        Ok(())
    }

    fn reserved_node(&self) -> PortResult<NodeId> {
        {
            let mut reservation = self.reservation.lock().map_err(|_| PortError::Io)?;
            if reservation.next < reservation.end {
                let node = NodeId(reservation.next);
                reservation.next += 1;
                return Ok(node);
            }
        }
        self.refill_reservation()?;
        self.reserved_node()
    }

    fn cached_directory(&self, node: NodeId) -> PortResult<bool> {
        Ok(self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .contains_key(&node))
    }

    fn cached_lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Option<PortResult<Attr>>> {
        let cache = self.cache.lock().map_err(|_| PortError::Io)?;
        let Some(entries) = cache.directories.get(&parent) else {
            return Ok(None);
        };
        let Some((node, _)) = entries.get(name) else {
            return Ok(Some(Err(PortError::NotFound)));
        };
        Ok(cache.attrs.get(node).copied().map(Ok))
    }

    fn remember(&self, parent: NodeId, name: &[u8], attr: Attr) -> PortResult<()> {
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        cache.attrs.insert(attr.node, attr);
        if let Some(entries) = cache.directories.get_mut(&parent) {
            entries.insert(name.to_vec(), (attr.node, attr.kind));
        }
        Ok(())
    }

    fn remember_directory(
        &self,
        node: NodeId,
        entries: &[(NodeId, Kind, Vec<u8>)],
    ) -> PortResult<()> {
        let entries = entries
            .iter()
            .filter(|(_, _, name)| name.as_slice() != b"." && name.as_slice() != b"..")
            .map(|(node, kind, name)| (name.clone(), (*node, *kind)))
            .collect();
        self.cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .insert(node, entries);
        Ok(())
    }
}

fn attr(response: Response) -> PortResult<Attr> {
    match response {
        Response::Attr(attr) => Ok(attr),
        _ => Err(PortError::Io),
    }
}

fn bytes(response: Response) -> PortResult<Vec<u8>> {
    match response {
        Response::Bytes(bytes) => Ok(bytes),
        _ => Err(PortError::Io),
    }
}

fn unit(response: Response) -> PortResult<()> {
    match response {
        Response::Unit => Ok(()),
        _ => Err(PortError::Io),
    }
}

fn is_zero(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut chunks = bytes.chunks_exact(16);
    chunks.all(|chunk| u128::from_ne_bytes(chunk.try_into().expect("exact chunk")) == 0)
        && chunks.remainder().iter().all(|byte| *byte == 0)
}

#[cfg(target_os = "linux")]
#[doc(hidden)]
pub fn serve_control(
    path: std::ffi::OsString,
    client: std::sync::Arc<ProxyClient>,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    let listener = std::os::unix::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut command = [0];
            let result = stream.read_exact(&mut command).and_then(|()| {
                let result = match command[0] {
                    b'b' => client.barrier(),
                    b'p' => client.pause(),
                    b'r' => {
                        client.resume();
                        Ok(())
                    }
                    _ => Err(PortError::Invalid),
                };
                stream.write_all(&[u8::from(result.is_ok())])
            });
            if result.is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[cfg(target_os = "linux")]
#[doc(hidden)]
pub fn control_call(
    path: &std::ffi::OsStr,
    command: &std::ffi::OsStr,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Read, Write};
    let command = match command.to_str() {
        Some("barrier") => b'b',
        Some("pause") => b'p',
        Some("resume") => b'r',
        _ => return Err("invalid control command".into()),
    };
    let mut stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.write_all(&[command])?;
    let mut accepted = [0];
    stream.read_exact(&mut accepted)?;
    if accepted == [1] {
        Ok(())
    } else {
        Err("control command failed".into())
    }
}
