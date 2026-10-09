//! LOOKUP/GETATTR/OPEN/OPENDIR share the existing atomic semantic plan.
use crate::{
    ports::{BaseDemandFailed, Fenced, RequestServices, ServiceError, ServiceReply},
    NextTurn,
};
use layerfs_overlay::NativeMount;
use layerfs_workspace::{
    NativeReadDecision, NativeReadFailure, NativeReadOperation, NativeReadOutcome, NativeReadValue,
    Refusal, VisitFacts,
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
    // Retain engine completion credit through every cloned outcome consumer.
    pub receipt: Option<ServiceReply<Arc<NativeReadOutcome>>>,
}
/// A deciding answer and the completion credit of the visit that decided it.
/// The request holds nothing else in the engine; the kernel's lookup, open
/// and directory owners persist independently.
pub struct NativeRead {
    pub(super) value: Result<NativeReadValue, Refusal>,
    pub(super) custody: Custody,
}
/// Exact failed step and original request identity. A visit records no
/// request source, so a failure owns nothing in the engine.
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
    /// Ends a request that was fenced or whose base demand failed. It holds
    /// nothing in the engine, so nothing is released and no job is
    /// submitted. Any other failure is returned unchanged.
    pub async fn relinquish(self) -> Result<(), ReadFailure> {
        if !self.fenced() && self.base_demand().is_none() {
            return Err(self);
        }
        Ok(())
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
        let mut custody = Custody::unowned(services, mount, request, protected, handle, operation);
        match custody.visit().await {
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
    /// It returns the visit's completion credit; nothing is released in the
    /// engine and the kernel's owners stay live.
    pub async fn dispose(self) -> Result<(), ReadFailure> {
        drop(self);
        Ok(())
    }
}
impl Custody {
    /// The request as it arrived: nothing is owned in the engine yet.
    pub(super) fn unowned(
        services: Arc<dyn RequestServices>,
        mount: NativeMount,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
    ) -> Self {
        Self {
            services,
            mount,
            request,
            protected,
            handle,
            operation,
            data_input: None,
            receipt: None,
        }
    }
    pub(super) const fn target(&self) -> (NativeMount, u64, Option<u64>) {
        (self.mount, self.protected, self.handle)
    }
    /// LOOKUP, GETATTR, OPEN and OPENDIR are served by owner visits that
    /// record no request source: nothing is acquired, so nothing is released
    /// afterwards. An undecided visit changed nothing; the base facts it
    /// asked for are read here, outside the owner, before the next visit.
    /// The visit that decides an OPEN or OPENDIR wrote its descriptor.
    async fn visit(&mut self) -> Result<Result<NativeReadValue, Refusal>, ServiceError> {
        let mut facts = Arc::new(VisitFacts::default());
        loop {
            let receipt = self
                .services
                .observe_visit(
                    self.mount,
                    self.request,
                    self.protected,
                    self.handle,
                    self.operation.clone(),
                    facts.clone(),
                )
                .await?;
            let original = receipt.get().clone();
            let needs = match (&original.result, &original.decision) {
                (Ok(None), Some(NativeReadDecision::Value(inode))) => {
                    let stat = inode.clone().into();
                    self.receipt = Some(receipt);
                    return Ok(Ok(NativeReadValue {
                        stat,
                        file: original.open_candidate,
                        directory: original.directory_candidate,
                        original,
                    }));
                }
                (Ok(None), Some(NativeReadDecision::Refused(refusal))) => return Ok(Err(*refusal)),
                (Ok(None), Some(NativeReadDecision::Needs(needs))) if !needs.is_empty() => needs,
                _ => {
                    self.receipt = Some(receipt);
                    return Err(Box::new(NativeReadFailure {
                        reason: "native read visit did not produce a usable original answer",
                        original,
                    }));
                }
            };
            // The job and its credit are gone before any provider wait.
            drop(receipt);
            // Provider reads never run on the loop that received the request.
            crate::LeaveReceiver::default().await;
            let base = self.services.base().await?;
            let result = Arc::make_mut(&mut facts).supply(&base, needs, None);
            drop(base); // Return the actual reader before another SQL wait.
            if let Err(error) = result {
                return Err(self.services.failed_base_read(error.into()));
            }
            NextTurn::default().await;
        }
    }
}
