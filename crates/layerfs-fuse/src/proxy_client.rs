use crate::protocol::{
    read_response_measured, write_request_measured, ClosedCreate, Request, Response,
};
use crate::write_metrics::{AtomicFuseReadMetrics, AtomicFuseWriteMetrics};
use crate::{Attr, FilesystemPort, Kind, NodeId, PortError, PortResult};
use std::collections::{BTreeMap, HashMap};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};

const CONNECTIONS: usize = 1;
const MAX_PENDING_UNLINKS: usize = 16_384;
const READ_AHEAD_BYTES: usize = 2 * 1024 * 1024;
const READ_AHEAD_ENTRIES: usize = 4;
const WRITE_COALESCE_BYTES: usize = 1024 * 1024;
type DirectoryEntries = Vec<(NodeId, Kind, Vec<u8>)>;
type DirectoryEntriesPlus = Vec<(Attr, Vec<u8>)>;

pub struct ProxyClient {
    streams: Vec<Mutex<TcpStream>>,
    next: AtomicUsize,
    cache: Mutex<Cache>,
    write_buffer: Mutex<Option<BufferedWrite>>,
    reservation: Mutex<Reservation>,
    gate: RwLock<()>,
    callbacks: RwLock<()>,
    paused: AtomicBool,
    pending: AtomicU64,
    metrics: AtomicFuseWriteMetrics,
    read_metrics: AtomicFuseReadMetrics,
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    notifier: std::sync::OnceLock<fuser::Notifier>,
}

#[derive(Default)]
struct Cache {
    attrs: HashMap<NodeId, Attr>,
    directories: HashMap<NodeId, CachedDirectory>,
    pending_creates: HashMap<NodeId, PendingCreate>,
    pending_closed: Vec<ClosedCreate>,
    pending_closed_bytes: usize,
    pending_unlinks: Vec<(NodeId, Vec<u8>)>,
    read_ahead: BTreeMap<NodeId, ReadAhead>,
}

struct CachedDirectory {
    special: DirectoryEntries,
    entries: BTreeMap<Vec<u8>, (NodeId, Kind)>,
}

struct ReadAhead {
    offset: u64,
    bytes: Vec<u8>,
    served: usize,
}

struct BufferedWrite {
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
            write_buffer: Mutex::new(None),
            reservation: Mutex::new(Reservation::default()),
            gate: RwLock::new(()),
            callbacks: RwLock::new(()),
            paused: AtomicBool::new(false),
            pending: AtomicU64::new(0),
            metrics: AtomicFuseWriteMetrics::default(),
            read_metrics: AtomicFuseReadMetrics::default(),
            #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
            notifier: std::sync::OnceLock::new(),
        };
        Ok(client)
    }

    fn exchange(&self, request: Request) -> PortResult<Response> {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.streams.len();
        self.exchange_at(index, request)
    }

    fn enter_callback(&self) -> PortResult<std::sync::RwLockReadGuard<'_, ()>> {
        let guard = self.callbacks.read().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        Ok(guard)
    }

    fn exchange_at(&self, index: usize, request: Request) -> PortResult<Response> {
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.flush_write_locked()?;
        self.raw_exchange_at(index, request)
    }

    fn raw_exchange_at(&self, index: usize, request: Request) -> PortResult<Response> {
        let mut stream = self.streams[index].lock().map_err(|_| PortError::Io)?;
        let is_read = matches!(&request, Request::Read(..));
        self.write_request(&mut stream, &request)?;
        let (response, measured) =
            read_response_measured(&mut *stream).map_err(|_| PortError::Io)?;
        if is_read {
            self.read_metrics.note_client_response(
                measured.frame_count,
                measured.frame_bytes,
                measured.payload_copy_bytes,
                measured.socket_ns,
                measured.decode_ns,
            );
        }
        match response {
            Response::Error(error) => Err(error),
            response => Ok(response),
        }
    }

    fn send_at(&self, index: usize, request: Request) -> PortResult<()> {
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.flush_write_locked()?;
        self.raw_send_at(index, request)
    }

    fn raw_send_at(&self, index: usize, request: Request) -> PortResult<()> {
        let mut stream = self.streams[index].lock().map_err(|_| PortError::Io)?;
        self.write_request(&mut stream, &request)?;
        self.pending.fetch_add(1, Ordering::Release);
        Ok(())
    }

    fn write_request(&self, stream: &mut TcpStream, request: &Request) -> PortResult<()> {
        let measured = write_request_measured(stream, request).map_err(|_| PortError::Io)?;
        if measured.logical_bytes != 0 {
            self.metrics.note_client_frame(
                measured.frame_bytes,
                measured.payload_copy_bytes,
                measured.encode_ns,
                measured.socket_ns,
            );
        }
        Ok(())
    }

    fn node_stream(&self, node: NodeId) -> usize {
        node.0 as usize % self.streams.len()
    }

    fn buffer_write(&self, node: NodeId, offset: u64, value: &[u8]) -> PortResult<()> {
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        let mut slot = self.write_buffer.lock().map_err(|_| PortError::Io)?;
        let contiguous = slot.as_ref().is_some_and(|buffer| {
            buffer.node == node
                && buffer
                    .offset
                    .checked_add(buffer.bytes.len() as u64)
                    .is_some_and(|end| end == offset)
                && buffer.bytes.len().saturating_add(value.len()) <= WRITE_COALESCE_BYTES
        });
        if !contiguous {
            if let Some(buffer) = slot.take() {
                self.send_buffer_locked(buffer)?;
            }
            if value.len() >= WRITE_COALESCE_BYTES {
                self.metrics.note_client_copy(value.len() as u64);
                return self.raw_send_at(
                    self.node_stream(node),
                    Request::Write(node, offset, value.to_vec()),
                );
            }
            *slot = Some(BufferedWrite {
                node,
                offset,
                bytes: Vec::with_capacity(WRITE_COALESCE_BYTES),
            });
        }
        self.metrics.note_client_copy(value.len() as u64);
        let buffer = slot.as_mut().expect("write buffer");
        buffer.bytes.extend_from_slice(value);
        if buffer.bytes.len() == WRITE_COALESCE_BYTES {
            let buffer = slot.take().expect("full write buffer");
            self.send_buffer_locked(buffer)?;
        }
        Ok(())
    }

    fn flush_write_locked(&self) -> PortResult<()> {
        if let Some(buffer) = self.write_buffer.lock().map_err(|_| PortError::Io)?.take() {
            self.send_buffer_locked(buffer)?;
        }
        Ok(())
    }

    fn flush_write_for_locked(&self, node: NodeId) -> PortResult<()> {
        let buffer = {
            let mut slot = self.write_buffer.lock().map_err(|_| PortError::Io)?;
            slot.as_ref()
                .is_some_and(|buffer| buffer.node == node)
                .then(|| slot.take().expect("matching write buffer"))
        };
        if let Some(buffer) = buffer {
            self.send_buffer_locked(buffer)?;
        }
        Ok(())
    }

    fn send_buffer_locked(&self, buffer: BufferedWrite) -> PortResult<()> {
        let stream = self.node_stream(buffer.node);
        if let Some(pending) = self
            .cache
            .lock()
            .map_err(|_| PortError::Io)?
            .pending_creates
            .remove(&buffer.node)
        {
            self.send_pending_create(stream, buffer.node, pending)?;
        }
        self.raw_send_at(
            stream,
            Request::Write(buffer.node, buffer.offset, buffer.bytes),
        )
    }

    fn flush_write(&self) -> PortResult<()> {
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.flush_write_locked()
    }

    fn flush_write_for(&self, node: NodeId) -> PortResult<()> {
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.flush_write_for_locked(node)
    }

    fn exchange_for(&self, node: NodeId, request: Request) -> PortResult<Response> {
        let _gate = self.gate.read().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.flush_write_for_locked(node)?;
        self.raw_exchange_at(self.node_stream(node), request)
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
        // Drain complete callbacks, including cache fills after transport replies.
        let _callbacks = self.callbacks.write().map_err(|_| PortError::Io)?;
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        self.flush_pending_locked()?;
        self.barrier_locked()?;
        self.paused.store(true, Ordering::Release);
        Ok(())
    }

    #[doc(hidden)]
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
    }

    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    pub fn set_notifier(&self, notifier: fuser::Notifier) -> std::io::Result<()> {
        self.notifier
            .set(notifier)
            .map_err(|_| std::io::Error::other("LayerFS notifier already set"))
    }

    fn invalidate_file(&self, node: NodeId) -> PortResult<()> {
        let _callbacks = self.callbacks.write().map_err(|_| PortError::Io)?;
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if node.0 == 0 || !self.paused.load(Ordering::Acquire) {
            return Err(PortError::Invalid);
        }
        self.cache
            .lock()
            .map_err(|_| PortError::Io)?
            .attrs
            .remove(&node);
        self.invalidate_read_ahead(node)?;
        // A mounted proxy must invalidate both kernel attributes and file pages.
        // Unmounted protocol clients have no kernel cache to invalidate.
        #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
        if let Some(notifier) = self.notifier.get() {
            notifier
                .inval_inode(fuser::INodeNo(node.0), 0, 0)
                .map_err(|_| PortError::Io)?;
        }
        Ok(())
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
        } else if let Ok(mut cache) = self.cache.lock() {
            // A deferred failure has no reliable inode attribution. Previously
            // acknowledged optimistic sizes/names/reads must be fetched again.
            // The caller already holds gate; do not recurse through invalidate_file.
            // Keep pending mutations and the original error/fence semantics intact.
            cache.attrs.clear();
            cache.directories.clear();
            for (_, read) in std::mem::take(&mut cache.read_ahead) {
                self.read_metrics.note_unused(read.bytes.len().saturating_sub(read.served) as u64);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl FilesystemPort for ProxyClient {
    fn note_kernel_operation(&self, operation: crate::KernelOperation) {
        self.read_metrics.note_kernel_operation(operation);
    }
    fn note_readdir_page(&self, offset: u64, entries: u64) {
        self.read_metrics.note_readdir_page(offset, entries);
    }

    fn note_fuse_max_write(&self, bytes: u32) {
        self.metrics.note_max_write(u64::from(bytes));
    }

    fn note_fuse_read_config(&self, max_readahead: u32, capabilities: u64) {
        self.read_metrics
            .note_config(u64::from(max_readahead), capabilities);
    }

    fn lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        let _callback = self.enter_callback()?;
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
        let _callback = self.enter_callback()?;
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
        let _callback = self.enter_callback()?;
        bytes(self.exchange_at(self.node_stream(node), Request::Readlink(node))?)
    }

    fn readdir(&self, node: NodeId) -> PortResult<Vec<(NodeId, Kind, Vec<u8>)>> {
        let _callback = self.enter_callback()?;
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

    fn readdirplus(&self, node: NodeId) -> PortResult<Vec<(Attr, Vec<u8>)>> {
        let _callback = self.enter_callback()?;
        self.barrier()?;
        if let Some(entries) = self.cached_readdirplus(node)? {
            return Ok(entries);
        }
        match self.exchange(Request::ReaddirPlus(node))? {
            Response::EntriesPlus(entries) => {
                self.remember_directory_plus(node, &entries)?;
                Ok(entries)
            }
            _ => Err(PortError::Io),
        }
    }

    fn create_file(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        let _callback = self.enter_callback()?;
        self.flush_unlink_for(parent, name)?;
        let attr = attr(self.exchange(Request::CreateFile(parent, name.to_vec(), mode))?)?;
        self.remember(parent, name, attr)?;
        Ok(attr)
    }

    fn create_file_open(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        let _callback = self.enter_callback()?;
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
        let _callback = self.enter_callback()?;
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
        let _callback = self.enter_callback()?;
        self.flush_unlink_for(parent, name)?;
        let attr = attr(self.exchange(Request::Symlink(parent, name.to_vec(), target))?)?;
        self.remember(parent, name, attr)?;
        Ok(attr)
    }

    fn link(&self, node: NodeId, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        let _callback = self.enter_callback()?;
        self.flush_pending_create(node)?;
        self.flush_unlink_for(parent, name)?;
        let attr = attr(self.exchange(Request::Link(node, parent, name.to_vec()))?)?;
        self.remember(parent, name, attr)?;
        Ok(attr)
    }

    fn unlink(&self, parent: NodeId, name: &[u8], directory: bool) -> PortResult<()> {
        let _callback = self.enter_callback()?;
        self.flush_write()?;
        let (parent_cached, pending) = {
            let cache = self.cache.lock().map_err(|_| PortError::Io)?;
            let parent_directory = cache.directories.get(&parent);
            (
                parent_directory.is_some(),
                parent_directory
                    .and_then(|directory| directory.entries.get(name))
                    .map(|(node, _)| *node),
            )
        };
        if let Some(node) = pending {
            self.flush_pending_create(node)?;
        }
        if directory {
            self.barrier()?;
            unit(self.exchange(Request::Unlink(parent, name.to_vec(), true))?)?;
        } else {
            self.queue_unlink(parent, name)?;
            // Without a complete directory cache, a subsequent lookup reaches
            // the host. Publish this deletion before acknowledging it so that
            // the old binding cannot reappear until a later unrelated barrier.
            if !parent_cached {
                self.barrier()?;
            }
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
        let _callback = self.enter_callback()?;
        self.flush_write()?;
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
        let source = cache
            .directories
            .get(&parent)
            .and_then(|directory| directory.entries.get(name))
            .map(|(node, _)| *node);
        let target = cache.directories.get(&new_parent).map(|directory| {
            directory.entries.get(new_name).map(|(node, _)| *node)
        });
        if source.is_some() && target == Some(source) {
            // The backend treats two names for the same inode as a no-op.
            // Preserve both cached bindings and the unchanged link count.
            return Ok(());
        }
        if !no_replace {
            match target {
                Some(Some(node)) => {
                    // Replacement unlinks this inode, including its other aliases.
                    // The next attr must fetch the backend's new link count.
                    cache.attrs.remove(&node);
                }
                None => {
                    // Without a complete destination directory, the overwritten
                    // inode is unknown. Preserve only not-yet-published creates:
                    // their attributes cannot be fetched from the backend yet.
                    let closed = cache.pending_closed.iter().map(|entry| entry.3)
                        .collect::<std::collections::HashSet<_>>();
                    let Cache { attrs, pending_creates, .. } = &mut *cache;
                    attrs.retain(|node, _| pending_creates.contains_key(node) || closed.contains(node));
                }
                Some(None) => (),
            }
        }
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
        let _callback = self.enter_callback()?;
        self.flush_pending_create(node)?;
        if !truncate && !writable {
            // Open may expose a handle only after the backend retained the inode.
            // A cached kernel inode can already have been replaced or unlinked.
            return unit(self.exchange_at(self.node_stream(node), Request::Pin(node, false, false))?);
        }
        self.invalidate_read_ahead(node)?;
        unit(self.exchange(Request::Pin(node, truncate, writable))?)
    }

    fn unpin(&self, node: NodeId, writable: bool) -> PortResult<()> {
        // Releases remain permitted while paused so owner quiescence can drain handles.
        let _callback = self.callbacks.read().map_err(|_| PortError::Io)?;
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        self.flush_write_locked()?;
        self.invalidate_read_ahead(node)?;
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
        let _callback = self.enter_callback()?;
        self.read_metrics.note_kernel_read(size as u64);
        self.flush_pending_create(node)?;
        self.flush_write_for(node)?;
        if let Some(bytes) = self.cached_read(node, offset, size)? {
            return Ok(bytes);
        }
        let requested = size.max(READ_AHEAD_BYTES);
        let fetched = bytes(self.exchange_for(node, Request::Read(node, offset, requested))?)?;
        let output = fetched[..fetched.len().min(size)].to_vec();
        self.read_metrics.note_read_ahead_miss(
            requested as u64,
            fetched.len() as u64,
            output.len() as u64,
        );
        if fetched.len() <= output.len() {
            return Ok(output);
        }
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        if let Some(previous) = cache.read_ahead.remove(&node) {
            self.read_metrics
                .note_unused(previous.bytes.len().saturating_sub(previous.served) as u64);
        }
        if cache.read_ahead.len() == READ_AHEAD_ENTRIES {
            if let Some((_, previous)) = cache.read_ahead.pop_first() {
                self.read_metrics
                    .note_unused(previous.bytes.len().saturating_sub(previous.served) as u64);
            }
        }
        cache.read_ahead.insert(
            node,
            ReadAhead {
                offset,
                bytes: fetched,
                served: output.len(),
            },
        );
        let cached = cache
            .read_ahead
            .values()
            .map(|read| read.bytes.len() as u64)
            .sum();
        self.read_metrics.note_config(cached, 0);
        Ok(output)
    }

    fn write(&self, node: NodeId, offset: u64, value: &[u8]) -> PortResult<usize> {
        let _callback = self.enter_callback()?;
        self.metrics.note_kernel_write(value.len() as u64);
        self.invalidate_read_ahead(node)?;
        if is_zero(value) {
            self.flush_write()?;
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
        self.buffer_write(node, offset, value)?;
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
        let _callback = self.enter_callback()?;
        self.flush_write()?;
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
        let _callback = self.enter_callback()?;
        self.flush_write()?;
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
        let _callback = self.enter_callback()?;
        self.flush_write()?;
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
        let _callback = self.enter_callback()?;
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
    #[doc(hidden)]
    pub fn take_write_metrics(&self) -> crate::FuseWriteMetrics {
        self.metrics.take()
    }

    #[doc(hidden)]
    pub fn take_read_metrics(&self) -> crate::FuseReadMetrics {
        if let Ok(mut cache) = self.cache.lock() {
            for (_, read) in std::mem::take(&mut cache.read_ahead) {
                self.read_metrics
                    .note_unused(read.bytes.len().saturating_sub(read.served) as u64);
            }
        }
        self.read_metrics.take()
    }

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

    fn cached_readdirplus(&self, node: NodeId) -> PortResult<Option<DirectoryEntriesPlus>> {
        let cache = self.cache.lock().map_err(|_| PortError::Io)?;
        let Some(directory) = cache.directories.get(&node) else {
            return Ok(None);
        };
        let mut output = Vec::with_capacity(directory.special.len() + directory.entries.len());
        for (node, _, name) in &directory.special {
            let Some(attr) = cache.attrs.get(node).copied() else {
                return Ok(None);
            };
            output.push((attr, name.clone()));
        }
        for (name, (node, _)) in &directory.entries {
            let Some(attr) = cache.attrs.get(node).copied() else {
                return Ok(None);
            };
            output.push((attr, name.clone()));
        }
        Ok(Some(output))
    }

    fn cached_read(&self, node: NodeId, offset: u64, size: usize) -> PortResult<Option<Vec<u8>>> {
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        let Some(read) = cache.read_ahead.get_mut(&node) else {
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
        if end - relative < size && read.bytes.len() == READ_AHEAD_BYTES {
            return Ok(None);
        }
        let output = read.bytes[relative..end].to_vec();
        read.served = read
            .served
            .saturating_add(output.len())
            .min(read.bytes.len());
        self.read_metrics.note_read_ahead_hit(output.len() as u64);
        Ok(Some(output))
    }

    fn invalidate_read_ahead(&self, node: NodeId) -> PortResult<()> {
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        if let Some(read) = cache.read_ahead.remove(&node) {
            self.read_metrics
                .note_unused(read.bytes.len().saturating_sub(read.served) as u64);
        }
        Ok(())
    }

    fn flush_pending_create(&self, node: NodeId) -> PortResult<()> {
        let (closed, pending) = {
            let cache = self.cache.lock().map_err(|_| PortError::Io)?;
            (
                cache.pending_closed.iter().any(|entry| entry.3 == node),
                cache.pending_creates.contains_key(&node),
            )
        };
        if !closed && !pending {
            return Ok(());
        }
        if closed {
            let _gate = self.gate.write().map_err(|_| PortError::Io)?;
            if self.paused.load(Ordering::Acquire) {
                return Err(PortError::Busy);
            }
            self.flush_pending_locked()?;
        }
        let _gate = self.gate.write().map_err(|_| PortError::Io)?;
        if self.paused.load(Ordering::Acquire) {
            return Err(PortError::Busy);
        }
        self.flush_write_locked()?;
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
        self.flush_write_locked()?;
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

    fn remember_directory_plus(&self, node: NodeId, entries: &[(Attr, Vec<u8>)]) -> PortResult<()> {
        let special = entries
            .iter()
            .filter(|(_, name)| matches!(name.as_slice(), b"." | b".."))
            .map(|(attr, name)| (attr.node, attr.kind, name.clone()))
            .collect();
        let directory_entries = entries
            .iter()
            .filter(|(_, name)| !matches!(name.as_slice(), b"." | b".."))
            .map(|(attr, name)| (name.clone(), (attr.node, attr.kind)))
            .collect();
        let mut cache = self.cache.lock().map_err(|_| PortError::Io)?;
        cache
            .attrs
            .extend(entries.iter().map(|(attr, _)| (attr.node, *attr)));
        cache.directories.insert(
            node,
            CachedDirectory {
                special,
                entries: directory_entries,
            },
        );
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
            if command == [b'm'] {
                let metrics = client.take_write_metrics();
                stream.write_all(&[1])?;
                metrics.write_to(&mut stream)?;
                continue;
            }
            if command == [b'n'] {
                let metrics = client.take_read_metrics();
                stream.write_all(&[1])?;
                metrics.write_to(&mut stream)?;
                continue;
            }
            if command == [b'i'] {
                let mut node = [0; 8];
                stream.read_exact(&mut node)?;
                let accepted = client
                    .invalidate_file(NodeId(u64::from_le_bytes(node)))
                    .is_ok();
                stream.write_all(&[u8::from(accepted)])?;
                continue;
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
    fn acknowledged_unlink_is_absent_with_and_without_parent_cache() {
        for parent_cached in [false, true] {
            let (stream, mut server) = stream_pair();
            server
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let original = Attr {
                node: NodeId(2),
                size: 6,
                kind: Kind::File,
                mode: 0o600,
                links: 1,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            };
            let host = std::thread::spawn(move || {
                let mut exists = true;
                let mut removed = false;
                while let Ok(request) = read_request(&mut server) {
                    match request {
                        Request::Lookup(NodeId(1), name) if name == b"deleted" => {
                            write_response(
                                &mut server,
                                &if exists {
                                    Response::Attr(original)
                                } else {
                                    Response::Error(PortError::NotFound)
                                },
                            )
                            .unwrap();
                        }
                        Request::UnlinkBatch(entries) => {
                            assert_eq!(entries, vec![(NodeId(1), b"deleted".to_vec())]);
                            exists = false;
                            removed = true;
                        }
                        Request::Fence => write_response(&mut server, &Response::Unit).unwrap(),
                        _ => panic!("unexpected unlink visibility request"),
                    }
                }
                removed
            });
            let client = ProxyClient {
                streams: vec![Mutex::new(stream)],
                next: AtomicUsize::new(0),
                cache: Mutex::new(Cache::default()),
                write_buffer: Mutex::new(None),
                reservation: Mutex::new(Reservation::default()),
                gate: RwLock::new(()),
                callbacks: RwLock::new(()),
                paused: AtomicBool::new(false),
                pending: AtomicU64::new(0),
                metrics: AtomicFuseWriteMetrics::default(),
                read_metrics: AtomicFuseReadMetrics::default(),
                #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
                notifier: std::sync::OnceLock::new(),
            };
            assert_eq!(client.lookup(NodeId(1), b"deleted").unwrap(), original);
            if parent_cached {
                client
                    .remember_directory(
                        NodeId(1),
                        &[(original.node, Kind::File, b"deleted".to_vec())],
                    )
                    .unwrap();
            }
            client.unlink(NodeId(1), b"deleted", false).unwrap();
            let lookup_after_ack = client.lookup(NodeId(1), b"deleted");
            let queued = client.cache.lock().unwrap().pending_unlinks.len();
            drop(client);
            let host_removed = host.join().unwrap();
            assert_eq!(
                lookup_after_ack,
                Err(PortError::NotFound),
                "parent_cached={parent_cached}"
            );
            assert_eq!(host_removed, !parent_cached);
            assert_eq!(queued, usize::from(parent_cached));
        }
    }

    #[test]
    fn metadata_cache_fill_is_inside_owner_pause_barrier() {
        let (stream, mut server) = stream_pair();
        let client = ProxyClient {
            streams: vec![Mutex::new(stream)],
            next: AtomicUsize::new(0),
            cache: Mutex::new(Cache::default()),
            write_buffer: Mutex::new(None),
            reservation: Mutex::new(Reservation::default()),
            gate: RwLock::new(()),
            callbacks: RwLock::new(()),
            paused: AtomicBool::new(false),
            pending: AtomicU64::new(0),
            metrics: AtomicFuseWriteMetrics::default(),
            read_metrics: AtomicFuseReadMetrics::default(),
            #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
            notifier: std::sync::OnceLock::new(),
        };
        let old = Attr {
            node: NodeId(2),
            size: 6,
            kind: Kind::File,
            mode: 0o600,
            links: 1,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        };
        std::thread::scope(|scope| {
            let request = scope.spawn(|| client.attr(NodeId(2)));
            assert!(matches!(
                read_request(&mut server).unwrap(),
                Request::Attr(NodeId(2))
            ));
            let cache = client.cache.lock().unwrap();
            write_response(&mut server, &Response::Attr(old)).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                if client.gate.try_write().is_ok() {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "response did not drain"
                );
                std::thread::yield_now();
            }
            // The transport has finished, but the callback cannot fill its cache yet.
            assert!(client.callbacks.try_write().is_err());
            drop(cache);
            assert_eq!(request.join().unwrap().unwrap(), old);
        });
        client.pause().unwrap();
        client.invalidate_file(NodeId(2)).unwrap();
        assert!(client.cache.lock().unwrap().attrs.is_empty());
        client.resume();
        std::thread::scope(|scope| {
            let request = scope.spawn(|| client.attr(NodeId(2)));
            assert!(matches!(
                read_request(&mut server).unwrap(),
                Request::Attr(NodeId(2))
            ));
            let new = Attr { size: 9, ..old };
            write_response(&mut server, &Response::Attr(new)).unwrap();
            assert_eq!(request.join().unwrap().unwrap(), new);
        });
    }

    #[test]
    fn readonly_pin_waits_for_ack_and_rejects_replaced_inode() {
        for missing in [false, true] {
            let (stream, mut server) = stream_pair();
            server.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
            let (requested, request_seen) = std::sync::mpsc::channel();
            let (acknowledge, ack_allowed) = std::sync::mpsc::channel();
            let host = std::thread::spawn(move || {
                assert!(matches!(read_request(&mut server).unwrap(), Request::Pin(NodeId(2), false, false)));
                requested.send(()).unwrap();
                ack_allowed.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
                // A replaced cached inode must fail at Open; otherwise pin success
                // means the backend has retained it before the handle is exposed.
                write_response(&mut server, &if missing {Response::Error(PortError::NotFound)} else {Response::Unit}).unwrap();
            });
            let client = Arc::new(ProxyClient {
                streams: vec![Mutex::new(stream)], next: AtomicUsize::new(0),
                cache: Mutex::new(Cache::default()), write_buffer: Mutex::new(None),
                reservation: Mutex::new(Reservation::default()), gate: RwLock::new(()),
                callbacks: RwLock::new(()), paused: AtomicBool::new(false), pending: AtomicU64::new(0),
                metrics: AtomicFuseWriteMetrics::default(), read_metrics: AtomicFuseReadMetrics::default(),
                #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
                notifier: std::sync::OnceLock::new(),
            });
            client.cache.lock().unwrap().read_ahead.insert(NodeId(2), ReadAhead {offset:0,bytes:vec![7;4],served:0});
            let (returned, result) = std::sync::mpsc::channel();
            let caller = { let client = client.clone(); std::thread::spawn(move || {
                returned.send(client.pin(NodeId(2), false, false)).unwrap();
            }) };
            request_seen.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            assert!(matches!(result.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)), "read Open completed before pin acknowledgement");
            acknowledge.send(()).unwrap();
            assert_eq!(result.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
                if missing {Err(PortError::NotFound)} else {Ok(())});
            assert_eq!(client.pending.load(Ordering::Acquire), 0);
            assert!(client.cache.lock().unwrap().read_ahead.contains_key(&NodeId(2)));
            caller.join().unwrap(); host.join().unwrap();
        }
    }

    #[test]
    fn rename_refreshes_overwritten_alias_attributes_and_preserves_same_inode_names() {
        // Known replacement, unknown destination, same-inode no-op, and a
        // completely cached absent destination exercise each cache decision.
        for scenario in 0..4 {
            let (stream, mut server) = stream_pair();
            server.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
            let old = Attr { node: NodeId(2), size: 6, kind: Kind::File, mode: 0o640,
                links: 2, mtime_seconds: 0, mtime_nanoseconds: 0 };
            let host = std::thread::spawn(move || {
                assert!(matches!(read_request(&mut server).unwrap(),
                    Request::Rename(NodeId(1), name, NodeId(3), target, false)
                    if name == b"source" && target == b"target"));
                write_response(&mut server, &Response::Unit).unwrap();
                if scenario < 2 {
                    assert!(matches!(read_request(&mut server).unwrap(), Request::Attr(NodeId(2))));
                    write_response(&mut server, &Response::Attr(Attr { links: 1, ..old })).unwrap();
                }
            });
            let client = ProxyClient {
                streams: vec![Mutex::new(stream)], next: AtomicUsize::new(0),
                cache: Mutex::new(Cache::default()), write_buffer: Mutex::new(None),
                reservation: Mutex::new(Reservation::default()), gate: RwLock::new(()),
                callbacks: RwLock::new(()), paused: AtomicBool::new(false), pending: AtomicU64::new(0),
                metrics: AtomicFuseWriteMetrics::default(), read_metrics: AtomicFuseReadMetrics::default(),
                #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
                notifier: std::sync::OnceLock::new(),
            };
            let source = if scenario == 2 { NodeId(2) } else { NodeId(4) };
            client.remember_directory(NodeId(1), &[(source, Kind::File, b"source".to_vec())]).unwrap();
            if scenario != 1 {
                let entries = if scenario == 3 { vec![] } else { vec![(NodeId(2), Kind::File, b"target".to_vec())] };
                client.remember_directory(NodeId(3), &entries).unwrap();
            }
            {
                let mut cache = client.cache.lock().unwrap();
                cache.attrs.insert(NodeId(2), old);
                cache.attrs.insert(NodeId(9), Attr { node: NodeId(9), links: 1, ..old });
                cache.attrs.insert(NodeId(10), Attr { node: NodeId(10), links: 1, ..old });
                cache.pending_closed.push((NodeId(8), b"closed".to_vec(), 0o640, NodeId(10), vec![], None));
                cache.pending_creates.insert(NodeId(9), PendingCreate {
                    parent: NodeId(8), name: b"pending".to_vec(), mode: 0o640,
                    mtime: None, zero_len: 6, writes: vec![], bytes: 0,
                });
            }
            client.rename(NodeId(1), b"source", NodeId(3), b"target", false).unwrap();
            assert_eq!(client.attr(NodeId(2)).unwrap().links, if scenario < 2 { 1 } else { 2 });
            let cache = client.cache.lock().unwrap();
            assert!(cache.attrs.contains_key(&NodeId(9)), "pending create attr remains locally available");
            assert!(cache.attrs.contains_key(&NodeId(10)), "pending closed-create attr remains locally available");
            assert_eq!(cache.directories[&NodeId(1)].entries.contains_key(b"source".as_slice()), scenario == 2);
            if scenario == 2 {
                assert_eq!(cache.directories[&NodeId(3)].entries[b"target".as_slice()].0, NodeId(2));
            }
            drop(cache);
            drop(client);
            host.join().unwrap();
        }
    }

    #[test]
    fn deferred_write_error_invalidates_optimistic_observations() {
        for failure in [PortError::Io, PortError::NoSpace] {
            let (stream, mut server) = stream_pair();
            server.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
            let original = Attr {node:NodeId(2),size:0,kind:Kind::File,mode:0o640,
                links:1,mtime_seconds:0,mtime_nanoseconds:0};
            let host = std::thread::spawn(move || {
                assert!(matches!(read_request(&mut server).unwrap(),Request::Attr(NodeId(2))));
                write_response(&mut server,&Response::Attr(original)).unwrap();
                assert!(matches!(read_request(&mut server).unwrap(),Request::Write(NodeId(2),0,bytes) if bytes==vec![7;4096]));
                // Backend rejects the append and keeps its original zero length.
                assert!(matches!(read_request(&mut server).unwrap(),Request::Fsync(Some(NodeId(2)))));
                write_response(&mut server,&Response::Error(failure)).unwrap();
                assert!(matches!(read_request(&mut server).unwrap(),Request::Attr(NodeId(2))));
                write_response(&mut server,&Response::Attr(original)).unwrap();
                assert!(matches!(read_request(&mut server).unwrap(),Request::Fence));
                write_response(&mut server,&Response::Unit).unwrap();
            });
            let client = ProxyClient {
                streams:vec![Mutex::new(stream)],next:AtomicUsize::new(0),
                cache:Mutex::new(Cache::default()),write_buffer:Mutex::new(None),
                reservation:Mutex::new(Reservation::default()),gate:RwLock::new(()),
                callbacks:RwLock::new(()),paused:AtomicBool::new(false),pending:AtomicU64::new(0),
                metrics:AtomicFuseWriteMetrics::default(),read_metrics:AtomicFuseReadMetrics::default(),
                #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
                notifier:std::sync::OnceLock::new(),
            };
            assert_eq!(client.attr(NodeId(2)).unwrap(),original);
            client.remember_directory(NodeId(1),&[(NodeId(2),Kind::File,b"file".to_vec())]).unwrap();
            client.cache.lock().unwrap().read_ahead.insert(NodeId(3),ReadAhead {offset:0,bytes:vec![1,2,3],served:0});
            assert_eq!(client.write(NodeId(2),0,&[7;4096]).unwrap(),4096);
            assert_eq!(client.attr(NodeId(2)).unwrap().size,4096);
            assert_eq!(client.fsync(Some(NodeId(2))),Err(failure));
            assert_eq!(client.pending.load(Ordering::Acquire),1);
            {
                let cache=client.cache.lock().unwrap();
                assert!(cache.attrs.is_empty());
                assert!(cache.directories.is_empty());
                assert!(cache.read_ahead.is_empty());
            }
            assert_eq!(client.attr(NodeId(2)).unwrap(),original);
            client.barrier().unwrap();
            assert_eq!(client.pending.load(Ordering::Acquire),0);
            drop(client);
            host.join().unwrap();
        }
    }

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
            write_buffer: Mutex::new(None),
            reservation: Mutex::new(Reservation::default()),
            gate: RwLock::new(()),
            callbacks: RwLock::new(()),
            paused: AtomicBool::new(false),
            pending: AtomicU64::new(2),
            metrics: AtomicFuseWriteMetrics::default(),
            read_metrics: AtomicFuseReadMetrics::default(),
            #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
            notifier: std::sync::OnceLock::new(),
        };

        assert_eq!(client.barrier_locked(), Err(PortError::NoSpace));
        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(seen.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn read_ordering_flushes_only_the_same_inode() {
        let (client_stream, mut server_stream) = stream_pair();
        let client = ProxyClient {
            streams: vec![Mutex::new(client_stream)],
            next: AtomicUsize::new(0),
            cache: Mutex::new(Cache::default()),
            write_buffer: Mutex::new(Some(BufferedWrite {
                node: NodeId(2),
                offset: 0,
                bytes: b"pending".to_vec(),
            })),
            reservation: Mutex::new(Reservation::default()),
            gate: RwLock::new(()),
            callbacks: RwLock::new(()),
            paused: AtomicBool::new(false),
            pending: AtomicU64::new(0),
            metrics: AtomicFuseWriteMetrics::default(),
            read_metrics: AtomicFuseReadMetrics::default(),
            #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
            notifier: std::sync::OnceLock::new(),
        };

        client.flush_write_for(NodeId(3)).unwrap();
        assert_eq!(
            client.write_buffer.lock().unwrap().as_ref().unwrap().node,
            NodeId(2)
        );

        client.flush_write_for(NodeId(2)).unwrap();
        assert!(matches!(
            read_request(&mut server_stream).unwrap(),
            Request::Write(NodeId(2), 0, bytes) if bytes == b"pending"
        ));
        assert!(client.write_buffer.lock().unwrap().is_none());
    }

    fn stream_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }
}
