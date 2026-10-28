//! Object locates and pack reads run without a transaction (#354, #352
//! F-035), each statement in its own snapshot. While another session
//! reclaims free pages, which moves pages inside the file, a read-only
//! session's locates and object reads answer exactly as before.
mod support;
use layerfs_content::{inode_leaf::InodeKind, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo},
    pack::{assemble, build_group, layout::PackLane},
    port::acquisition::{Acquisition, Begin, EntryKey, HostIdentity, NewEntry},
    port::*,
    Storage, StoragePolicy,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Reclamation jobs and read rounds at most: both loops end on their own.
const JOBS: usize = 4096;
const ROUNDS: usize = 20_000;

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}
fn history() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"immutable-reads".to_vec(),
        incarnation: 1,
        cursor_key: [83; 32],
    }
}

/// Eight objects in published pack 1, one group each.
fn publish(h: &Handles) -> Vec<FinalizedObject> {
    let mut random = 7177u64;
    let objects: Vec<_> = (0..8)
        .map(|_| {
            let bytes: Vec<u8> = (0..9000)
                .map(|_| {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    random as u8
                })
                .collect();
            FinalizedObject::new(
                ObjectRole::FileState,
                layerfs_content::object::codec::encode_bytes_object(&bytes).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let groups: Vec<_> = objects
        .iter()
        .map(|o| {
            let mut record = vec![0];
            record.extend_from_slice(o.canonical());
            build_group(PackLane::Ordinary, &[record], None).unwrap()
        })
        .collect();
    let body = assemble(PackLane::Ordinary, &groups).unwrap();
    h.storage
        .publish(&Publication {
            packs: vec![PublishedPack {
                info: PackInfo {
                    pack_id: 1,
                    domain: PackDomain::Metadata,
                    key: ObjectKey::for_bytes(&body),
                    length: body.len(),
                },
                body: Arc::new(body),
            }],
            objects: objects
                .iter()
                .enumerate()
                .map(|(n, o)| ObjectLocation {
                    object_id: o.id(),
                    role: o.role(),
                    canonical_length: o.canonical_len(),
                    pack_id: 1,
                    group_number: n,
                    record_number: 0,
                })
                .collect(),
            ..Publication::default()
        })
        .unwrap();
    objects
}

/// Free pages: acquisition rows written and then discarded.
fn free_pages(h: &Handles) {
    let owner = h
        .acquisition
        .begin(&Begin {
            source_device: 1,
            source_inode: 2,
            stack: [3; 16],
            scope: ObjectId::for_bytes(b"scope"),
        })
        .unwrap();
    for start in (1..=2300u64).step_by(128) {
        let rows: Vec<_> = (start..=(start + 127).min(2300))
            .map(|position| NewEntry {
                key: EntryKey {
                    parent: Some(0),
                    name: format!("f{position:08}").into_bytes(),
                },
                position: Some(position),
                kind: InodeKind::RegularFile,
                metadata: layerfs_content::filesystem::attributes::portable::PortableMetadata {
                    mode: 0o644,
                    mtime_seconds: 1_700_000_000,
                    mtime_nanoseconds: 0,
                },
                target_root: None,
                host_path: Some(vec![b'p'; 3500]),
                host: Some(HostIdentity {
                    device: 1,
                    inode: position,
                    evidence: [0; 44],
                }),
            })
            .collect();
        h.acquisition.put_entries(owner, &rows).unwrap();
    }
    for _ in 0..2 {
        if h.acquisition.discard(owner, 4096).unwrap().remaining_rows == 0 {
            break;
        }
    }
    h.acquisition.release(owner).unwrap();
}

#[test]
fn immutable_reads_are_unchanged_while_free_pages_are_reclaimed() {
    let temp = support::Temp::new("immutable-reads-reclaim");
    let path = temp.join("store.sqlite");
    let writer =
        Handles::create(config(&path), StoragePolicy::frozen_default(), &history()).unwrap();
    assert_eq!(writer.profile().auto_vacuum, 2, "a reclaimable Store");
    let objects = publish(&writer);
    free_pages(&writer);
    let ids: Vec<_> = objects.iter().map(FinalizedObject::id).collect();
    let canonical: Vec<_> = objects.iter().map(|o| o.canonical().to_vec()).collect();

    let reader =
        Handles::open_read_only(config(&path), &history().binding_key, history().cursor_key)
            .unwrap();
    let mut located = Vec::new();
    reader.storage.locate(&ids, &mut located).unwrap();
    assert_eq!(located.len(), ids.len());

    let done = AtomicBool::new(false);
    let (jobs, rounds, reclaimed) = std::thread::scope(|scope| {
        let reclaim = scope.spawn(|| {
            let (mut jobs, mut reclaimed, mut busy) = (0, 0, 0);
            while jobs < JOBS {
                // The Store binds no busy handler, so a job that meets the
                // concurrent reader's snapshot is refused `Busy` without
                // effect; the documented caller response is a later job.
                let job = match writer.reclaim_space(1) {
                    Err(PersistenceError::Busy) if busy < JOBS => {
                        busy += 1;
                        std::thread::yield_now();
                        continue;
                    }
                    other => other.unwrap(),
                };
                jobs += 1;
                reclaimed += job.reclaimed_pages;
                if job.remaining_free_pages == 0 {
                    break;
                }
            }
            done.store(true, Ordering::Relaxed);
            (jobs, reclaimed)
        });
        let mut rounds = 0;
        while rounds < ROUNDS && !done.load(Ordering::Relaxed) {
            let mut again = Vec::new();
            reader.storage.locate(&ids, &mut again).unwrap();
            assert_eq!(again, located, "round {rounds}: locations changed");
            // A fresh reader each round, so every object comes from the Store.
            let read = Storage::new(reader.storage.clone())
                .unwrap()
                .reader()
                .unwrap()
                .read_objects(&ids)
                .unwrap();
            assert_eq!(read, canonical, "round {rounds}: objects changed");
            rounds += 1;
        }
        let (jobs, reclaimed) = reclaim.join().expect("reclamation");
        (jobs, rounds, reclaimed)
    });
    println!("{jobs} reclamation jobs ({reclaimed} pages) during {rounds} read rounds");
    assert!(reclaimed > 1, "pages were reclaimed");
    assert!(rounds > 0, "reads ran while pages were reclaimed");
}
