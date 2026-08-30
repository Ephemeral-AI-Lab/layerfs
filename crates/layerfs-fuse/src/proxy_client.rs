use crate::protocol::{read_response, write_request, ClosedCreate, Request, Response};
use crate::{Attr, FilesystemPort, Kind, NodeId, PortError, PortResult};
use std::collections::{BTreeMap, HashMap};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};

const CONNECTIONS: usize = 1;
const MAX_PENDING_UNLINKS: usize = 16_384;
const READ_AHEAD_BYTES: usize = 16 * 1024 * 1024;
type DirectoryEntries = Vec<(NodeId, Kind, Vec<u8>)>;

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
    directories: HashMap<NodeId, CachedDirectory>,
    pending_creates: HashMap<NodeId, PendingCreate>,
    pending_closed: Vec<ClosedCreate>,
    pending_closed_bytes: usize,
    pending_unlinks: Vec<(NodeId, Vec<u8>)>,
    read_ahead: Option<ReadAhead>,
}

struct CachedDirectory {
    special: DirectoryEntries,
    entries: BTreeMap<Vec<u8>, (NodeId, Kind)>,
}

struct ReadAhead {
    node: NodeId,
    offset: u64,
    bytes: Vec<u8>,
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
            stream.write_all(b"d")?;
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
        self.synchronize_locked(None)
    }

    fn synchronize_locked(&self, final_request: Option<(usize, Request)>) -> PortResult<()> {
        let has_pending = self.pending.load(Ordering::Acquire) != 0;
        if !has_pending && final_request.is_none() {
            return Ok(());
        }
        let final_stream = final_request.as_ref().map(|(index, _)| *index);
        let mut first_error = None;
        if has_pending {
            for index in 0..self.streams.len() {
                if Some(index) != final_stream {
                    retain_first(
                        &mut first_error,
                        self.raw_exchange_at(index, Request::Fence).and_then(unit),
                    );
                }
            }
        }
        if let Some((index, request)) = final_request {
            retain_first(
                &mut first_error,
                self.raw_exchange_at(index, request).and_then(unit),
            );
        }
        if first_error.is_none() {
            self.pending.store(0, Ordering::Release);
        }
        first_error.map_or(Ok(()), Err)
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
            Err(PortError::NotFound) => Err(PortError::NotFound),
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
        if let Some(entries) = self.cached_readdir(node)? {
            return Ok(entries);
        }
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
            .is_some_and(|directory| !directory.entries.contains_key(name));
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
        let reservable = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .get(&parent)
            .is_some_and(|directory| !directory.entries.contains_key(name));
        if reservable {
            let node = self.reserved_node()?;
            let attr = Attr {
                node,
                size: 0,
                kind: Kind::Directory,
                mode: mode & 0o1777,
                links: 2,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            };
            self.send_at(
                self.node_stream(node),
                Request::MkdirReserved(parent, name.to_vec(), mode, node),
            )?;
            self.remember(parent, name, attr)?;
            self.remember_new_directory(node, parent)?;
            return Ok(attr);
        }
        let attr = attr(self.exchange(Request::Mkdir(parent, name.to_vec(), mode))?)?;
        self.remember(parent, name, attr)?;
        self.remember_new_directory(attr.node, parent)?;
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
            .and_then(|directory| directory.entries.get(name))
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
            .and_then(|directory| directory.entries.remove(name));
        if directory {
            if let Some((node, _)) = removed {
                cache.directories.remove(&node);
            }
        }
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
            .and_then(|directory| directory.entries.get(name))
            .map(|(node, _)| *node);
        self.flush_unlink_for(new_parent, new_name)?;
        if let Some(node) = moved {
            let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
            let target_exists = cache
                .directories
                .get(&new_parent)
                .is_some_and(|directory| directory.entries.contains_key(new_name));
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
                    .and_then(|directory| directory.entries.remove(name));
                if let (Some(moved), Some(directory)) =
                    (moved, cache.directories.get_mut(&new_parent))
                {
                    directory.entries.insert(new_name.to_vec(), moved);
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
            .and_then(|directory| directory.entries.remove(name));
        match moved {
            Some(moved) => {
                if moved.1 == Kind::Directory {
                    if let Some(directory) = cache.directories.get_mut(&moved.0) {
                        if let Some(parent) = directory
                            .special
                            .iter_mut()
                            .find(|(_, _, name)| name.as_slice() == b"..")
                        {
                            parent.0 = new_parent;
                        }
                    }
                }
                if let Some(directory) = cache.directories.get_mut(&new_parent) {
                    directory.entries.insert(new_name.to_vec(), moved);
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
        self.invalidate_read_ahead(node)?;
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
        if let Some(bytes) = self.cached_read(node, offset, size)? {
            return Ok(bytes);
        }
        let fetched = bytes(self.exchange_at(
            self.node_stream(node),
            Request::Read(node, offset, size.max(READ_AHEAD_BYTES)),
        )?)?;
        let output = fetched[..fetched.len().min(size)].to_vec();
        self.cache.lock().map_err(|_| PortError::Io)?.read_ahead = Some(ReadAhead {
            node,
            offset,
            bytes: fetched,
        });
        Ok(output)
    }

    fn write(&self, node: NodeId, offset: u64, value: &[u8]) -> PortResult<usize> {
        self.invalidate_read_ahead(node)?;
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
        self.invalidate_read_ahead(node)?;
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
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.flush_pending_locked()?;
        let stream = node.map_or(0, |node| self.node_stream(node));
        self.synchronize_locked(Some((stream, Request::Fsync(node))))
    }
}

impl ProxyClient {
    fn remember_new_directory(&self, node: NodeId, parent: NodeId) -> PortResult<()> {
        self.cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .insert(
                node,
                CachedDirectory {
                    special: vec![
                        (node, Kind::Directory, b".".to_vec()),
                        (parent, Kind::Directory, b"..".to_vec()),
                    ],
                    entries: BTreeMap::new(),
                },
            );
        Ok(())
    }

    fn cached_readdir(&self, node: NodeId) -> PortResult<Option<DirectoryEntries>> {
        let cache = self.cache.lock().map_err(|_| PortError::Io)?;
        let Some(directory) = cache.directories.get(&node) else {
            return Ok(None);
        };
        let mut output = directory.special.clone();
        output.extend(
            directory
                .entries
                .iter()
                .map(|(name, (node, kind))| (*node, *kind, name.clone())),
        );
        Ok(Some(output))
    }

    fn cached_read(&self, node: NodeId, offset: u64, size: usize) -> PortResult<Option<Vec<u8>>> {
        let cache = self.cache.lock().map_err(|_| PortError::Io)?;
        let Some(read) = cache.read_ahead.as_ref().filter(|read| read.node == node) else {
            return Ok(None);
        };
        let Some(relative) = offset
            .checked_sub(read.offset)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return Ok(None);
        };
        if relative >= read.bytes.len() {
            return Ok(None);
        }
        let end = relative.saturating_add(size).min(read.bytes.len());
        Ok(Some(read.bytes[relative..end].to_vec()))
    }

    fn invalidate_read_ahead(&self, node: NodeId) -> PortResult<()> {
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        if cache
            .read_ahead
            .as_ref()
            .is_some_and(|read| read.node == node)
        {
            cache.read_ahead = None;
        }
        Ok(())
    }

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

    fn reserved_node(&self) -> PortResult<NodeId> {
        let mut reservation = self.reservation.lock().map_err(|_| PortError::Io)?;
        if reservation.next == reservation.end {
            let start = match self.exchange(Request::ReserveNodes(65_536))? {
                Response::Node(node) => node.0,
                _ => return Err(PortError::Io),
            };
            reservation.next = start;
            reservation.end = start.checked_add(65_536).ok_or(PortError::Invalid)?;
        }
        let node = NodeId(reservation.next);
        reservation.next += 1;
        Ok(node)
    }

    fn cached_lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Option<PortResult<Attr>>> {
        let cache = self.cache.lock().map_err(|_| PortError::Io)?;
        let Some(entries) = cache.directories.get(&parent) else {
            return Ok(None);
        };
        let Some((node, _)) = entries.entries.get(name) else {
            return Ok(Some(Err(PortError::NotFound)));
        };
        Ok(cache.attrs.get(node).copied().map(Ok))
    }

    fn remember(&self, parent: NodeId, name: &[u8], attr: Attr) -> PortResult<()> {
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        cache.attrs.insert(attr.node, attr);
        if let Some(directory) = cache.directories.get_mut(&parent) {
            directory
                .entries
                .insert(name.to_vec(), (attr.node, attr.kind));
        }
        Ok(())
    }

    fn remember_directory(
        &self,
        node: NodeId,
        entries: &[(NodeId, Kind, Vec<u8>)],
    ) -> PortResult<()> {
        let special = entries
            .iter()
            .filter(|(_, _, name)| matches!(name.as_slice(), b"." | b".."))
            .cloned()
            .collect();
        let entries = entries
            .iter()
            .filter(|(_, _, name)| !matches!(name.as_slice(), b"." | b".."))
            .map(|(node, kind, name)| (name.clone(), (*node, *kind)))
            .collect();
        self.cache
            .lock()
            .map_err(|_| PortError::Io)?
            .directories
            .insert(node, CachedDirectory { special, entries });
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

fn retain_first(first: &mut Option<PortError>, result: PortResult<()>) {
    if let Err(error) = result {
        first.get_or_insert(error);
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

#[doc(hidden)]
pub fn serve_remote_control(
    endpoint: impl ToSocketAddrs,
    capability: [u8; 32],
    client: std::sync::Arc<ProxyClient>,
) -> std::io::Result<RemoteControl> {
    use std::io::{Read, Write};
    let address = endpoint
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::other("LayerFS control endpoint"))?;
    let mut stream = TcpStream::connect(address)?;
    stream.set_nodelay(true)?;
    stream.write_all(&capability)?;
    stream.write_all(b"c")?;
    let mut accepted = [0];
    stream.read_exact(&mut accepted)?;
    if accepted != [1] {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "LayerFS control capability",
        ));
    }
    let (shutdown_send, shutdown) = std::sync::mpsc::sync_channel(1);
    let (finished, finished_receive) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        let mut stream = stream;
        loop {
            let mut command = [0];
            stream.read_exact(&mut command)?;
            if command == [b's'] {
                let accepted =
                    shutdown_send.send(()).is_ok() && finished_receive.recv().unwrap_or(false);
                stream.write_all(&[u8::from(accepted)])?;
                return Ok(());
            }
            let accepted = u8::from(apply_control(&client, command[0]).is_ok());
            stream.write_all(&[accepted])?;
        }
    });
    Ok(RemoteControl {
        shutdown,
        finished,
        thread: Some(thread),
    })
}

#[doc(hidden)]
pub struct RemoteControl {
    shutdown: std::sync::mpsc::Receiver<()>,
    finished: std::sync::mpsc::SyncSender<bool>,
    thread: Option<std::thread::JoinHandle<std::io::Result<()>>>,
}

impl RemoteControl {
    pub fn wait_for_shutdown(&self) -> std::io::Result<()> {
        self.shutdown
            .recv()
            .map_err(|_| std::io::Error::other("LayerFS control disconnected"))
    }

    pub fn finish_shutdown(mut self, accepted: bool) -> std::io::Result<()> {
        self.finished
            .send(accepted)
            .map_err(|_| std::io::Error::other("LayerFS control disconnected"))?;
        self.thread
            .take()
            .expect("remote control thread")
            .join()
            .map_err(|_| std::io::Error::other("LayerFS control thread"))?
    }
}

fn apply_control(client: &ProxyClient, command: u8) -> PortResult<()> {
    match command {
        b'b' => client.barrier(),
        b'p' => client.pause(),
        b'r' => {
            client.resume();
            Ok(())
        }
        _ => Err(PortError::Invalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{read_request, write_response};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;

    #[test]
    fn multi_stream_barrier_drains_every_stream_after_the_first_error() {
        let (client_a, mut server_a) = stream_pair();
        let (client_b, mut server_b) = stream_pair();
        let seen = Arc::new(AtomicUsize::new(0));
        let first = {
            let seen = seen.clone();
            std::thread::spawn(move || {
                assert!(matches!(
                    read_request(&mut server_a).unwrap(),
                    Request::Fence
                ));
                seen.fetch_add(1, Ordering::Relaxed);
                write_response(&mut server_a, &Response::Error(PortError::NoSpace)).unwrap();
            })
        };
        let second = {
            let seen = seen.clone();
            std::thread::spawn(move || {
                assert!(matches!(
                    read_request(&mut server_b).unwrap(),
                    Request::Fence
                ));
                seen.fetch_add(1, Ordering::Relaxed);
                write_response(&mut server_b, &Response::Unit).unwrap();
            })
        };
        let client = ProxyClient {
            streams: vec![Mutex::new(client_a), Mutex::new(client_b)],
            next: AtomicUsize::new(0),
            cache: Mutex::new(Cache::default()),
            reservation: Mutex::new(Reservation::default()),
            gate: RwLock::new(()),
            paused: AtomicBool::new(false),
            pending: AtomicU64::new(2),
        };

        assert_eq!(client.barrier_locked(), Err(PortError::NoSpace));
        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(seen.load(Ordering::Relaxed), 2);
    }

    fn stream_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }
}
