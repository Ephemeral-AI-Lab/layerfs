//! Resumable kernel stat/lookup decisions using the ordinary Workspace evaluator.
use crate::{eval::Eval, BaseFacts, Need, Refusal, ViewStat, WorkspaceError, WorkspaceResult};
use layerfs_content::{filesystem::PathName, ContentError};
use layerfs_overlay::{
    Inode, InodeKind, KernelDecision, KernelDirectory, KernelMount, KernelObservation, OpenFile,
    OverlayError, OverlayResult, SourceRows,
};
use std::{fmt, sync::Arc};

#[derive(Clone, Debug)]
pub enum KernelReadOperation {
    Lookup {
        parent: u64,
        name: PathName,
    },
    /// Attributes only: no independent read is retained.
    Getattr {
        serial: u64,
    },
    /// The target of a READ or READLINK, with the read its bytes come from.
    Data {
        serial: u64,
    },
    Open {
        serial: u64,
        writable: bool,
    },
    Opendir {
        serial: u64,
    },
}
#[derive(Debug)]
pub enum KernelReadDecision {
    Needs(Vec<Need>),
    Value(Inode),
    Refused(Refusal),
    Failed(WorkspaceError),
}
pub type KernelReadOutcome = KernelObservation<KernelReadDecision>;

#[derive(Debug)]
pub struct KernelReadValue {
    pub stat: ViewStat,
    pub file: Option<OpenFile>,
    pub directory: Option<KernelDirectory>,
    pub original: Arc<KernelReadOutcome>,
}
#[derive(Debug)]
pub struct KernelReadFailure {
    pub reason: &'static str,
    pub original: Arc<KernelReadOutcome>,
}
impl fmt::Display for KernelReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.reason, self.original)
    }
}
impl std::error::Error for KernelReadFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match (&self.original.result, &self.original.decision) {
            (Err(error), _) => Some(error),
            (_, Some(KernelReadDecision::Failed(error))) => Some(error),
            _ => None,
        }
    }
}
/// One evaluation of a kernel read over current rows and supplied facts.
pub(crate) fn decide_read(
    operation: &KernelReadOperation,
    facts: &BaseFacts,
    mount: KernelMount,
    rows: SourceRows<'_>,
    protected: u64,
) -> OverlayResult<KernelDecision<KernelReadDecision>> {
    let wanted = match *operation {
        KernelReadOperation::Lookup { parent, .. } => parent,
        KernelReadOperation::Getattr { serial }
        | KernelReadOperation::Data { serial }
        | KernelReadOperation::Open { serial, .. }
        | KernelReadOperation::Opendir { serial } => serial,
    };
    if wanted != protected {
        return Err(OverlayError::Stale);
    }
    let mut eval = Eval {
        rows,
        facts,
        root: mount.root_serial(),
        open_serial: None,
        kernel: None,
        needs: Vec::new(),
    };
    let decision = match decide(operation, &mut eval) {
        Ok(Some(inode)) => KernelReadDecision::Value(inode),
        Ok(None) if !eval.needs.is_empty() => KernelReadDecision::Needs(eval.needs),
        Ok(None) => KernelReadDecision::Failed(
            ContentError::InvalidRecord("empty kernel read needs").into(),
        ),
        Err(WorkspaceError::Overlay(error)) => return Err(error),
        Err(WorkspaceError::Refused(refusal)) => KernelReadDecision::Refused(refusal),
        Err(error) => KernelReadDecision::Failed(error),
    };
    Ok(match decision {
        KernelReadDecision::Needs(_) => KernelDecision::Needs(decision),
        KernelReadDecision::Value(ref inode) => KernelDecision::Finished {
            inode: Some(inode.clone()),
            value: decision,
        },
        _ => KernelDecision::Finished {
            inode: None,
            value: decision,
        },
    })
}
fn decide(operation: &KernelReadOperation, eval: &mut Eval<'_>) -> WorkspaceResult<Option<Inode>> {
    match operation {
        // The source's independent read reference permits removed metadata.
        KernelReadOperation::Getattr { serial } | KernelReadOperation::Data { serial } => {
            eval.target(*serial)
        }
        KernelReadOperation::Opendir { serial } => eval.directory(*serial),
        // The kernel names an inode it still references. A file whose
        // last name is gone stays openable under that reference, so a
        // path open racing an unlink or replacement gets the old file.
        KernelReadOperation::Open { serial, .. } => {
            let inode = eval.target(*serial)?;
            match inode {
                Some(inode) if inode.kind == InodeKind::File => Ok(Some(inode)),
                Some(inode) if inode.kind == InodeKind::Directory => {
                    Err(WorkspaceError::Refused(Refusal::IsDirectory))
                }
                Some(_) => Err(WorkspaceError::Refused(Refusal::Invalid)),
                None => Ok(None),
            }
        }
        KernelReadOperation::Lookup { parent, name } => {
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
