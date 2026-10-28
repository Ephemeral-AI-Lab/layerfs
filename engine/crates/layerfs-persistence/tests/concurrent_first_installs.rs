//! F-036: two writers, each with its own opened Store handle as a daemon has,
//! save the same new content at the same time. Both Saves complete and every
//! object reads back, whichever writer won each first-wins registration.
#![cfg(target_os = "linux")]
mod support;
use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StoragePolicy};
use std::{path::Path, sync::mpsc, time::Duration};

const CHAINS: usize = 384;
const VERSIONS: usize = 8;

fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"f036-concurrent-installs".to_vec(),
        cursor_key: [36; 32],
        incarnation: 1,
    }
}
fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}

/// A whole-file object for `raw`, built the way C1 frames small files.
fn whole(raw: &[u8]) -> FinalizedObject {
    let mut value = b"LFS5SML\0".to_vec();
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(raw);
    let canonical = layerfs_content::object::codec::encode_bytes_object(&value).unwrap();
    FinalizedObject::new(ObjectRole::WholeFile, canonical).unwrap()
}

/// Package-like content: each chain is one file edited `VERSIONS` times.
fn contents() -> Vec<Vec<Vec<u8>>> {
    let mut state = 0x0f03_6f03_6f03_6f03_u64;
    (0..CHAINS)
        .map(|_| {
            let mut raw: Vec<u8> = (0..3072)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state as u8
                })
                .collect();
            (0..VERSIONS)
                .map(|version| {
                    let at = (version * 389) % (raw.len() - 8);
                    raw[at..at + 8].copy_from_slice(&(state ^ version as u64).to_le_bytes());
                    raw.clone()
                })
                .collect()
        })
        .collect()
}

/// One writer's Save input. Writer a takes each chain oldest first, each
/// version hinted at the one before, so its winners are PREFIX chains. Writer
/// b, like a producer whose similarity candidates differ, takes each chain
/// newest first with each version hinted two back: it probes a base before
/// that base is stored, then meets the other writer's chain through it.
fn writer(contents: &[Vec<Vec<u8>>], b: bool) -> Vec<FinalizedObject> {
    let mut out = Vec::with_capacity(CHAINS * VERSIONS);
    for chain in contents {
        let objects: Vec<_> = chain.iter().map(|raw| whole(raw)).collect();
        let hint = if b { 2 } else { 1 };
        let mut order: Vec<usize> = (0..VERSIONS).collect();
        if b {
            order.reverse();
        }
        for version in order {
            let mut object = objects[version].clone();
            if version >= hint {
                let base = objects[version - hint].id();
                object = object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap());
            }
            out.push(object);
        }
    }
    out
}

#[test]
fn two_writers_saving_the_same_new_content_both_complete() {
    let temp = support::Temp::new("f036-concurrent-installs");
    let path = temp.join("store.sqlite");
    let first =
        Handles::create(config(&path), StoragePolicy::frozen_default(), &authority()).unwrap();
    let second = Handles::open_writable(
        config(&path),
        &authority().binding_key,
        authority().cursor_key,
    )
    .unwrap();
    let contents = contents();
    let orders = [writer(&contents, false), writer(&contents, true)];
    let (ready_tx, ready_rx) = mpsc::channel();
    let mut starts = Vec::new();
    let mut jobs = Vec::new();
    for (provider, objects) in [first.storage.clone(), second.storage.clone()]
        .into_iter()
        .zip(orders)
    {
        let ready = ready_tx.clone();
        let (start_tx, start_rx) = mpsc::channel::<()>();
        starts.push(start_tx);
        jobs.push(std::thread::spawn(move || {
            let storage = Storage::new(provider).unwrap();
            ready.send(()).unwrap();
            start_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let save = storage.begin_save().unwrap();
            for object in objects {
                save.accept(object)?;
            }
            save.finish()
        }));
    }
    for _ in 0..2 {
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    for start in starts {
        start.send(()).unwrap();
    }
    let outcomes: Vec<_> = jobs.into_iter().map(|job| job.join().unwrap()).collect();
    for (writer, outcome) in ["a", "b"].iter().zip(&outcomes) {
        assert!(outcome.is_ok(), "writer {writer}: {outcome:?}");
    }
    let objects = writer(&contents, false);
    let inserted: u64 = outcomes.iter().map(|o| o.as_ref().unwrap().inserted).sum();
    assert_eq!(
        inserted,
        objects.len() as u64,
        "each object is inserted once"
    );
    let reader = Storage::new(first.storage.clone()).unwrap();
    for window in objects.chunks(256) {
        let ids: Vec<_> = window.iter().map(FinalizedObject::id).collect();
        let read = reader.reader().unwrap().read_objects(&ids).unwrap();
        for (object, bytes) in window.iter().zip(read) {
            assert_eq!(bytes, object.canonical(), "object {}", object.id());
        }
    }
}
