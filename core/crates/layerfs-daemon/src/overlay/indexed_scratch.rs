//! Credited short raw-record jobs, with no Content algorithm or provider I/O.
use layerfs_overlay::{
    indexed_changes_bytes, IndexedApply, IndexedChange, IndexedKey, IndexedScope, Overlay,
    OverlayError, OverlayResult, Route, PAGE_ROWS, SCRATCH_BYTES,
};

/// Complete bounded inputs for the existing Scratch service class.
#[derive(Debug)]
pub enum IndexedScratchJob {
    Contains {
        scope: IndexedScope,
        key: IndexedKey,
    },
    Get {
        scope: IndexedScope,
        key: IndexedKey,
    },
    Apply {
        scope: IndexedScope,
        changes: Vec<IndexedChange>,
    },
    FirstKeys {
        scope: IndexedScope,
        kind: u32,
        excluded_root: [u8; 32],
    },
    FirstKeysAll {
        scope: IndexedScope,
        kind: u32,
    },
    KeysAfter {
        scope: IndexedScope,
        kind: u32,
        after: Option<[u8; 32]>,
    },
}

/// Original deciding outcomes stay inside the existing credited Completion.
#[derive(Debug)]
pub enum IndexedScratchReply {
    Contains(bool),
    Value(Option<Vec<u8>>),
    Applied(IndexedApply),
    Keys(Vec<[u8; 32]>),
}

impl IndexedScratchJob {
    pub(crate) fn charge(&self) -> Option<usize> {
        let (input, reply) = match self {
            Self::Contains { .. } => (0, 0),
            Self::Get { .. } => (0, SCRATCH_BYTES),
            Self::Apply { changes, .. } => {
                let used = indexed_changes_bytes(changes).ok()?;
                let spare = changes
                    .capacity()
                    .checked_sub(changes.len())?
                    .checked_mul(std::mem::size_of::<IndexedChange>())?;
                let input = used.checked_add(spare)?;
                if input > SCRATCH_BYTES {
                    return None;
                }
                (input, SCRATCH_BYTES)
            }
            Self::FirstKeys { .. } | Self::FirstKeysAll { .. } | Self::KeysAfter { .. } => {
                (0, PAGE_ROWS * 32)
            }
        };
        std::mem::size_of::<Self>()
            .checked_add(std::mem::size_of::<IndexedScratchReply>())?
            .checked_add(input)?
            .checked_add(reply)
    }

    pub(crate) fn perform(self, db: &Overlay, route: Route) -> OverlayResult<IndexedScratchReply> {
        let scope = match &self {
            Self::Contains { scope, .. }
            | Self::Get { scope, .. }
            | Self::Apply { scope, .. }
            | Self::FirstKeys { scope, .. }
            | Self::FirstKeysAll { scope, .. }
            | Self::KeysAfter { scope, .. } => *scope,
        };
        if scope.owner.route() != route {
            return Err(OverlayError::Stale);
        }
        match self {
            Self::Contains { key, .. } => db
                .indexed_scratch_contains(scope, key)
                .map(IndexedScratchReply::Contains),
            Self::Get { key, .. } => db
                .indexed_scratch_get(scope, key)
                .map(IndexedScratchReply::Value),
            Self::Apply { changes, .. } => db
                .indexed_scratch_apply(scope, &changes)
                .map(IndexedScratchReply::Applied),
            Self::FirstKeys {
                kind,
                excluded_root,
                ..
            } => db
                .indexed_scratch_first_keys(scope, kind, excluded_root)
                .map(IndexedScratchReply::Keys),
            Self::FirstKeysAll { kind, .. } => db
                .indexed_scratch_keys(scope, kind, None)
                .map(IndexedScratchReply::Keys),
            Self::KeysAfter { kind, after, .. } => db
                .indexed_scratch_keys_after(scope, kind, after)
                .map(IndexedScratchReply::Keys),
        }
    }
}
