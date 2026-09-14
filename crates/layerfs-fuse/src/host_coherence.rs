//! Explicit SDK cache scope. No Commit or snapshot-acquisition caller enters it.
use super::*;
use crate::live_owner::{ControlHandler, KernelEdit, LiveControl};
use crate::live_runtime::{CacheFlush, OperationCut};
use crate::live_wire;
use layerfs_content::ObjectId;

const SDK_MEMORY: usize = 2 * wire::MAX_IO + live_wire::MAX_FRAME;

#[derive(Clone, Copy)]
pub(super) struct Lease {
    pub id: u64,
    pub size: u64,
}
#[derive(Default)]
pub(super) struct State {
    book: AsyncMutex<Book>,
    pub(super) protection: Mutex<Option<Arc<KernelEdit<Lease>>>>,
    pub(super) writeback_order: AsyncMutex<()>,
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    notifier: std::sync::OnceLock<fuser::Notifier>,
    #[cfg(any(target_os = "linux", test))]
    root: Mutex<Option<Arc<std::fs::File>>>,
    #[cfg(test)]
    kernel: Mutex<Option<Arc<dyn Fn(NodeId) -> PortFuture<'static, ()> + Send + Sync>>>,
}
#[derive(Default)]
struct Book {
    active: Option<Arc<Scope>>,
    settled: Option<(u64, Option<ObjectId>)>,
}
struct Scope {
    id: u64,
    node: NodeId,
    ending: AtomicBool,
    phase: Mutex<Phase>,
    changed: Notify,
}
struct Phase {
    cut: Option<OperationCut>,
    flush: Option<CacheFlush>,
    reservation: Option<LiveReservation>,
    identity: Option<ObjectId>,
    lease: Option<u64>,
    reconciled: bool,
    running: bool,
    result: Option<PortResult<()>>,
}

impl ControlHandler for HostClient {
    fn request<'a>(&'a self, bytes: &'a [u8]) -> PortFuture<'a, Vec<u8>> {
        Box::pin(self.local_control(bytes))
    }
}
impl HostClient {
    pub(super) async fn writeback_owned(
        &self,
        node: NodeId,
        offset: u64,
        bytes: &[u8],
    ) -> PortResult<usize> {
        if bytes.len() > wire::MAX_IO || offset.checked_add(bytes.len() as u64).is_none() {
            return Err(PortError::Invalid);
        }
        let protection = self
            .0
            .sdk
            .protection
            .lock()
            .map_err(|_| PortError::Io)?
            .clone()
            .filter(|edit| edit.node == node);
        let Some(edit) = protection else {
            return self.written(node, offset, bytes).await;
        };
        // Only the one active SDK target needs this ordering while queued
        // folios are reconciled. There is no resident per-inode lock registry.
        let _order = self.0.sdk.writeback_order.lock().await;
        let count = (bytes.len() as u64).min(edit.file.size.saturating_sub(offset)) as usize;
        if count == 0 {
            return Ok(bytes.len());
        }
        let mut patched = bytes[..count].to_vec();
        for range in &edit.ranges {
            let start = offset.max(range.start);
            let end = (offset + count as u64).min(range.end);
            if start < end {
                let replacement = self
                    .read_lease(edit.file.id, start, (end - start) as u32)
                    .await?;
                if replacement.len() as u64 != end - start {
                    return Err(PortError::Io);
                }
                patched[(start - offset) as usize..(end - offset) as usize]
                    .copy_from_slice(&replacement);
            }
        }
        self.written(node, offset, &patched).await?;
        Ok(bytes.len())
    }

    pub fn serve_control(&self, endpoint: String, capability: [u8; 32]) -> io::Result<LiveControl> {
        LiveControl::serve(self.clone(), self.0.scheduler.clone(), endpoint, capability)
    }
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    pub fn set_notifier(&self, notifier: fuser::Notifier) -> io::Result<()> {
        self.0
            .sdk
            .notifier
            .set(notifier)
            .map_err(|_| live_wire::invalid())
    }
    #[cfg(any(target_os = "linux", test))]
    pub fn set_kernel_root(&self, root: std::fs::File) -> io::Result<()> {
        let mut slot = self.0.sdk.root.lock().map_err(|_| live_wire::invalid())?;
        if slot.is_some() || self.0.closing.load(Ordering::Acquire) {
            return Err(live_wire::invalid());
        }
        *slot = Some(Arc::new(root));
        Ok(())
    }
    async fn flush_inode(&self, node: NodeId) -> PortResult<()> {
        #[cfg(test)]
        {
            let kernel = self.0.sdk.kernel.lock().map_err(|_| PortError::Io)?.clone();
            if let Some(kernel) = kernel {
                return kernel(node).await;
            }
        }
        #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
        {
            use std::os::fd::AsRawFd;
            let notifier = self.0.sdk.notifier.get().ok_or(PortError::Io)?.clone();
            let root = self
                .0
                .sdk
                .root
                .lock()
                .map_err(|_| PortError::Io)?
                .clone()
                .ok_or(PortError::Io)?;
            self.0
                .scheduler
                .kernel(move || {
                    match notifier.inval_inode(fuser::INodeNo(node.0), 0, 0) {
                        // A host-only inode has no cached kernel pages to invalidate.
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                        result => result?,
                    }
                    // INVAL_INODE discards laundering errors in the kernel. This
                    // descriptor predates writes and observes the mount errseq.
                    nix::unistd::syncfs(root.as_raw_fd()).map_err(io::Error::from)
                })
                .await
                .map_err(|_| PortError::Io)
        }
        #[cfg(not(all(target_os = "linux", any(feature = "host", feature = "proxy"))))]
        {
            let _ = node;
            Err(PortError::Invalid)
        }
    }
    pub async fn local_control(&self, bytes: &[u8]) -> PortResult<Vec<u8>> {
        match bytes {
            [live_wire::SHUTDOWN] => {
                self.prepare_shutdown().map_err(|_| PortError::Io)?;
                return Ok(Vec::new());
            }
            [live_wire::WRITE_METRICS] => {
                let mut metrics = self.0.writes.take();
                metrics.merge(self.0.connection.lock().await.backing.metrics.take());
                let mut out = Vec::new();
                metrics.write_to(&mut out).map_err(|_| PortError::Io)?;
                return Ok(out);
            }
            [live_wire::READ_METRICS] => {
                let mut out = Vec::new();
                self.0
                    .reads
                    .take()
                    .write_to(&mut out)
                    .map_err(|_| PortError::Io)?;
                return Ok(out);
            }
            // The host owns observation and canonical publication state. A
            // daemon mirror must not fabricate a root/head or reinstate FREEZE.
            [live_wire::OBSERVE] | [live_wire::FREEZE] | [live_wire::RESUME] => {
                return Err(PortError::Invalid)
            }
            _ => {}
        }
        match wire::decode_coherence(bytes).map_err(|_| PortError::Invalid)? {
            wire::Coherence::Begin { scope, node } => self.sdk_begin(scope, node).await?,
            wire::Coherence::Cancel { scope } => self.sdk_cancel(scope).await?,
            wire::Coherence::Apply {
                scope,
                lease,
                node,
                size,
                ranges,
            } => {
                self.sdk_apply(scope, lease, node, size, ranges, ObjectId::for_bytes(bytes))
                    .await?;
            }
        }
        Ok(Vec::new())
    }
    async fn sdk_begin(&self, id: u64, node: NodeId) -> PortResult<()> {
        if self.0.closing.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        let mut book = self.0.sdk.book.lock().await;
        if let Some(scope) = &book.active {
            return if scope.id == id
                && scope.node == node
                && scope
                    .phase
                    .lock()
                    .map_err(|_| PortError::Io)?
                    .identity
                    .is_none()
            {
                Ok(())
            } else {
                Err(PortError::Busy)
            };
        }
        if book.settled.as_ref().is_some_and(|(last, _)| id <= *last) {
            return Err(PortError::Invalid);
        }
        // Reserve mask and maximum protected-folio working memory before the
        // host is permitted to install its SDK mutation.
        let reservation = self
            .0
            .scheduler
            .reserve_live(SDK_MEMORY)
            .map_err(|_| PortError::NoSpace)?;
        let flush = self.0.gate.cache_flush().await;
        self.flush_inode(node).await?;
        let cut = flush.finish().await;
        book.active = Some(Arc::new(Scope {
            id,
            node,
            ending: AtomicBool::new(false),
            changed: Notify::new(),
            phase: Mutex::new(Phase {
                cut: Some(cut),
                flush: None,
                reservation: Some(reservation),
                identity: None,
                lease: None,
                reconciled: false,
                running: false,
                result: None,
            }),
        }));
        Ok(())
    }
    async fn sdk_cancel(&self, id: u64) -> PortResult<()> {
        let mut book = self.0.sdk.book.lock().await;
        let Some(scope) = &book.active else {
            if book.settled == Some((id, None)) {
                return Ok(());
            }
            // BEGIN's reply can be lost before or after its scope is created.
            // A newer cancellation authoritatively excludes a delayed BEGIN
            // with that identity; no mutation has been installed in this case.
            if book.settled.as_ref().is_none_or(|(last, _)| id > *last) {
                book.settled = Some((id, None));
                return Ok(());
            }
            return Err(PortError::Invalid);
        };
        if scope.id != id
            || scope
                .phase
                .lock()
                .map_err(|_| PortError::Io)?
                .identity
                .is_some()
        {
            return Err(PortError::Invalid);
        }
        book.active.take();
        book.settled = Some((id, None));
        Ok(())
    }
    async fn sdk_apply(
        &self,
        id: u64,
        lease: u64,
        node: NodeId,
        size: u64,
        encoded_ranges: &[u8],
        identity: ObjectId,
    ) -> PortResult<()> {
        let scope = {
            let book = self.0.sdk.book.lock().await;
            let Some(scope) = &book.active else {
                return if book.settled == Some((id, Some(identity))) {
                    Ok(())
                } else {
                    Err(PortError::Invalid)
                };
            };
            if scope.id != id || scope.node != node {
                return Err(PortError::Invalid);
            }
            if scope.ending.load(Ordering::Acquire) {
                return Err(PortError::Io);
            }
            scope.clone()
        };
        let start = {
            let mut phase = scope.phase.lock().map_err(|_| PortError::Io)?;
            if scope.ending.load(Ordering::Acquire) {
                return Err(PortError::Io);
            }
            if phase.identity.is_some_and(|old| old != identity) {
                return Err(PortError::Invalid);
            }
            if phase.identity.is_none() {
                let mut ranges = Vec::new();
                ranges
                    .try_reserve_exact(encoded_ranges.len() / 16)
                    .map_err(|_| PortError::NoSpace)?;
                ranges.extend(wire::coherence_ranges(encoded_ranges));
                let charge = phase.reservation.take().ok_or(PortError::Io)?;
                *self.0.sdk.protection.lock().map_err(|_| PortError::Io)? =
                    Some(Arc::new(KernelEdit {
                        node,
                        file: Lease { id: lease, size },
                        ranges,
                        _charge: charge,
                    }));
                phase.identity = Some(identity);
                phase.lease = Some(lease);
            }
            if phase.running {
                false
            } else {
                phase.running = true;
                phase.result = None;
                true
            }
        };
        if start {
            let client = self.clone();
            let scope = scope.clone();
            // Response cancellation does not cancel the admitted SDK worker or
            // drop its cut/protected lease halfway through reconciliation.
            self.0.scheduler.handle.spawn(async move {
                client.sdk_apply_work(scope).await;
            });
        }
        loop {
            let notified = scope.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = scope.phase.lock().map_err(|_| PortError::Io)?.result {
                return result;
            }
            notified.await;
        }
    }
    async fn sdk_apply_work(&self, scope: Arc<Scope>) {
        let (reconciled, cut, flush, lease) = {
            let mut phase = scope.phase.lock().unwrap();
            (
                phase.reconciled,
                phase.cut.take(),
                phase.flush.take(),
                phase.lease.unwrap(),
            )
        };
        let cut = if reconciled {
            cut.expect("reconciled SDK scope owns cut")
        } else {
            let flush = flush
                .unwrap_or_else(|| cut.expect("prepared SDK scope owns cut").reopen_writeback());
            if let Err(error) = self.flush_inode(scope.node).await {
                let mut phase = scope.phase.lock().unwrap();
                phase.flush = Some(flush);
                phase.running = false;
                phase.result = Some(Err(error));
                drop(phase);
                scope.changed.notify_waiters();
                return;
            }
            let cut = flush.finish().await;
            scope.phase.lock().unwrap().reconciled = true;
            cut
        };
        if let Err(error) = self.release_lease(lease).await {
            let mut phase = scope.phase.lock().unwrap();
            phase.cut = Some(cut);
            phase.running = false;
            phase.result = Some(Err(error));
            drop(phase);
            scope.changed.notify_waiters();
            return;
        }
        self.0.sdk.protection.lock().unwrap().take();
        let identity = scope.phase.lock().unwrap().identity;
        drop(cut);
        {
            let mut book = self.0.sdk.book.lock().await;
            if book
                .active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &scope))
            {
                book.active.take();
                book.settled = Some((scope.id, identity));
            }
        }
        let mut phase = scope.phase.lock().unwrap();
        phase.running = false;
        phase.result = Some(Ok(()));
        drop(phase);
        scope.changed.notify_waiters();
    }

    pub(super) async fn release_sdk_after_detach(&self) -> PortResult<()> {
        let scope = {
            let book = self.0.sdk.book.lock().await;
            let Some(scope) = &book.active else {
                return Ok(());
            };
            scope.ending.store(true, Ordering::Release);
            scope.clone()
        };
        loop {
            let notified = scope.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !scope.phase.lock().map_err(|_| PortError::Io)?.running {
                break;
            }
            notified.await;
        }
        let lease = {
            let phase = scope.phase.lock().map_err(|_| PortError::Io)?;
            if phase.result == Some(Ok(())) {
                None
            } else {
                phase.lease
            }
        };
        if let Some(lease) = lease {
            self.release_lease(lease).await?;
        }
        self.0
            .sdk
            .protection
            .lock()
            .map_err(|_| PortError::Io)?
            .take();
        {
            let mut phase = scope.phase.lock().map_err(|_| PortError::Io)?;
            phase.cut.take();
            phase.flush.take();
            phase.reservation.take();
            phase.result = Some(Err(PortError::Io));
        }
        let mut book = self.0.sdk.book.lock().await;
        if book
            .active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &scope))
        {
            book.active.take();
            book.settled = Some((scope.id, None));
        }
        scope.changed.notify_waiters();
        Ok(())
    }

    pub fn prepare_shutdown(&self) -> io::Result<()> {
        self.0.closing.store(true, Ordering::Release);
        // The preopened mount descriptor itself prevents ordinary unmount.
        // In-flight syncfs work keeps its independent Arc until completion.
        #[cfg(any(target_os = "linux", test))]
        self.0
            .sdk
            .root
            .lock()
            .map_err(|_| live_wire::invalid())?
            .take();
        // Actual kernel detach owns final callback/lease draining. Do not
        // silently cancel an installed SDK scope as an ordinary error fallback.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;

    #[derive(Default)]
    struct Model {
        reads: AtomicUsize,
        releases: AtomicUsize,
        unavailable: AtomicBool,
        fail_release_once: AtomicBool,
        writes: Mutex<Vec<Vec<u8>>>,
    }
    fn client(model: &Arc<Model>) -> HostClient {
        let model = model.clone();
        HostClient::local(
            Arc::new(move |frame| {
                let request = wire::decode_request(frame).map_err(|_| PortError::Invalid)?;
                let result = match request.operation {
                    Op::ReadLease {
                        lease: 7,
                        offset,
                        size,
                    } => {
                        assert!(
                            !model.unavailable.load(Ordering::Acquire),
                            "reconciled retry must not reread released lease"
                        );
                        model.reads.fetch_add(size as usize, Ordering::AcqRel);
                        Ok((offset..offset + size as u64)
                            .map(|offset| (offset % 251) as u8)
                            .collect::<Vec<_>>())
                    }
                    Op::ReleaseLease { lease: 7 } => {
                        model.releases.fetch_add(1, Ordering::AcqRel);
                        model.unavailable.store(true, Ordering::Release);
                        if model.fail_release_once.swap(false, Ordering::AcqRel) {
                            Err(PortError::Io)
                        } else {
                            Ok(Vec::new())
                        }
                    }
                    Op::Write { bytes, .. } => {
                        model.writes.lock().unwrap().push(bytes.to_vec());
                        Ok((bytes.len() as u64).to_be_bytes().to_vec())
                    }
                    Op::Detach => Ok(Vec::new()),
                    _ => Err(PortError::Invalid),
                };
                wire::reply(
                    request.sequence,
                    result.as_ref().map(Vec::as_slice).map_err(|error| *error),
                )
                .map_err(|_| PortError::Io)
            }),
            [51; 16],
            LiveRuntime::shared().unwrap().scheduler(),
        )
        .unwrap()
    }
    fn kernel(client: &HostClient, calls: Arc<AtomicUsize>, fail: Arc<AtomicBool>) {
        let owner = Arc::downgrade(&client.0);
        *client.0.sdk.kernel.lock().unwrap() = Some(Arc::new(move |node| {
            let owner = owner.clone();
            let calls = calls.clone();
            let fail = fail.clone();
            Box::pin(async move {
                if calls.fetch_add(1, Ordering::AcqRel) == 0 {
                    return Ok(());
                }
                let client = HostClient(owner.upgrade().unwrap());
                let _callback = client
                    .callback_gate(crate::KernelOperation::Write, true)
                    .await?
                    .unwrap();
                let mut old = vec![b'A'; 4096];
                old[128] = b'u';
                assert_eq!(client.writeback_owned(node, 0, &old).await?, 4096);
                if fail.swap(false, Ordering::AcqRel) {
                    return Err(PortError::Io);
                }
                Ok(())
            })
        }));
    }

    #[test]
    fn sdk_error_retains_exact_protection_and_scope_until_retry() {
        let model = Arc::new(Model::default());
        let client = client(&model);
        let calls = Arc::new(AtomicUsize::new(0));
        kernel(&client, calls.clone(), Arc::new(AtomicBool::new(true)));
        LiveRuntime::shared().unwrap().block_on(async {
            let begin = wire::coherence_begin(1, NodeId(2)).unwrap();
            client.local_control(&begin).await.unwrap();
            client.local_control(&begin).await.unwrap();
            assert_eq!(calls.load(Ordering::Acquire), 1);
            let apply = wire::coherence_apply(1, 7, NodeId(2), 4096, &[16..17]).unwrap();
            assert_eq!(client.local_control(&apply).await, Err(PortError::Io));
            assert!(client.0.sdk.protection.lock().unwrap().is_some());
            assert_eq!(model.releases.load(Ordering::Acquire), 0);
            assert_eq!(
                client
                    .local_control(&wire::coherence_cancel(1).unwrap())
                    .await,
                Err(PortError::Invalid)
            );
            {
                let mut ordinary =
                    std::pin::pin!(client.callback_gate(crate::KernelOperation::Write, false));
                let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                assert!(ordinary.as_mut().poll(&mut context).is_pending());
            }
            let changed = wire::coherence_apply(1, 7, NodeId(2), 4096, &[16..18]).unwrap();
            assert_eq!(
                client.local_control(&changed).await,
                Err(PortError::Invalid)
            );
            client.local_control(&apply).await.unwrap();
            client.local_control(&apply).await.unwrap();
            assert_eq!(calls.load(Ordering::Acquire), 3);
            assert_eq!(model.releases.load(Ordering::Acquire), 1);
            assert!(client.0.sdk.protection.lock().unwrap().is_none());
            for bytes in model.writes.lock().unwrap().iter() {
                assert_eq!(bytes[16], 16);
                assert_eq!(bytes[128], b'u');
                assert_eq!(bytes[15], b'A');
            }
            assert_eq!(model.reads.load(Ordering::Acquire), 2);
            assert!(client
                .callback_gate(crate::KernelOperation::Write, false)
                .await
                .unwrap()
                .is_some());
            client
                .local_control(&wire::coherence_begin(2, NodeId(2)).unwrap())
                .await
                .unwrap();
            client
                .local_control(&wire::coherence_cancel(2).unwrap())
                .await
                .unwrap();
            client
                .local_control(&wire::coherence_cancel(2).unwrap())
                .await
                .unwrap();
            client.detach().await.unwrap();
        });
    }

    #[test]
    fn shutdown_releases_mount_descriptor_before_unmount_and_closes_local_control() {
        let model = Arc::new(Model::default());
        let client = client(&model);
        client
            .set_kernel_root(std::fs::File::open(std::env::temp_dir()).unwrap())
            .unwrap();
        let weak = Arc::downgrade(client.0.sdk.root.lock().unwrap().as_ref().unwrap());
        LiveRuntime::shared().unwrap().block_on(async {
            assert!(client
                .local_control(&[live_wire::SHUTDOWN])
                .await
                .unwrap()
                .is_empty());
            assert!(
                weak.upgrade().is_none(),
                "mount root FD retained after shutdown acknowledgment"
            );
            assert!(client.0.closing.load(Ordering::Acquire));
            assert!(client
                .set_kernel_root(std::fs::File::open(std::env::temp_dir()).unwrap())
                .is_err());
            assert_eq!(
                client
                    .local_control(&wire::coherence_begin(1, NodeId(2)).unwrap())
                    .await,
                Err(PortError::Io)
            );
            client.prepare_shutdown().unwrap();
            client.detach().await.unwrap();
        });
    }

    #[test]
    fn cancellation_settles_an_unknown_begin_without_accepting_delayed_identity() {
        let model = Arc::new(Model::default());
        let client = client(&model);
        let calls = Arc::new(AtomicUsize::new(0));
        kernel(&client, calls.clone(), Arc::new(AtomicBool::new(false)));
        LiveRuntime::shared().unwrap().block_on(async {
            client
                .local_control(&wire::coherence_cancel(5).unwrap())
                .await
                .unwrap();
            client
                .local_control(&wire::coherence_cancel(5).unwrap())
                .await
                .unwrap();
            assert_eq!(
                client
                    .local_control(&wire::coherence_begin(5, NodeId(2)).unwrap())
                    .await,
                Err(PortError::Invalid)
            );
            assert_eq!(calls.load(Ordering::Acquire), 0);
            client
                .local_control(&wire::coherence_begin(6, NodeId(2)).unwrap())
                .await
                .unwrap();
            assert_eq!(
                client
                    .local_control(&wire::coherence_cancel(7).unwrap())
                    .await,
                Err(PortError::Invalid)
            );
            assert!(client.0.sdk.book.lock().await.active.is_some());
            client
                .local_control(&wire::coherence_cancel(6).unwrap())
                .await
                .unwrap();
            client.detach().await.unwrap();
        });
    }

    #[test]
    fn shifted_terabyte_suffix_reads_only_queued_folio_and_release_retry_reuses_cut() {
        let model = Arc::new(Model::default());
        model.fail_release_once.store(true, Ordering::Release);
        let client = client(&model);
        let calls = Arc::new(AtomicUsize::new(0));
        kernel(&client, calls.clone(), Arc::new(AtomicBool::new(false)));
        LiveRuntime::shared().unwrap().block_on(async {
            client
                .local_control(&wire::coherence_begin(1, NodeId(2)).unwrap())
                .await
                .unwrap();
            let apply =
                wire::coherence_apply(1, 7, NodeId(2), 1024 * 1024 * 1024 * 1024, &[1..u64::MAX])
                    .unwrap();
            assert_eq!(client.local_control(&apply).await, Err(PortError::Io));
            assert_eq!(model.reads.load(Ordering::Acquire), 4095);
            assert!(client.0.sdk.protection.lock().unwrap().is_some());
            client.local_control(&apply).await.unwrap();
            assert_eq!(model.reads.load(Ordering::Acquire), 4095);
            assert_eq!(calls.load(Ordering::Acquire), 2);
            assert_eq!(model.releases.load(Ordering::Acquire), 2);
            assert_eq!(model.writes.lock().unwrap()[0][0], b'A');
            assert_eq!(model.writes.lock().unwrap()[0][1], 1);
            client.detach().await.unwrap();
        });
    }
}
