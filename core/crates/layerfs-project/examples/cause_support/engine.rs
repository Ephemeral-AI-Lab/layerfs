//! Candidate public-API facade; baseline facade is a separate frozen adapter.
use layerfs_content::{
    AuthenticatedObjects, ContentError, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_storage::{Storage as ProductStorage, StorageError, StoragePolicy, StorageResult};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
#[derive(Debug)]
pub struct Diagnostics;
pub struct Storage {
    pub inner: ProductStorage,
    handles: Arc<layerfs_persistence::Handles>,
    records: RefCell<Vec<String>>,
    inventory: RefCell<BTreeMap<ObjectId, (u8, usize)>>,
}
impl Storage {
    pub fn new(inner: ProductStorage, handles: Arc<layerfs_persistence::Handles>) -> Self {
        Self {
            inner,
            handles,
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
        Ok(Box::new(self.inner.reader()?))
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
        Ok(Save {
            sql_before: self
                .handles
                .diagnostics()
                .map_err(|e| StorageError::Io(std::io::Error::other(e)))?,
            inner: self.inner.begin_save()?,
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
    inner: layerfs_storage::Save<'a>,
    sql_before: layerfs_persistence::SqlWork,
    owner: &'a Storage,
    failure: RefCell<Option<StorageError>>,
}
impl<'a> Save<'a> {
    pub fn accept(&self, o: FinalizedObject) -> StorageResult<()> {
        self.owner.accepted(&o)?;
        self.inner.accept(o)
    }
    pub fn sink<'b>(&'b self) -> Sink<'b, 'a> {
        Sink(self)
    }
    pub fn take_failure(&self) -> Option<StorageError> {
        self.failure.borrow_mut().take()
    }
    pub fn finish(self) -> StorageResult<()> {
        let outer = crate::observer::span(crate::observer::Step::SaveFinish);
        let outcome = self.inner.finish()?;
        drop(outer);
        let sql = self
            .owner
            .handles
            .diagnostics()
            .map_err(|e| StorageError::Io(std::io::Error::other(e)))?;
        let before = self.sql_before;
        eprintln!("CAUSE_CANDIDATE_SAVE_SQL statements={} vm_steps_UNQUALIFIED_AFTER_TRACE={} bound_bytes={} statement_ns={} commit_ns={} transactions={} write_transactions={} commits={} write_commits={} sealed_inserts={} sealed_body_bytes={}",sql.statements-before.statements,sql.vm_steps-before.vm_steps,sql.bound_bytes-before.bound_bytes,sql.statement_ns-before.statement_ns,sql.commit_ns-before.commit_ns,sql.transactions-before.transactions,sql.write_transactions-before.write_transactions,sql.commits-before.commits,sql.write_commits-before.write_commits,sql.sealed_inserts-before.sealed_inserts,sql.sealed_body_bytes-before.sealed_body_bytes);
        let history = self.owner.inner.save_work();
        let work = history.recent[((history.completed - 1) % 4) as usize];
        let p = work.selection;
        let stages = work
            .stages
            .iter()
            .map(|row| format!("{{\"calls\":{},\"wall_ns\":{}}}", row.calls, row.wall_ns))
            .collect::<Vec<_>>()
            .join(",");
        self.owner.records.borrow_mut().push(format!("{{\"inserted\":{},\"reused\":{},\"packs\":{},\"canonical_bytes\":{},\"full_ns\":{},\"delta_ns\":{},\"group_ns\":{},\"cost_ns\":{},\"eligible_ns\":{},\"acquire_ns\":{},\"probe_ns\":{},\"inclusive_stages\":[{}]}}",outcome.inserted,outcome.reused,outcome.packs,outcome.canonical_bytes,p.full_ns,p.delta_ns,p.group_ns,p.resolve.cost_ns,p.resolve.eligible_ns,p.resolve.acquire_ns,p.diag.probe_ns,stages));
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
