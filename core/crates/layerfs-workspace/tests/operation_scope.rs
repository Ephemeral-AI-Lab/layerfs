//! Shared immutable allowance and actual scoped Workspace/owner transitions.
mod common;
mod harness;

use common::{file, fixture, Store};
use layerfs_content::filesystem::{
    update_filesystem, FilesystemInput, FilesystemObjects, FilesystemResources, InodeUpdate,
};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_workspace::{
    BaseView, CanonicalCache, CanonicalClient, Operation, Outcome, Position, Time,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct OperationSource {
    store: Store,
    fail: Option<ObjectId>,
    failed: AtomicBool,
}
impl OperationSource {
    fn new(store: Store, fail: Option<ObjectId>) -> Self {
        Self {
            store,
            fail,
            failed: AtomicBool::new(false),
        }
    }
}
impl AuthenticatedObjects for OperationSource {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if self.failed.load(Ordering::Relaxed) || self.fail.is_some_and(|id| ids.contains(&id)) {
            self.failed.store(true, Ordering::Relaxed);
            return Err(ContentError::ProviderFailure {
                what: "retained operation source failure",
            });
        }
        self.store.read_canonical_batch(ids)
    }
}

fn client(store: &Store, cache: Arc<CanonicalCache>) -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::with_cache(
        Arc::new(store.clone()),
        Some(Arc::new(store.clone())),
        cache,
    ))
}

#[test]
fn fresh_clients_share_one_allowance_and_return_independent_bytes() {
    let store = Store::default();
    let a = file(&store, &[1; 32]);
    let b = file(&store, &[2; 32]);
    let c = file(&store, &[3; 32]);
    let expected = store.objects.lock().unwrap().get(&a).unwrap().clone();
    let allowance = expected.len() + 256;
    let cache = Arc::new(CanonicalCache::new(allowance));
    let first = client(&store, cache.clone());
    let mut returned = first.read_canonical(a).unwrap();
    assert_eq!(returned, expected);
    returned.fill(0);
    let demand = store.demand.load(Ordering::Relaxed);
    let second = client(&store, cache.clone());
    assert_eq!(second.read_canonical(a).unwrap(), expected);
    assert_eq!(store.demand.load(Ordering::Relaxed), demand);
    assert_eq!(first.diagnostics().unwrap(), second.diagnostics().unwrap());
    for root in [b, c] {
        let operation = client(&store, cache.clone());
        operation.read_canonical(root).unwrap();
        let work = cache.diagnostics().unwrap();
        assert!(work.charged_cache_bytes <= allowance);
        assert_eq!(work.cached_objects, 1);
    }
    let work = cache.diagnostics().unwrap();
    assert_eq!(work.cache_hits, 1);
    assert_eq!(work.upstream_batches, 3);
    assert_eq!(work.evictions, 2);
    assert_eq!(work.authenticated_bytes, (expected.len() * 3) as u64);
}

#[test]
fn failed_operation_provider_does_not_poison_shared_cache_or_fresh_provider() {
    let store = Store::default();
    let cached = file(&store, b"cached");
    let demanded = file(&store, b"not yet cached");
    let cache = Arc::new(CanonicalCache::new(4096));
    let source = Arc::new(OperationSource::new(store.clone(), Some(demanded)));
    let failed = CanonicalClient::with_cache(source.clone(), None, cache.clone());
    let expected = failed.read_canonical(cached).unwrap();
    let failure = failed.read_canonical(demanded).unwrap_err();
    assert_eq!(
        failure,
        ContentError::ProviderFailure {
            what: "retained operation source failure"
        }
    );
    assert!(source.failed.load(Ordering::Relaxed));
    let replacement_source = Arc::new(OperationSource::new(store.clone(), None));
    let replacement = CanonicalClient::with_cache(replacement_source.clone(), None, cache.clone());
    let before = store.demand.load(Ordering::Relaxed);
    assert_eq!(replacement.read_canonical(cached).unwrap(), expected);
    assert_eq!(store.demand.load(Ordering::Relaxed), before);
    replacement.read_canonical(demanded).unwrap();
    assert!(!replacement_source.failed.load(Ordering::Relaxed));
    assert!(source.failed.load(Ordering::Relaxed));
    assert_eq!(cache.diagnostics().unwrap().upstream_batches, 3);
    assert_eq!(cache.diagnostics().unwrap().cached_objects, 2);
    // A later uncached demand on the original provider still refuses; sharing
    // successful bytes does not reset or replay that provider's first failure.
    let other = file(&store, b"another uncached object");
    let before = store.demand.load(Ordering::Relaxed);
    assert_eq!(failed.read_canonical(other).unwrap_err(), failure);
    assert_eq!(store.demand.load(Ordering::Relaxed), before);
}

#[test]
fn checked_base_client_override_performs_no_acquisition_and_uses_new_lengths() {
    let f = fixture();
    let cache = Arc::new(CanonicalCache::new(0));
    let original = BaseView::open(client(&f.store, cache.clone()), f.root, f.scope).unwrap();
    let before = f.store.demand.load(Ordering::Relaxed);
    let scoped = original.with_client(client(&f.store, cache));
    assert_eq!(f.store.demand.load(Ordering::Relaxed), before);
    assert_eq!(scoped.identity(), original.identity());
    assert_eq!(scoped.root(), original.root());
    assert_eq!(scoped.stat(2).unwrap(), original.stat(2).unwrap());
    let without_lengths =
        original.with_client(Arc::new(CanonicalClient::new(Arc::new(f.store.clone()), 0)));
    assert!(matches!(
        without_lengths.stat(2),
        Err(layerfs_workspace::WorkspaceError::MissingLengthProvider)
    ));
    assert_eq!(original.stat(2).unwrap().logical_len, 10);
}

#[test]
fn scoped_views_share_original_install_serials_and_keep_original_provider() {
    let b = harness::Bench::with_cache("operation-scoped-install", 0);
    let store = &b.fixture.store;
    let cache = Arc::new(CanonicalCache::new(0));
    let fail_id = file(store, b"trigger this operation's retained failure");
    let source = Arc::new(OperationSource::new(store.clone(), Some(fail_id)));
    let operation_client = Arc::new(CanonicalClient::with_cache(
        source.clone(),
        Some(Arc::new(store.clone())),
        cache.clone(),
    ));
    let before = b.demand();
    let scoped = b.workspace.scoped(operation_client.clone()).unwrap();
    let other = b.workspace.scoped(client(store, cache.clone())).unwrap();
    assert_eq!(b.demand(), before, "scoping must not acquire the root");
    assert_eq!(scoped.route(), b.workspace.route());
    assert_eq!(other.route(), scoped.route());
    assert_eq!(b.workspace.next_serial(&b.allocator).unwrap(), 1000);
    assert_eq!(scoped.next_serial(&b.allocator).unwrap(), 1001);
    assert_eq!(other.next_serial(&b.allocator).unwrap(), 1002);
    assert_eq!(b.allocator.calls.load(Ordering::Relaxed), 1);
    let original = b.workspace.base().unwrap();
    let retained = original.plan_read(2, 0, 32).unwrap();
    let changed: &[u8] = b"changed!!!";
    let owned = b.overlay.acquire_base_source(b.route(), 901).unwrap();
    let view = scoped.view_for_source(owned).unwrap();
    let result = scoped
        .mutate(
            &b.overlay,
            &b.allocator,
            &view,
            Operation::Write {
                serial: 2,
                position: Position::At(0),
                data: changed.into(),
            },
            Time {
                seconds: i64::MAX,
                nanoseconds: 999_999_999,
            },
        )
        .unwrap();
    let Outcome::Applied { publication, .. } = result else {
        panic!("write must have an original publication");
    };
    b.overlay.reply_attempted(publication).unwrap();
    drop(view);
    b.overlay.release_base_source(owned).unwrap();
    let capture = b.overlay.capture(b.route()).unwrap();
    let mut value = original.inode(2).unwrap().value;
    value.content_root = file(store, changed);
    let updates = [InodeUpdate { serial: 2, value }];
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let next = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(b.fixture.root),
            scope: b.fixture.scope,
            root_serial: 1,
            directories: &[],
            inodes: &updates,
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let prepared = scoped.prepare_base_install(capture, next).unwrap();
    operation_client.read_canonical(fail_id).unwrap_err();
    assert!(source.failed.load(Ordering::Relaxed));
    scoped.install_prepared_base(&b.overlay, prepared).unwrap();
    assert_eq!(
        b.overlay.state(b.route()).unwrap().base_root,
        next.0.to_bytes()
    );
    for workspace in [&b.workspace, &other] {
        assert_eq!(workspace.base().unwrap().identity(), next);
        let mut output = Vec::new();
        workspace
            .base()
            .unwrap()
            .plan_read(2, 0, 32)
            .unwrap()
            .emit(&mut output)
            .unwrap();
        assert_eq!(output, changed);
    }
    assert!(scoped.base().unwrap().plan_read(2, 0, 32).is_err());
    let mut old_bytes = Vec::new();
    retained.emit(&mut old_bytes).unwrap();
    assert_eq!(old_bytes, b"original\0\xff");
    assert_eq!(b.workspace.next_serial(&b.allocator).unwrap(), 1003);
    assert_eq!(b.allocator.calls.load(Ordering::Relaxed), 1);
    let fresh = other.scoped(client(store, cache)).unwrap();
    assert_eq!(fresh.base().unwrap().identity(), next);
    assert_eq!(fresh.next_serial(&b.allocator).unwrap(), 1004);
    assert_eq!(b.allocator.calls.load(Ordering::Relaxed), 1);
}
