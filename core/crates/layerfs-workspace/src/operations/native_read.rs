//! Resumable native stat/lookup decisions using the ordinary Workspace evaluator.
use crate::{eval::Eval, BaseFacts, Need, Refusal, ViewStat, WorkspaceError, WorkspaceResult};
use layerfs_content::{filesystem::PathName, ContentError};
use layerfs_overlay::{
    Inode, InodeKind, NativeDecision, NativeDirectory, NativeMount, NativeObservation, OpenFile,
    OverlayError, OverlayResult, SourceRows,
};
use std::{fmt, sync::Arc};

#[derive(Clone, Debug)]
pub enum NativeReadOperation {
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
pub enum NativeReadDecision {
    Needs(Vec<Need>),
    Value(Inode),
    Refused(Refusal),
    Failed(WorkspaceError),
}
pub type NativeReadOutcome = NativeObservation<NativeReadDecision>;

#[derive(Debug)]
pub struct NativeReadValue {
    pub stat: ViewStat,
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
/// One evaluation of a native read over current rows and supplied facts.
pub(crate) fn decide_read(
    operation: &NativeReadOperation,
    facts: &BaseFacts,
    mount: NativeMount,
    rows: SourceRows<'_>,
    protected: u64,
) -> OverlayResult<NativeDecision<NativeReadDecision>> {
    let wanted = match *operation {
        NativeReadOperation::Lookup { parent, .. } => parent,
        NativeReadOperation::Getattr { serial }
        | NativeReadOperation::Data { serial }
        | NativeReadOperation::Open { serial, .. }
        | NativeReadOperation::Opendir { serial } => serial,
    };
    if wanted != protected {
        return Err(OverlayError::Stale);
    }
    let mut eval = Eval {
        rows,
        facts,
        root: mount.root_serial(),
        open_serial: None,
        native: None,
        needs: Vec::new(),
    };
    let decision = match decide(operation, &mut eval) {
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
fn decide(operation: &NativeReadOperation, eval: &mut Eval<'_>) -> WorkspaceResult<Option<Inode>> {
    match operation {
        // The source's independent read reference permits removed metadata.
        NativeReadOperation::Getattr { serial } | NativeReadOperation::Data { serial } => {
            eval.target(*serial)
        }
        NativeReadOperation::Opendir { serial } => eval.directory(*serial),
        // The kernel names an inode it still references. A file whose
        // last name is gone stays openable under that reference, so a
        // path open racing an unlink or replacement gets the old file.
        NativeReadOperation::Open { serial, .. } => {
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
