//! FUSE-side host operation adapter. Mutations are acknowledged by the host;
//! connection uncertainty retains the exact session request until resolved.
use crate::host_wire::{self as wire, Operation as Op, Reply, Request};
use crate::live_runtime::{LiveReservation, LiveRuntime, OperationGate, Scheduler};
use crate::live_transport::{BackingConnection, BackingHandler};
use crate::{Attr, FilesystemPort, Kind, NodeId, PortError, PortFuture, PortResult};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::{Mutex as AsyncMutex, Notify, OwnedSemaphorePermit, Semaphore};

#[path = "host_coherence.rs"]
mod coherence;

// Matches the existing runtime's admitted callback count. Waiting for one of
// these slots precedes a host reference; there is no unowned cleanup backlog.
const CLEANUP_SLOTS: usize = 256;
const CLEANUP_MEMORY: usize = CLEANUP_SLOTS * 4096;

#[derive(Clone)]
enum Endpoint {
    Remote {
        address: String,
        capability: [u8; 32],
    },
    Local(Arc<BackingHandler>),
}
struct Pending {
    sequence: u64,
    frame: Vec<u8>,
    response_bound: usize,
    references: Option<OwnedReferences>,
    _charge: LiveReservation,
}
struct Connection {
    backing: BackingConnection,
    acknowledged: u64,
    pending: Option<Pending>,
    reconnect: bool,
}
struct Inner {
    scheduler: Scheduler,
    endpoint: Endpoint,
    session: [u8; 16],
    connection: AsyncMutex<Connection>,
    closing: AtomicBool,
    gate: OperationGate,
    sdk: coherence::State,
    cleanup_slots: Arc<Semaphore>,
    cleanup_active: AtomicUsize,
    cleanup_done: Notify,
    cleanup_failed: Mutex<Vec<FailedCleanup>>,
    _cleanup_memory: LiveReservation,
    reads: crate::write_metrics::AtomicFuseReadMetrics,
    writes: crate::write_metrics::AtomicFuseWriteMetrics,
}
struct FailedCleanup {
    _nodes: Vec<NodeId>,
    _permit: OwnedSemaphorePermit,
}
#[derive(Clone)]
pub struct HostClient(Arc<Inner>);

struct Response {
    frame: Vec<u8>,
    references: Option<OwnedReferences>,
    _charge: LiveReservation,
}
impl Response {
    fn bytes(&self) -> &[u8] {
        &self.frame[10..]
    }
    fn into_bytes(mut self) -> Vec<u8> {
        self.frame.drain(..10);
        self.frame
    }
    fn take_references(&mut self) -> PortResult<crate::KernelReferences> {
        let mut references = self.references.take().ok_or(PortError::Io)?;
        Ok(crate::KernelReferences {
            owner: Some(crate::port::KernelOwner::Host(
                references.cleanup.take().ok_or(PortError::Io)?,
            )),
            nodes: std::mem::take(&mut references.nodes),
        })
    }
}

struct OwnedReferences {
    cleanup: Option<ReferenceCleanup>,
    nodes: Vec<NodeId>,
}
impl Drop for OwnedReferences {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup.release(std::mem::take(&mut self.nodes), false);
        }
    }
}

/// One pre-admitted rollback for an entry/page reply. The reply guard tells us
/// how many entries reached the kernel; cancellation still releases the whole
/// unsubmitted page, without allocating an additional cleanup job.
pub(crate) struct ReferenceCleanup {
    client: Weak<Inner>,
    permit: OwnedSemaphorePermit,
    pub(crate) emitted: usize,
}
impl ReferenceCleanup {
    pub(crate) fn release(self, mut nodes: Vec<NodeId>, submitted: bool) {
        if submitted {
            nodes.drain(..self.emitted.min(nodes.len()));
        }
        if let Some(client) = self.client.upgrade() {
            HostClient(client).enqueue_cleanup(nodes, self.permit);
        }
    }
}

impl HostClient {
    pub async fn connect(
        address: String,
        capability: [u8; 32],
        session: [u8; 16],
        scheduler: Scheduler,
    ) -> io::Result<Self> {
        let backing = BackingConnection::connect(address.clone(), capability, &scheduler).await?;
        Self::new(
            Endpoint::Remote {
                address,
                capability,
            },
            backing,
            session,
            scheduler,
        )
    }
    pub fn local(
        handler: Arc<BackingHandler>,
        session: [u8; 16],
        scheduler: Scheduler,
    ) -> io::Result<Self> {
        let backing = BackingConnection::local(handler.clone(), scheduler.clone());
        Self::new(Endpoint::Local(handler), backing, session, scheduler)
    }
    fn new(
        endpoint: Endpoint,
        backing: BackingConnection,
        session: [u8; 16],
        scheduler: Scheduler,
    ) -> io::Result<Self> {
        let charge = scheduler.reserve_live(CLEANUP_MEMORY)?;
        let mut cleanup_failed = Vec::new();
        cleanup_failed
            .try_reserve_exact(CLEANUP_SLOTS)
            .map_err(io::Error::other)?;
        Ok(Self(Arc::new(Inner {
            endpoint,
            session,
            scheduler,
            connection: AsyncMutex::new(Connection {
                backing,
                acknowledged: 0,
                pending: None,
                reconnect: false,
            }),
            closing: AtomicBool::new(false),
            gate: OperationGate::default(),
            sdk: coherence::State::default(),
            cleanup_slots: Arc::new(Semaphore::new(CLEANUP_SLOTS)),
            cleanup_active: AtomicUsize::new(0),
            cleanup_done: Notify::new(),
            cleanup_failed: Mutex::new(cleanup_failed),
            _cleanup_memory: charge,
            reads: Default::default(),
            writes: Default::default(),
        })))
    }
    fn run<T>(&self, future: impl std::future::Future<Output = PortResult<T>>) -> PortResult<T> {
        LiveRuntime::shared()
            .map_err(|_| PortError::Io)?
            .block_on(future)
    }
    async fn reconnect(&self, state: &mut Connection) -> PortResult<()> {
        state.backing = match &self.0.endpoint {
            Endpoint::Remote {
                address,
                capability,
            } => BackingConnection::connect(address.clone(), *capability, &self.0.scheduler)
                .await
                .map_err(|_| PortError::Io)?,
            Endpoint::Local(handler) => {
                BackingConnection::local(handler.clone(), self.0.scheduler.clone())
            }
        };
        state.reconnect = false;
        Ok(())
    }
    /// A resumed/cancelled caller cannot assign a fresh mutation sequence until
    /// the previous envelope has a known operation result. One reconnect is a
    /// bounded recovery attempt, never a differently-numbered mutation retry.
    async fn resolve_pending(&self, state: &mut Connection) -> PortResult<Response> {
        let pending = state.pending.as_ref().ok_or(PortError::Io)?;
        let response_charge = self
            .0
            .scheduler
            .reserve_transfer(pending.response_bound * 2)
            .map_err(|_| PortError::NoSpace)?;
        for attempt in 0..2 {
            if state.reconnect {
                self.reconnect(state).await?;
            }
            // Cancellation poisons/takes BackingConnection's partial stream.
            // Mark the next attempt before awaiting so cancellation also keeps
            // an authoritative reconnect obligation alongside this exact frame.
            state.reconnect = true;
            let pending = state.pending.as_ref().ok_or(PortError::Io)?;
            match state.backing.call(&pending.frame).await {
                Ok(frame) if frame.len() <= pending.response_bound => {
                    let known = match wire::decode_reply(&frame, pending.sequence)
                        .map_err(|_| PortError::Io)?
                    {
                        Reply::Known(result) => result.map(|_| ()),
                        Reply::Unknown => return Err(PortError::Io),
                    };
                    if known.is_ok() {
                        let operation = wire::decode_request(&pending.frame)
                            .map_err(|_| PortError::Io)?
                            .operation;
                        let nodes = Self::validate_result(operation, &frame[10..])?;
                        if let Some(references) =
                            &mut state.pending.as_mut().ok_or(PortError::Io)?.references
                        {
                            references.nodes.extend(nodes);
                        }
                    }
                    let pending = state.pending.take().ok_or(PortError::Io)?;
                    state.acknowledged = pending.sequence;
                    state.reconnect = false;
                    known?;
                    return Ok(Response {
                        frame,
                        references: pending.references,
                        _charge: response_charge,
                    });
                }
                _ if attempt == 0 => {}
                _ => return Err(PortError::Io),
            }
        }
        Err(PortError::Io)
    }
    async fn call(&self, operation: Op<'_>) -> PortResult<Response> {
        self.call_owned(operation, None).await
    }
    fn validate_result(operation: Op<'_>, bytes: &[u8]) -> PortResult<Vec<NodeId>> {
        let mut nodes = Vec::new();
        match operation {
            // The SETATTR-class replies carry the installed attr, but the
            // kernel already owns this inode's lookup, so no reference is
            // registered here (unlike Create/Lookup with `kernel: true`).
            Op::Attr(_) | Op::Truncate { .. } | Op::Chmod { .. } | Op::Mtime { .. } => {
                wire::attr_in(bytes).map_err(|_| PortError::Io)?;
            }
            Op::Lookup { kernel, .. }
            | Op::Create { kernel, .. }
            | Op::Mkdir { kernel, .. }
            | Op::Symlink { kernel, .. }
            | Op::Link { kernel, .. } => {
                let attr = wire::attr_in(bytes).map_err(|_| PortError::Io)?;
                if kernel {
                    nodes.push(attr.node);
                }
            }
            Op::Directory { kernel, .. } => {
                let rows = wire::page_in(bytes).map_err(|_| PortError::Io)?;
                if kernel {
                    nodes.extend(
                        rows.into_iter()
                            .filter(|(cookie, _, _)| *cookie > 2)
                            .map(|(_, attr, _)| attr.node),
                    );
                }
            }
            Op::Read { size, .. } | Op::ReadLease { size, .. } => {
                if bytes.len() > size as usize {
                    return Err(PortError::Io);
                }
            }
            Op::Readlink(_) => {
                if bytes.len() > 4096 {
                    return Err(PortError::Io);
                }
            }
            Op::Write { bytes: input, .. } => {
                let size = u64::from_be_bytes(bytes.try_into().map_err(|_| PortError::Io)?);
                if size > input.len() as u64 {
                    return Err(PortError::Io);
                }
            }
            _ => {
                if !bytes.is_empty() {
                    return Err(PortError::Io);
                }
            }
        }
        Ok(nodes)
    }
    async fn call_owned(
        &self,
        operation: Op<'_>,
        references: Option<OwnedReferences>,
    ) -> PortResult<Response> {
        let mut state = self.0.connection.lock().await;
        if state.pending.is_some() {
            // Known failure still resolves the old request. Its original caller
            // may have been cancelled; it must never be assigned this new call.
            match self.resolve_pending(&mut state).await {
                Ok(_) => {}
                Err(_) if state.pending.is_none() => {}
                Err(error) => return Err(error),
            }
        }
        if state.reconnect {
            self.reconnect(&mut state).await?;
        }
        let sequence = if operation.needs_replay() {
            state.acknowledged.checked_add(1).ok_or(PortError::Io)?
        } else {
            0
        };
        let request_bound = operation.request_bound().map_err(|_| PortError::Invalid)?;
        let request_charge = self
            .0
            .scheduler
            .reserve_transfer(request_bound)
            .map_err(|_| PortError::NoSpace)?;
        let frame = wire::encode_request(Request {
            session: self.0.session,
            sequence,
            acknowledged: state.acknowledged,
            operation,
        })
        .map_err(|_| PortError::Invalid)?;
        if operation.needs_replay() {
            state.pending = Some(Pending {
                sequence,
                frame,
                response_bound: operation.response_bound(),
                references,
                _charge: request_charge,
            });
            self.resolve_pending(&mut state).await
        } else {
            let response_charge = self
                .0
                .scheduler
                .reserve_transfer(operation.response_bound() * 2)
                .map_err(|_| PortError::NoSpace)?;
            state.reconnect = true;
            let response = state.backing.call(&frame).await?;
            if response.len() > operation.response_bound() {
                return Err(PortError::Io);
            }
            match wire::decode_reply(&response, 0).map_err(|_| PortError::Io)? {
                Reply::Known(result) => {
                    result?;
                }
                Reply::Unknown => return Err(PortError::Io),
            }
            state.reconnect = false;
            Self::validate_result(operation, &response[10..])?;
            Ok(Response {
                frame: response,
                references: None,
                _charge: response_charge,
            })
        }
    }
    async fn unit(&self, operation: Op<'_>) -> PortResult<()> {
        if !self.call(operation).await?.bytes().is_empty() {
            return Err(PortError::Io);
        }
        Ok(())
    }
    pub async fn read_lease(&self, lease: u64, offset: u64, size: u32) -> PortResult<Vec<u8>> {
        Ok(self
            .call(Op::ReadLease {
                lease,
                offset,
                size,
            })
            .await?
            .into_bytes())
    }
    pub async fn release_lease(&self, lease: u64) -> PortResult<()> {
        self.unit(Op::ReleaseLease { lease }).await
    }
    async fn attribute(&self, operation: Op<'_>) -> PortResult<Attr> {
        wire::attr_in(self.call(operation).await?.bytes()).map_err(|_| PortError::Io)
    }
    async fn page(
        &self,
        node: NodeId,
        after: u64,
        kernel: bool,
    ) -> PortResult<crate::DirectoryPage> {
        wire::page_in(
            self.call(Op::Directory {
                node,
                after,
                kernel,
            })
            .await?
            .bytes(),
        )
        .map_err(|_| PortError::Io)
    }
    async fn cleanup_reservation(&self) -> PortResult<OwnedReferences> {
        let permit = self
            .0
            .cleanup_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| PortError::Io)?;
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(wire::MAX_PAGE)
            .map_err(|_| PortError::NoSpace)?;
        Ok(OwnedReferences {
            cleanup: Some(ReferenceCleanup {
                client: Arc::downgrade(&self.0),
                permit,
                emitted: wire::MAX_PAGE,
            }),
            nodes,
        })
    }
    fn enqueue_cleanup(&self, nodes: Vec<NodeId>, permit: OwnedSemaphorePermit) {
        if nodes.is_empty() {
            return;
        }
        self.0.cleanup_active.fetch_add(1, Ordering::AcqRel);
        let client = self.clone();
        self.0.scheduler.submit(async move {
            let mut entries = Vec::with_capacity(nodes.len() * 16);
            for node in &nodes {
                crate::live_wire::u64_out(&mut entries, node.0);
                crate::live_wire::u64_out(&mut entries, 1);
            }
            let result = client.unit(Op::ForgetBatch { entries: &entries }).await;
            if result.is_err() {
                // The finite pre-admitted slot and node list stay owned until
                // an authoritative detach releases all session kernel refs.
                client.0.cleanup_failed.lock().unwrap().push(FailedCleanup {
                    _nodes: nodes,
                    _permit: permit,
                });
            }
            if client.0.cleanup_active.fetch_sub(1, Ordering::AcqRel) == 1 {
                client.0.cleanup_done.notify_one();
            }
        });
    }
    pub async fn detach(&self) -> PortResult<()> {
        self.0.closing.store(true, Ordering::Release);
        self.release_sdk_after_detach().await?;
        // Explicit lifecycle only. Commit/snapshot acquisition never obtains
        // this cut; existing callbacks retain their guards through the reply.
        let _cut = self.0.gate.cache_flush().await.finish().await;
        while self.0.cleanup_active.load(Ordering::Acquire) != 0 {
            self.0.cleanup_done.notified().await;
        }
        self.unit(Op::Detach).await?;
        // Resolving an abandoned pending entry before Detach can schedule its
        // rollback. The host detach is authoritative, but its admitted job must
        // also finish before releasing the session's local cleanup ownership.
        while self.0.cleanup_active.load(Ordering::Acquire) != 0 {
            self.0.cleanup_done.notified().await;
        }
        self.0
            .cleanup_failed
            .lock()
            .map_err(|_| PortError::Io)?
            .clear();
        Ok(())
    }
    async fn kernel_attribute(
        &self,
        operation: Op<'_>,
    ) -> PortResult<(Attr, crate::KernelReferences)> {
        if self.0.closing.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        let cleanup = self.cleanup_reservation().await?;
        let mut response = self.call_owned(operation, Some(cleanup)).await?;
        let attr = wire::attr_in(response.bytes()).map_err(|_| PortError::Io)?;
        Ok((attr, response.take_references()?))
    }
    async fn written(&self, node: NodeId, offset: u64, bytes: &[u8]) -> PortResult<usize> {
        let response = self
            .call(Op::Write {
                node,
                offset,
                bytes,
            })
            .await?;
        let count =
            u64::from_be_bytes(response.bytes().try_into().map_err(|_| PortError::Io)?) as usize;
        if count > bytes.len() {
            return Err(PortError::Io);
        }
        Ok(count)
    }
}

impl FilesystemPort for HostClient {
    fn callback_gate(
        &self,
        operation: crate::KernelOperation,
        writeback: bool,
    ) -> PortFuture<'_, Option<tokio::sync::OwnedRwLockReadGuard<()>>> {
        // Preserve existing SDK/lifecycle gate classes: folio reads and flush
        // completion must accompany writeback; final handle release owns no cut.
        Box::pin(async move {
            if matches!(
                operation,
                crate::KernelOperation::Release | crate::KernelOperation::Releasedir
            ) {
                return Ok(None);
            }
            let writeback = writeback
                || matches!(
                    operation,
                    crate::KernelOperation::Read
                        | crate::KernelOperation::Fsync
                        | crate::KernelOperation::Fsyncdir
                        | crate::KernelOperation::Flush
                );
            let guard = self.0.gate.enter(writeback).await;
            if self.0.closing.load(Ordering::Acquire) {
                return Err(PortError::Io);
            }
            Ok(Some(guard))
        })
    }
    fn admit_callback(
        &self,
        operation: crate::KernelOperation,
        bytes: usize,
        writeback: bool,
    ) -> PortResult<crate::CallbackGuard> {
        if self.0.closing.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        match operation {
            crate::KernelOperation::Write => self.0.writes.note_kernel_write(bytes as u64),
            crate::KernelOperation::Read => self.0.reads.note_kernel_read(bytes as u64),
            _ => {}
        }
        let control = writeback
            || matches!(
                operation,
                crate::KernelOperation::Read
                    | crate::KernelOperation::Fsync
                    | crate::KernelOperation::Fsyncdir
                    | crate::KernelOperation::Flush
            );
        let bytes = bytes
            .checked_mul(3)
            .and_then(|n| n.checked_add(65536))
            .ok_or(PortError::NoSpace)?;
        Ok(crate::CallbackGuard {
            _gate: None,
            _admission: Some(
                self.0
                    .scheduler
                    .try_admit_from_receiver(bytes, control)
                    .map_err(|_| PortError::NoSpace)?,
            ),
        })
    }
    fn note_kernel_operation(&self, operation: crate::KernelOperation) {
        self.0.reads.note_kernel_operation(operation);
    }
    fn note_readdir_page(&self, offset: u64, entries: u64) {
        self.0.reads.note_readdir_page(offset, entries);
    }
    fn note_fuse_max_write(&self, bytes: u32) {
        self.0.writes.note_max_write(bytes as u64);
    }
    fn note_fuse_read_config(&self, max_readahead: u32, capabilities: u64) {
        self.0.reads.note_config(max_readahead as u64, capabilities);
    }
    fn supports_kernel_lifetime(&self) -> bool {
        true
    }
    fn validate_kernel_open(&self, node: NodeId) -> PortResult<()> {
        if self.attr(node)?.kind != Kind::File {
            return Err(PortError::Invalid);
        }
        Ok(())
    }
    fn prepare_kernel_open(&self, node: NodeId, _: bool) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if self.attribute(Op::Attr(node)).await?.kind != Kind::File {
                return Err(PortError::Invalid);
            }
            Ok(())
        })
    }
    fn kernel_entry_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        operation: crate::KernelEntry,
    ) -> PortFuture<'a, (Attr, crate::KernelReferences)> {
        Box::pin(async move {
            let operation = match &operation {
                crate::KernelEntry::Lookup => Op::Lookup {
                    parent,
                    name,
                    kernel: true,
                },
                crate::KernelEntry::Create { mode } => Op::Create {
                    parent,
                    name,
                    mode: *mode,
                    open: false,
                    kernel: true,
                },
                crate::KernelEntry::Mkdir { mode } => Op::Mkdir {
                    parent,
                    name,
                    mode: *mode,
                    kernel: true,
                },
                crate::KernelEntry::Symlink { target } => Op::Symlink {
                    parent,
                    name,
                    target,
                    kernel: true,
                },
                crate::KernelEntry::Link { node } => Op::Link {
                    node: *node,
                    parent,
                    name,
                    kernel: true,
                },
            };
            self.kernel_attribute(operation).await
        })
    }
    fn kernel_directory_page_async<'a>(
        &'a self,
        node: NodeId,
        after: u64,
    ) -> PortFuture<'a, (crate::DirectoryPage, crate::KernelReferences)> {
        Box::pin(async move {
            if self.0.closing.load(Ordering::Acquire) {
                return Err(PortError::Io);
            }
            let cleanup = self.cleanup_reservation().await?;
            let mut response = self
                .call_owned(
                    Op::Directory {
                        node,
                        after,
                        kernel: true,
                    },
                    Some(cleanup),
                )
                .await?;
            let page = wire::page_in(response.bytes()).map_err(|_| PortError::Io)?;
            Ok((page, response.take_references()?))
        })
    }
    fn kernel_forget(&self, node: NodeId, count: u64) -> PortResult<()> {
        // Native FORGET ingress has no borrowed reply/future. Reply rollback
        // inside Tokio uses pre-admitted ReferenceCleanup instead of this path.
        self.run(self.unit(Op::Forget { node, count }))
    }
    fn kernel_detach(&self) -> PortResult<()> {
        self.run(self.detach())
    }
    fn directory_page_async<'a>(
        &'a self,
        node: NodeId,
        after: u64,
    ) -> PortFuture<'a, crate::DirectoryPage> {
        Box::pin(self.page(node, after, false))
    }
    fn lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        self.run(self.lookup_async(parent, name))
    }
    fn lookup_async<'a>(&'a self, parent: NodeId, name: &'a [u8]) -> PortFuture<'a, Attr> {
        Box::pin(self.attribute(Op::Lookup {
            parent,
            name,
            kernel: false,
        }))
    }
    fn attr(&self, node: NodeId) -> PortResult<Attr> {
        self.run(self.attribute(Op::Attr(node)))
    }
    fn attr_async(&self, node: NodeId) -> PortFuture<'_, Attr> {
        Box::pin(self.attribute(Op::Attr(node)))
    }
    fn readlink(&self, node: NodeId) -> PortResult<Vec<u8>> {
        self.run(async { Ok(self.call(Op::Readlink(node)).await?.into_bytes()) })
    }
    fn readlink_async(&self, node: NodeId) -> PortFuture<'_, Vec<u8>> {
        Box::pin(async move { Ok(self.call(Op::Readlink(node)).await?.into_bytes()) })
    }
    fn readdir(&self, node: NodeId) -> PortResult<Vec<(NodeId, Kind, Vec<u8>)>> {
        self.readdirplus(node).map(|rows| {
            rows.into_iter()
                .map(|(attr, name)| (attr.node, attr.kind, name))
                .collect()
        })
    }
    fn readdirplus(&self, node: NodeId) -> PortResult<Vec<(Attr, Vec<u8>)>> {
        self.run(async {
            let mut after = 0;
            let mut output = Vec::new();
            let mut charges = Vec::new();
            loop {
                let page = self.page(node, after, false).await?;
                if page.is_empty() {
                    return Ok(output);
                }
                charges.push(
                    self.0
                        .scheduler
                        .reserve_live(page.len() * 512)
                        .map_err(|_| PortError::NoSpace)?,
                );
                after = page.last().unwrap().0;
                output.extend(page.into_iter().map(|(_, attr, name)| (attr, name)));
            }
        })
    }
    fn create_file(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.run(self.create_file_async(parent, name, mode))
    }
    fn create_file_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> PortFuture<'a, Attr> {
        Box::pin(self.attribute(Op::Create {
            parent,
            name,
            mode,
            open: false,
            kernel: false,
        }))
    }
    fn create_file_open(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.run(self.create_file_open_async(parent, name, mode))
    }
    fn create_file_open_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> PortFuture<'a, Attr> {
        Box::pin(self.attribute(Op::Create {
            parent,
            name,
            mode,
            open: true,
            kernel: false,
        }))
    }
    fn mkdir(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.run(self.mkdir_async(parent, name, mode))
    }
    fn mkdir_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> PortFuture<'a, Attr> {
        Box::pin(self.attribute(Op::Mkdir {
            parent,
            name,
            mode,
            kernel: false,
        }))
    }
    fn symlink(&self, parent: NodeId, name: &[u8], target: Vec<u8>) -> PortResult<Attr> {
        self.run(self.symlink_async(parent, name, target))
    }
    fn symlink_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        target: Vec<u8>,
    ) -> PortFuture<'a, Attr> {
        Box::pin(async move {
            self.attribute(Op::Symlink {
                parent,
                name,
                target: &target,
                kernel: false,
            })
            .await
        })
    }
    fn link(&self, node: NodeId, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        self.run(self.link_async(node, parent, name))
    }
    fn link_async<'a>(
        &'a self,
        node: NodeId,
        parent: NodeId,
        name: &'a [u8],
    ) -> PortFuture<'a, Attr> {
        Box::pin(self.attribute(Op::Link {
            node,
            parent,
            name,
            kernel: false,
        }))
    }
    fn unlink(&self, parent: NodeId, name: &[u8], directory: bool) -> PortResult<()> {
        self.run(self.unlink_async(parent, name, directory))
    }
    fn unlink_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        directory: bool,
    ) -> PortFuture<'a, ()> {
        Box::pin(self.unit(Op::Unlink {
            parent,
            name,
            directory,
        }))
    }
    fn rename(
        &self,
        parent: NodeId,
        name: &[u8],
        new_parent: NodeId,
        new_name: &[u8],
        no_replace: bool,
    ) -> PortResult<()> {
        self.run(self.rename_async(parent, name, new_parent, new_name, no_replace))
    }
    fn rename_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        new_parent: NodeId,
        new_name: &'a [u8],
        no_replace: bool,
    ) -> PortFuture<'a, ()> {
        Box::pin(self.unit(Op::Rename {
            parent,
            name,
            new_parent,
            new_name,
            no_replace,
        }))
    }
    fn pin(&self, node: NodeId, truncate: bool, writable: bool) -> PortResult<()> {
        self.run(self.pin_async(node, truncate, writable))
    }
    fn pin_async<'a>(&'a self, node: NodeId, truncate: bool, writable: bool) -> PortFuture<'a, ()> {
        Box::pin(self.unit(Op::Pin {
            node,
            directory: false,
            truncate,
            writable,
        }))
    }
    fn unpin(&self, node: NodeId, writable: bool) -> PortResult<()> {
        self.run(self.unit(Op::Unpin {
            node,
            directory: false,
            writable,
        }))
    }
    fn unpin_async(&self, node: NodeId, writable: bool) -> PortFuture<'_, ()> {
        Box::pin(self.unit(Op::Unpin {
            node,
            directory: false,
            writable,
        }))
    }
    fn pin_directory(&self, node: NodeId) -> PortResult<()> {
        self.run(self.unit(Op::Pin {
            node,
            directory: true,
            truncate: false,
            writable: false,
        }))
    }
    fn pin_directory_async(&self, node: NodeId) -> PortFuture<'_, ()> {
        Box::pin(self.unit(Op::Pin {
            node,
            directory: true,
            truncate: false,
            writable: false,
        }))
    }
    fn unpin_directory(&self, node: NodeId) -> PortResult<()> {
        self.run(self.unit(Op::Unpin {
            node,
            directory: true,
            writable: false,
        }))
    }
    fn unpin_directory_async(&self, node: NodeId) -> PortFuture<'_, ()> {
        Box::pin(self.unit(Op::Unpin {
            node,
            directory: true,
            writable: false,
        }))
    }
    fn read(&self, node: NodeId, offset: u64, size: usize) -> PortResult<Vec<u8>> {
        let size = u32::try_from(size).map_err(|_| PortError::Invalid)?;
        self.run(async {
            Ok(self
                .call(Op::Read { node, offset, size })
                .await?
                .into_bytes())
        })
    }
    fn write(&self, node: NodeId, offset: u64, bytes: &[u8]) -> PortResult<usize> {
        self.run(self.written(node, offset, bytes))
    }
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    fn submit_write(
        &self,
        node: NodeId,
        offset: u64,
        bytes: &[u8],
        writeback: bool,
        mut reply: crate::WriteReply,
    ) {
        // The adapter's owned reply retains admission for this copied input.
        let bytes = bytes.to_vec();
        let client = self.clone();
        self.0.scheduler.submit(async move {
            reply._guard._gate = match client
                .callback_gate(crate::KernelOperation::Write, writeback)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.complete(Err(error));
                    return;
                }
            };
            let result = if writeback {
                client.writeback_owned(node, offset, &bytes).await
            } else {
                client.written(node, offset, &bytes).await
            };
            reply.complete(result);
        });
    }
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    fn submit_read(&self, node: NodeId, offset: u64, size: usize, mut reply: crate::ReadReply) {
        let client = self.clone();
        self.0.scheduler.submit(async move {
            reply._guard._gate = match client
                .callback_gate(crate::KernelOperation::Read, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.complete(Err(error));
                    return;
                }
            };
            let size = match u32::try_from(size) {
                Ok(size) => size,
                Err(_) => {
                    reply.complete(Err(PortError::Invalid));
                    return;
                }
            };
            match client.call(Op::Read { node, offset, size }).await {
                Ok(mut response) => {
                    response.frame.drain(..10);
                    reply.complete(Ok(std::mem::take(&mut response.frame)));
                    // Retain response-buffer accounting through the kernel reply.
                    drop(response);
                }
                Err(error) => reply.complete(Err(error)),
            }
        });
    }
    fn truncate(&self, node: NodeId, size: u64) -> PortResult<()> {
        self.run(self.truncate_async(node, size)).map(|_| ())
    }
    fn truncate_async<'a>(&'a self, node: NodeId, size: u64) -> PortFuture<'a, Attr> {
        Box::pin(self.attribute(Op::Truncate { node, size }))
    }
    fn chmod(&self, node: NodeId, mode: u32) -> PortResult<()> {
        self.run(self.chmod_async(node, mode)).map(|_| ())
    }
    fn chmod_async<'a>(&'a self, node: NodeId, mode: u32) -> PortFuture<'a, Attr> {
        Box::pin(self.attribute(Op::Chmod { node, mode }))
    }
    fn set_mtime(&self, node: NodeId, seconds: i64, nanos: u32) -> PortResult<()> {
        self.run(self.set_mtime_async(node, seconds, nanos))
            .map(|_| ())
    }
    fn set_mtime_async<'a>(
        &'a self,
        node: NodeId,
        seconds: i64,
        nanos: u32,
    ) -> PortFuture<'a, Attr> {
        Box::pin(self.attribute(Op::Mtime {
            node,
            seconds,
            nanos,
        }))
    }
    fn fsync(&self, node: Option<NodeId>) -> PortResult<()> {
        self.run(self.fsync_async(node))
    }
    fn fsync_async<'a>(&'a self, node: Option<NodeId>) -> PortFuture<'a, ()> {
        Box::pin(self.unit(Op::Fsync(node)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicU64;

    #[derive(Default)]
    struct Backend {
        completed: Mutex<BTreeMap<u64, (Vec<u8>, Vec<u8>)>>,
        observed: Mutex<Vec<Vec<u8>>>,
        applied: AtomicUsize,
        refs: AtomicU64,
        lose_reply: AtomicBool,
        unknown: AtomicBool,
        installed: Mutex<Option<Attr>>,
        started: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        release: (Mutex<bool>, std::sync::Condvar),
    }
    fn attr(node: u64) -> Attr {
        Attr {
            node: NodeId(node),
            size: 4,
            kind: Kind::File,
            mode: 0o600,
            links: 1,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        }
    }
    impl Backend {
        fn handle(&self, frame: &[u8]) -> PortResult<Vec<u8>> {
            self.observed.lock().unwrap().push(frame.to_vec());
            let request = wire::decode_request(frame).map_err(|_| PortError::Invalid)?;
            if request.sequence != 0 {
                if self.unknown.load(Ordering::Acquire) {
                    return Ok(wire::unknown(request.sequence));
                }
                if let Some((original, response)) =
                    self.completed.lock().unwrap().get(&request.sequence)
                {
                    assert_eq!(
                        original, frame,
                        "uncertain mutation must retain exact envelope"
                    );
                    return Ok(response.clone());
                }
            }
            let result = match request.operation {
                Op::Attr(node) => Ok(wire::attr_out(
                    self.installed.lock().unwrap().unwrap_or(attr(node.0)),
                )),
                // #144 R1a: SETATTR-class replies carry the installed attr.
                Op::Chmod { node, mode } => {
                    let mut installed = self.installed.lock().unwrap();
                    let attr = installed.get_or_insert_with(|| attr(node.0));
                    attr.mode = mode;
                    self.applied.fetch_add(1, Ordering::AcqRel);
                    Ok(wire::attr_out(*attr))
                }
                Op::Mtime {
                    node,
                    seconds,
                    nanos,
                } => {
                    let mut installed = self.installed.lock().unwrap();
                    let attr = installed.get_or_insert_with(|| attr(node.0));
                    attr.mtime_seconds = seconds;
                    attr.mtime_nanoseconds = nanos;
                    self.applied.fetch_add(1, Ordering::AcqRel);
                    Ok(wire::attr_out(*attr))
                }
                Op::Truncate { node, size } => {
                    let mut installed = self.installed.lock().unwrap();
                    let attr = installed.get_or_insert_with(|| attr(node.0));
                    attr.size = size;
                    self.applied.fetch_add(1, Ordering::AcqRel);
                    Ok(wire::attr_out(*attr))
                }
                Op::Lookup { kernel, .. } => {
                    if kernel {
                        self.refs.fetch_add(1, Ordering::AcqRel);
                    }
                    Ok(wire::attr_out(attr(2)))
                }
                Op::Write {
                    bytes: b"no-space", ..
                } => Err(PortError::NoSpace),
                Op::Write { bytes, .. } => {
                    self.applied.fetch_add(1, Ordering::AcqRel);
                    Ok((bytes.len() as u64).to_be_bytes().to_vec())
                }
                Op::Directory { kernel, .. } => {
                    if kernel {
                        self.refs.fetch_add(2, Ordering::AcqRel);
                    }
                    Ok(wire::page_out(&[
                        (3, attr(2), b"one".to_vec()),
                        (4, attr(3), b"two".to_vec()),
                    ])
                    .unwrap())
                }
                Op::Forget { count, .. } => {
                    self.refs.fetch_sub(count, Ordering::AcqRel);
                    Ok(Vec::new())
                }
                Op::ForgetBatch { entries } => {
                    let count = wire::forget_batch_in(entries)
                        .unwrap()
                        .iter()
                        .map(|(_, count)| count)
                        .sum::<u64>();
                    self.refs
                        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                            Some(current.saturating_sub(count))
                        })
                        .unwrap();
                    Ok(Vec::new())
                }
                Op::Detach => {
                    self.refs.store(0, Ordering::Release);
                    Ok(Vec::new())
                }
                _ => Err(PortError::Invalid),
            };
            let response = wire::reply(
                request.sequence,
                result.as_ref().map(Vec::as_slice).map_err(|error| *error),
            )
            .unwrap();
            if request.sequence != 0 {
                self.completed
                    .lock()
                    .unwrap()
                    .insert(request.sequence, (frame.to_vec(), response.clone()));
            }
            if let Some(started) = self.started.lock().unwrap().take() {
                let _ = started.send(());
                let (lock, ready) = &self.release;
                let guard = lock.lock().unwrap();
                let (guard, timeout) = ready
                    .wait_timeout_while(guard, std::time::Duration::from_secs(10), |release| {
                        !*release
                    })
                    .unwrap();
                assert!(
                    *guard && !timeout.timed_out(),
                    "test response latch timed out"
                );
            }
            if self.lose_reply.swap(false, Ordering::AcqRel) {
                return Err(PortError::Io);
            }
            Ok(response)
        }
        fn client(self: &Arc<Self>) -> HostClient {
            let backend = self.clone();
            HostClient::local(
                Arc::new(move |bytes| backend.handle(bytes)),
                [7; 16],
                LiveRuntime::shared().unwrap().scheduler(),
            )
            .unwrap()
        }
    }

    #[test]
    fn explicit_detach_waits_for_an_admitted_callback() {
        use std::future::Future;
        let backend = Arc::new(Backend::default());
        let client = backend.client();
        LiveRuntime::shared().unwrap().block_on(async {
            let callback = client
                .callback_gate(crate::KernelOperation::Write, false)
                .await
                .unwrap()
                .unwrap();
            let mut detach = std::pin::pin!(client.detach());
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(detach.as_mut().poll(&mut context).is_pending());
            assert!(backend.observed.lock().unwrap().is_empty());
            drop(callback);
            detach.await.unwrap();
            assert!(matches!(
                wire::decode_request(backend.observed.lock().unwrap().last().unwrap())
                    .unwrap()
                    .operation,
                Op::Detach
            ));
        });
    }

    #[test]
    fn host_acknowledgment_and_unknown_recovery_keep_one_exact_mutation() {
        let backend = Arc::new(Backend::default());
        let client = backend.client();
        backend.lose_reply.store(true, Ordering::Release);
        assert_eq!(client.write(NodeId(2), 0, b"four").unwrap(), 4);
        assert_eq!(backend.applied.load(Ordering::Acquire), 1);
        assert_eq!(
            client.write(NodeId(2), 0, b"no-space"),
            Err(PortError::NoSpace)
        );
        assert_eq!(client.write(NodeId(2), 4, b"next").unwrap(), 4);
        assert_eq!(backend.applied.load(Ordering::Acquire), 2);
        backend.unknown.store(true, Ordering::Release);
        assert_eq!(client.write(NodeId(2), 8, b"wait"), Err(PortError::Io));
        assert_eq!(client.attr(NodeId(2)), Err(PortError::Io));
        let frames = backend.observed.lock().unwrap();
        let last = frames.last().unwrap();
        assert_eq!(last, &frames[frames.len() - 2]);
        assert_eq!(wire::decode_request(last).unwrap().sequence, 4);
        drop(frames);
        backend.unknown.store(false, Ordering::Release);
        client.attr(NodeId(2)).unwrap();
        assert_eq!(backend.applied.load(Ordering::Acquire), 3);
        assert!(client
            .run(async { Ok(client.0.connection.lock().await.pending.is_none()) })
            .unwrap());
    }

    #[test]
    fn cancelled_entry_and_partial_page_keep_pre_admitted_cleanup() {
        let backend = Arc::new(Backend::default());
        let client = backend.client();
        LiveRuntime::shared().unwrap().block_on(async {
            let (started, waiting) = tokio::sync::oneshot::channel();
            *backend.started.lock().unwrap() = Some(started);
            let worker = client.clone();
            let task = tokio::spawn(async move {
                worker
                    .kernel_attribute(Op::Lookup {
                        parent: NodeId(1),
                        name: b"cold",
                        kernel: true,
                    })
                    .await
            });
            waiting.await.unwrap();
            assert_eq!(backend.refs.load(Ordering::Acquire), 1);
            task.abort();
            assert!(matches!(task.await, Err(error) if error.is_cancelled()));
            assert_eq!(
                client.0.cleanup_slots.available_permits(),
                CLEANUP_SLOTS - 1
            );
            *backend.release.0.lock().unwrap() = true;
            backend.release.1.notify_all();
            client.attribute(Op::Attr(NodeId(2))).await.unwrap();
            while client.0.cleanup_active.load(Ordering::Acquire) != 0 {
                client.0.cleanup_done.notified().await;
            }
            assert_eq!(backend.refs.load(Ordering::Acquire), 0);
            assert_eq!(client.0.cleanup_slots.available_permits(), CLEANUP_SLOTS);

            let (_, references) = client
                .kernel_attribute(Op::Lookup {
                    parent: NodeId(1),
                    name: b"submitted",
                    kernel: true,
                })
                .await
                .unwrap();
            references.submitted();
            assert_eq!(backend.refs.load(Ordering::Acquire), 1);
            client
                .unit(Op::Forget {
                    node: NodeId(2),
                    count: 1,
                })
                .await
                .unwrap();

            let (page, mut references) = client
                .kernel_directory_page_async(NodeId(1), 0)
                .await
                .unwrap();
            assert_eq!(page.len(), 2);
            assert_eq!(backend.refs.load(Ordering::Acquire), 2);
            references.release_unemitted(1).unwrap();
            references.submitted();
            while client.0.cleanup_active.load(Ordering::Acquire) != 0 {
                client.0.cleanup_done.notified().await;
            }
            assert_eq!(backend.refs.load(Ordering::Acquire), 1);
            client
                .unit(Op::Forget {
                    node: NodeId(2),
                    count: 1,
                })
                .await
                .unwrap();
            client.detach().await.unwrap();
            assert_eq!(backend.refs.load(Ordering::Acquire), 0);
            assert_eq!(client.0.cleanup_slots.available_permits(), CLEANUP_SLOTS);
        });
    }
    /// #144 R1a: each SETATTR-class mutation is exactly one wire frame whose
    /// reply carries the installed attr. The client must not issue a follow-up
    /// `Op::Attr` to build the kernel reply.
    #[test]
    fn setattr_class_mutations_take_one_frame_and_reply_with_the_installed_attr() {
        let backend = Arc::new(Backend::default());
        let client = backend.client();
        LiveRuntime::shared().unwrap().block_on(async {
            let attr = client.chmod_async(NodeId(2), 0o640).await.unwrap();
            assert_eq!((attr.node, attr.mode), (NodeId(2), 0o640));
            let attr = client
                .set_mtime_async(NodeId(2), 1_700_000_000, 7)
                .await
                .unwrap();
            assert_eq!(
                (attr.mtime_seconds, attr.mtime_nanoseconds),
                (1_700_000_000, 7)
            );
            assert_eq!(
                attr.mode, 0o640,
                "the mutation reply carries the whole record"
            );
            let attr = client.truncate_async(NodeId(2), 9).await.unwrap();
            assert_eq!((attr.size, attr.mode), (9, 0o640));
            let observed = backend.observed.lock().unwrap().clone();
            assert_eq!(
                observed.len(),
                3,
                "one frame per mutation: the reply is the attr, not a re-fetch"
            );
            assert!(observed.iter().all(|frame| {
                !matches!(wire::decode_request(frame).unwrap().operation, Op::Attr(_))
            }));
            assert_eq!(backend.applied.load(Ordering::Acquire), 3);
        });
    }
}
