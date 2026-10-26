//! One kernel mutation: owner visits that record no request source, fact
//! reads between undecided visits, a single publishing visit, then the exact
//! post-reply release of its ticket.
use crate::ports::{BaseDemandFailed, Fenced, RequestServices, ServiceError, ServiceReply};
use layerfs_overlay::{KernelMount, OpenFile, Publication};
use layerfs_workspace::{
    JobOutcome, KernelMutationOutcome, KernelVisitRequest, Refusal, Time, ViewStat, VisitFacts,
    WorkspaceError,
};
use std::{fmt, sync::Arc};

/// What the kernel asked for, before a handle's descriptor is resolved.
/// What a kernel mutation was asked to do, before its descriptor is known.
pub use layerfs_workspace::KernelInput as MutationInput;
#[derive(Clone, Debug)]
pub struct MutationRequest {
    pub mount: KernelMount,
    pub request: u64,
    /// The parent or target whose kernel reference admits this request.
    pub protected: u64,
    pub handle: Option<u64>,
    pub input: MutationInput,
    /// CREATE replies an open descriptor of this access mode with its entry.
    pub open: Option<bool>,
    /// Assigned at processing; the engine owns every stored time.
    pub now: Time,
}
/// A definite answer without filesystem publication or a ticket. Attempted
/// SQL and transient allocation observations remain counted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Declined {
    Refused(Refusal),
    /// Physical admission refused before BEGIN, or the atomic kernel job
    /// returned direct SQLite Full after known rollback and allocation
    /// observation. SQL/transient allocation work is paid; no filesystem
    /// mutation or publication ticket exists. Unknown outcomes stay retained.
    NoSpace,
    /// The serial allocator's writer was contended; nothing was reserved.
    Contended,
    /// A symlink target the kernel could not read back in one page.
    TargetTooLong,
}
/// The published result a reply is computed from. `changed` is false for a
/// successful operation that had nothing to publish.
#[derive(Debug)]
pub struct Published {
    pub stat: Option<ViewStat>,
    pub file: Option<OpenFile>,
    pub changed: bool,
}
pub(super) struct Custody {
    services: Arc<dyn RequestServices>,
    request: MutationRequest,
    // Retain the engine receipt through every consumer of the outcome.
    receipt: Option<ServiceReply<Arc<KernelMutationOutcome>>>,
    /// Published and not yet released: exactly one reply attempt is owed.
    publication: Option<Publication>,
}
/// A decided mutation. It still owns its publication ticket until the single
/// reply attempt has returned.
pub struct KernelMutation {
    value: Result<Published, Declined>,
    custody: Custody,
}
/// Exact failed step with the original request and every still-owned engine
/// reference. No destructor issues SQL, guesses whether a job ran, releases a
/// ticket or undoes a publication.
pub struct MutationFailure {
    pub reason: ServiceError,
    pub provider: Option<ServiceError>,
    custody: Custody,
}
/// An attempted owner job whose original result is not a usable answer.
#[derive(Debug)]
struct Attempted(Arc<KernelMutationOutcome>);
impl fmt::Display for Attempted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "attempted kernel mutation: {:?}", self.0.result)
    }
}
impl std::error::Error for Attempted {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.result.as_ref().err().map(|error| error as _)
    }
}
impl fmt::Debug for MutationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MutationFailure")
            .field("reason", &self.reason)
            .field("provider", &self.provider)
            .field("request", &self.custody.request)
            .field("publication", &self.custody.publication)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for MutationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.reason.fmt(f)
    }
}
impl std::error::Error for MutationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.reason.as_ref())
    }
}
impl MutationFailure {
    fn new(reason: ServiceError, custody: Custody) -> Self {
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
    pub fn request(&self) -> &MutationRequest {
        &self.custody.request
    }
    /// A ticket still owed its reply-attempt release. The mutation it covers
    /// is published and stays published.
    pub const fn publication(&self) -> Option<Publication> {
        self.custody.publication
    }
    pub fn outcome(&self) -> Option<&KernelMutationOutcome> {
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
    /// Ends a mutation that was fenced or whose base demand failed. Both
    /// kinds of step precede the publishing visit, so it holds nothing and
    /// no job is issued; a failure that owes a ticket anyway, or that is of
    /// neither kind, is returned unchanged.
    pub async fn relinquish(self) -> Result<(), MutationFailure> {
        if (!self.fenced() && self.base_demand().is_none()) || self.custody.publication.is_some() {
            return Err(self);
        }
        let Self { mut custody, .. } = self;
        custody.receipt = None;
        match custody.release().await {
            Ok(()) => Ok(()),
            Err(reason) => Err(MutationFailure::new(reason, custody)),
        }
    }
}
impl KernelMutation {
    /// Drives one mutation to its deciding owner job. Need rounds publish
    /// nothing; the publishing job is the only attempt and is never replayed.
    pub async fn perform(
        services: Arc<dyn RequestServices>,
        request: MutationRequest,
    ) -> Result<Self, MutationFailure> {
        let mut custody = Custody {
            services,
            request,
            receipt: None,
            publication: None,
        };
        match custody.decide().await {
            Ok(value) => Ok(Self { value, custody }),
            Err(reason) => Err(MutationFailure::new(reason, custody)),
        }
    }
    pub fn value(&self) -> Result<&Published, Declined> {
        self.value.as_ref().map_err(|declined| *declined)
    }
    /// Keeps a decided mutation whose reply could not be composed. Its ticket
    /// stays owned; the publication is not undone.
    pub fn retain(self, reason: ServiceError) -> MutationFailure {
        let Self { value, custody } = self;
        drop(value);
        MutationFailure::new(reason, custody)
    }
    /// Called only after the single reply attempt returned. The publication
    /// ticket is released in one owner job, exactly once; a mutation that
    /// published nothing has nothing to release and issues no job. Kernel
    /// lookup and open owners persist independently.
    pub async fn replied(self) -> Result<(), MutationFailure> {
        let Self { value, mut custody } = self;
        drop(value);
        custody.receipt = None;
        match custody.release().await {
            Ok(()) => Ok(()),
            Err(reason) => Err(MutationFailure::new(reason, custody)),
        }
    }
}
impl Custody {
    /// A mutation is served by owner visits that record no request source.
    /// A visit that publishes also takes the kernel custody of its reply; an
    /// undecided visit changed nothing, and the base facts it asked for are
    /// read here, outside the owner, before the next visit. The only thing
    /// left to release is the reply ticket of a publication.
    async fn decide(&mut self) -> Result<Result<Published, Declined>, ServiceError> {
        let MutationRequest {
            mount,
            request,
            protected,
            handle,
            open,
            now,
            ..
        } = self.request;
        let by_handle = !matches!(self.request.input, MutationInput::Named(_));
        let fresh = match &self.request.input {
            MutationInput::Named(operation) if operation.creates() => {
                match self.services.reserve_serial()? {
                    Some(serial) => Some(serial),
                    None => return Ok(Err(Declined::Contended)),
                }
            }
            _ => None,
        };
        // What a write's own bytes say about committed content is settled
        // once, before the first owner visit, and is part of every visit.
        if let MutationInput::Write {
            offset,
            data,
            cached: false,
        } = &mut self.request.input
        {
            *data = self
                .services
                .recognize(protected, *offset, data.clone())
                .await?;
        }
        let mut facts = Arc::new(VisitFacts::default());
        loop {
            let visit = KernelVisitRequest {
                mount,
                request,
                serial: protected,
                handle: handle.filter(|_| by_handle),
                input: self.request.input.clone(),
                open,
                now,
                fresh,
                facts: facts.clone(),
            };
            let started = std::time::Instant::now();
            let receipt = self.services.mutate_visit(visit).await;
            crate::phase::charge(crate::phase::Phase::MutateVisit, started);
            self.receipt = Some(receipt?);
            let original = self.receipt.as_ref().unwrap().get().clone();
            let needs = match &original.result {
                Ok(JobOutcome::Applied { publication, inode }) => {
                    self.publication = Some(*publication);
                    return Ok(Ok(Published {
                        stat: inode.clone().map(Into::into),
                        file: original.file,
                        changed: true,
                    }));
                }
                Ok(JobOutcome::Unchanged { inode }) => {
                    return Ok(Ok(Published {
                        stat: inode.clone().map(Into::into),
                        file: None,
                        changed: false,
                    }))
                }
                Ok(JobOutcome::Refused(refusal)) => return Ok(Err(Declined::Refused(*refusal))),
                Ok(JobOutcome::Needs(needs)) if !needs.is_empty() => needs,
                // Only the atomic visit's direct, known rollback qualifies.
                // Keep its original completion until the one ENOSPC reply and
                // normal release. Uncertain wrappers/provider ambiguity remain
                // attempted failures with their original custody.
                Err(WorkspaceError::Overlay(error))
                    if error.is_no_space()
                        && self.publication.is_none()
                        && self
                            .services
                            .provider_failure()
                            .is_ok_and(|failure| failure.is_none()) =>
                {
                    return Ok(Err(Declined::NoSpace))
                }
                _ => return Err(Box::new(Attempted(original))),
            };
            self.receipt = None;
            // Provider reads never run on the loop that received the request.
            let started = std::time::Instant::now();
            let base = self.services.base().await?;
            let path = match &self.request.input {
                MutationInput::Named(operation) => operation.destination_path(),
                _ => None,
            };
            let result = Arc::make_mut(&mut facts).supply(&base, needs, path);
            drop(base); // Return the actual reader before another SQL wait.
            crate::phase::charge(crate::phase::Phase::MutateSupply, started);
            if let Err(error) = result {
                return Err(self.services.failed_base_read(error.into()));
            }
        }
    }
    async fn release(&mut self) -> Result<(), ServiceError> {
        if let Some(publication) = self.publication {
            let started = std::time::Instant::now();
            let released = self.services.reply_attempted(publication).await;
            crate::phase::charge(crate::phase::Phase::MutateRelease, started);
            drop(released?);
            self.publication = None;
        }
        Ok(())
    }
}
