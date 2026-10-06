//! Public bounded owner jobs and retained original scratch completions.
use layerfs_daemon::{
    Command, Completion, IndexedScratchJob, IndexedScratchReply, Owner, OwnerClient, OwnerConfig,
    OwnerError, Pending, Response,
};
use layerfs_overlay::{
    ExpectedValue, IndexedApply, IndexedChange, IndexedKey, IndexedScope, OverlayError,
    ProfileConfig, Route, SCRATCH_BYTES,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-indexed-owner-{}-{}",
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
        assert!(
            Instant::now() < deadline,
            "bounded original job did not complete"
        );
        std::thread::yield_now();
    }
}
fn submit(client: &OwnerClient, route: Option<Route>, command: Command) -> Completion {
    match client.try_submit(route, command) {
        Ok(pending) => wait(pending),
        Err((error, command)) => panic!("{error:?}: {command:?}"),
    }
}
fn open(client: &OwnerClient, tag: u8) -> Route {
    let done = submit(
        client,
        None,
        Command::Open {
            incarnation: [tag; 32],
            base_root: [3; 32],
        },
    );
    match done.result() {
        Ok(Response::Opened(route)) => *route,
        result => panic!("{result:?}"),
    }
}
fn scope(client: &OwnerClient, route: Route, request: u64) -> IndexedScope {
    let done = submit(client, Some(route), Command::AcquireOperation { request });
    match done.result() {
        Ok(Response::Operation(Some(owner))) => IndexedScope {
            owner: *owner,
            file_scope: u64::MAX,
        },
        result => panic!("{result:?}"),
    }
}
fn key(number: u64) -> IndexedKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&number.to_be_bytes());
    IndexedKey { kind: 9, key }
}
fn put(key: IndexedKey, value: Vec<u8>) -> IndexedChange {
    IndexedChange {
        key,
        expected: ExpectedValue::Missing,
        value: Some(value),
    }
}
fn scratch(client: &OwnerClient, scope: IndexedScope, job: IndexedScratchJob) -> Completion {
    submit(
        client,
        Some(scope.owner.route()),
        Command::IndexedScratch(Box::new(job)),
    )
}
fn credits(client: &OwnerClient, outstanding: usize) -> layerfs_daemon::OwnerWork {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let work = client.diagnostics().unwrap();
        if work.outstanding == outstanding {
            return work;
        }
        assert!(
            Instant::now() < deadline,
            "credit last-owner release did not complete: {work:?}"
        );
        std::thread::yield_now();
    }
}

#[test]
fn original_atomic_refusal_and_credits_survive_independent_owner_jobs() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let route = open(&client, 171);
    let other = open(&client, 172);
    let scope = scope(&client, route, 1);
    let done = scratch(
        &client,
        scope,
        IndexedScratchJob::Apply {
            scope,
            changes: vec![put(key(1), vec![7; 8192])],
        },
    );
    assert!(matches!(
        done.result(),
        Ok(Response::IndexedScratch(IndexedScratchReply::Applied(
            IndexedApply::Applied
        )))
    ));
    assert!(done.work().sql.vm_steps > 0);
    drop(done);
    let before = credits(&client, 0);
    let deciding = scratch(
        &client,
        scope,
        IndexedScratchJob::Apply {
            scope,
            changes: vec![
                put(key(0), vec![3]),
                IndexedChange {
                    key: key(1),
                    expected: ExpectedValue::ExactBytes(vec![99]),
                    value: None,
                },
            ],
        },
    );
    match deciding.result() {
        Ok(Response::IndexedScratch(IndexedScratchReply::Applied(IndexedApply::NotApplied {
            index,
            key: found,
            actual,
        }))) => {
            assert_eq!((*index, *found), (1, key(1)));
            assert_eq!(actual.as_deref(), Some([7; 8192].as_slice()));
        }
        result => panic!("{result:?}"),
    }
    let held = credits(&client, before.outstanding + 1);
    assert_eq!(held.outstanding, before.outstanding + 1);
    assert!(held.credited_bytes > before.credited_bytes);
    let healthy = submit(&client, Some(other), Command::State);
    assert!(healthy.result().is_ok());
    drop(healthy);
    let missed = scratch(
        &client,
        scope,
        IndexedScratchJob::Contains { scope, key: key(0) },
    );
    assert!(matches!(
        missed.result(),
        Ok(Response::IndexedScratch(IndexedScratchReply::Contains(
            false
        )))
    ));
    assert_eq!(
        missed.work().sql.returned_blob_bytes,
        32 // Only the owning Workspace root; membership projects no draft BLOB.
    );
    drop(missed);
    assert_eq!(
        credits(&client, held.outstanding).outstanding,
        held.outstanding
    );
    drop(deciding);
    let released = credits(&client, before.outstanding);
    assert_eq!(released.credited_bytes, before.credited_bytes);
    let wrong_route = submit(
        &client,
        Some(other),
        Command::IndexedScratch(Box::new(IndexedScratchJob::Get { scope, key: key(1) })),
    );
    assert!(matches!(
        wrong_route.result(),
        Err(OwnerError::Overlay(OverlayError::Stale))
    ));
    drop(wrong_route);
    let mut oversized = Vec::with_capacity(SCRATCH_BYTES);
    oversized.push(put(key(2), vec![]));
    let before = client.diagnostics().unwrap();
    match client.try_submit(
        Some(route),
        Command::IndexedScratch(Box::new(IndexedScratchJob::Apply {
            scope,
            changes: oversized,
        })),
    ) {
        Err((OwnerError::InvalidAdmission, Command::IndexedScratch(job))) => match *job {
            IndexedScratchJob::Apply {
                scope: returned,
                changes,
            } => {
                assert_eq!(returned, scope);
                assert_eq!(changes.len(), 1);
                assert_eq!(changes.capacity(), SCRATCH_BYTES);
                assert_eq!(changes[0].key, key(2));
            }
            command => panic!("{command:?}"),
        },
        _ => panic!("capacity refusal lost its original command"),
    }
    assert_eq!(client.diagnostics().unwrap().admitted, before.admitted);
    let value = scratch(
        &client,
        scope,
        IndexedScratchJob::Get { scope, key: key(1) },
    );
    assert!(
        matches!(value.result(), Ok(Response::IndexedScratch(IndexedScratchReply::Value(Some(bytes)))) if bytes == &vec![7; 8192])
    );
    drop(value);
    owner.stop().unwrap();
}

#[test]
fn released_indexed_rows_reclaim_automatically_and_close_preserves_actual_owner() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let route = open(&client, 173);
    let other = open(&client, 174);
    let scope = scope(&client, route, 1);
    for number in 0..72 {
        let done = scratch(
            &client,
            scope,
            IndexedScratchJob::Apply {
                scope,
                changes: vec![put(key(number), vec![5; 6000])],
            },
        );
        assert!(matches!(
            done.result(),
            Ok(Response::IndexedScratch(IndexedScratchReply::Applied(
                IndexedApply::Applied
            )))
        ));
    }
    let done = scratch(
        &client,
        scope,
        IndexedScratchJob::FirstKeys {
            scope,
            kind: 9,
            excluded_root: key(0).key,
        },
    );
    match done.result() {
        Ok(Response::IndexedScratch(IndexedScratchReply::Keys(keys))) => {
            assert_eq!(keys.len(), 64);
            assert_eq!(keys.first(), Some(&key(1).key));
        }
        result => panic!("{result:?}"),
    }
    drop(done);
    assert!(submit(&client, Some(route), Command::Close)
        .result()
        .is_ok());
    let read = scratch(
        &client,
        scope,
        IndexedScratchJob::Get { scope, key: key(1) },
    );
    assert!(
        matches!(read.result(), Ok(Response::IndexedScratch(IndexedScratchReply::Value(Some(bytes)))) if bytes.len() == 6000)
    );
    drop(read);
    let denied = scratch(
        &client,
        scope,
        IndexedScratchJob::Apply {
            scope,
            changes: vec![put(key(99), vec![])],
        },
    );
    assert!(matches!(
        denied.result(),
        Err(OwnerError::Overlay(OverlayError::Closed))
    ));
    drop(denied);
    let resources = submit(&client, Some(route), Command::Resources { global: false });
    assert!(
        matches!(resources.result(), Ok(Response::Resources(r)) if r.counts.scratch_rows == 72 && r.counts.scratch_bytes == 72 * 6000)
    );
    drop(resources);
    assert!(
        submit(&client, Some(route), Command::ReleaseOperation(scope.owner))
            .result()
            .is_ok()
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let resources = submit(&client, Some(other), Command::Resources { global: true });
        if matches!(resources.result(), Ok(Response::Resources(r)) if r.counts.scratch_rows == 0 && r.counts.namespaces == 1)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "automatic indexed cleanup did not finish"
        );
        drop(resources);
        std::thread::yield_now();
    }
    assert!(client.diagnostics().unwrap().maintenance_data_bytes >= 72 * 6000);
    assert!(client.maintenance_failure().unwrap().is_none());
    owner.stop().unwrap();
}
