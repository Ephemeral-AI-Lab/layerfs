//! One first-original-failure owner shared by all short record/read methods.
use super::owner::CapturedFileWork;
use crate::{IndexedConstructionRecords, OverlayScratch, WorkspaceError};
use layerfs_content::{
    ContentError, ContentResult, Edit, EditRecordApply, EditRecordChange, EditRecordExpected,
    EditRecordKey, IndexedEditBacking,
};
use std::{cell::RefCell, ops::Deref};

pub(super) const CONTEXT: u32 = 0x4346_0000;
pub(super) const EDITS: u32 = 0x4346_0001;
pub(super) struct State<'a, P: OverlayScratch + ?Sized> {
    pub records: IndexedConstructionRecords<'a, P>,
    pub failure: Option<WorkspaceError>,
    pub work: CapturedFileWork,
    pub cache: Option<(usize, Edit)>,
}
pub(super) struct Shared<'a, P: OverlayScratch + ?Sized>(RefCell<State<'a, P>>);
impl<'a, P: OverlayScratch + ?Sized> Shared<'a, P> {
    pub fn new(state: State<'a, P>) -> Self {
        Self(RefCell::new(state))
    }
    pub fn into_inner(self) -> State<'a, P> {
        self.0.into_inner()
    }
}
impl<'a, P: OverlayScratch + ?Sized> Deref for Shared<'a, P> {
    type Target = RefCell<State<'a, P>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<P: OverlayScratch + ?Sized> State<'_, P> {
    pub fn ready(&self) -> ContentResult<()> {
        if self.failure.is_some() || self.records.failure().is_some() {
            Err(ContentError::ProviderFailure {
                what: "captured file terminal",
            })
        } else {
            Ok(())
        }
    }
    pub fn original(&mut self, original: WorkspaceError) -> ContentError {
        if self.failure.is_none() && self.records.failure().is_none() {
            self.failure = Some(original);
        }
        ContentError::ProviderFailure {
            what: "captured file original refusal",
        }
    }
    pub fn content(&mut self, original: ContentError) -> ContentError {
        if self.failure.is_none() && self.records.failure().is_none() {
            self.failure = Some(WorkspaceError::Content(original.clone()));
        }
        original
    }
    pub fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<()> {
        self.ready()?;
        match self.records.apply(changes) {
            Ok(EditRecordApply::Applied) => Ok(()),
            Ok(EditRecordApply::NotApplied { .. }) => Err(ContentError::ProviderFailure {
                what: "captured file deciding record refusal",
            }),
            Err(original) => Err(self.content(original)),
        }
    }
    pub fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.ready()?;
        let result = self.records.get(key);
        let value = result.map_err(|error| self.content(error))?;
        if let Some(value) = &value {
            if value.capacity() > layerfs_overlay::SCRATCH_BYTES {
                return Err(self.content(ContentError::BoundedCapacityExceeded {
                    what: "captured record returned capacity",
                    limit: layerfs_overlay::SCRATCH_BYTES as u64,
                    actual: value.capacity() as u64,
                }));
            }
        }
        Ok(value)
    }
}
pub(super) fn key(kind: u32, number: u64) -> EditRecordKey {
    let mut key = [0; 32];
    key[..8].copy_from_slice(&number.to_be_bytes());
    EditRecordKey { kind, key }
}
pub(super) fn change(kind: u32, number: u64, value: Vec<u8>) -> EditRecordChange {
    EditRecordChange {
        key: key(kind, number),
        expected: EditRecordExpected::Missing,
        value: Some(value),
    }
}
pub(super) fn encode(edit: Edit) -> Vec<u8> {
    let mut value = Vec::with_capacity(25);
    value.push(1);
    value.extend_from_slice(&edit.start().to_be_bytes());
    value.extend_from_slice(&edit.end().to_be_bytes());
    value.extend_from_slice(&edit.replacement_len().to_be_bytes());
    value
}
pub(super) fn decode(value: &[u8]) -> ContentResult<Edit> {
    if value.len() != 25 || value[0] != 1 {
        return Err(ContentError::InvalidRecord("captured file edit record"));
    }
    let read = |at| {
        value[at..at + 8]
            .try_into()
            .map(u64::from_be_bytes)
            .map_err(|_| ContentError::UnexpectedEof)
    };
    let edit = Edit::new(read(1)?, read(9)?, read(17)?);
    if edit.start() > edit.end() {
        return Err(ContentError::InvalidEdit {
            what: "captured file edit range",
        });
    }
    Ok(edit)
}
