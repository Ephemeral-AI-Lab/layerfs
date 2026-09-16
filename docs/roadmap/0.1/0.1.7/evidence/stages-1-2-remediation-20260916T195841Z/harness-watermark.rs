//! Publication-watermark differential.
//!
//! One workload, run unchanged against two trees. It creates a store, builds a
//! 5 MiB file through C1, accepts every emitted object into one open save, and then
//! asks an *unrelated* reader -- a second `Store` handle on the same database -- for
//! each accepted id, one call at a time, while the save is still open. It then
//! acknowledges the save and asks again.
//!
//! Expected fixed behaviour: nothing readable while the save is open, every refusal
//! attributed to the publication watermark rather than to an absent row, and
//! everything readable after acknowledgement.

use layerfs_content::{
    construct_bytes, ConstructionPolicy, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_storage::{StorageError, StoragePolicy, Store};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};

#[derive(Default)]
struct Collected(Vec<(ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>)>);

impl FinalizedConsumer for Collected {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object.into_parts());
        Ok(())
    }
}

fn disabled<T, E>(
    body: impl FnOnce(&TimingScope<'_, Active>) -> Result<T, E>,
) -> Result<T, E> {
    Timing::disabled("harness.operation", body).0
}

fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push((state >> 24) as u8);
    }
    out
}

/// Reads every id through its own call so one refusal cannot mask the rest.
fn probe(store: &Store, ids: &[ObjectId]) -> [usize; 3] {
    let mut counts = [0usize; 3];
    for id in ids {
        let outcome = disabled(|scope| store.read_batch(std::slice::from_ref(id), scope.child("probe")));
        match outcome {
            Ok(_) => counts[0] += 1,
            Err(StorageError::VisibilityCeiling { .. }) => counts[1] += 1,
            Err(_) => counts[2] += 1,
        }
    }
    counts
}

fn main() {
    let dir = std::env::temp_dir().join(format!("wm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("wm.sqlite");
    let policy = ConstructionPolicy::frozen_default();
    let store = disabled(|scope| Store::create(&path, StoragePolicy::frozen_default(), scope.child("create")))
        .unwrap();

    let body = noise(5 * 1024 * 1024);
    let mut collected = Collected::default();
    let file = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &body,
            &mut collected,
            scope.child("content"),
        )
    })
    .unwrap();
    let root = file.root;
    let ids: Vec<ObjectId> = collected.0.iter().map(|entry| entry.0).collect();
    println!(
        "workload: file {} bytes, objects {}, root {}",
        body.len(),
        ids.len(),
        root
    );

    let mut operation = disabled(|scope| store.begin_save(scope.child("begin"))).unwrap();
    for (_, role, bytes, references) in &collected.0 {
        let object = FinalizedObject::new(*role, bytes.clone())
            .unwrap()
            .with_references(references.clone());
        disabled(|scope| operation.accept(object, scope.child("accept"))).unwrap();
    }

    // The save is deliberately still open: no finish has run.
    let unrelated =
        disabled(|scope| Store::open(&path, scope.child("open"))).unwrap();
    let while_open = probe(&unrelated, &ids);
    println!(
        "WHILE SAVE OPEN: readable={} visibility_ceiling={} other_refusals={} of {}",
        while_open[0],
        while_open[1],
        while_open[2],
        ids.len()
    );

    disabled(|scope| operation.finish(scope.child("finish"))).unwrap();
    let after = probe(&unrelated, &ids);
    println!(
        "AFTER ACKNOWLEDGEMENT: readable={} visibility_ceiling={} other_refusals={} of {}",
        after[0],
        after[1],
        after[2],
        ids.len()
    );
    println!(
        "root readable after acknowledgement: {}",
        disabled(|scope| unrelated.read_batch(&[root], scope.child("root"))).is_ok()
    );
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
