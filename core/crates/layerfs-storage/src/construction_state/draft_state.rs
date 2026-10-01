//! Fixed private draft owner and bounded expected/proposed transition custody.
use crate::error::{StorageError, StorageResult};
use layerfs_content::file::edit::{DraftScope, DraftStats};
use layerfs_content::ObjectId;
use std::cell::Cell;

/// Actual retained first-party draft provider working class, not full Save credit.
pub(crate) const WORKING_BYTES: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Ledger {
    pub(crate) records: u64,
    pub(crate) bytes: usize,
    pub(crate) next_job: u64,
    pub(crate) selected: Option<ObjectId>,
    pub(crate) ended: bool,
}
impl Default for Ledger {
    fn default() -> Self {
        Self {
            records: 0,
            bytes: 0,
            next_job: 1,
            selected: None,
            ended: false,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Header {
    pub(crate) id: ObjectId,
    pub(crate) form: u8,
    pub(crate) role: u8,
    pub(crate) stage: u8,
    pub(crate) references: u16,
    pub(crate) predecessors: u8,
    pub(crate) cursor: u16,
    pub(crate) charge: usize,
    pub(crate) queued: Option<u64>,
    pub(crate) digest: [u8; 32],
}
#[derive(Clone, Debug)]
pub(crate) enum Effect {
    HeaderInsert(Header),
    HeaderChange {
        before: Header,
        after: Header,
    },
    HeaderDelete(Header),
    BodyInsert {
        id: ObjectId,
        body: Vec<u8>,
    },
    BodyDelete {
        id: ObjectId,
        body: Vec<u8>,
    },
    ReferenceInsert {
        id: ObjectId,
        ordinal: u16,
        child: ObjectId,
        linked: bool,
    },
    ReferenceDelete {
        id: ObjectId,
        ordinal: u16,
        child: ObjectId,
        linked: bool,
    },
    PredecessorInsert {
        id: ObjectId,
        ordinal: u8,
        value: ObjectId,
        provenance: u8,
    },
    PredecessorDelete {
        id: ObjectId,
        ordinal: u8,
        value: ObjectId,
        provenance: u8,
    },
    Count {
        id: ObjectId,
        before: Option<u64>,
        after: Option<u64>,
    },
    JobInsert {
        sequence: u64,
        id: ObjectId,
    },
    JobDelete {
        sequence: u64,
        id: ObjectId,
    },
    ResolveInsert {
        id: ObjectId,
        canonical: ObjectId,
    },
    ResolveDelete {
        id: ObjectId,
        canonical: ObjectId,
    },
    EmissionInsert {
        canonical: ObjectId,
        draft: ObjectId,
    },
    EmissionAccept {
        canonical: ObjectId,
        draft: ObjectId,
    },
    EmissionDelete {
        canonical: ObjectId,
    },
}
impl Effect {
    pub(crate) fn width(&self) -> usize {
        match self {
            Self::HeaderInsert(_) | Self::HeaderChange { .. } | Self::HeaderDelete(_) => 151 * 2,
            Self::BodyInsert { body, .. } | Self::BodyDelete { body, .. } => body.len() + 55,
            Self::ReferenceInsert { .. } | Self::ReferenceDelete { .. } => 89,
            Self::PredecessorInsert { .. } | Self::PredecessorDelete { .. } => 90,
            Self::ResolveInsert { .. } | Self::ResolveDelete { .. } => 87,
            Self::JobInsert { .. } | Self::JobDelete { .. } => 71,
            _ => 126,
        }
    }
}
pub(crate) struct Attempt {
    pub(crate) kind: &'static str,
    pub(crate) before: Ledger,
    pub(crate) after: Ledger,
    pub(crate) effects: Vec<Effect>,
}
pub(crate) struct Drafts {
    pub(crate) scope: DraftScope,
    pub(crate) ledger: Ledger,
    pub(crate) stats: DraftStats,
    pub(crate) failed: Cell<bool>,
    pub(crate) attempt: Option<Box<Attempt>>,
}
impl Drafts {
    pub(crate) fn new(scope: DraftScope) -> StorageResult<Box<Self>> {
        scope.capacity().validated()?;
        if std::mem::size_of::<Self>()
            + std::mem::size_of::<Attempt>()
            + 128 * std::mem::size_of::<Effect>()
            + 65_536
            + 32 * std::mem::size_of::<ObjectId>()
            + 32 * std::mem::size_of::<(ObjectId, u64)>()
            + 32 * std::mem::size_of::<layerfs_content::file::mapping::NodeSummary>()
            > WORKING_BYTES
        {
            return Err(StorageError::Integrity("draft compiled working shape"));
        }
        // Compiled fixed shape is checked before the box/allocation, within the
        // admitted1MiB provider working class; no second full Save reservation.
        Ok(Box::new(Self {
            scope,
            ledger: Ledger::default(),
            stats: DraftStats::default(),
            failed: Cell::new(false),
            attempt: None,
        }))
    }
    pub(crate) fn description(&self) -> String {
        match &self.attempt {
            Some(attempt) => format!(
                "draft {}; before={:?}; proposed={:?}; effects={:?}",
                attempt.kind, attempt.before, attempt.after, attempt.effects
            ),
            None => format!(
                "draft acknowledged={:?}; failed={}",
                self.ledger,
                self.failed.get()
            ),
        }
    }
}
