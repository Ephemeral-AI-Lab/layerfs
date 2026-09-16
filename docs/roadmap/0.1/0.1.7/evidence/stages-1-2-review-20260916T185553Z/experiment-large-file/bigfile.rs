//! Reviewer experiment: is 16 MiB a file-size limit, or a per-object limit?
//! Uses only the public candidate APIs; no product source is modified.
use std::path::PathBuf;
use std::time::Instant;

use layerfs_content::{
    construct_bytes, read_all, AuthenticatedObjects, ConstructionPolicy, ContentError,
    ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_storage::{StoragePolicy, Store};
use layerfs_telemetry::timer::Timing;

fn disabled<T, E>(
    body: impl FnOnce(&layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>) -> Result<T, E>,
) -> Result<T, E> {
    Timing::disabled("review", body).0
}

#[derive(Default)]
struct Collected(Vec<(ObjectId, layerfs_content::ObjectRole, Vec<u8>, Vec<ObjectId>)>);
impl FinalizedConsumer for Collected {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let (id, role, bytes, refs) = object.into_parts();
        self.0.push((id, role, bytes, refs));
        Ok(())
    }
}

struct Provider<'a>(&'a Store);
impl AuthenticatedObjects for Provider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        disabled(|s| self.0.read_batch(ids, s.child("read")))
            .map(|(v, _)| v)
            .map_err(|_| ContentError::MissingObject)
    }
}

fn noise(len: usize) -> Vec<u8> {
    let mut s = 0x9e37_79b9_7f4a_7c15u64;
    (0..len).map(|_| { s ^= s << 7; s ^= s >> 9; s ^= s << 8; s as u8 }).collect()
}

fn main() {
    let dir = PathBuf::from("/tmp/rev-harness/big");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("big.sqlite");

    const MIB: usize = 1024 * 1024;
    let len = 20 * MIB;                       // deliberately larger than 16 MiB
    let body = noise(len);
    println!("input: {len} bytes = {} MiB", len / MIB);

    // ---- C1: construct a >16 MiB file -------------------------------------
    let policy = ConstructionPolicy::frozen_default();
    let mut collected = Collected::default();
    let t0 = Instant::now();
    let constructed = disabled(|s| {
        construct_bytes(policy, &policy.capacities(), &body, &mut collected, s.child("c1"))
    })
    .expect("C1 must accept a file larger than 16 MiB");
    let c1 = t0.elapsed();
    println!("C1: root={} logical_len={} objects={} accepted_in={:.1}ms",
        constructed.root, constructed.logical_len, collected.0.len(), c1.as_secs_f64() * 1e3);

    let mut per_role = std::collections::BTreeMap::new();
    let mut largest = 0usize;
    for (_, role, bytes, _) in &collected.0 {
        *per_role.entry(format!("{role:?}")).or_insert(0usize) += 1;
        largest = largest.max(bytes.len());
    }
    println!("roles: {per_role:?}");
    println!("largest single canonical object: {largest} bytes ({:.1} KiB) = {:.4}% of 16 MiB",
        largest as f64 / 1024.0, 100.0 * largest as f64 / (16.0 * MIB as f64));

    // ---- C2: save it, reopen, read it back --------------------------------
    let store = disabled(|s| Store::create(&path, StoragePolicy::frozen_default(), s.child("create"))).unwrap();
    let t0 = Instant::now();
    let outcome = disabled(|s| {
        let mut op = store.begin_save(s.child("begin"))?;
        for (_, role, bytes, refs) in &collected.0 {
            let o = FinalizedObject::new(*role, bytes.clone()).unwrap().with_references(refs.clone());
            op.accept(o, s.child("accept"))?;
        }
        op.finish(s.child("finish"))
    })
    .expect("C2 must store a file larger than 16 MiB");
    println!("C2 save: inserted={} reused={} packs_created={} commits={} in {:.1}ms",
        outcome.inserted, outcome.reused, outcome.packs_created, outcome.commits,
        t0.elapsed().as_secs_f64() * 1e3);
    drop(store);

    let reopened = disabled(|s| Store::open(&path, s.child("open"))).unwrap();
    let mut out = Vec::with_capacity(len);
    let t0 = Instant::now();
    disabled(|s| read_all(&Provider(&reopened), constructed.root, &mut out, s.child("read")))
        .expect("authenticated logical readback");
    println!("C2 reopen + C1 readback: {} bytes in {:.1}ms, byte-identical={}",
        out.len(), t0.elapsed().as_secs_f64() * 1e3, out == body);

    let db = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    println!("store file: {db} bytes ({:.1} MiB), overhead vs input = {:.2}%",
        db as f64 / MIB as f64, 100.0 * (db as f64 - len as f64) / len as f64);
}
