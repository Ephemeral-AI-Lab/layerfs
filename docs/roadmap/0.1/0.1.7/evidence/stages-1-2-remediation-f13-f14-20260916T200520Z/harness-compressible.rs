//! Workload shape for highly compressible but distinct chunks.
//!
//! Measures how many records share a framed group when their canonical payloads are
//! large but their encoded records are tiny. The number matters because a group is
//! bounded by its *framed* bytes: retaining canonical bytes for the members waiting in
//! an open group would retain `records_per_group * canonical` bytes, not `64 KiB`.
use layerfs_content::{
    construct_bytes, ConstructionPolicy, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_storage::{StoragePolicy, Store};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};

#[derive(Default)]
struct Collected(Vec<(ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>)>);

impl FinalizedConsumer for Collected {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object.into_parts());
        Ok(())
    }
}

fn disabled<T, E>(body: impl FnOnce(&TimingScope<'_, Active>) -> Result<T, E>) -> Result<T, E> {
    Timing::disabled("harness.operation", body).0
}

fn main() {
    let dir = std::env::temp_dir().join(format!("compressible-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c.sqlite");
    let policy = ConstructionPolicy::frozen_default();
    let store = disabled(|scope| {
        Store::create(&path, StoragePolicy::frozen_default(), scope.child("create"))
    })
    .unwrap();

    let mut body = vec![0u8; 4 * 1024 * 1024];
    for (index, stamp) in body.chunks_mut(64 * 1024).enumerate() {
        stamp[..4].copy_from_slice(&(index as u32).to_be_bytes());
    }
    let mut collected = Collected::default();
    disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &body,
            &mut collected,
            scope.child("content"),
        )
    })
    .unwrap();
    let chunks: Vec<usize> = collected
        .0
        .iter()
        .filter(|entry| entry.1 == ObjectRole::Chunk)
        .map(|entry| entry.2.len())
        .collect();
    let max_chunk = chunks.iter().copied().max().unwrap_or(0);
    let min_chunk = chunks.iter().copied().min().unwrap_or(0);

    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("begin"))?;
        for (_, role, bytes, references) in &collected.0 {
            let object = FinalizedObject::new(*role, bytes.clone())
                .unwrap()
                .with_references(references.clone());
            operation.accept(object, scope.child("accept"))?;
        }
        operation.finish(scope.child("finish"))
    })
    .unwrap();

    println!("file 4 MiB of zeros with a 4-byte stamp every 64 KiB");
    println!(
        "objects={} distinct chunks={} chunk canonical bytes {}..{}",
        collected.0.len(),
        chunks.len(),
        min_chunk,
        max_chunk
    );
    println!(
        "inserted={} reused={} packs_created={} pack_appends={}",
        outcome.inserted, outcome.reused, outcome.packs_created, outcome.pack_appends
    );
    println!(
        "canonical bytes retained if waiting members kept their payloads: up to {} bytes per group",
        chunks.len() * max_chunk
    );
    let _ = std::fs::remove_dir_all(&dir);
}
