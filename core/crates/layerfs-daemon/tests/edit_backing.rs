//! Actual short owner jobs behind the Content-neutral terminal record adapter.
#[path = "../../layerfs-content/tests/support/mod.rs"]
mod content_support;
use layerfs_content::{
    EditRecordApply, EditRecordChange, EditRecordExpected, EditRecordKey, IndexedEditBacking,
};
use layerfs_daemon::{
    Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Pending, Response,
};
use layerfs_overlay::{IndexedApply, IndexedScope, ProfileConfig, Route};
use layerfs_workspace::{EditInputRefusal, IndexedEditRecords, WorkspaceError};
use std::{
    error::Error,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-edit-backing-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn wait(pending: Pending) -> Completion {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "bounded original owner result");
        std::thread::yield_now();
    }
}
fn job(client: &OwnerClient, route: Option<Route>, command: Command) -> Completion {
    wait(
        client
            .try_submit(route, command)
            .unwrap_or_else(|(cause, command)| panic!("{cause:?}: {command:?}")),
    )
}
fn scope(client: &OwnerClient, tag: u8, file_scope: u64) -> IndexedScope {
    let opened = job(
        client,
        None,
        Command::Open {
            incarnation: [tag; 32],
            base_root: [3; 32],
        },
    );
    let route = match opened.result() {
        Ok(Response::Opened(route)) => *route,
        v => panic!("{v:?}"),
    };
    drop(opened);
    let acquired = job(
        client,
        Some(route),
        Command::AcquireOperation { request: 1 },
    );
    let owner = match acquired.result() {
        Ok(Response::Operation(Some(owner))) => *owner,
        v => panic!("{v:?}"),
    };
    IndexedScope { owner, file_scope }
}
fn key(kind: u32, n: u64) -> EditRecordKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&n.to_be_bytes());
    EditRecordKey { kind, key }
}
fn put(key: EditRecordKey, value: Vec<u8>) -> EditRecordChange {
    EditRecordChange {
        key,
        expected: EditRecordExpected::Missing,
        value: Some(value),
    }
}
fn release(client: &OwnerClient, scope: IndexedScope) {
    let done = job(
        client,
        Some(scope.owner.route()),
        Command::ReleaseOperation(scope.owner),
    );
    assert!(done.result().is_ok(), "{done:?}");
}

#[test]
fn terminal_refusal_keeps_original_completion_and_other_scope_progresses() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let first = scope(&client, 11, u64::MAX);
    let other = scope(&client, 12, u64::MAX);
    let mut backing = IndexedEditRecords::new(&client, first);
    assert!(!backing.contains(key(5, 0)).unwrap());
    assert_eq!(
        backing.apply(vec![put(key(5, 0), vec![7; 8192])]).unwrap(),
        EditRecordApply::Applied
    );
    assert_eq!(backing.get(key(5, 0)).unwrap(), Some(vec![7; 8192]));
    assert_eq!(backing.first_keys(5, None).unwrap(), vec![key(5, 0).key]);
    assert!(backing
        .first_keys(5, Some(key(5, 0).key))
        .unwrap()
        .is_empty());
    assert_eq!(backing.work().copied_reply_bytes, 8192 + 32);
    let outcome = backing.apply(vec![put(key(5, 0), vec![99])]).unwrap();
    assert_eq!(
        outcome,
        EditRecordApply::NotApplied {
            index: 0,
            key: key(5, 0),
            actual: Some(vec![7; 8192])
        }
    );
    let done = backing
        .failure()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref::<Completion>()
        .expect("original attempted Completion retained");
    assert!(matches!(
        done.result(),
        Ok(Response::IndexedScratch(
            layerfs_daemon::IndexedScratchReply::Applied(IndexedApply::NotApplied { index: 0, .. })
        ))
    ));
    assert!(done.work().sql.total().vm_steps > 0);
    let before = client.diagnostics().unwrap();
    assert_eq!(before.outstanding, 1);
    assert!(before.credited_bytes > 0);
    let calls = backing.work().calls;
    assert!(backing.contains(key(5, 0)).is_err());
    assert!(backing.get(key(5, 0)).is_err());
    assert!(backing.apply(vec![put(key(5, 1), vec![])]).is_err());
    assert!(backing.first_keys(5, None).is_err());
    assert_eq!(backing.work().calls, calls);
    assert_eq!(backing.work().terminal_calls, 4);
    let after = client.diagnostics().unwrap();
    assert_eq!(before.admitted, after.admitted);
    let mut healthy = IndexedEditRecords::new(&client, other);
    assert_eq!(
        healthy.apply(vec![put(key(5, 0), vec![4])]).unwrap(),
        EditRecordApply::Applied
    );
    assert_eq!(healthy.get(key(5, 0)).unwrap(), Some(vec![4]));
    assert_eq!(client.diagnostics().unwrap().outstanding, 1);
    let custody = backing.into_custody();
    assert_eq!(custody.scope, first);
    drop(custody);
    drop(healthy);
    // Releasing reply custody does not release the original operation owner.
    let mut inspect = IndexedEditRecords::new(&client, first);
    assert_eq!(inspect.get(key(5, 0)).unwrap(), Some(vec![7; 8192]));
    drop(inspect);
    release(&client, first);
    release(&client, other);
    owner.stop().unwrap();
}

#[test]
fn oversized_unattempted_input_and_stopped_owner_retain_first_cause() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let scope = scope(&client, 21, 0);
    let before = client.diagnostics().unwrap();
    let mut backing = IndexedEditRecords::new(&client, scope);
    let mut changes = Vec::with_capacity(2048);
    changes.push(put(key(1, 0), vec![8]));
    assert!(backing.apply(changes).is_err());
    let failure = backing
        .failure()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref::<EditInputRefusal>()
        .unwrap();
    assert_eq!(failure.changes.capacity(), 2048);
    assert_eq!(failure.changes[0].value, Some(vec![8]));
    assert!(failure.retained_bytes.unwrap() > 65536);
    assert_eq!(backing.work().calls, 0);
    assert_eq!(client.diagnostics().unwrap().admitted, before.admitted);
    assert!(backing.first_keys(1, None).is_err());
    drop(backing.into_custody());
    // The refused batch had no SQL effect and no implicit owner release.
    let mut inspect = IndexedEditRecords::new(&client, scope);
    assert!(!inspect.contains(key(1, 0)).unwrap());
    drop(inspect);
    release(&client, scope);
    owner.stop().unwrap();
    let mut stopped = IndexedEditRecords::new(&client, scope);
    assert!(stopped.get(key(1, 0)).is_err());
    match stopped.failure().unwrap() {
        WorkspaceError::Service(error) => assert!(matches!(
            error.downcast_ref::<OwnerError>(),
            Some(OwnerError::Unattempted { .. })
        )),
        error => panic!("original unattempted command missing: {error:?}"),
    }
    assert!(stopped.contains(key(1, 0)).is_err());
    assert_eq!(stopped.work().calls, 1);
}

#[test]
fn canonical_file_edit_uses_real_owner_records_and_explicit_last_owner_cleanup() {
    use content_support::{
        build_file, disabled_scope,
        edits::{Edits, Parts},
        noise, read_back, MemoryStore,
    };
    use layerfs_content::{apply_edits, apply_edits_backed, ConstructionPolicy, Edit, EditRequest};
    let base = noise(3 * 1024 * 1024 + 71);
    let (stored, original) = build_file(&base);
    let edits = Edits::new(
        base.len() as u64,
        vec![
            Edit::insert(19137, 60003),
            Edit::overwrite(811111, 820111),
            Edit::delete(1377777, 1387777),
        ],
    )
    .unwrap();
    let mut source = Parts::new();
    source.push(noise(60003));
    source.push(vec![0x31; 9000]);
    source.push(vec![]);
    let policy = ConstructionPolicy::frozen_default();
    let mut memory = MemoryStore::new();
    let expected = disabled_scope(|timing| {
        apply_edits(
            policy,
            &policy.capacities(),
            &stored,
            EditRequest {
                root: original.root,
                edits: &edits,
                source: &source,
            },
            &mut memory,
            timing.child("memory"),
        )
    })
    .unwrap();
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let scope = scope(&client, 31, 0);
    let mut backing = IndexedEditRecords::new(&client, scope);
    let mut emitted = MemoryStore::new();
    let actual = disabled_scope(|timing| {
        apply_edits_backed(
            policy,
            &policy.capacities(),
            &stored,
            EditRequest {
                root: original.root,
                edits: &edits,
                source: &source,
            },
            &mut emitted,
            &mut backing,
            timing.child("backed-owner"),
        )
    })
    .unwrap();
    assert_eq!(
        (actual.root, actual.logical_len, actual.counters),
        (expected.root, expected.logical_len, expected.counters)
    );
    let mut oracle = base;
    oracle.splice(19137..19137, noise(60003));
    oracle.splice(811111..820111, vec![0x31; 9000]);
    oracle.drain(1377777..1387777);
    let mut merged = stored.merged_clone();
    merged.absorb(&emitted);
    assert_eq!(read_back(&merged, actual.root).unwrap(), oracle);
    assert!(backing.failure().is_none());
    assert!(backing.work().calls > 0);
    assert!(backing.work().copied_reply_bytes > 0);
    assert!(backing.work().peak_reply_capacity_bytes <= 65536);
    assert!(backing.work().peak_conversion_heap_bytes <= 131072);
    let custody = backing.into_custody();
    assert_eq!(custody.scope, scope);
    drop(custody);
    let before = job(
        &client,
        Some(scope.owner.route()),
        Command::Resources { global: false },
    );
    match before.result() {
        Ok(Response::Resources(work)) => {
            assert_eq!(work.counts.owner_rows, 1);
            assert!(work.counts.scratch_rows > 0);
        }
        v => panic!("{v:?}"),
    }
    drop(before);
    release(&client, scope);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let done = job(
            &client,
            Some(scope.owner.route()),
            Command::Resources { global: false },
        );
        let work = match done.result() {
            Ok(Response::Resources(work)) => work,
            _ => panic!("{done:?}"),
        };
        if work.counts.scratch_rows == 0 {
            assert_eq!(work.counts.owner_rows, 0);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual automatic released-scope cleanup did not finish"
        );
        drop(done);
        std::thread::yield_now();
    }
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    owner.stop().unwrap();
}

#[test]
fn direct_overlay_port_keeps_deciding_and_unadmitted_inputs_without_owner_release() {
    use layerfs_overlay::{ExpectedValue, IndexedChange, IndexedKey, Overlay};
    use layerfs_workspace::{OverlayScratch, ScratchInputRefusal, ScratchRefusal};
    let temp = Temp::new();
    let db = Overlay::create(&temp.0.join("db"), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([41; 32], [3; 32]).unwrap();
    let scope = IndexedScope {
        owner: db.acquire_operation(route, 1).unwrap(),
        file_scope: u64::MAX,
    };
    let mut adapter = IndexedEditRecords::new(&db, scope);
    assert_eq!(
        adapter.apply(vec![put(key(1, 0), vec![6; 4096])]).unwrap(),
        EditRecordApply::Applied
    );
    assert_eq!(adapter.get(key(1, 0)).unwrap(), Some(vec![6; 4096]));
    assert_eq!(
        adapter.work().copied_reply_bytes,
        0,
        "direct SQL ownership moves; SQL column acquisition remains separate"
    );
    assert!(matches!(
        adapter.apply(vec![put(key(1, 0), vec![])]).unwrap(),
        EditRecordApply::NotApplied {
            actual: Some(_),
            ..
        }
    ));
    let original = adapter
        .failure()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref::<ScratchRefusal>()
        .unwrap();
    assert!(
        matches!(&original.0,IndexedApply::NotApplied { actual:Some(value),.. } if value==&vec![6;4096])
    );
    assert_eq!(adapter.work().copied_reply_bytes, 4096);
    let calls = adapter.work().calls;
    assert!(adapter.get(key(1, 0)).is_err());
    assert_eq!(adapter.work().calls, calls);
    drop(adapter.into_custody());
    let raw_key = IndexedKey {
        kind: 1,
        key: key(1, 1).key,
    };
    let mut excess = Vec::with_capacity(2048);
    excess.push(IndexedChange {
        key: raw_key,
        expected: ExpectedValue::Missing,
        value: Some(vec![9]),
    });
    let error = db.scratch_apply(scope, excess).unwrap_err();
    let original = error
        .source()
        .unwrap()
        .downcast_ref::<ScratchInputRefusal>()
        .unwrap();
    assert_eq!(original.changes.capacity(), 2048);
    assert_eq!(original.changes[0].value, Some(vec![9]));
    assert!(!db.indexed_scratch_contains(scope, raw_key).unwrap());
    assert_eq!(db.resources(Some(route)).unwrap().counts.owner_rows, 1);
    db.release_operation(scope.owner).unwrap();
    assert_eq!(db.resources(Some(route)).unwrap().counts.owner_rows, 0);
}
