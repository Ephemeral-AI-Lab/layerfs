//! Resumable native stat/lookup decisions using the ordinary Workspace evaluator.
use crate::{
    eval::Eval, BaseFacts, Need, Refusal, SourceView, ViewStat, WorkspaceError, WorkspaceResult,
};
use layerfs_content::{filesystem::PathName, ContentError};
use layerfs_overlay::{
    BaseSource, FileRead, Inode, InodeKind, NativeDecision, NativeDirectory, NativeMount,
    NativeObservation, OpenFile, Overlay, OverlayError, OverlayResult, SourceRows,
};
use std::{fmt, sync::Arc};

#[derive(Clone, Debug)]
pub enum NativeReadOperation {
    Lookup { parent: u64, name: PathName },
    Getattr { serial: u64 },
    Open { serial: u64, writable: bool },
    Opendir { serial: u64 },
}
#[derive(Clone, Debug)]
pub struct NativeReadJob {
    source: BaseSource,
    mount: NativeMount,
    operation: NativeReadOperation,
    facts: BaseFacts,
}
#[derive(Debug)]
pub enum NativeReadDecision {
    Needs(Vec<Need>),
    Value(Inode),
    Refused(Refusal),
    Failed(WorkspaceError),
}
pub type NativeReadOutcome = NativeObservation<NativeReadDecision>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeReadStage {
    Owner,
    Base,
    Finished,
}
pub struct NativeReadPlan {
    job: NativeReadJob,
    needs: Vec<Need>,
    stage: NativeReadStage,
}
#[derive(Debug)]
pub struct NativeReadValue {
    pub stat: ViewStat,
    pub read: FileRead,
    pub file: Option<OpenFile>,
    pub directory: Option<NativeDirectory>,
    pub original: Arc<NativeReadOutcome>,
}
#[derive(Debug)]
pub struct NativeReadFailure {
    pub reason: &'static str,
    pub original: Arc<NativeReadOutcome>,
}
impl fmt::Display for NativeReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.reason, self.original)
    }
}
impl std::error::Error for NativeReadFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match (&self.original.result, &self.original.decision) {
            (Err(error), _) => Some(error),
            (_, Some(NativeReadDecision::Failed(error))) => Some(error),
            _ => None,
        }
    }
}
impl NativeReadJob {
    pub const fn source(&self) -> BaseSource {
        self.source
    }
    pub fn charge(&self) -> usize {
        self.facts.charge() + 255
    }
    /// One owner transaction contains current local reads, the positive lookup
    /// increment and its independent processing owner. Provider I/O is absent.
    pub fn perform(&self, db: &Overlay) -> NativeReadOutcome {
        let lookup = matches!(self.operation, NativeReadOperation::Lookup { .. });
        if let NativeReadOperation::Open { writable, .. } = self.operation {
            db.observe_native_open(self.mount, self.source, writable, |rows, protected| {
                self.decide_on(rows, protected)
            })
        } else if matches!(self.operation, NativeReadOperation::Opendir { .. }) {
            db.observe_native_directory(self.mount, self.source, |rows, protected| {
                self.decide_on(rows, protected)
            })
        } else {
            db.observe_native(self.mount, self.source, lookup, |rows, protected| {
                self.decide_on(rows, protected)
            })
        }
    }
    fn decide_on(
        &self,
        rows: SourceRows<'_>,
        protected: u64,
    ) -> OverlayResult<NativeDecision<NativeReadDecision>> {
        let wanted = match self.operation {
            NativeReadOperation::Lookup { parent, .. } => parent,
            NativeReadOperation::Getattr { serial }
            | NativeReadOperation::Open { serial, .. }
            | NativeReadOperation::Opendir { serial } => serial,
        };
        if wanted != protected {
            return Err(OverlayError::Stale);
        }
        let mut eval = Eval {
            rows,
            facts: &self.facts,
            root: self.mount.root_serial(),
            open_serial: None,
            needs: Vec::new(),
        };
        let decision = match self.decide(&mut eval) {
            Ok(Some(inode)) => NativeReadDecision::Value(inode),
            Ok(None) if !eval.needs.is_empty() => NativeReadDecision::Needs(eval.needs),
            Ok(None) => NativeReadDecision::Failed(
                ContentError::InvalidRecord("empty native read needs").into(),
            ),
            Err(WorkspaceError::Overlay(error)) => return Err(error),
            Err(WorkspaceError::Refused(refusal)) => NativeReadDecision::Refused(refusal),
            Err(error) => NativeReadDecision::Failed(error),
        };
        Ok(match decision {
            NativeReadDecision::Needs(_) => NativeDecision::Needs(decision),
            NativeReadDecision::Value(ref inode) => NativeDecision::Finished {
                inode: Some(inode.clone()),
                value: decision,
            },
            _ => NativeDecision::Finished {
                inode: None,
                value: decision,
            },
        })
    }
    fn decide(&self, eval: &mut Eval<'_>) -> WorkspaceResult<Option<Inode>> {
        match &self.operation {
            // The source's independent read reference permits removed metadata.
            NativeReadOperation::Getattr { serial } => eval.target(*serial),
            NativeReadOperation::Opendir { serial } => eval.directory(*serial),
            NativeReadOperation::Open { serial, .. } => {
                let inode = eval.existing(*serial)?;
                match inode {
                    Some(inode) if inode.kind == InodeKind::File => Ok(Some(inode)),
                    Some(inode) if inode.kind == InodeKind::Directory => {
                        Err(WorkspaceError::Refused(Refusal::IsDirectory))
                    }
                    Some(_) => Err(WorkspaceError::Refused(Refusal::Invalid)),
                    None => Ok(None),
                }
            }
            NativeReadOperation::Lookup { parent, name } => {
                let directory = eval.directory(*parent)?;
                let layers = eval.layers(*parent, name)?;
                let bound = eval.bound(*parent, directory.as_ref(), name, layers);
                let (Some(_), Some(bound)) = (directory, bound) else {
                    return Ok(None);
                };
                let serial = bound.ok_or(WorkspaceError::Refused(Refusal::Missing))?;
                eval.target(serial)
            }
        }
    }
}
impl SourceView {
    /// No I/O or acquisition. The supplied source must be the original native
    /// source protecting the operation's parent/target, and remain owned.
    pub fn native_read_plan(
        &self,
        mount: NativeMount,
        operation: NativeReadOperation,
    ) -> WorkspaceResult<NativeReadPlan> {
        if mount.route() != self.source.route() || mount.root_serial() != self.root_serial() {
            return Err(OverlayError::Stale.into());
        }
        Ok(NativeReadPlan {
            job: NativeReadJob {
                source: self.source,
                mount,
                operation,
                facts: BaseFacts::default(),
            },
            needs: Vec::new(),
            stage: NativeReadStage::Owner,
        })
    }
}
impl NativeReadPlan {
    pub const fn stage(&self) -> NativeReadStage {
        self.stage
    }
    pub fn job(&self) -> Option<&NativeReadJob> {
        (self.stage == NativeReadStage::Owner).then_some(&self.job)
    }
    pub fn charge(&self) -> usize {
        self.job.charge() + self.needs.capacity() * (std::mem::size_of::<Need>() + 255)
    }
    /// The original complete observation remains in success/failure custody.
    /// Engine completion must succeed before any semantic value can be used.
    pub fn accept(
        &mut self,
        original: Arc<NativeReadOutcome>,
    ) -> Result<Option<NativeReadValue>, NativeReadFailure> {
        let previous = std::mem::replace(&mut self.stage, NativeReadStage::Finished);
        if previous == NativeReadStage::Owner {
            match (&original.result, &original.decision) {
                (Ok(Some(read)), Some(NativeReadDecision::Value(inode)))
                    if read.serial() == inode.serial =>
                {
                    return Ok(Some(NativeReadValue {
                        stat: inode.clone().into(),
                        read: *read,
                        file: original.open_candidate,
                        directory: original.directory_candidate,
                        original,
                    }));
                }
                (Ok(None), Some(NativeReadDecision::Needs(needs))) if !needs.is_empty() => {
                    self.needs = needs.clone();
                    self.stage = NativeReadStage::Base;
                    return Ok(None);
                }
                _ => {}
            }
        }
        Err(NativeReadFailure {
            reason: "native read did not produce a usable original answer",
            original,
        })
    }
    /// Only call with admitted immutable-provider capacity; no owner job runs
    /// here. A failed demand retains input/facts/needs and finishes the plan.
    pub fn supply(&mut self, view: &SourceView) -> WorkspaceResult<()> {
        let previous = std::mem::replace(&mut self.stage, NativeReadStage::Finished);
        if previous != NativeReadStage::Base || view.source() != self.job.source {
            return Err(OverlayError::Stale.into());
        }
        view.supply(&mut self.job.facts, &self.needs, None)?;
        self.needs.clear();
        self.stage = NativeReadStage::Owner;
        Ok(())
    }
}
