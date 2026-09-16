//! Are F13 (same-save read of an unsealed group) and F14 (cross-wave duplicate
//! identity) reachable through the public API?
use std::path::PathBuf;
use layerfs_content::{construct_bytes, ConstructionPolicy, FinalizedConsumer, FinalizedObject, ObjectId};
use layerfs_storage::{StoragePolicy, Store};
use layerfs_telemetry::timer::Timing;

fn disabled<T, E>(f: impl FnOnce(&layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>) -> Result<T,E>) -> Result<T,E> { Timing::disabled("r", f).0 }

#[derive(Default)]
struct Collected(Vec<(ObjectId, layerfs_content::ObjectRole, Vec<u8>, Vec<ObjectId>)>);
impl FinalizedConsumer for Collected {
    fn accept(&mut self, o: FinalizedObject) -> layerfs_content::ContentResult<()> {
        let (id, role, bytes, refs) = o.into_parts(); self.0.push((id, role, bytes, refs)); Ok(())
    }
}

fn main() {
    let dir = PathBuf::from("/tmp/rev-harness/repro");
    let _ = &dir;
    let _ = std::fs::remove_dir_all(&dir); std::fs::create_dir_all(&dir).unwrap();
    let policy = ConstructionPolicy::frozen_default();

    // --- F14: a large repetitive file emits the same chunk identity many times ---
    for (label, body) in [
        ("2 MiB of one repeated byte", vec![0x21u8; 2 * 1024 * 1024]),
        ("4 MiB of one repeated byte", vec![0x21u8; 4 * 1024 * 1024]),
    ] {
        let mut collected = Collected::default();
        let built = disabled(|s| construct_bytes(policy, &policy.capacities(), &body, &mut collected, s.child("c"))).unwrap();
        let distinct: std::collections::BTreeSet<_> = collected.0.iter().map(|(id,_,_,_)| *id).collect();
        let chunks = collected.0.iter().filter(|(_,r,_,_)| *r == layerfs_content::ObjectRole::Chunk).count();
        let path = dir.join(format!("f14-{}.sqlite", body.len()));
        let store = disabled(|s| Store::create(&path, StoragePolicy::frozen_default(), s.child("c"))).unwrap();
        let outcome = disabled(|s| {
            let mut op = store.begin_save(s.child("b"))?;
            for (_, role, bytes, refs) in &collected.0 {
                let o = FinalizedObject::new(*role, bytes.clone()).unwrap().with_references(refs.clone());
                op.accept(o, s.child("a"))?;
            }
            op.finish(s.child("f"))
        });
        println!("F14 {label}: objects={} distinct={} chunks={} dup_occurrences={} -> {}",
            collected.0.len(), distinct.len(), chunks, collected.0.len() - distinct.len(),
            match &outcome { Ok(o) => format!("OK inserted={} packs={}", o.inserted, o.packs_created),
                             Err(e) => format!("FAILED: {e}") });
        let _ = built;
    }

    // --- F13: same-save read of an object accepted but still in an unsealed group ---
    let body = vec![0x9eu8; 3 * 1024 * 1024];   // incompressible-ish, distinct chunks
    let mut collected = Collected::default();
    let _ = disabled(|s| construct_bytes(policy, &policy.capacities(), &body, &mut collected, s.child("c"))).unwrap();
    let path = dir.join("f13.sqlite");
    let store = disabled(|s| Store::create(&path, StoragePolicy::frozen_default(), s.child("c"))).unwrap();
    let ids: Vec<ObjectId> = collected.0.iter().map(|(id,_,_,_)| *id).collect();
    let r = disabled(|s| {
        let mut op = store.begin_save(s.child("b"))?;
        for (_, role, bytes, refs) in &collected.0 {
            let o = FinalizedObject::new(*role, bytes.clone()).unwrap().with_references(refs.clone());
            op.accept(o, s.child("a"))?;
        }
        // Same-save read of everything the operation just accepted.
        let values = op.read_batch(&ids, s.child("r"))?;
        Ok::<usize, layerfs_storage::StorageError>(values.len())
    });
    println!("F13 same-save read of {} accepted objects -> {}", ids.len(),
        match r { Ok(n) => format!("OK {n} values"), Err(e) => format!("FAILED: {e}") });

    f13_small_records(std::path::Path::new("/tmp/rev-harness"), &dir);
}

/// F13 with small records: CDC-sized chunks leave the last group of a lane
/// partial, so its members have no row when the caller reads them.
fn f13_small_records(_root: &std::path::Path, dir: &std::path::Path) {
    let policy = ConstructionPolicy::frozen_default();
    let body: Vec<u8> = {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        (0..5 * 1024 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect()
    };
    let mut collected = Collected::default();
    disabled(|s| construct_bytes(policy, &policy.capacities(), &body, &mut collected, s.child("c"))).unwrap();
    let path = dir.join("f13-small.sqlite");
    let store = disabled(|s| Store::create(&path, StoragePolicy::frozen_default(), s.child("c"))).unwrap();
    let ids: Vec<ObjectId> = collected.0.iter().map(|(id, _, _, _)| *id).collect();
    let result = disabled(|s| {
        let mut op = store.begin_save(s.child("b"))?;
        for (_, role, bytes, refs) in &collected.0 {
            let o = FinalizedObject::new(*role, bytes.clone()).unwrap().with_references(refs.clone());
            op.accept(o, s.child("a"))?;
        }
        let mut ok = 0usize;
        let mut refused = Vec::new();
        for id in &ids {
            match op.read_batch(std::slice::from_ref(id), s.child("r")) {
                Ok(values) if values.len() == 1 => ok += 1,
                Ok(_) => refused.push(format!("{id}: cardinality")),
                Err(error) => refused.push(format!("{id}: {error}")),
            }
        }
        Ok::<_, layerfs_storage::StorageError>((ok, refused))
    });
    match result {
        Ok((ok, refused)) => {
            println!("F13 small records (5 MiB noise, {} objects): ok={ok} refused={}", ids.len(), refused.len());
            for line in refused.iter().take(3) {
                println!("    refused {line}");
            }
        }
        Err(error) => println!("F13 small records: save failed: {error}"),
    }
}
