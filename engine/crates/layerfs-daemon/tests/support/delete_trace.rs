//! External forwarding count witness; no product hook or payload cache.
use layerfs_content::{
    AuthenticatedObjects, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_daemon::OwnerClient;
use layerfs_overlay::{
    CapturedReader, CapturedRunCursor, CapturedRunReply, IndexedOperationRecordChange,
    IndexedOperationRecordKey, IndexedOperationRecordScope, Inode,
};
use layerfs_workspace::{
    OperationRecordApply, OperationRecordReply, OverlayCapturedRuns, OverlayOperationRecords,
    WorkspaceResult,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

#[derive(Debug)]
pub enum Event {
    Get(IndexedOperationRecordKey, bool),
    Contains(u32),
    Apply,
    First(u32, Vec<[u8; 32]>),
    After,
}
pub struct Records<'a> {
    pub inner: &'a OwnerClient,
    pub events: RefCell<Vec<Event>>,
}
impl OverlayCapturedRuns for Records<'_> {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<Inode>> {
        self.inner.captured_inode(reader, serial)
    }
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply> {
        self.inner.captured_run_step(cursor)
    }
}
impl OverlayOperationRecords for Records<'_> {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>> {
        self.events.borrow_mut().push(Event::Contains(key.kind));
        self.inner.operation_record_contains(scope, key)
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>> {
        let reply = self.inner.operation_record_get(scope, key)?;
        let zero = reply
            .value
            .as_ref()
            .is_none_or(|v| v.len() == 8 && v.iter().all(|b| *b == 0));
        self.events.borrow_mut().push(Event::Get(key, zero));
        Ok(reply)
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>> {
        self.events.borrow_mut().push(Event::Apply);
        self.inner.operation_record_apply(scope, changes)
    }
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        let reply = self.inner.operation_record_keys(scope, kind, excluded)?;
        self.events
            .borrow_mut()
            .push(Event::First(kind, reply.value.clone()));
        Ok(reply)
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.events.borrow_mut().push(Event::After);
        self.inner.operation_record_keys_after(scope, kind, after)
    }
}
impl Records<'_> {
    pub fn counts(&self) -> (u64, u64, BTreeMap<u32, u64>) {
        let events = self.events.borrow();
        let mut candidates = BTreeSet::new();
        let mut gets = BTreeMap::new();
        let mut kinds = BTreeMap::new();
        let (mut settled, mut duplicates) = (0, 0);
        for event in events.iter() {
            match event {
                Event::First(kind @ (5 | 6), keys) => {
                    candidates = keys.iter().map(|key| (kind - 2, *key)).collect();
                    gets.clear();
                    settled += keys.len() as u64;
                }
                Event::Apply => gets.clear(),
                Event::Get(key, zero) => {
                    *kinds.entry(key.kind).or_default() += 1;
                    if *zero && candidates.contains(&(key.kind, key.key)) {
                        let count = gets.entry((key.kind, key.key)).or_insert(0);
                        *count += 1;
                        if *count == 2 {
                            duplicates += 1;
                        }
                    }
                }
                Event::Contains(kind) => *kinds.entry(100 + kind).or_default() += 1,
                _ => {}
            }
        }
        (settled, duplicates, kinds)
    }
}
#[derive(Default)]
pub struct Objects(pub BTreeMap<ObjectId, Vec<u8>>);
impl FinalizedConsumer for Objects {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.insert(object.id(), object.canonical().to_vec());
        Ok(())
    }
}
pub struct Combined<'a> {
    pub source: &'a dyn AuthenticatedObjects,
    pub output: &'a Objects,
}
impl AuthenticatedObjects for Combined<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| match self.output.0.get(id) {
                Some(bytes) => Ok(bytes.clone()),
                None => {
                    let mut values = self.source.read_canonical_batch(&[*id])?;
                    if values.len() != 1 {
                        return Err(layerfs_content::ContentError::InvalidRecord(
                            "diagnostic source cardinality",
                        ));
                    }
                    Ok(values.pop().unwrap())
                }
            })
            .collect()
    }
}
