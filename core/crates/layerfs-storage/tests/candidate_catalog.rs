//! External raw-page catalog exercises the unchanged content-keyed candidate algorithm.
use layerfs_content::{encode_whole_file_payload, ObjectId};
use layerfs_storage::{
    encoding::delta::candidates::{
        signature, CandidateCatalog, CandidateRow, Candidates, INDEX_BYTES,
    },
    StorageError, StorageResult,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
#[derive(Default)]
struct Catalog {
    rows: RefCell<BTreeMap<u16, CandidateRow>>,
    reads: Cell<usize>,
    writes: Cell<usize>,
    fail_read: Cell<Option<usize>>,
    fail_write: Cell<Option<usize>>,
}
impl CandidateCatalog for Catalog {
    fn candidate_page(&self, after: u64, limit: usize) -> StorageResult<Vec<CandidateRow>> {
        assert_eq!(limit, 128);
        let call = self.reads.get() + 1;
        self.reads.set(call);
        if self.fail_read.get() == Some(call) {
            return Err(StorageError::Integrity("external page refusal"));
        }
        let mut rows: Vec<_> = self
            .rows
            .borrow()
            .values()
            .copied()
            .filter(|r| r.stamp > after)
            .collect();
        rows.sort_by_key(|r| r.stamp);
        rows.truncate(limit);
        Ok(rows)
    }
    fn write_candidates(&self, rows: &[CandidateRow]) -> StorageResult<()> {
        assert!(rows.len() <= 128);
        let call = self.writes.get() + 1;
        self.writes.set(call);
        if self.fail_write.get() == Some(call) {
            return Err(StorageError::Integrity("external staging refusal"));
        }
        for row in rows {
            self.rows.borrow_mut().insert(row.slot, *row);
        }
        Ok(())
    }
}
fn populated(catalog: &Catalog) -> (Candidates, Vec<(ObjectId, [u64; 8])>) {
    let mut index = Candidates::new().unwrap();
    index.reload_catalog(catalog).unwrap();
    let mut queries = Vec::new();
    for n in 0..300u32 {
        let raw: Vec<u8> = (0..512u32)
            .map(|i| ((i.wrapping_mul(73) ^ n.wrapping_mul(37)) % 251) as u8)
            .collect();
        let id = ObjectId::for_bytes(&encode_whole_file_payload(&raw).unwrap());
        let sig = signature(&raw);
        index.insert(id, sig);
        queries.push((id, sig));
    }
    (index, queries)
}
#[test]
fn raw_pages_rehydrate_existing_fold_reference_winners_and_ties() {
    let catalog = Catalog::default();
    let (mut original, queries) = populated(&catalog);
    let shared = signature(b"a sufficiently long shared signature for a deterministic tie");
    let a = ObjectId::for_bytes(&encode_whole_file_payload(b"first canonical identity").unwrap());
    let b = ObjectId::for_bytes(&encode_whole_file_payload(b"second canonical identity").unwrap());
    original.insert(a, shared);
    original.insert(b, shared);
    assert_eq!(original.flush_catalog(&catalog).unwrap(), 302);
    assert_eq!(catalog.writes.get(), 3);
    assert_eq!(original.flush_catalog(&catalog).unwrap(), 0);
    let mut reloaded = Candidates::new().unwrap();
    let before = catalog.reads.get();
    reloaded.reload_catalog(&catalog).unwrap();
    assert_eq!(catalog.reads.get() - before, 4);
    assert_eq!(reloaded.entries(), original.entries());
    assert!(reloaded.live_bytes() <= INDEX_BYTES);
    for (id, sig) in queries {
        assert_eq!(reloaded.find(id, &sig), original.find(id, &sig));
    }
    let excluded = ObjectId::for_bytes(b"query excludes neither tied candidate");
    assert_eq!(original.find(excluded, &shared), Some(a.min(b)));
    assert_eq!(reloaded.find(excluded, &shared), Some(a.min(b)));
}
#[test]
fn failed_hydration_and_staging_invalidate_the_disposable_derivation() {
    let catalog = Catalog::default();
    let (mut index, _) = populated(&catalog);
    index.flush_catalog(&catalog).unwrap();
    catalog.fail_read.set(Some(catalog.reads.get() + 2));
    assert!(matches!(
        index.reload_catalog(&catalog),
        Err(StorageError::Integrity("external page refusal"))
    ));
    assert!(index.needs_load());
    assert_eq!(index.entries(), 0);
    let fresh = Catalog::default();
    let (mut index, _) = populated(&fresh);
    fresh.fail_write.set(Some(2));
    assert!(matches!(
        index.flush_catalog(&fresh),
        Err(StorageError::Integrity("external staging refusal"))
    ));
    assert!(index.needs_load());
    assert_eq!(index.entries(), 0);
    assert_eq!(fresh.rows.borrow().len(), 128);
}
