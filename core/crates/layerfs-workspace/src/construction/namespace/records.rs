//! One first-original-failure owner, the sealed record kinds and their codecs.
use crate::construction::outcome::CapturedNamespaceWork;
use crate::{
    CapturedFileCustody, IndexedConstructionRecords, OverlayOperationRecords, WorkspaceError,
};
use layerfs_content::{
    filesystem::{FilesystemRootId, InodeScope},
    ContentError, ContentResult, EditRecordApply, EditRecordChange, EditRecordExpected,
    EditRecordKey, IndexedEditBacking, ObjectId,
};
use layerfs_overlay::{CapturedReader, OperationOwner, OPERATION_RECORD_BYTES};
use std::{cell::RefCell, ops::Deref};

// Producer kinds 0x434E_0000..=0x434E_000F share the operation's file scope 0
// with Content's filesystem kinds; each changed file has its own file scope.
pub(crate) const CONTEXT: u32 = 0x434E_0000;
/// Parent serial to its exact captured change count.
pub(crate) const HEADER: u32 = 0x434E_0001;
/// Serial to its typed final inode value.
pub(crate) const VALUE: u32 = 0x434E_0002;
/// Fresh serial to its dense rank in serial order.
pub(crate) const FRESH: u32 = 0x434E_0003;

/// Exact inputs of one attempt, fixed before the first record is written.
#[derive(Clone, Copy)]
pub(crate) struct Facts {
    pub reader: CapturedReader,
    pub operation: OperationOwner,
    pub scope: InodeScope,
    pub profile: ObjectId,
    pub root: FilesystemRootId,
    pub root_serial: u64,
}
/// The three declared row totals the sealed context binds.
#[derive(Clone, Copy, Default)]
pub(crate) struct Totals {
    pub headers: u64,
    pub values: u64,
    pub fresh: u64,
}
pub(crate) struct State<'a, P: OverlayOperationRecords + ?Sized> {
    pub records: IndexedConstructionRecords<'a, P>,
    pub failure: Option<WorkspaceError>,
    pub file: Option<Box<CapturedFileCustody>>,
    pub work: CapturedNamespaceWork,
}
pub(crate) struct Shared<'a, P: OverlayOperationRecords + ?Sized>(RefCell<State<'a, P>>);
impl<'a, P: OverlayOperationRecords + ?Sized> Shared<'a, P> {
    pub fn new(state: State<'a, P>) -> Self {
        Self(RefCell::new(state))
    }
    pub fn into_inner(self) -> State<'a, P> {
        self.0.into_inner()
    }
}
impl<'a, P: OverlayOperationRecords + ?Sized> Deref for Shared<'a, P> {
    type Target = RefCell<State<'a, P>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
/// Work counters saturate: they are observations, never an admission limit.
pub(crate) fn add(counter: &mut u64, value: u64) {
    *counter = counter.saturating_add(value);
}
impl<P: OverlayOperationRecords + ?Sized> State<'_, P> {
    fn vacant(&self) -> bool {
        self.failure.is_none() && self.file.is_none() && self.records.failure().is_none()
    }
    pub fn ready(&self) -> ContentResult<()> {
        if self.vacant() {
            Ok(())
        } else {
            Err(ContentError::ProviderFailure {
                what: "captured namespace terminal",
            })
        }
    }
    pub fn original(&mut self, original: WorkspaceError) -> ContentError {
        if self.vacant() {
            self.failure = Some(original);
        }
        ContentError::ProviderFailure {
            what: "captured namespace original refusal",
        }
    }
    pub fn content(&mut self, original: ContentError) -> ContentError {
        if self.vacant() {
            self.failure = Some(WorkspaceError::Content(original.clone()));
        }
        original
    }
    /// Retains the failing file's whole custody; its own failure stays inside.
    pub fn file(&mut self, custody: CapturedFileCustody) -> ContentError {
        if self.vacant() {
            self.file = Some(Box::new(custody));
        }
        ContentError::ProviderFailure {
            what: "captured namespace file refusal",
        }
    }
    pub fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<()> {
        self.ready()?;
        add(&mut self.work.record_jobs, 1);
        match self.records.apply(changes) {
            Ok(EditRecordApply::Applied) => Ok(()),
            Ok(EditRecordApply::NotApplied { .. }) => Err(ContentError::ProviderFailure {
                what: "captured namespace deciding record refusal",
            }),
            Err(original) => Err(self.content(original)),
        }
    }
    pub fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.ready()?;
        add(&mut self.work.record_jobs, 1);
        let result = self.records.get(key);
        let value = result.map_err(|error| self.content(error))?;
        if let Some(value) = &value {
            if value.capacity() > OPERATION_RECORD_BYTES {
                return Err(self.content(ContentError::BoundedCapacityExceeded {
                    what: "captured record returned capacity",
                    limit: OPERATION_RECORD_BYTES as u64,
                    actual: value.capacity() as u64,
                }));
            }
        }
        Ok(value)
    }
    pub fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        self.ready()?;
        add(&mut self.work.record_jobs, 1);
        let result = self.records.contains(key);
        result.map_err(|error| self.content(error))
    }
    pub fn keys_after(
        &mut self,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.ready()?;
        add(&mut self.work.record_jobs, 1);
        let result = self.records.keys_after(kind, after);
        result.map_err(|error| self.content(error))
    }
}
/// The serial is the last eight bytes, big-endian: key order is serial order.
pub(crate) fn key(kind: u32, serial: u64) -> EditRecordKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&serial.to_be_bytes());
    EditRecordKey { kind, key }
}
pub(crate) fn serial(key: &[u8; 32]) -> ContentResult<u64> {
    if key[..24].iter().any(|byte| *byte != 0) {
        return Err(ContentError::InvalidRecord("captured namespace record key"));
    }
    key[24..]
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| ContentError::UnexpectedEof)
}
pub(crate) fn change(kind: u32, serial: u64, value: Vec<u8>) -> EditRecordChange {
    EditRecordChange {
        key: key(kind, serial),
        expected: EditRecordExpected::Missing,
        value: Some(value),
    }
}
pub(crate) fn number(value: u64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}
pub(crate) fn read_number(value: &[u8], what: &'static str) -> ContentResult<u64> {
    value
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| ContentError::InvalidRecord(what))
}
/// Exact reader identity, base and totals of this attempt's sealed records.
pub(crate) fn context(facts: Facts, totals: Totals, sealed: bool) -> Vec<u8> {
    let mut value = Vec::with_capacity(170);
    value.extend_from_slice(&[1, u8::from(sealed)]);
    for number in [
        facts.reader.capture().route().namespace() as u64,
        facts.reader.owner_id(),
        facts.reader.installed_floor(),
        facts.reader.capture().generation.number() as u64,
        facts.reader.capture().revision as u64,
        facts.root_serial,
        totals.headers,
        totals.values,
        totals.fresh,
    ] {
        value.extend_from_slice(&number.to_be_bytes());
    }
    value.extend_from_slice(&facts.reader.root());
    value.extend_from_slice(facts.scope.object().as_bytes());
    value.extend_from_slice(facts.profile.as_bytes());
    value
}
pub(crate) fn verify<P: OverlayOperationRecords + ?Sized>(
    shared: &Shared<'_, P>,
    expected: &[u8],
) -> ContentResult<()> {
    let mut owner = shared.borrow_mut();
    let actual = owner.get(key(CONTEXT, 0))?;
    if actual.as_deref() != Some(expected) {
        return Err(owner.content(ContentError::InvalidRecord("captured namespace context")));
    }
    Ok(())
}
/// Content's filesystem state over the same scope and the same single owner.
pub(crate) struct Backing<'s, 'a, P: OverlayOperationRecords + ?Sized>(pub &'s Shared<'a, P>);
impl<P: OverlayOperationRecords + ?Sized> IndexedEditBacking for Backing<'_, '_, P> {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.contains(key);
        result.map_err(|error| owner.content(error))
    }
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.get(key);
        let value = result.map_err(|error| owner.content(error))?;
        if let Some(value) = &value {
            if value.capacity() > OPERATION_RECORD_BYTES {
                return Err(owner.content(ContentError::BoundedCapacityExceeded {
                    what: "captured record returned capacity",
                    limit: OPERATION_RECORD_BYTES as u64,
                    actual: value.capacity() as u64,
                }));
            }
        }
        Ok(value)
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.apply(changes);
        result.map_err(|error| owner.content(error))
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.first_keys(kind, excluded);
        result.map_err(|error| owner.content(error))
    }
    fn keys_after(&mut self, kind: u32, after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        let mut owner = self.0.borrow_mut();
        owner.ready()?;
        let result = owner.records.keys_after(kind, after);
        result.map_err(|error| owner.content(error))
    }
}
