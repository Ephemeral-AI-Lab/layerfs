//! LOOKUP/GETATTR/OPEN/OPENDIR share the existing atomic semantic plan.
use crate::{
    ports::{BaseDemandFailed, Fenced, RequestServices, ServiceError, ServiceReply},
    NextTurn,
};
use layerfs_overlay::{BaseSource, FileRead, NativeMount};
use layerfs_workspace::{
    NativeReadDecision, NativeReadOperation, NativeReadOutcome, NativeReadPlan, NativeReadValue,
    Refusal, SourceView,
};
use std::{fmt, sync::Arc};

pub(super) struct Custody {
    pub services: Arc<dyn RequestServices>,
    mount: NativeMount,
    request: u64,
    protected: u64,
    handle: Option<u64>,
    operation: NativeReadOperation,
    pub data_input: Option<super::ReadDataInput>,
    source: Option<BaseSource>,
    read: Option<FileRead>,
    pub view: Option<SourceView>,
    pub plan: Option<NativeReadPlan>,
    // Retain engine completion credit through every cloned outcome consumer.
    pub receipt: Option<ServiceReply<Arc<NativeReadOutcome>>>,
}
/// A deciding answer still owns processing references until reply consumers end.
/// The kernel's lookup/open/directory owners persist independently afterwards.
pub struct NativeRead {
    pub(super) value: Result<NativeReadValue, Refusal>,
    pub(super) custody: Custody,
}
/// Exact failed step, original request identity and still-owned engine references.
/// No destructor issues SQL, guesses whether a job ran, or releases these owners.
pub struct ReadFailure {
    pub reason: ServiceError,
    pub provider: Option<ServiceError>,
    custody: Custody,
}
impl fmt::Debug for ReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadFailure")
            .field("reason", &self.reason)
            .field("provider", &self.provider)
            .field("mount", &self.custody.mount)
            .field("request", &self.custody.request)
            .field("operation", &self.custody.operation)
            .field("data_input", &self.custody.data_input)
            .field("source", &self.custody.source)
            .field("read", &self.custody.read)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.reason.fmt(f)
    }
}
impl std::error::Error for ReadFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.reason.as_ref())
    }
}
impl ReadFailure {
    pub(super) fn new(reason: ServiceError, custody: Custody) -> Self {
        let provider = match custody.services.provider_failure() {
            Ok(original) => original,
            Err(error) => Some(error),
        };
        Self {
            reason,
            provider,
            custody,
        }
    }
    pub const fn mount(&self) -> NativeMount {
        self.custody.mount
    }
    pub const fn request(&self) -> u64 {
        self.custody.request
    }
    pub const fn protected(&self) -> u64 {
        self.custody.protected
    }
    pub const fn handle(&self) -> Option<u64> {
        self.custody.handle
    }
    pub fn operation(&self) -> &NativeReadOperation {
        &self.custody.operation
    }
    pub const fn data_input(&self) -> Option<super::ReadDataInput> {
        self.custody.data_input
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn with_data_input(mut self, input: Option<super::ReadDataInput>) -> Self {
        self.custody.data_input = input;
        self
    }
    pub const fn retained_source(&self) -> Option<BaseSource> {
        self.custody.source
    }
    pub const fn retained_read(&self) -> Option<FileRead> {
        self.custody.read
    }
    pub fn observation(&self) -> Option<&NativeReadOutcome> {
        self.custody
            .receipt
            .as_ref()
            .map(|receipt| receipt.get().as_ref())
    }
    /// The mount's stopped fence refused the failed step before its attempt.
    pub fn fenced(&self) -> bool {
        self.reason.is::<Fenced>()
    }
    /// The failed step was a base demand of this request alone.
    pub fn base_demand(&self) -> Option<&BaseDemandFailed> {
        self.reason.downcast_ref()
    }
    /// Ends a request that was fenced or whose base demand failed: its read
    /// and source are released once through the disposal calls, which the
    /// fence never refuses. Nothing is demanded again. Any other failure is
    /// returned unchanged, and a failed release keeps what remains.
    pub async fn relinquish(self) -> Result<(), ReadFailure> {
        if !self.fenced() && self.base_demand().is_none() {
            return Err(self);
        }
        self.custody.dispose().await
    }
}
impl NativeRead {
    pub async fn prepare(
        services: Arc<dyn RequestServices>,
        mount: NativeMount,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
    ) -> Result<Self, ReadFailure> {
        let mut custody = Custody {
            services,
            mount,
            request,
            protected,
            handle,
            operation,
            data_input: None,
            source: None,
            read: None,
            view: None,
            plan: None,
            receipt: None,
        };
        match custody.prepare().await {
            Ok(value) => Ok(Self { value, custody }),
            Err(reason) => Err(ReadFailure::new(reason, custody)),
        }
    }
    pub fn value(&self) -> Result<&NativeReadValue, Refusal> {
        self.value.as_ref().map_err(|refusal| *refusal)
    }
    /// Preserve a successfully decided request when native reply conversion or
    /// another downstream invariant fails before consumer disposal.
    pub fn retain(self, reason: ServiceError) -> ReadFailure {
        let Self { value, custody } = self;
        drop(value);
        ReadFailure::new(reason, custody)
    }
    /// Called only after the single reply attempt and disposal of its payload.
    /// Processing releases use original tokens once; kernel owners stay live.
    pub async fn dispose(self) -> Result<(), ReadFailure> {
        let Self { value, custody } = self;
        drop(value);
        custody.dispose().await
    }
}
impl Custody {
    pub async fn dispose(mut self) -> Result<(), ReadFailure> {
        self.plan = None;
        self.view = None;
        self.receipt = None;
        match self.release().await {
            Ok(()) => Ok(()),
            Err(reason) => Err(ReadFailure::new(reason, self)),
        }
    }
    async fn prepare(&mut self) -> Result<Result<NativeReadValue, Refusal>, ServiceError> {
        let granted = self
            .services
            .source(self.mount, self.request, self.protected, self.handle)
            .await?;
        let source = *granted.get();
        self.source = Some(source);
        drop(granted);
        let view = self.services.view(source)?;
        let plan = view.native_read_plan(self.mount, self.operation.clone())?;
        self.view = Some(view);
        self.plan = Some(plan);
        loop {
            let job = self
                .plan
                .as_ref()
                .unwrap()
                .job()
                .expect("native owner stage")
                .clone();
            self.receipt = Some(self.services.observe(job).await?);
            let original = self.receipt.as_ref().unwrap().get().clone();
            // Even a failed transaction may have produced a candidate. Preserve
            // the entire observation; only a successful result is usable.
            self.read = original.candidate;
            match self.plan.as_mut().unwrap().accept(original) {
                Ok(Some(value)) => return Ok(Ok(value)),
                Err(failure) => {
                    let outcome = &failure.original;
                    if matches!(outcome.result, Ok(None))
                        && outcome.candidate.is_none()
                        && outcome.open_candidate.is_none()
                        && outcome.directory_candidate.is_none()
                    {
                        if let Some(NativeReadDecision::Refused(refusal)) = outcome.decision {
                            return Ok(Err(refusal));
                        }
                    }
                    return Err(Box::new(failure));
                }
                Ok(None) => {}
            }
            self.receipt = None;
            let immutable = self.services.immutable(self.view.as_ref().unwrap()).await?;
            let result = self.plan.as_mut().unwrap().supply(&immutable);
            drop(immutable); // Return the actual reader before another SQL wait.
            if let Err(error) = result {
                return Err(self.services.failed_base_read(error.into()));
            }
            NextTurn::default().await;
        }
    }
    async fn release(&mut self) -> Result<(), ServiceError> {
        if let Some(read) = self.read {
            drop(self.services.release_read(read).await?);
            self.read = None;
        }
        if let Some(source) = self.source {
            drop(self.services.release_source(source).await?);
            self.source = None;
        }
        Ok(())
    }
}
