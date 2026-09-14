use std::future::Future;
use std::io;
use std::sync::Arc;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::sync::{
    OwnedRwLockReadGuard, OwnedRwLockWriteGuard, OwnedSemaphorePermit, RwLock, Semaphore,
};

pub type LiveReservation = OwnedSemaphorePermit;

const REQUESTS: usize = 256;
const TRANSFER_BYTES: usize = 32 * 1024 * 1024;
const LIFECYCLE_BYTES: usize = 8 * 1024 * 1024;
const SNAPSHOT_REQUESTS: usize = 4;
const SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

/// Process-owned runtime; destroy it only after mounts/services drain, outside
/// their state locks. Mounted owners clone Scheduler, never the Runtime itself.
pub struct LiveRuntime {
    runtime: Runtime,
    scheduler: Scheduler,
}

#[derive(Clone)]
pub struct Scheduler {
    pub(crate) handle: Handle,
    immutable_reads: Arc<crate::immutable_read_cache::ImmutableReadCache>,
    requests: Arc<Semaphore>,
    transfer: Arc<Semaphore>,
    backing: Arc<Semaphore>,
    live: Arc<Semaphore>,
    kernel: Arc<Semaphore>,
    #[cfg(any(
        test,
        all(target_os = "linux", any(feature = "host", feature = "proxy"))
    ))]
    prefill: std::sync::mpsc::SyncSender<Box<dyn FnOnce() + Send>>,
    control_requests: Arc<Semaphore>,
    control_transfer: Arc<Semaphore>,
    lifecycle_requests: Arc<Semaphore>,
    lifecycle_transfer: Arc<Semaphore>,
    snapshot_requests: Arc<Semaphore>,
    snapshot_transfer: Arc<Semaphore>,
}

pub struct RequestAdmission {
    pub slot: OwnedSemaphorePermit,
    pub bytes: OwnedSemaphorePermit,
}

impl LiveRuntime {
    pub fn shared() -> io::Result<&'static Self> {
        static RUNTIME: std::sync::OnceLock<Result<LiveRuntime, String>> =
            std::sync::OnceLock::new();
        RUNTIME
            .get_or_init(|| Self::new().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(|error| io::Error::other(error.clone()))
    }

    pub fn new() -> io::Result<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(2)
            .thread_name("layerfs-live")
            .enable_io()
            .enable_time()
            .build()?;
        #[cfg(any(
            test,
            all(target_os = "linux", any(feature = "host", feature = "proxy"))
        ))]
        let prefill = {
            let (sender, receiver) = std::sync::mpsc::sync_channel::<Box<dyn FnOnce() + Send>>(0);
            // Optional work has its own bounded worker: STORE may wait for a
            // folio READ that needs the ordinary backing/kernel workers.
            let _ = std::thread::Builder::new()
                .name("layerfs-prefill".into())
                .stack_size(256 * 1024)
                .spawn(move || {
                    for work in receiver {
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
                    }
                });
            sender
        };
        let scheduler = Scheduler {
            handle: runtime.handle().clone(),
            immutable_reads: Default::default(),
            requests: Arc::new(Semaphore::new(REQUESTS)),
            transfer: Arc::new(Semaphore::new(TRANSFER_BYTES)),
            backing: Arc::new(Semaphore::new(2)),
            live: Arc::new(Semaphore::new(128 * 1024 * 1024)),
            kernel: Arc::new(Semaphore::new(1)),
            #[cfg(any(
                test,
                all(target_os = "linux", any(feature = "host", feature = "proxy"))
            ))]
            prefill,
            control_requests: Arc::new(Semaphore::new(32)),
            control_transfer: Arc::new(Semaphore::new(4 * 1024 * 1024)),
            lifecycle_requests: Arc::new(Semaphore::new(2)),
            lifecycle_transfer: Arc::new(Semaphore::new(LIFECYCLE_BYTES)),
            snapshot_requests: Arc::new(Semaphore::new(SNAPSHOT_REQUESTS)),
            snapshot_transfer: Arc::new(Semaphore::new(SNAPSHOT_BYTES)),
        };
        Ok(Self { runtime, scheduler })
    }

    pub fn scheduler(&self) -> Scheduler {
        self.scheduler.clone()
    }

    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.runtime.block_on(future)
    }
}

impl Scheduler {
    pub(crate) fn immutable_reads(&self) -> &crate::immutable_read_cache::ImmutableReadCache {
        &self.immutable_reads
    }

    /// Poll ready local work on ingress; only pending work enters the existing
    /// runtime. The owned future retains its admission and reply until completion.
    pub fn submit(&self, future: impl Future<Output = ()> + Send + 'static) {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let mut future = Box::pin(future);
        let entered = self.handle.enter();
        let pending = future.as_mut().poll(&mut context).is_pending();
        drop(entered);
        if pending {
            self.handle.spawn(future);
        }
    }

    /// Ingress cannot wait behind a full ordinary queue: that would hide kernel
    /// writeback/release requests from the one receiver. Capacity failure is explicit.
    pub fn try_admit_from_receiver(
        &self,
        bytes: usize,
        control: bool,
    ) -> io::Result<RequestAdmission> {
        let (requests, transfer) = if control {
            (&self.control_requests, &self.control_transfer)
        } else {
            (&self.requests, &self.transfer)
        };
        let slot = requests
            .clone()
            .try_acquire_owned()
            .map_err(io::Error::other)?;
        let bytes = transfer
            .clone()
            .try_acquire_many_owned(u32::try_from(bytes).map_err(io::Error::other)?)
            .map_err(io::Error::other)?;
        Ok(RequestAdmission { slot, bytes })
    }

    pub fn reserve_transfer(&self, bytes: usize) -> io::Result<LiveReservation> {
        self.transfer
            .clone()
            .try_acquire_many_owned(u32::try_from(bytes).map_err(io::Error::other)?)
            .map_err(io::Error::other)
    }

    pub fn reserve_live(&self, bytes: usize) -> io::Result<OwnedSemaphorePermit> {
        self.live
            .clone()
            .try_acquire_many_owned(u32::try_from(bytes).map_err(io::Error::other)?)
            .map_err(io::Error::other)
    }

    pub async fn admit(&self, bytes: usize) -> io::Result<RequestAdmission> {
        self.admit_inner(bytes, false).await
    }

    /// Lifecycle frames must progress while filesystem callbacks are parked at a cut.
    pub async fn admit_lifecycle(&self, bytes: usize) -> io::Result<RequestAdmission> {
        self.admit_inner(bytes, true).await
    }

    /// Dedicated snapshot-lane admission: a stalled bulk transfer can neither
    /// consume ordinary request capacity nor block control/lifecycle service.
    pub async fn admit_snapshot(&self, bytes: usize) -> io::Result<RequestAdmission> {
        if bytes > SNAPSHOT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "snapshot transfer size",
            ));
        }
        let slot = self
            .snapshot_requests
            .clone()
            .acquire_owned()
            .await
            .map_err(io::Error::other)?;
        let bytes = self
            .snapshot_transfer
            .clone()
            .acquire_many_owned(bytes as u32)
            .await
            .map_err(io::Error::other)?;
        Ok(RequestAdmission { slot, bytes })
    }

    async fn admit_inner(&self, bytes: usize, lifecycle: bool) -> io::Result<RequestAdmission> {
        let (requests, transfer, limit) = if lifecycle {
            (
                &self.lifecycle_requests,
                &self.lifecycle_transfer,
                LIFECYCLE_BYTES,
            )
        } else {
            (&self.requests, &self.transfer, TRANSFER_BYTES)
        };
        if bytes > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "live transfer size",
            ));
        }
        let slot = requests
            .clone()
            .acquire_owned()
            .await
            .map_err(io::Error::other)?;
        let bytes = transfer
            .clone()
            .acquire_many_owned(bytes as u32)
            .await
            .map_err(io::Error::other)?;
        Ok(RequestAdmission { slot, bytes })
    }

    /// Called by a mount receive loop before copying borrowed kernel arguments.
    /// Waiting holds its already-budgeted receive buffer, no filesystem worker.
    pub fn admit_from_receiver(&self, bytes: usize) -> io::Result<RequestAdmission> {
        self.handle.block_on(self.admit(bytes))
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        slot: OwnedSemaphorePermit,
        future: impl Future<Output = T> + Send + 'static,
    ) -> tokio::task::JoinHandle<T> {
        self.handle.spawn(async move {
            let _slot = slot;
            future.await
        })
    }

    #[cfg(any(
        test,
        all(target_os = "linux", any(feature = "host", feature = "proxy"))
    ))]
    pub(crate) fn try_prefill(&self, work: Box<dyn FnOnce() + Send>) -> bool {
        // A rendezvous channel has no queue; busy/unavailable workers simply
        // drop the optional job and its gates/completion on this caller.
        self.prefill.try_send(work).is_ok()
    }

    /// A kernel invalidation can wait for FUSE writeback. Keep only one such
    /// job active, leaving the second blocking worker for backing/other progress.
    pub async fn kernel<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        let permit = self
            .kernel
            .clone()
            .acquire_owned()
            .await
            .map_err(io::Error::other)?;
        self.handle
            .spawn_blocking(move || {
                let _permit = permit;
                work()
            })
            .await
            .map_err(io::Error::other)?
    }

    /// Admission precedes Tokio's blocking queue and any physical job creation.
    pub async fn physical<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        let permit = self
            .backing
            .clone()
            .acquire_owned()
            .await
            .map_err(io::Error::other)?;
        self.handle
            .spawn_blocking(move || {
                let _permit = permit;
                work()
            })
            .await
            .map_err(io::Error::other)?
    }
}

/// Filesystem-operation admission only. Commands and open-handle lifetimes do
/// not acquire these guards. Observation of owner metadata also bypasses them.
#[derive(Default)]
pub struct OperationGate {
    ordinary: Arc<RwLock<()>>,
    writeback: Arc<RwLock<()>>,
}

pub struct CacheFlush {
    ordinary: OwnedRwLockWriteGuard<()>,
    writeback: Arc<RwLock<()>>,
}

pub struct OperationCut {
    _ordinary: OwnedRwLockWriteGuard<()>,
    _writeback: OwnedRwLockWriteGuard<()>,
}

impl OperationCut {
    /// Admit folio reads/writeback during cache reconciliation while keeping
    /// namespace/size mutations excluded until the caller drops the guard.
    pub(crate) fn reopen_writeback(self) -> CacheFlush {
        CacheFlush {
            ordinary: self._ordinary,
            writeback: OwnedRwLockWriteGuard::rwlock(&self._writeback).clone(),
        }
    }
}

impl OperationGate {
    #[cfg(any(
        test,
        all(target_os = "linux", any(feature = "host", feature = "proxy"))
    ))]
    pub(crate) fn try_ordinary(&self) -> Option<OwnedRwLockReadGuard<()>> {
        self.ordinary.clone().try_read_owned().ok()
    }

    /// Retain through the kernel reply, including any immutable/backing wait.
    pub async fn enter(&self, writeback: bool) -> OwnedRwLockReadGuard<()> {
        if writeback {
            &self.writeback
        } else {
            &self.ordinary
        }
        .clone()
        .read_owned()
        .await
    }

    /// Drain ordinary callbacks before invalidating cached pages. Writeback is
    /// still admitted so the kernel can launder dirty pages during invalidation.
    pub async fn cache_flush(&self) -> CacheFlush {
        CacheFlush {
            ordinary: self.ordinary.clone().write_owned().await,
            writeback: self.writeback.clone(),
        }
    }
}

impl CacheFlush {
    /// Call only after the actual kernel flush/invalidation has completed.
    pub async fn finish(self) -> OperationCut {
        OperationCut {
            _ordinary: self.ordinary,
            _writeback: self.writeback.write_owned().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn operation_cut_drains_callbacks_and_allows_laundering_before_freeze() {
        let runtime = LiveRuntime::new().unwrap();
        runtime.block_on(async {
            let gate = OperationGate::default();
            let admitted = gate.enter(false).await;
            assert!(
                tokio::time::timeout(Duration::from_millis(10), gate.cache_flush())
                    .await
                    .is_err()
            );
            drop(admitted);
            let flush = gate.cache_flush().await;
            assert!(
                tokio::time::timeout(Duration::from_millis(10), gate.enter(false))
                    .await
                    .is_err()
            );
            let writeback = tokio::time::timeout(Duration::from_secs(2), gate.enter(true))
                .await
                .unwrap();
            let mut pending_cut = Box::pin(flush.finish());
            assert!(
                tokio::time::timeout(Duration::from_millis(10), &mut pending_cut)
                    .await
                    .is_err()
            );
            drop(writeback);
            let cut = pending_cut.await;
            assert!(
                tokio::time::timeout(Duration::from_millis(10), gate.enter(true))
                    .await
                    .is_err()
            );
            assert!(
                tokio::time::timeout(Duration::from_millis(10), gate.enter(false))
                    .await
                    .is_err()
            );
            let flush = cut.reopen_writeback();
            let laundering = tokio::time::timeout(Duration::from_secs(2), gate.enter(true))
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(10), gate.enter(false))
                    .await
                    .is_err()
            );
            let mut finished = Box::pin(flush.finish());
            std::future::poll_fn(|context| {
                assert!(std::future::Future::poll(finished.as_mut(), context).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            assert!(gate.try_ordinary().is_none());
            drop(laundering);
            let cut = finished.await;
            assert!(gate.try_ordinary().is_none());
            drop(cut);
            let _continued = tokio::time::timeout(Duration::from_secs(2), gate.enter(false))
                .await
                .unwrap();
        });
    }

    #[test]
    fn parked_socket_operations_release_workers_and_reservations() {
        let runtime = LiveRuntime::new().unwrap();
        let scheduler = runtime.scheduler();
        runtime.block_on(async {
            let ordinary = scheduler
                .requests
                .clone()
                .acquire_many_owned(REQUESTS as u32)
                .await
                .unwrap();
            let kernel = scheduler
                .control_requests
                .clone()
                .acquire_many_owned(32)
                .await
                .unwrap();
            let lifecycle =
                tokio::time::timeout(Duration::from_secs(1), scheduler.admit_lifecycle(1024))
                    .await
                    .unwrap()
                    .unwrap();
            drop((ordinary, kernel, lifecycle));
            assert_eq!(scheduler.lifecycle_requests.available_permits(), 2);
            assert_eq!(
                scheduler.lifecycle_transfer.available_permits(),
                LIFECYCLE_BYTES
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut peers = Vec::new();
            let mut tasks = Vec::new();
            for _ in 0..2 {
                let mut socket = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
                    .await
                    .unwrap();
                let (peer, _) = listener.accept().await.unwrap();
                peers.push(peer);
                let admitted = scheduler.admit(1).await.unwrap();
                tasks.push(scheduler.spawn(admitted.slot, async move {
                    let _bytes = admitted.bytes;
                    socket.read_u8().await.unwrap()
                }));
            }
            let admitted = scheduler.admit(0).await.unwrap();
            let ready = scheduler.spawn(admitted.slot, async move {
                drop(admitted.bytes);
                42
            });
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), ready)
                    .await
                    .unwrap()
                    .unwrap(),
                42
            );
            assert_eq!(scheduler.transfer.available_permits(), TRANSFER_BYTES - 2);
            for peer in &mut peers {
                peer.write_u8(7).await.unwrap();
            }
            for task in tasks {
                assert_eq!(
                    tokio::time::timeout(Duration::from_secs(2), task)
                        .await
                        .unwrap()
                        .unwrap(),
                    7
                );
            }
            assert_eq!(scheduler.requests.available_permits(), REQUESTS);
            assert_eq!(scheduler.transfer.available_permits(), TRANSFER_BYTES);
            assert!(scheduler.admit(TRANSFER_BYTES + 1).await.is_err());
            let held = scheduler.admit(TRANSFER_BYTES).await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(10), scheduler.admit(1))
                    .await
                    .is_err()
            );
            assert_eq!(scheduler.requests.available_permits(), REQUESTS - 1);
            drop(held);
            assert_eq!(scheduler.requests.available_permits(), REQUESTS);
            assert_eq!(scheduler.transfer.available_permits(), TRANSFER_BYTES);
        });
    }
}
