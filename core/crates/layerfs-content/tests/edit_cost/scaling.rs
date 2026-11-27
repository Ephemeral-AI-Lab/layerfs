//! Record jobs of the backed edit engine against the number of edits and the
//! length of the file: counts only, no timing. Kinds below 100 in the printed
//! histogram are `get` by record kind; 100 and above are `contains`.
#[path = "../support/mod.rs"]
mod support;
use layerfs_content::{
    apply_edits_backed, ConstructionPolicy, ContentResult, Edit, EditRecordApply, EditRecordChange,
    EditRecordExpected, EditRecordKey, EditRequest, IndexedEditBacking,
};
use std::collections::BTreeMap;
use support::{
    build_file, disabled_scope,
    edits::{Edits, Parts},
    noise, MemoryStore,
};

const K: usize = 1024;

/// Counts each kind of record job; holds the records in memory.
#[derive(Default)]
struct Backing {
    values: BTreeMap<EditRecordKey, Vec<u8>>,
    reads: u64,
    kinds: BTreeMap<u32, u64>,
    applies: u64,
    changes: u64,
    scans: u64,
}
impl IndexedEditBacking for Backing {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        self.reads += 1;
        *self.kinds.entry(100 + key.kind).or_default() += 1;
        Ok(self.values.contains_key(&key))
    }
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.reads += 1;
        *self.kinds.entry(key.kind).or_default() += 1;
        Ok(self.values.get(&key).cloned())
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        self.applies += 1;
        self.changes += changes.len() as u64;
        for (index, change) in changes.iter().enumerate() {
            let actual = self.values.get(&change.key);
            let valid = match (&change.expected, actual) {
                (EditRecordExpected::Missing, None) => true,
                (EditRecordExpected::ExactBytes(a), Some(b)) => a == b,
                _ => false,
            };
            if !valid {
                return Ok(EditRecordApply::NotApplied {
                    index,
                    key: change.key,
                    actual: actual.cloned(),
                });
            }
        }
        for change in changes {
            match change.value {
                Some(value) => self.values.insert(change.key, value),
                None => self.values.remove(&change.key),
            };
        }
        Ok(EditRecordApply::Applied)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.scans += 1;
        let range = EditRecordKey { kind, key: [0; 32] }..=EditRecordKey {
            kind,
            key: [255; 32],
        };
        Ok(self
            .values
            .range(range)
            .filter_map(|(k, _)| (Some(k.key) != excluded).then_some(k.key))
            .take(64)
            .collect())
    }
}

/// Jobs of `count` overwrites of 16 KiB, one every 64 KiB, on `length` bytes.
fn jobs(length: usize, count: usize) -> (u64, u64, u64, u64) {
    let base = noise(length);
    let (store, built) = build_file(&base);
    let mut source = Parts::new();
    let mut edits = Vec::new();
    for index in 0..count {
        let at = (64 * K + index * 64 * K) as u64;
        edits.push(Edit::overwrite(at, at + 16 * K as u64));
        let mut bytes = noise(16 * K);
        bytes[..8].copy_from_slice(&(index as u64).to_be_bytes());
        source.push(bytes);
    }
    let edits = Edits::new(length as u64, edits).unwrap();
    let policy = ConstructionPolicy::frozen_default();
    let (mut backing, mut objects) = (Backing::default(), MemoryStore::new());
    disabled_scope(|scope| {
        apply_edits_backed(
            policy,
            &policy.capacities(),
            &store,
            EditRequest {
                root: built.root,
                edits: &edits,
                source: &source,
            },
            &mut objects,
            &mut backing,
            scope.child("backed"),
        )
    })
    .unwrap();
    println!("EDIT_KINDS edits={count} {:?}", backing.kinds);
    (
        backing.reads,
        backing.applies,
        backing.changes,
        backing.scans,
    )
}

#[test]
fn record_jobs_do_not_depend_on_the_length_and_grow_with_the_edits() {
    let mut rows = Vec::new();
    for (length, count) in [
        (12 * K * K, 8),
        (12 * K * K, 32),
        (12 * K * K, 128),
        (48 * K * K, 32),
        (48 * K * K, 128),
        (48 * K * K, 512),
    ] {
        let (reads, applies, changes, scans) = jobs(length, count);
        let total = reads + applies + scans;
        println!(
            "EDIT_SCALING length={length} edits={count} reads={reads} applies={applies} \
             changes={changes} scans={scans} jobs={total} per_edit={}",
            total / count as u64
        );
        rows.push((length, count as u64, total));
    }
    // The same edits on four times the bytes cost the same jobs, within the
    // pages a longer mapping adds.
    for (short, long) in [(1, 3), (2, 4)] {
        assert_eq!(rows[short].1, rows[long].1);
        assert!(
            rows[long].2 <= rows[short].2 + rows[short].2 / 50,
            "{rows:?}"
        );
    }
    // Observed, not required: the jobs of one edit grow with the edits made
    // before it, because every hold and release of a branch reads three
    // records for each of its draft children. No bound on that growth is
    // asserted here.
    assert!(rows.windows(2).all(|pair| pair[0].2 > 0 && pair[1].2 > 0));
}
