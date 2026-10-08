//! One native mutation over Workspace's resumable plan: source, fact rounds,
//! a single publishing owner job, then exact post-reply ticket/source release.
use crate::{
    ports::{RequestServices, ServiceError, ServiceReply},
    NextTurn,
};
use layerfs_overlay::{BaseSource, NativeMount, OpenFile, OverlayError, Publication};
use layerfs_workspace::{
    MutationInputFailure, MutationPlan, NativeMutationOutcome, Operation, Outcome, Position,
    Refusal, SourceView, Time, ViewStat, WorkspaceError, WriteData,
};
use std::{fmt, sync::Arc};

/// What the kernel asked for, before a handle's descriptor is resolved.
#[derive(Clone, Debug)]
pub enum MutationInput {
    /// Addressed by serials and names under the request's lookup custody.
    Named(Operation),
    /// WRITE through a handle, always at the kernel's offset. `cached` is the
    /// per-request page-cache flag of a shared-mapping store.
    Write {
        offset: u64,
        data: WriteData,
        cached: bool,
    },
    /// SETATTR through a handle.
    Attributes {
        mode: Option<u32>,
        mtime: Option<Time>,
        size: Option<u64>,
    },
}
/// One received kernel mutation with the identities that protect it.
#[derive(Clone, Debug)]
pub struct MutationRequest {
    pub mount: NativeMount,
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
/// A definite answer with no effect: nothing was published, no ticket exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Declined {
    Refused(Refusal),
    /// The engine's physical reservation refused the transaction before it.
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
    source: Option<BaseSource>,
    file: Option<OpenFile>,
    view: Option<SourceView>,
    plan: Option<MutationPlan>,
    // Retain engine completion credit through every consumer of the outcome.
    receipt: Option<ServiceReply<Arc<NativeMutationOutcome>>>,
    /// Published and not yet released: exactly one reply attempt is owed.
    publication: Option<Publication>,
    refused_input: Option<Box<MutationInputFailure>>,
}
/// A decided mutation. It still owns its publication ticket and processing
/// source until the single reply attempt has returned.
pub struct NativeMutation {
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
struct Attempted(Arc<NativeMutationOutcome>);
impl fmt::Display for Attempted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "attempted native mutation: {:?}", self.0.result)
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
            .field("source", &self.custody.source)
            .field("file", &self.custody.file)
            .field("publication", &self.custody.publication)
            .field("refused_input", &self.custody.refused_input)
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
    pub const fn retained_source(&self) -> Option<BaseSource> {
        self.custody.source
    }
    /// A ticket still owed its reply-attempt release. The mutation it covers
    /// is published and stays published.
    pub const fn publication(&self) -> Option<Publication> {
        self.custody.publication
    }
    pub fn outcome(&self) -> Option<&NativeMutationOutcome> {
        self.custody
            .receipt
            .as_ref()
            .map(|receipt| receipt.get().as_ref())
    }
}
impl NativeMutation {
    /// Drives one mutation to its deciding owner job. Need rounds publish
    /// nothing; the publishing job is the only attempt and is never replayed.
    pub async fn perform(
        services: Arc<dyn RequestServices>,
        request: MutationRequest,
    ) -> Result<Self, MutationFailure> {
        let mut custody = Custody {
            services,
            request,
            source: None,
            file: None,
            view: None,
            plan: None,
            receipt: None,
            publication: None,
            refused_input: None,
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
    /// and source stay owned; the publication is not undone.
    pub fn retain(self, reason: ServiceError) -> MutationFailure {
        let Self { value, custody } = self;
        drop(value);
        MutationFailure::new(reason, custody)
    }
    /// Called only after the single reply attempt returned. The publication
    /// ticket is released first, then the processing source; each exactly
    /// once. Kernel lookup and open owners persist independently.
    pub async fn replied(self) -> Result<(), MutationFailure> {
        let Self { value, mut custody } = self;
        drop(value);
        custody.plan = None;
        custody.view = None;
        custody.receipt = None;
        match custody.release().await {
            Ok(()) => Ok(()),
            Err(reason) => Err(MutationFailure::new(reason, custody)),
        }
    }
}
impl Custody {
    fn operation(&self) -> Result<Operation, Declined> {
        let invalid = Declined::Refused(Refusal::Invalid);
        Ok(match (&self.request.input, self.file) {
            (MutationInput::Named(operation), None) => operation.clone(),
            (
                MutationInput::Write {
                    offset,
                    data,
                    cached,
                },
                Some(file),
            ) => {
                if *cached {
                    Operation::StoreOpen {
                        file,
                        offset: *offset,
                        data: data.clone(),
                    }
                } else {
                    Operation::WriteOpen {
                        file,
                        position: Position::At(*offset),
                        data: data.clone(),
                    }
                }
            }
            (MutationInput::Attributes { mode, mtime, size }, Some(file)) => {
                Operation::SetOpenAttributes {
                    file,
                    mode: *mode,
                    mtime: *mtime,
                    size: *size,
                }
            }
            _ => return Err(invalid),
        })
    }
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
        match handle.filter(|_| by_handle) {
            Some(handle) => {
                let granted = self
                    .services
                    .open_source(mount, request, protected, handle)
                    .await?;
                let (source, file) = *granted.get();
                self.source = Some(source);
                self.file = Some(file);
            }
            None => {
                let granted = self
                    .services
                    .source(mount, request, protected, None)
                    .await?;
                self.source = Some(*granted.get());
            }
        }
        let source = self.source.expect("acquired native source");
        let operation = match self.operation() {
            Ok(operation) => operation,
            Err(declined) => return Ok(Err(declined)),
        };
        let serial = if operation.creates() {
            match self.services.reserve_serial()? {
                Some(serial) => Some(serial),
                None => return Ok(Err(Declined::Contended)),
            }
        } else {
            None
        };
        let view = self.services.view(source)?;
        match self.services.prepare(&view, operation, now, serial) {
            Ok(plan) => self.plan = Some(plan),
            Err(refused) => {
                let reason = format!("native mutation preparation: {:?}", refused.error);
                self.refused_input = Some(refused);
                return Err(reason.into());
            }
        }
        self.view = Some(view);
        loop {
            let job = self
                .plan
                .as_ref()
                .and_then(|plan| plan.native_job(mount, open))
                .expect("native mutation owner stage");
            self.receipt = Some(self.services.mutate(job).await?);
            let original = self.receipt.as_ref().unwrap().get().clone();
            let outcome = match &original.result {
                Ok(outcome) => outcome.clone(),
                // The engine's reservation refuses before any statement of
                // the transaction: a definite answer with no effect.
                Err(WorkspaceError::Overlay(OverlayError::Reservation { .. })) => {
                    return Ok(Err(Declined::NoSpace))
                }
                Err(_) => return Err(Box::new(Attempted(original))),
            };
            match self.plan.as_mut().unwrap().accept(Ok(outcome)) {
                Ok(Some(Outcome::Applied { publication, stat })) => {
                    self.publication = Some(publication);
                    return Ok(Ok(Published {
                        stat,
                        file: original.file,
                        changed: true,
                    }));
                }
                Ok(Some(Outcome::Unchanged { stat })) => {
                    return Ok(Ok(Published {
                        stat,
                        file: None,
                        changed: false,
                    }))
                }
                Ok(None) => {}
                Err(WorkspaceError::Refused(refusal)) => {
                    return Ok(Err(Declined::Refused(refusal)))
                }
                Err(error) => return Err(Box::new(error)),
            }
            self.receipt = None;
            let immutable = self.services.immutable(self.view.as_ref().unwrap()).await?;
            let result = self.plan.as_mut().unwrap().supply(&immutable);
            drop(immutable); // Return the actual reader before another SQL wait.
            result?;
            NextTurn::default().await;
        }
    }
    async fn release(&mut self) -> Result<(), ServiceError> {
        if let Some(publication) = self.publication {
            drop(self.services.reply_attempted(publication).await?);
            self.publication = None;
        }
        if let Some(source) = self.source {
            drop(self.services.release_source(source).await?);
            self.source = None;
        }
        Ok(())
    }
}
