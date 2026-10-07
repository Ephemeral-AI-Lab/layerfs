//! Real credited range jobs and first-original construction adapter custody.
use layerfs_content::{
    ConstructionRecordApply, ConstructionRecordChange, ConstructionRecordExpected,
    ConstructionRecordKey, ContentError, ContentResult, IndexedConstructionBacking,
};
use layerfs_daemon::{
    Command, Completion, IndexedScratchJob, Owner, OwnerClient, OwnerConfig, OwnerError, Pending,
    Response,
};
use layerfs_overlay::{IndexedKey, IndexedScope, OverlayError, ProfileConfig, Route};
use layerfs_workspace::{
    IndexedConstructionRecords, OverlayScratch, ScratchApply, ScratchReply, WorkspaceError,
    WorkspaceResult,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-construction-cursor-{}-{}",
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
        assert!(Instant::now() < deadline, "bounded original cursor job");
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
fn scope(client: &OwnerClient, tag: u8) -> IndexedScope {
    let opened = job(
        client,
        None,
        Command::Open {
            incarnation: [tag; 32],
            base_root: [4; 32],
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
    IndexedScope {
        owner,
        file_scope: u64::MAX,
    }
}
fn key(number: u64) -> ConstructionRecordKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&number.to_be_bytes());
    ConstructionRecordKey { kind: 27, key }
}
fn put(number: u64) -> ConstructionRecordChange {
    ConstructionRecordChange {
        key: key(number),
        expected: ConstructionRecordExpected::Missing,
        value: Some(vec![number as u8]),
    }
}
fn release(client: &OwnerClient, scope: IndexedScope) {
    assert!(job(
        client,
        Some(scope.owner.route()),
        Command::ReleaseOperation(scope.owner)
    )
    .result()
    .is_ok());
}
fn no_credits(client: &OwnerClient) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let work = client.diagnostics().unwrap();
        if work.outstanding == 0 && work.credited_bytes == 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "bounded original reply release: {work:?}"
        );
        std::thread::yield_now();
    }
}

#[test]
fn real_owner_preserves_sealed_keys_and_observes_bounded_reply_copies() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let scope = scope(&client, 191);
    let mut records = IndexedConstructionRecords::new(&client, scope);
    let changes = (0..130).map(put).collect();
    assert_eq!(
        records.apply(changes).unwrap(),
        ConstructionRecordApply::Applied
    );
    let initial = records.work();
    let mut after = None;
    let mut count = 0;
    for expected in [64, 64, 2, 0] {
        let keys = records.keys_after(27, after).unwrap();
        assert_eq!(keys.len(), expected);
        assert!(keys.capacity() <= 64);
        for actual in &keys {
            assert_eq!(*actual, key(count).key);
            count += 1;
        }
        if let Some(last) = keys.last() {
            after = Some(*last);
        }
    }
    assert_eq!(count, 130);
    let observed = records.work();
    assert_eq!(
        observed.copied_reply_bytes - initial.copied_reply_bytes,
        130 * 32
    );
    assert_eq!(observed.reply_allocations - initial.reply_allocations, 3);
    assert!(observed.peak_reply_capacity_bytes <= 64 * 32);
    assert_eq!(records.get(key(0)).unwrap(), Some(vec![0]));
    assert_eq!(records.get(key(129)).unwrap(), Some(vec![129]));
    assert_eq!(records.keys_after(27, None).unwrap()[0], [0; 32]);
    drop(records.into_custody());
    release(&client, scope);
    owner.stop().unwrap();
}

#[test]
fn attempted_and_unattempted_range_failures_keep_original_custody_without_resend() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let first = scope(&client, 193);
    let healthy = scope(&client, 194);
    release(&client, first);
    let mut failed = IndexedConstructionRecords::new(&client, first);
    assert!(failed.keys_after(27, Some(key(81).key)).is_err());
    let original = match failed.failure().unwrap() {
        WorkspaceError::Service(error) => error.downcast_ref::<Completion>().unwrap(),
        v => panic!("{v:?}"),
    };
    assert!(matches!(
        original.result(),
        Err(OwnerError::Overlay(OverlayError::Stale))
    ));
    let before = client.diagnostics().unwrap();
    assert_eq!(before.outstanding, 1);
    let calls = failed.work().calls;
    assert!(failed.keys_after(27, None).is_err());
    assert!(failed.contains(key(0)).is_err());
    assert!(failed.apply(vec![put(0)]).is_err());
    assert_eq!(failed.work().calls, calls);
    assert_eq!(client.diagnostics().unwrap().admitted, before.admitted);
    let mut other = IndexedConstructionRecords::new(&client, healthy);
    assert_eq!(
        other.apply(vec![put(0)]).unwrap(),
        ConstructionRecordApply::Applied
    );
    assert_eq!(other.keys_after(27, None).unwrap(), vec![key(0).key]);
    drop(failed.into_custody());
    drop(other.into_custody());
    no_credits(&client);
    release(&client, healthy);
    owner.stop().unwrap();
    let mut stopped = IndexedConstructionRecords::new(&client, healthy);
    let boundary = key(99).key;
    assert!(stopped.keys_after(27, Some(boundary)).is_err());
    match stopped.failure().unwrap() {
        WorkspaceError::Service(error) => match error.downcast_ref::<OwnerError>().unwrap() {
            OwnerError::Unattempted { command, .. } => match command.as_ref() {
                Command::IndexedScratch(job) => match job.as_ref() {
                    IndexedScratchJob::KeysAfter { scope, kind, after } => {
                        assert_eq!((*scope, *kind, *after), (healthy, 27, Some(boundary)))
                    }
                    v => panic!("{v:?}"),
                },
                v => panic!("{v:?}"),
            },
            v => panic!("{v:?}"),
        },
        v => panic!("{v:?}"),
    };
    assert!(stopped.keys_after(27, None).is_err());
    assert_eq!(stopped.work().calls, 1);
}

struct OlderProvider;
impl IndexedConstructionBacking for OlderProvider {
    fn contains(&mut self, _: ConstructionRecordKey) -> ContentResult<bool> {
        panic!("no membership substitute")
    }
    fn get(&mut self, _: ConstructionRecordKey) -> ContentResult<Option<Vec<u8>>> {
        panic!("no point substitute")
    }
    fn apply(
        &mut self,
        _: Vec<ConstructionRecordChange>,
    ) -> ContentResult<ConstructionRecordApply> {
        panic!("no mutation substitute")
    }
    fn first_keys(&mut self, _: u32, _: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        panic!("no destructive traversal substitute")
    }
}
impl OverlayScratch for OlderProvider {
    fn scratch_contains(
        &self,
        _: IndexedScope,
        _: IndexedKey,
    ) -> WorkspaceResult<ScratchReply<bool>> {
        panic!("no membership substitute")
    }
    fn scratch_get(
        &self,
        _: IndexedScope,
        _: IndexedKey,
    ) -> WorkspaceResult<ScratchReply<Option<Vec<u8>>>> {
        panic!("no point substitute")
    }
    fn scratch_apply(
        &self,
        _: IndexedScope,
        _: Vec<layerfs_overlay::IndexedChange>,
    ) -> WorkspaceResult<ScratchReply<ScratchApply>> {
        panic!("no mutation substitute")
    }
    fn scratch_keys(
        &self,
        _: IndexedScope,
        _: u32,
        _: Option<[u8; 32]>,
    ) -> WorkspaceResult<ScratchReply<Vec<[u8; 32]>>> {
        panic!("no destructive traversal substitute")
    }
}

#[test]
fn unavailable_capability_retains_typed_original_without_provider_substitution() {
    let original = ContentError::ProviderFailure {
        what: "indexed construction key enumeration unavailable",
    };
    assert_eq!(OlderProvider.keys_after(27, None), Err(original.clone()));
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let scope = scope(&client, 197);
    let before = client.diagnostics().unwrap();
    let provider = OlderProvider;
    let mut records = IndexedConstructionRecords::new(&provider, scope);
    assert!(records.keys_after(27, Some(key(42).key)).is_err());
    assert!(matches!(records.failure(),Some(WorkspaceError::Content(error)) if *error==original));
    assert_eq!(records.work().calls, 1);
    assert!(records.keys_after(27, None).is_err());
    assert!(records.get(key(0)).is_err());
    assert!(records.first_keys(27, None).is_err());
    assert_eq!(records.work().calls, 1);
    assert_eq!(records.work().terminal_calls, 3);
    assert_eq!(client.diagnostics().unwrap().admitted, before.admitted);
    let custody = records.into_custody();
    assert_eq!(custody.scope, scope);
    assert!(matches!(&custody.failure,Some(WorkspaceError::Content(error)) if *error==original));
    drop(custody);
    no_credits(&client);
    release(&client, scope);
    owner.stop().unwrap();
}
