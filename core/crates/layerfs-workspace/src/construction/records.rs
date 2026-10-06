//! Content-owned meanings over explicit local operation/file custody.
use crate::{OverlayScratch, ScratchApply, ScratchCopies, ScratchReply, WorkspaceError};
use layerfs_content::{
    ContentError, ContentResult, EditRecordApply, EditRecordChange, EditRecordExpected,
    EditRecordKey, IndexedEditBacking,
};
use layerfs_overlay::{ExpectedValue, IndexedChange, IndexedKey, IndexedScope, SCRATCH_BYTES};
use std::fmt;

/// Source-scoped conversion/service-copy observations, not whole-operation
/// copies, SQL bytes, phase residency or eligible debt. Saturation is explicit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EditBackingWork {
    pub calls: u64,
    pub converted_changes: u64,
    pub conversion_allocations: u64,
    pub peak_conversion_heap_bytes: usize,
    pub copied_reply_bytes: u64,
    pub reply_allocations: u64,
    pub peak_reply_capacity_bytes: usize,
    pub terminal_calls: u64,
    pub saturated: bool,
}
fn add(counter: &mut u64, value: u64, saturated: &mut bool) {
    match counter.checked_add(value) {
        Some(next) => *counter = next,
        None => {
            *counter = u64::MAX;
            *saturated = true;
        }
    }
}
/// Original pre-admission input retained if its actual capacity exceeds the
/// fixed job envelope. It has not been submitted or partially converted.
#[derive(Debug)]
pub struct EditInputRefusal {
    pub retained_bytes: Option<usize>,
    pub changes: Vec<EditRecordChange>,
}
impl fmt::Display for EditInputRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "indexed edit input refusal: retained={:?}, changes={}",
            self.retained_bytes,
            self.changes.len()
        )
    }
}
impl std::error::Error for EditInputRefusal {}
/// Explicit handoff of the scope and first original failure. Dropping this
/// value releases no engine operation owner; the caller performs real fences.
#[derive(Debug)]
pub struct EditBackingCustody {
    pub scope: IndexedScope,
    pub failure: Option<WorkspaceError>,
    pub work: EditBackingWork,
}
/// One explicit operation/file adapter. It acquires no owner or database and
/// never releases either on Drop. A first refusal permanently ends its requests.
pub struct IndexedEditRecords<'a, P: OverlayScratch + ?Sized> {
    provider: &'a P,
    scope: IndexedScope,
    failure: Option<WorkspaceError>,
    work: EditBackingWork,
}
impl<'a, P: OverlayScratch + ?Sized> IndexedEditRecords<'a, P> {
    pub fn new(provider: &'a P, scope: IndexedScope) -> Self {
        Self {
            provider,
            scope,
            failure: None,
            work: EditBackingWork::default(),
        }
    }
    pub const fn scope(&self) -> IndexedScope {
        self.scope
    }
    pub fn failure(&self) -> Option<&WorkspaceError> {
        self.failure.as_ref()
    }
    pub const fn work(&self) -> EditBackingWork {
        self.work
    }
    pub fn into_custody(self) -> EditBackingCustody {
        EditBackingCustody {
            scope: self.scope,
            failure: self.failure,
            work: self.work,
        }
    }
    fn ready(&mut self) -> ContentResult<()> {
        if self.failure.is_some() {
            add(&mut self.work.terminal_calls, 1, &mut self.work.saturated);
            Err(ContentError::ProviderFailure {
                what: "indexed edit backing terminal",
            })
        } else {
            Ok(())
        }
    }
    fn observe<T>(&mut self, result: Result<ScratchReply<T>, WorkspaceError>) -> ContentResult<T> {
        add(&mut self.work.calls, 1, &mut self.work.saturated);
        match result {
            Ok(ScratchReply { value, copies }) => {
                self.copies(copies);
                Ok(value)
            }
            Err(original) => {
                self.failure = Some(original);
                Err(ContentError::ProviderFailure {
                    what: "indexed edit backing original refusal",
                })
            }
        }
    }
    fn copies(&mut self, copies: ScratchCopies) {
        add(
            &mut self.work.copied_reply_bytes,
            copies.bytes,
            &mut self.work.saturated,
        );
        add(
            &mut self.work.reply_allocations,
            copies.allocations,
            &mut self.work.saturated,
        );
        self.work.peak_reply_capacity_bytes = self
            .work
            .peak_reply_capacity_bytes
            .max(copies.retained_bytes);
    }
}
fn key(key: EditRecordKey) -> IndexedKey {
    IndexedKey {
        kind: key.kind,
        key: key.key,
    }
}
fn retained(changes: &Vec<EditRecordChange>) -> Option<usize> {
    let mut bytes = changes
        .capacity()
        .checked_mul(std::mem::size_of::<EditRecordChange>())?;
    for change in changes {
        if let EditRecordExpected::ExactBytes(value) = &change.expected {
            bytes = bytes.checked_add(value.capacity())?;
        }
        if let Some(value) = &change.value {
            bytes = bytes.checked_add(value.capacity())?;
        }
    }
    Some(bytes)
}
impl<P: OverlayScratch + ?Sized> IndexedEditBacking for IndexedEditRecords<'_, P> {
    fn contains(&mut self, record: EditRecordKey) -> ContentResult<bool> {
        self.ready()?;
        self.observe(self.provider.scratch_contains(self.scope, key(record)))
    }
    fn get(&mut self, record: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.ready()?;
        self.observe(self.provider.scratch_get(self.scope, key(record)))
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        self.ready()?;
        let input = retained(&changes);
        let input_bytes = match input {
            Some(bytes) if bytes <= SCRATCH_BYTES => bytes,
            _ => {
                self.failure = Some(WorkspaceError::Service(Box::new(EditInputRefusal {
                    retained_bytes: input,
                    changes,
                })));
                return Err(ContentError::ProviderFailure {
                    what: "indexed edit input admission",
                });
            }
        };
        // The raw byte buffers move. The conversion temporarily retains both
        // bounded descriptor allocations; charge that overlap explicitly.
        let mut converted = Vec::with_capacity(changes.len());
        let capacity = converted.capacity() * std::mem::size_of::<IndexedChange>();
        self.work.peak_conversion_heap_bytes = self
            .work
            .peak_conversion_heap_bytes
            .max(input_bytes + capacity);
        add(
            &mut self.work.conversion_allocations,
            u64::from(!changes.is_empty()),
            &mut self.work.saturated,
        );
        add(
            &mut self.work.converted_changes,
            changes.len() as u64,
            &mut self.work.saturated,
        );
        for change in changes {
            converted.push(IndexedChange {
                key: key(change.key),
                expected: match change.expected {
                    EditRecordExpected::Missing => ExpectedValue::Missing,
                    EditRecordExpected::ExactBytes(bytes) => ExpectedValue::ExactBytes(bytes),
                },
                value: change.value,
            });
        }
        let outcome = self.observe(self.provider.scratch_apply(self.scope, converted))?;
        match outcome {
            ScratchApply::Applied => Ok(EditRecordApply::Applied),
            ScratchApply::NotApplied {
                index,
                key,
                actual,
                original,
            } => {
                self.failure = Some(original);
                Ok(EditRecordApply::NotApplied {
                    index,
                    key: EditRecordKey {
                        kind: key.kind,
                        key: key.key,
                    },
                    actual,
                })
            }
        }
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.ready()?;
        self.observe(self.provider.scratch_keys(self.scope, kind, excluded))
    }
}
