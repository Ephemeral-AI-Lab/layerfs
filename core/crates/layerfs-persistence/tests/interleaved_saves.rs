//! S0: a scoped session registry borrows initialized handles without self-reference.
#![cfg(target_os = "macos")]
mod support;

use layerfs_content::{construct_bytes, read_range};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use support::Temp;

#[test]
fn scoped_registry_interleaves_pending_reads_finish_abort_and_slot_reuse() {
    let temp = Temp::new("interleaved-saves");
    let mut config = PersistenceConfig::sqlite(temp.join("store"));
    config.sqlite_profile = SqlitePersistenceProfile::Disposable;
    let handles = Handles::create(
        config,
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"scoped-runtime".to_vec(),
            cursor_key: [17; 32],
            incarnation: 5,
        },
    )
    .unwrap();
    // Owner storage handles are allocated first, outside the registry's scope.
    // A serving loop can hold these borrows on its stack for its full lifetime.
    let owners: Vec<_> = (0..3)
        .map(|_| Storage::new(handles.storage.clone()).unwrap())
        .collect();
    let demand = Storage::new(handles.storage.clone()).unwrap();
    let mut sessions: Vec<_> = owners
        .iter()
        .map(|owner| Some(owner.begin_save().unwrap()))
        .collect();
    let construct = |index: usize, bytes: &[u8]| {
        let owner = &owners[index];
        let save = sessions[index].as_ref().unwrap();
        Timing::disabled("construct", |scope| {
            construct_bytes(
                owner.policy().construction(),
                &owner.policy().construction().capacities(),
                bytes,
                &mut save.sink(),
                scope.child("content"),
            )
        })
        .0
        .unwrap()
        .root
    };
    let a = construct(0, b"pending A");
    let b = construct(1, b"pending B");
    let abandoned = construct(2, b"abandoned C");
    let mut bytes = Vec::new();
    Timing::disabled("same-save", |scope| {
        read_range(
            sessions[1].as_ref().unwrap(),
            b,
            0..9,
            &mut bytes,
            scope.child("read"),
        )
    })
    .0
    .unwrap();
    assert_eq!(bytes, b"pending B");
    // B finishes while A and C remain admitted. Demand reads stay independent.
    sessions[1].take().unwrap().finish().unwrap();
    bytes.clear();
    Timing::disabled("demand", |scope| {
        read_range(
            &demand.reader().unwrap(),
            b,
            0..9,
            &mut bytes,
            scope.child("read"),
        )
    })
    .0
    .unwrap();
    assert_eq!(bytes, b"pending B");
    drop(sessions[2].take());
    assert!(demand.reader().unwrap().read_objects(&[abandoned]).is_err());
    sessions[1] = Some(owners[1].begin_save().unwrap());
    let reused = Timing::disabled("reuse", |scope| {
        construct_bytes(
            owners[1].policy().construction(),
            &owners[1].policy().construction().capacities(),
            b"pending A",
            &mut sessions[1].as_ref().unwrap().sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root;
    assert_eq!(a, reused);
    sessions[0].take().unwrap().finish().unwrap();
    let outcome = sessions[1].take().unwrap().finish().unwrap();
    assert_eq!(outcome.reused, 1);
    assert_eq!(outcome.inserted, 0);
    assert!(handles.diagnostics().unwrap().vm_steps > 0);
}
