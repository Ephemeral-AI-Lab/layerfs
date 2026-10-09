//! A native request served by owner visits that record no request source.
//! Each visit is one job that decides over current rows; the job is the
//! request's whole window in the owner, so nothing is left to release. Base
//! facts a visit needs are read from objects already resident in the
//! canonical cache, inside the job. A fact that is not resident leaves the
//! visit undecided and unchanged: the caller reads it outside the owner,
//! tagged with its base root, and visits again. A later visit uses facts only
//! of the base the Workspace has at that moment, so an install between two
//! visits costs another read and never a stale answer.
use crate::job::Decided;
use crate::operations::native_mutation::effect;
use crate::operations::native_read::decide_read;
use crate::{
    BaseFacts, BaseView, JobOutcome, NamespaceJob, NativeMutationOutcome, NativeReadDecision,
    NativeReadOperation, NativeReadOutcome, Need, Operation, Position, Refusal, Time, VisitFacts,
    Workspace, WorkspaceError, WorkspaceResult, WriteData,
};
use layerfs_content::{
    filesystem::{identity::MAXIMUM_INODE_SERIAL, PathName},
    ContentError,
};
use layerfs_overlay::{BaseSource, NativeDecision, NativeMount, OpenFile, Overlay, OverlayError};
use std::{borrow::Cow, fmt, sync::Arc};

/// Fact rounds one visit may take from resident objects. A window of one job,
/// not a limit of any operation: an undecided visit continues elsewhere.
const ROUNDS: usize = 4;

/// What a native mutation was asked to do, before its descriptor is known.
#[derive(Clone, Debug)]
pub enum NativeInput {
    Named(Operation),
    Write {
        offset: u64,
        data: WriteData,
        cached: bool,
    },
    Attributes {
        mode: Option<u32>,
        mtime: Option<Time>,
        size: Option<u64>,
    },
}
impl NativeInput {
    /// The operation over the request's descriptor, when the two agree.
    pub fn operation(&self, file: Option<OpenFile>) -> Option<Operation> {
        Some(match (self, file) {
            (Self::Named(operation), None) => operation.clone(),
            (
                Self::Write {
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
            (Self::Attributes { mode, mtime, size }, Some(file)) => Operation::SetOpenAttributes {
                file,
                mode: *mode,
                mtime: *mtime,
                size: *size,
            },
            _ => return None,
        })
    }
    fn creates(&self) -> bool {
        matches!(self, Self::Named(operation) if operation.creates())
    }
    fn charge(&self) -> usize {
        match self {
            Self::Write { data, .. } => data.len(),
            Self::Named(Operation::Write { data, .. }) => data.len(),
            _ => 0,
        }
    }
}
/// One native mutation request as its callback received it.
#[derive(Clone, Debug)]
pub struct NativeVisitRequest {
    pub mount: NativeMount,
    pub request: u64,
    /// The inode the kernel holds for this request: a parent or the target.
    pub serial: u64,
    /// The descriptor of a handle-addressed mutation.
    pub handle: Option<u64>,
    pub input: NativeInput,
    pub open: Option<bool>,
    pub now: Time,
    /// The reserved serial of a creating operation.
    pub fresh: Option<u64>,
    /// Base facts read for earlier undecided visits of this request.
    pub facts: Arc<VisitFacts>,
}
/// LOOKUP or GETATTR as one owner job.
#[derive(Clone)]
pub struct NativeReadVisit {
    mount: NativeMount,
    serial: u64,
    handle: Option<u64>,
    operation: NativeReadOperation,
    facts: Arc<VisitFacts>,
    resident: BaseView,
}
/// One native mutation as one owner job.
#[derive(Clone)]
pub struct NativeMutationVisit {
    request: NativeVisitRequest,
    root: u64,
    resident: BaseView,
}
impl fmt::Debug for NativeReadVisit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeReadVisit")
            .field("mount", &self.mount)
            .field("serial", &self.serial)
            .field("handle", &self.handle)
            .field("operation", &self.operation)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for NativeMutationVisit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeMutationVisit")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}
/// The request's facts of the visit's own base, or none.
fn carried(facts: &VisitFacts, source: BaseSource) -> Cow<'_, BaseFacts> {
    match facts.of(source.root()) {
        Some(facts) => Cow::Borrowed(facts),
        None => Cow::Owned(BaseFacts::default()),
    }
}
/// Supplies what one evaluation asked for from resident objects of the
/// visit's own base. False leaves the decision as it was.
fn supply(
    resident: &BaseView,
    source: BaseSource,
    facts: &mut BaseFacts,
    needs: &[Need],
    path: Option<&[PathName]>,
) -> bool {
    resident.identity().0.to_bytes() == source.root() && resident.supply(facts, needs, path).is_ok()
}
impl Workspace {
    /// `resident` reads canonical objects from memory only. No I/O here.
    pub fn native_read_visit(
        &self,
        resident: Arc<crate::CanonicalClient>,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
        facts: Arc<VisitFacts>,
    ) -> WorkspaceResult<NativeReadVisit> {
        let base = self.base()?;
        if mount.route() != self.route() || mount.root_serial() != base.root().root_inode().serial()
        {
            return Err(OverlayError::Stale.into());
        }
        if !matches!(
            operation,
            NativeReadOperation::Lookup { .. } | NativeReadOperation::Getattr { .. }
        ) {
            return Err(ContentError::InvalidRecord("native visit operation").into());
        }
        Ok(NativeReadVisit {
            mount,
            serial,
            handle,
            operation,
            facts,
            resident: base.with_client(resident),
        })
    }
    pub fn native_mutation_visit(
        &self,
        resident: Arc<crate::CanonicalClient>,
        request: NativeVisitRequest,
    ) -> WorkspaceResult<NativeMutationVisit> {
        let base = self.base()?;
        let root = base.root().root_inode().serial();
        if request.mount.route() != self.route() || request.mount.root_serial() != root {
            return Err(OverlayError::Stale.into());
        }
        if request.input.creates() != request.fresh.is_some()
            || request
                .fresh
                .is_some_and(|serial| serial == 0 || serial > MAXIMUM_INODE_SERIAL)
        {
            return Err(ContentError::InvalidRecord("mutation reserved serial").into());
        }
        Ok(NativeMutationVisit {
            request,
            root,
            resident: base.with_client(resident),
        })
    }
}
impl NativeReadVisit {
    pub const fn mount(&self) -> NativeMount {
        self.mount
    }
    pub fn charge(&self) -> usize {
        self.facts.charge() + 255
    }
    /// The answer, a refusal or the needs of an undecided visit. An
    /// undecided visit has written nothing.
    pub fn perform(&self, db: &Overlay) -> NativeReadOutcome {
        let lookup = matches!(self.operation, NativeReadOperation::Lookup { .. });
        db.observe_native_visit(
            self.mount,
            self.serial,
            self.handle,
            lookup,
            |rows, source| {
                let mut facts = carried(&self.facts, source);
                let mut round = 0;
                loop {
                    let decision =
                        decide_read(&self.operation, &facts, self.mount, rows, self.serial)?;
                    let NativeDecision::Needs(NativeReadDecision::Needs(needs)) = &decision else {
                        return Ok(decision);
                    };
                    if round == ROUNDS
                        || !supply(&self.resident, source, facts.to_mut(), needs, None)
                    {
                        return Ok(decision);
                    }
                    round += 1;
                }
            },
        )
    }
}
impl NativeMutationVisit {
    pub const fn mount(&self) -> NativeMount {
        self.request.mount
    }
    pub fn charge(&self) -> usize {
        self.request.input.charge() + self.request.facts.charge() + 2 * 255 + 4096
    }
    /// The applied outcome, a definite answer without effect, or the needs
    /// of an undecided visit. An undecided visit has written nothing.
    pub fn perform(&self, db: &Overlay) -> NativeMutationOutcome {
        let NativeVisitRequest {
            mount,
            request,
            serial,
            handle,
            open,
            now,
            fresh,
            ..
        } = self.request;
        let mut decided = None;
        let mut published = None;
        let applied = db.mutate_native_visit(mount, request, serial, handle, |rows, file| {
            let source = rows.source();
            let Some(operation) = self.request.input.operation(file) else {
                decided = Some(Ok(JobOutcome::Refused(Refusal::Invalid)));
                return Ok(None);
            };
            let mut job = NamespaceJob {
                source,
                root: self.root,
                operation,
                now,
                serial: fresh,
                facts: carried(&self.request.facts, source).into_owned(),
            };
            let mut round = 0;
            loop {
                match job.decide_visit(db, rows, mount) {
                    Ok(Decided::Final(JobOutcome::Needs(needs)))
                        if round < ROUNDS
                            && supply(
                                &self.resident,
                                source,
                                &mut job.facts,
                                &needs,
                                job.operation.destination_path(),
                            ) =>
                    {
                        round += 1;
                    }
                    Ok(Decided::Final(outcome)) => {
                        decided = Some(Ok(outcome));
                        return Ok(None);
                    }
                    Ok(Decided::Publish { changes, inode }) => {
                        published = Some(inode);
                        return Ok(Some((changes, effect(&job.operation, job.serial, open))));
                    }
                    Err(WorkspaceError::Overlay(error)) => return Err(error),
                    Err(error) => {
                        decided = Some(Err(error));
                        return Ok(None);
                    }
                }
            }
        });
        let mut file = None;
        let result = match (applied, decided, published) {
            (Err(error), _, _) => Err(error.into()),
            (Ok(Some(applied)), _, Some(inode)) => {
                file = applied.file;
                Ok(JobOutcome::Applied {
                    publication: applied.publication,
                    inode,
                })
            }
            (Ok(None), Some(decided), _) => decided,
            _ => Err(ContentError::InvalidRecord("undecided native visit").into()),
        };
        NativeMutationOutcome { result, file }
    }
}
