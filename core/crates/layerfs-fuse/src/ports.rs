//! External engine/Store boundaries; all waiting returns to the native pool.
use layerfs_overlay::{
    BaseSource, FileRead, LocalRead, NativeCookiePlan, NativeDirectory, NativeDirectoryPage,
    NativeDirectoryRead, NativeMount,
};
use layerfs_workspace::{NativeReadJob, NativeReadOutcome, SourceView};
use std::{error::Error, future::Future, pin::Pin, sync::Arc};

pub type ServiceError = Box<dyn Error + Send + Sync>;
pub type ServiceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ServiceError>> + Send + 'a>>;

/// A projection and its original service completion. Field order drops the
/// projection before its credit owner. Keep this receipt through every consumer
/// sharing its original payload. Consuming a bounded response into independent
/// request-owned storage ends those consumers; Arc::clone alone does not. Copying
/// an engine token acquires no new lease and does not release its backed owner.
pub struct ServiceReply<T> {
    value: ReplyValue<T>,
}
enum ReplyValue<T> {
    Owned {
        value: T,
        _completion: Box<dyn Send + Sync>,
    },
    Borrowed(Box<dyn AsRef<T> + Send + Sync>),
}
impl<T> ServiceReply<T> {
    pub fn new(value: T, completion: impl Send + Sync + 'static) -> Self {
        Self {
            value: ReplyValue::Owned {
                value,
                _completion: Box::new(completion),
            },
        }
    }
    /// A payload borrowed directly from its original completion avoids an
    /// otherwise redundant full read-window copy at the service boundary.
    pub fn borrowed(owner: impl AsRef<T> + Send + Sync + 'static) -> Self {
        Self {
            value: ReplyValue::Borrowed(Box::new(owner)),
        }
    }
    pub fn get(&self) -> &T {
        match &self.value {
            ReplyValue::Owned { value, .. } => value,
            ReplyValue::Borrowed(owner) => owner.as_ref().as_ref(),
        }
    }
}

/// One shared mounted Workspace; starting a request creates fresh failure
/// custody without provider I/O, a new cache or a whole-Workspace lock.
pub trait MountServices: Send + Sync {
    fn request(&self) -> Result<Arc<dyn RequestServices>, ServiceError>;
}
/// One kernel request's original operations. No concrete daemon command,
/// completion, registry or application configuration crosses this boundary.
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
    fn local_read(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
    ) -> ServiceFuture<'_, ServiceReply<Option<LocalRead>>>;
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
    fn immutable<'a>(&'a self, view: &'a SourceView) -> ServiceFuture<'a, SourceView>;
    fn observe(
        &self,
        job: NativeReadJob,
    ) -> ServiceFuture<'_, ServiceReply<Arc<NativeReadOutcome>>>;
    fn release_read(&self, read: FileRead) -> ServiceFuture<'_, ServiceReply<()>>;
    fn release_source(&self, source: BaseSource) -> ServiceFuture<'_, ServiceReply<()>>;
    fn forget(
        &self,
        mount: NativeMount,
        serial: u64,
        count: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>>;
}
