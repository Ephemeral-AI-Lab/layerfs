//! External engine/Store boundaries; all waiting returns to the native pool.
use layerfs_overlay::{
    BaseSource, FileRead, NativeCookiePlan, NativeDirectory, NativeDirectoryPage,
    NativeDirectoryRead, NativeMount, OpenFile, Publication,
};
use layerfs_workspace::{
    BaseView, MutationInputFailure, MutationPlan, NativeMutationJob, NativeMutationOutcome,
    NativeReadJob, NativeReadOperation, NativeReadOutcome, NativeVisitRequest, NativeWindow,
    Operation, SourceView, Time, VisitFacts,
};
use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

pub type ServiceError = Box<dyn Error + Send + Sync>;
pub type ServiceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ServiceError>> + Send + 'a>>;

/// One mount's terminal stop, shared by its lane, its requests and their
/// service ports. Only forced teardown sets it, once; nothing clears it. A
/// default fence is never stopped. It carries no request, job or lease.
///
/// The same shared record keeps the mount's bounded diagnostic of requests
/// whose base demand failed: a count, the first original cause and the most
/// recent one, in fixed slots. Recording stops nothing.
#[derive(Clone, Debug, Default)]
pub struct Fence(Arc<FenceState>);
#[derive(Debug, Default)]
struct FenceState {
    stopped: AtomicBool,
    terminal_replies: AtomicU64,
    failed_demands: Mutex<FailedDemands>,
}
/// A mount's failed base demands: how many were recorded, the original cause
/// of the first and of the most recent. Causes between the two are not kept
/// here. The drain receipt of the mount's connection carries this record.
#[derive(Clone, Debug, Default)]
pub struct FailedDemands {
    pub count: u64,
    pub first: Option<Arc<dyn Error + Send + Sync>>,
    pub latest: Option<Arc<dyn Error + Send + Sync>>,
}
/// A canonical base demand made for one request failed in the Store read
/// path: admission to a reader, or a read through the admitted view. It
/// carries the original cause from that request's demand scope. Only the
/// service adapter produces it, through [`Fence::failed_demand`], and never
/// for an owner job's failure, an uncertain outcome or a failed release. The
/// failure belongs to the request alone; it says nothing about the mount.
#[derive(Clone, Debug)]
pub struct BaseDemandFailed {
    cause: Arc<dyn Error + Send + Sync>,
}
impl BaseDemandFailed {
    pub fn cause(&self) -> &Arc<dyn Error + Send + Sync> {
        &self.cause
    }
}
impl fmt::Display for BaseDemandFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "canonical base demand failed: {}", self.cause)
    }
}
impl Error for BaseDemandFailed {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
impl Fence {
    /// For the service adapter: records one failed base demand in this
    /// mount's slot and returns the marker its request fails with. The marker
    /// has no other constructor, so none exists without its record.
    pub fn failed_demand(&self, cause: Arc<dyn Error + Send + Sync>) -> BaseDemandFailed {
        let mut record = self
            .0
            .failed_demands
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        record.count = record.count.saturating_add(1);
        // The first original cause is never replaced.
        record.first.get_or_insert_with(|| cause.clone());
        let earlier = record.latest.replace(cause.clone());
        drop(record);
        drop(earlier);
        BaseDemandFailed { cause }
    }
    /// Observation only; the slots are fixed and are never cleared.
    pub fn failed_demands(&self) -> FailedDemands {
        self.0
            .failed_demands
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
    pub fn stopped(&self) -> bool {
        self.0.stopped.load(Ordering::Acquire)
    }
    /// Admitted requests that made their one terminal reply attempt because
    /// this fence refused them before an attempt.
    pub fn terminal_replies(&self) -> u64 {
        self.0.terminal_replies.load(Ordering::Acquire)
    }
    pub(crate) fn stop(&self) {
        self.0.stopped.store(true, Ordering::Release);
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn replied(&self) {
        self.0.terminal_replies.fetch_add(1, Ordering::AcqRel);
    }
}
/// A stopped fence refused this acquisition before any attempt: no job was
/// submitted and no provider demand was made. It is never the result of an
/// attempted operation.
#[derive(Clone, Copy, Debug)]
pub struct Fenced;
impl fmt::Display for Fenced {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native mount stopped before the attempt")
    }
}
impl Error for Fenced {}

/// A projection and its original service completion. Field order drops the
/// projection before its credit owner. Keep this receipt through every consumer
/// sharing its original payload. Consuming a bounded response into independent
/// request-owned storage ends those consumers; Arc::clone alone does not. Copying
/// an engine token acquires no new lease and does not release its backed owner.
pub struct ServiceReply<T> {
    value: T,
    _completion: Box<dyn Send + Sync>,
}
impl<T> ServiceReply<T> {
    pub fn new(value: T, completion: impl Send + Sync + 'static) -> Self {
        Self {
            value,
            _completion: Box::new(completion),
        }
    }
    pub fn get(&self) -> &T {
        &self.value
    }
}

/// One shared mounted Workspace; starting a request creates fresh failure
/// custody without provider I/O, a new cache or a whole-Workspace lock. The
/// request's services observe its mount's fence.
pub trait MountServices: Send + Sync {
    fn request(&self, fence: &Fence) -> Result<Arc<dyn RequestServices>, ServiceError>;
}
/// One kernel request's original operations. No concrete daemon command,
/// completion, registry or application configuration crosses this boundary.
///
/// Once the fence is stopped, every acquiring call returns [`Fenced`] before
/// its attempt, also from a wait it was already in: `source`, `open_source`,
/// `observe`, `observe_visit`, `read_visit`, `mutate_visit`, `mutate`, `base`,
/// `immutable`, `reserve_serial`, `directory`, `directory_read`,
/// `directory_page`, `directory_cookies` and `publish_cookies`. A job already submitted is awaited to its original
/// result. The disposal calls are never refused by the fence:
/// `release_read`, `release_source`, `reply_attempted`, `replied`,
/// `close_file`, `close_directory` and `forget`.
pub trait RequestServices: Send + Sync {
    fn directory(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<NativeDirectory>>;
    fn directory_read(
        &self,
        directory: NativeDirectory,
        request: u64,
        offset: u64,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeDirectoryRead>>>;
    fn directory_page(
        &self,
        read: Arc<NativeDirectoryRead>,
        after: Option<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeDirectoryPage>>>;
    fn directory_cookies(
        &self,
        read: Arc<NativeDirectoryRead>,
        names: Vec<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeCookiePlan>>>;
    fn publish_cookies(
        &self,
        plan: Arc<NativeCookiePlan>,
        accepted: usize,
    ) -> ServiceFuture<'_, ServiceReply<()>>;
    fn close_directory(&self, directory: NativeDirectory) -> ServiceFuture<'_, ServiceReply<()>>;
    /// READ (through `handle`) or READLINK (no handle) as one read-only owner
    /// job: the window's local part and the base root the rest belongs to.
    /// It records nothing, so its request holds nothing afterwards.
    fn read_visit(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeWindow>>>;
    fn close_file(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>>;
    /// Original immutable-provider failure from this request's demand scope.
    fn provider_failure(&self) -> Result<Option<ServiceError>, ServiceError>;
    fn source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
    ) -> ServiceFuture<'_, ServiceReply<BaseSource>>;
    fn view(&self, source: BaseSource) -> Result<SourceView, ServiceError>;
    /// Await actual reader admission before obtaining a provider-capable view.
    /// Drop all resulting views/plans before parking for another engine job.
    /// A refused or failed admission is a [`BaseDemandFailed`].
    fn immutable<'a>(&'a self, view: &'a SourceView) -> ServiceFuture<'a, SourceView>;
    /// Classifies the failure of one canonical read made on a view from
    /// `immutable`. When this request's demand scope recorded the original
    /// provider failure the answer is a [`BaseDemandFailed`] carrying it;
    /// any other failure of that step is returned unchanged. No demand is
    /// made here and none is repeated.
    fn failed_base_read(&self, step: ServiceError) -> ServiceError;
    fn observe(
        &self,
        job: NativeReadJob,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeReadOutcome>>>;
    /// LOOKUP or GETATTR as one owner job with no request source. An
    /// undecided outcome has changed nothing and holds nothing.
    fn observe_visit(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
        facts: Arc<VisitFacts>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeReadOutcome>>>;
    /// The Workspace's current base over an admitted Store reader, for the
    /// facts an undecided visit asked for and the inherited bytes of a read
    /// window. It acquires nothing in the owner.
    fn base(&self) -> ServiceFuture<'_, BaseView>;
    /// A native mutation as one owner job with no request source. An
    /// undecided outcome has changed nothing and holds nothing.
    fn mutate_visit(
        &self,
        request: NativeVisitRequest,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeMutationOutcome>>>;
    fn release_read(&self, read: FileRead) -> ServiceFuture<'_, ServiceReply<()>>;
    fn release_source(&self, source: BaseSource) -> ServiceFuture<'_, ServiceReply<()>>;
    fn forget(
        &self,
        mount: NativeMount,
        serial: u64,
        count: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>>;
    /// The exact descriptor and an independent request source in one job,
    /// for a mutation the kernel addressed through a handle.
    fn open_source(
        &self,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<(BaseSource, OpenFile)>>;
    /// One unused inode serial for a creating mutation. Local while the bound
    /// Workspace's reserved range lasts; a refill is one bounded allocator
    /// write that never waits. `None` is that allocator's exact contended
    /// refusal: nothing was reserved and no mutation was attempted.
    fn reserve_serial(&self) -> Result<Option<u64>, ServiceError>;
    /// Workspace preparation of one mutation: no SQL, provider I/O or effect.
    fn prepare(
        &self,
        view: &SourceView,
        operation: Operation,
        now: Time,
        serial: Option<u64>,
    ) -> Result<MutationPlan, Box<MutationInputFailure>>;
    /// One owner round of a prepared mutation. The original outcome carries
    /// the exact failure of an attempted job; it is never resubmitted.
    fn mutate(
        &self,
        job: NativeMutationJob,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeMutationOutcome>>>;
    /// Releases one publication ticket after its single reply attempt.
    fn reply_attempted(&self, publication: Publication) -> ServiceFuture<'_, ServiceReply<()>>;
    /// The same release together with the request's processing source, as
    /// one owner job. A failure leaves both in the caller's custody.
    fn replied(
        &self,
        publication: Publication,
        source: BaseSource,
    ) -> ServiceFuture<'_, ServiceReply<()>>;
}
