//! Unmodified Phase4.5 public C2 save facade for the shared diagnostic caller.
use layerfs_content::{
    AuthenticatedObjects, ContentError, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_storage::{
    SaveOperation, StorageError, StoragePolicy, StorageResult, Store, StoreProvider,
};
use layerfs_telemetry::timer::Timing;
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Debug)]
pub struct Diagnostics;
pub struct Storage {
    pub inner: Store,
    records: RefCell<Vec<String>>,
    inventory: RefCell<BTreeMap<ObjectId, (u8, usize)>>,
}
impl Storage {
    pub fn new(inner: Store) -> Self {
        Self {
            inner,
            records: RefCell::new(Vec::new()),
            inventory: RefCell::new(BTreeMap::new()),
        }
    }
    pub fn policy(&self) -> StoragePolicy {
        self.inner.policy()
    }
    pub fn diagnostics(&self) -> Diagnostics {
        Diagnostics
    }
    pub fn reader(&self) -> StorageResult<Box<dyn AuthenticatedObjects + '_>> {
        Ok(Box::new(StoreProvider::new(&self.inner)))
    }
    pub fn begin_save(&self) -> StorageResult<Save<'_>> {
        if self.records.borrow().len() == 3 {
            return Err(StorageError::CapacityExceeded {
                what: "cause.save_records",
                limit: 3,
                actual: 4,
            });
        }
        let _span = crate::observer::span(crate::observer::Step::SaveBegin);
        let inner = Timing::disabled("cause.begin", |t| self.inner.begin_save(t.child("save"))).0?;
        Ok(Save {
            inner: RefCell::new(Some(inner)),
            owner: self,
            failure: RefCell::new(None),
        })
    }
    pub fn records(&self) -> String {
        self.records.borrow().join(",")
    }
    pub fn census(&self) -> (usize, u64, String) {
        let inventory = self.inventory.borrow();
        let mut hash = crate::digest::Sha256::new();
        for (id, (role, len)) in inventory.iter() {
            hash.update(id.as_bytes());
            hash.update(&[*role]);
            hash.update(&(*len as u64).to_le_bytes());
        }
        (
            inventory.len(),
            inventory.values().map(|(_, len)| *len as u64).sum(),
            crate::digest::hex(&hash.finish()),
        )
    }
    fn accepted(&self, o: &FinalizedObject) -> StorageResult<()> {
        let inventory = self.inventory.borrow();
        if !inventory.contains_key(&o.id()) && inventory.len() == 8191 {
            return Err(StorageError::CapacityExceeded {
                what: "cause.inventory_ids",
                limit: 8191,
                actual: 8192,
            });
        }
        drop(inventory);
        self.inventory
            .borrow_mut()
            .insert(o.id(), (o.role().code(), o.canonical_len()));
        Ok(())
    }
}
pub struct Save<'a> {
    inner: RefCell<Option<SaveOperation>>,
    owner: &'a Storage,
    failure: RefCell<Option<StorageError>>,
}
impl<'a> Save<'a> {
    pub fn accept(&self, o: FinalizedObject) -> StorageResult<()> {
        self.owner.accepted(&o)?;
        self.inner
            .borrow_mut()
            .as_mut()
            .expect("live save")
            .accept(o)
    }
    pub fn sink<'b>(&'b self) -> Sink<'b, 'a> {
        Sink(self)
    }
    pub fn take_failure(&self) -> Option<StorageError> {
        self.failure.borrow_mut().take()
    }
    pub fn finish(self) -> StorageResult<()> {
        let outer = crate::observer::span(crate::observer::Step::SaveFinish);
        let inner = self.inner.into_inner().expect("live save");
        let o = Timing::disabled("cause.finish", |t| inner.finish(t.child("save"))).0?;
        drop(outer);
        let p = o.profile;
        self.owner.records.borrow_mut().push(format!("{{\"inserted\":{},\"reused\":{},\"packs\":{},\"pack_appends\":{},\"pack_bytes_written\":{},\"write_commits\":{},\"object_insert_statements\":{},\"presence_queries\":{},\"full_ns\":{},\"delta_ns\":{},\"cost_ns\":{},\"eligible_ns\":{},\"acquire_ns\":{},\"exact_reuse_ns\":{},\"pooled_resolve_ns\":{},\"group_ns\":{},\"placement_select_ns\":{},\"sql_ns\":{},\"transaction_cadence_ns\":{}}}",o.inserted,o.reused,o.packs_created,o.pack_appends,o.pack_bytes_written,o.commits,o.statements,o.presence_queries,p.full_ns,p.delta_ns,p.resolve.cost_ns,p.resolve.eligible_ns,p.resolve.acquire_ns,p.resolve.reuse_ns,p.resolve.pooled_ns,p.group_ns,p.place_ns,p.sql_ns,p.commit_ns));
        eprintln!("CAUSE_REFERENCE_SAVE {:?}", o);
        Ok(())
    }
}
pub struct Sink<'a, 'b>(&'a Save<'b>);
impl FinalizedConsumer for Sink<'_, '_> {
    fn accept(&mut self, o: FinalizedObject) -> Result<(), ContentError> {
        self.0.accept(o).map_err(|e| {
            *self.0.failure.borrow_mut() = Some(e);
            ContentError::OutputRejected
        })
    }
}
