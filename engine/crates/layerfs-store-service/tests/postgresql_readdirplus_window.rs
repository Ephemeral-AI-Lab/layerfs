//! LP73 Ask 1-B on a PostgreSQL Store: a READDIRPLUS window reads its
//! inherited facts in batches — one batch length demand for the window and
//! one batch inode demand for each level of the inode tree the cache does not
//! already answer — as a SQLite Store does, and answers the facts a per-name
//! LOOKUP answers. What it counts is a partly warm window: the visits before
//! it read the inode table's upper level into the same cache, so its batches
//! are not a fresh mount's first window's.
//!
//! The window is driven through the product's mount path, not a fixture store:
//! a Workspace bound on the Store, OPENDIR, the reading visit, and the window's
//! facts read outside the owner over the Workspace's own base. Those are the
//! calls `layerfs-mount-ports`' `DirectoryStream` makes for a READDIRPLUS
//! reply. `Store::work()` counts the port demands themselves, so a PostgreSQL
//! and a SQLite Store are compared on the same counters.
//!
//! Needs a PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for example
//! `host=127.0.0.1 port=5432 user=layerfs password=… dbname=layerfs_test
//! sslmode=disable`). Without it the two PostgreSQL cases report NOT_RUN and
//! assert nothing, while the SQLite cases still run. The whole file is behind
//! `#![cfg(feature = "postgresql")]`, so a build without that feature runs
//! none of them.
#![cfg(feature = "postgresql")]
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;

use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, WorkspaceId,
};
use layerfs_persistence::{PostgresHandles, PostgresSelection, StoreSelection};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{ReservationBlocks, Storage, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_store_service::store::{BindRequest, StoreWork};
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{
    CanonicalCache, CanonicalClient, KernelDirectoryRead, KernelDirectoryWindow,
    KernelReadDecision, VisitFacts,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"readdirplus-window-batches";
const CURSOR: [u8; 32] = [73; 32];
const PROJECT: &str = "window";
const BRANCH_NAME: &str = "work";
/// The daemon's Store object cache.
const CACHE_BYTES: usize = 64 * 1024 * 1024;
/// Most OPENDIR rounds one undecided visit gets before the test fails.
const ROUNDS: usize = 4;
/// The branch the SQLite fixture's installed Store already holds: stack
/// `[23; 16]`, branch `[24; 16]`, named `main` under the project `installed`.
const INSTALLED_BRANCH: [u8; 16] = [24; 16];

/// The names below the root: `files` regular files of known bytes, and one
/// subdirectory holding one file of its own.
fn tree(files: usize) -> Vec<(String, Vec<u8>)> {
    (0..files)
        .map(|n| {
            let name = format!("f{n:04}");
            let bytes = vec![(n % 251) as u8; 16 + n % 97];
            (name, bytes)
        })
        .collect()
}

fn build(source: &Path, files: usize) {
    for (name, bytes) in tree(files) {
        std::fs::write(source.join(name), bytes).unwrap();
    }
    let sub = source.join("sub");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("inner"), b"the subdirectory's only file\n").unwrap();
}

/// The port demands one window's facts made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Counts {
    object_batches: u64,
    object_ids: u64,
    length_batches: u64,
    length_ids: u64,
}
impl Counts {
    fn between(before: StoreWork, after: StoreWork) -> Self {
        Self {
            object_batches: after.object_batches - before.object_batches,
            object_ids: after.object_ids - before.object_ids,
            length_batches: after.length_batches - before.length_batches,
            length_ids: after.length_ids - before.length_ids,
        }
    }
}
/// One entry's facts as the reply showed them.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Fact {
    name: String,
    kind: InodeKind,
    metadata: PortableMetadata,
    logical_len: u64,
    subdirs: u64,
}
impl Fact {
    /// What two Stores must answer identically: name, kind, permission bits,
    /// length and child-directory count. The mtime is the source tree's, and
    /// each test builds its own.
    fn shape(&self) -> (&str, InodeKind, u32, u64, u64) {
        (
            &self.name,
            self.kind,
            self.metadata.mode,
            self.logical_len,
            self.subdirs,
        )
    }
}

/// What one READDIRPLUS window answered and what it cost.
struct Measured {
    counts: Counts,
    facts: Vec<Fact>,
    exhausted: bool,
}

/// One READDIRPLUS window of `branch`'s root, driven as a mount drives
/// it: OPENDIR, the reading visit, one Store reader for the window's facts,
/// and the reply's offsets published and then reused.
fn measure(
    service: &StoreService,
    owner: &Owner,
    files: usize,
    tag: &str,
    branch: BranchId,
) -> Measured {
    let bound = service
        .store
        .bind(
            owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([91; 32]).unwrap(),
                branch,
            },
        )
        .unwrap_or_else(|refusal| panic!("{tag}: bind refused: {refusal:?}"))
        .workspace;
    let operation = bound.operation().unwrap();
    let route = bound.route();
    let jobs = operation.overlay();
    let workspace = operation.workspace();
    let base = workspace.base().unwrap();
    let root = base.root().root_inode().serial();
    let mount = jobs.kernel_mount(route, root).unwrap();
    // The cache starts empty, so every fact this window shows is the Store's
    // to read. The OPENDIR below and the facts it supplies fill it with the
    // inode table's upper level, so the window that follows is not a fresh
    // mount's first one.
    let resident = Arc::new(CanonicalClient::resident(Arc::new(CanonicalCache::new(
        1 << 20,
    ))));

    // OPENDIR: owner jobs that ask the Store nothing, and the facts they name
    // read outside them.
    let mut facts = VisitFacts::default();
    let mut directory = None;
    for _ in 0..ROUNDS {
        let visit = workspace
            .kernel_opendir_visit(resident.clone(), mount, 1, root, Arc::new(facts.clone()))
            .unwrap();
        let before = service.store.work();
        let outcome = jobs.kernel_observe_visit(route, Box::new(visit)).unwrap();
        assert_eq!(
            service.store.work(),
            before,
            "{tag}: an owner job made a Store demand"
        );
        match &outcome.decision {
            Some(KernelReadDecision::Needs(needs)) => facts
                .supply(&workspace.base().unwrap(), needs, None)
                .unwrap(),
            Some(KernelReadDecision::Value(_)) => {
                directory = outcome.directory_candidate;
                break;
            }
            other => panic!("{tag}: undecided OPENDIR: {other:?}"),
        }
    }
    let directory = directory.unwrap_or_else(|| panic!("{tag}: an open directory"));

    // The reading visit: one read-only owner job that makes no Store demand.
    let visit = workspace
        .kernel_directory_visit(
            resident.clone(),
            mount,
            root,
            directory.owner_id(),
            KernelDirectoryRead {
                offset: 2,
                after: None,
                attributes: true,
            },
        )
        .unwrap();
    let before_visit = service.store.work();
    let reply = jobs.kernel_directory_visit(route, visit).unwrap();
    assert_eq!(
        service.store.work(),
        before_visit,
        "{tag}: the reading visit made a Store demand"
    );
    let window = KernelDirectoryWindow::clone(&reply);
    drop(reply);
    assert!(
        window.needs_base(true),
        "{tag}: the window leaves its facts to the Store"
    );

    // The window's facts, outside the owner, over the Workspace's reader: one
    // batch length demand for the window and one batch inode demand per tree
    // level the OPENDIR left unread.
    let before = service.store.work();
    let batch = window.finish_with(Some(&base), true).unwrap();
    let after = service.store.work();
    let counts = Counts::between(before, after);
    eprintln!("{tag}: port demands of one window: {counts:?}");

    let names: Vec<Vec<u8>> = batch
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    let mut shown = Vec::with_capacity(batch.entries.len());
    for entry in &batch.entries {
        let name = String::from_utf8(entry.name.clone()).unwrap();
        let stat = entry
            .stat
            .as_ref()
            .unwrap_or_else(|| panic!("{tag}: no facts for {name}"));
        assert_eq!(stat.kind, entry.kind, "{tag}: kind of {name}");
        // A per-name LOOKUP answers the same facts, field for field: the
        // window's batch read and the single demand agree.
        let lookup = base.stat(entry.serial).unwrap();
        assert_eq!(stat.kind, lookup.value.kind, "{tag}: LOOKUP kind of {name}");
        assert_eq!(
            stat.metadata, lookup.metadata,
            "{tag}: LOOKUP metadata of {name}"
        );
        assert_eq!(
            stat.logical_len, lookup.logical_len,
            "{tag}: LOOKUP length of {name}"
        );
        shown.push(Fact {
            name,
            kind: stat.kind,
            metadata: stat.metadata,
            logical_len: stat.logical_len,
            subdirs: stat.subdirs,
        });
    }
    // Every inherited name and the subdirectory, exactly once, with the bytes
    // the fixture wrote.
    let expected: BTreeMap<String, usize> = tree(files)
        .into_iter()
        .map(|(name, bytes)| (name, bytes.len()))
        .collect();
    assert_eq!(shown.len(), files + 1, "{tag}: every name of the window");
    for fact in &shown {
        if let Some(len) = expected.get(&fact.name) {
            assert_eq!(
                fact.kind,
                InodeKind::RegularFile,
                "{tag}: kind {}",
                fact.name
            );
            assert_eq!(fact.logical_len, *len as u64, "{tag}: length {}", fact.name);
        } else {
            assert_eq!(fact.name, "sub", "{tag}: the subdirectory");
            assert_eq!(fact.kind, InodeKind::Directory, "{tag}: kind sub");
            assert_eq!(fact.subdirs, 0, "{tag}: sub holds no directory");
            assert_eq!(fact.logical_len, 0, "{tag}: a directory has no length");
        }
    }

    // The reply's offsets are published once, and a request inside that reply
    // is answered at them instead of recording them again: the handles a
    // READDIRPLUS reply hands out follow these offsets.
    let first = batch.first;
    let exhausted = batch.exhausted;
    assert!(exhausted, "{tag}: one window lists the root");
    let offer = batch
        .publish
        .clone()
        .unwrap_or_else(|| panic!("{tag}: a fresh reply publishes its names"));
    jobs.kernel_directory_publish(route, offer, names.clone())
        .unwrap();
    let reuse = workspace
        .kernel_directory_visit(
            resident.clone(),
            mount,
            root,
            directory.owner_id(),
            KernelDirectoryRead {
                offset: 2,
                after: None,
                attributes: true,
            },
        )
        .unwrap();
    let reply = jobs.kernel_directory_visit(route, reuse).unwrap();
    let window = KernelDirectoryWindow::clone(&reply);
    drop(reply);
    let reused = window.finish_with(Some(&base), true).unwrap();
    assert_eq!(
        reused
            .entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>(),
        names,
        "{tag}: the published reply is answered at its offsets"
    );
    assert_eq!(reused.first, first, "{tag}: at the published offsets");
    assert!(
        reused.publish.is_none(),
        "{tag}: a reused reply records nothing again"
    );
    // A seek inside the published reply resumes strictly after its name, and
    // hands out a fresh range for what follows it.
    let seek = workspace
        .kernel_directory_visit(
            resident,
            mount,
            root,
            directory.owner_id(),
            KernelDirectoryRead {
                offset: first + names.len() as u64 - 2,
                after: None,
                attributes: true,
            },
        )
        .unwrap();
    let reply = jobs.kernel_directory_visit(route, seek).unwrap();
    let window = KernelDirectoryWindow::clone(&reply);
    drop(reply);
    let next = window.finish_with(Some(&base), true).unwrap();
    assert_eq!(
        next.entries
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>(),
        names[names.len() - 1..],
        "{tag}: the offset resumes strictly after its name"
    );
    assert!(
        next.entries.iter().all(|entry| entry.stat.is_some()),
        "{tag}: the resumed window's facts too"
    );

    jobs.kernel_directory_close(route, mount, root, directory.owner_id())
        .unwrap();
    jobs.kernel_revoke(route, mount).unwrap();
    bound.close().unwrap();
    owner.client().retire_namespace(route.namespace()).unwrap();
    Measured {
        counts,
        facts: shown,
        exhausted,
    }
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

/// The test's own PostgreSQL schema and source tree, dropped when it ends,
/// pass or fail.
struct Postgres {
    url: String,
    schema: String,
    directory: PathBuf,
    stack: LayerStackId,
}
impl Postgres {
    fn new(tag: &str, url: &str, files: usize, stack: [u8; 16]) -> Self {
        let directory =
            std::env::temp_dir().join(format!("lfs73-window-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let schema = format!("lfs73_window_{tag}_{}", std::process::id());
        let source = directory.join("source");
        std::fs::create_dir(&source).unwrap();
        build(&source, files);
        Self {
            url: url.to_owned(),
            schema,
            directory,
            stack: LayerStackId::from_authority(stack),
        }
    }
    /// One PostgreSQL Store holding the fixture's tree on one Branch.
    fn open(&self) -> (Owner, StoreService) {
        let selection = PostgresSelection::new(&self.url, &self.schema);
        let created =
            PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                .unwrap();
        let storage = Storage::new(created.storage.clone()).unwrap();
        let initialized = Timing::disabled("init", |timing| {
            init(
                &storage,
                &created.history,
                InitRequest {
                    source: &self.directory.join("source"),
                    acquisition: &created.acquisition,
                    stack: self.stack,
                    name: HistoryName::new(PROJECT).unwrap(),
                    scope_seed: [75; 32],
                    deadline: Instant::now() + Duration::from_secs(120),
                },
                timing,
            )
        })
        .0
        .unwrap();
        created
            .history
            .fork(&ForkRequest {
                stack: initialized.stack.id,
                branch: pg_branch(self.stack),
                name: HistoryName::new(BRANCH_NAME).unwrap(),
                source: ForkSource::Layer(initialized.stack.head_layer),
            })
            .unwrap();
        drop(storage);
        drop(created);
        let config = OwnerConfig::default();
        let opened = open_store_observed(
            StoreSelection::Postgres(selection),
            BINDING,
            CURSOR,
            config.namespaces,
            CACHE_BYTES,
            ReservationBlocks::default(),
        )
        .unwrap();
        let owner = Owner::start(
            &self.directory.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            config,
        )
        .unwrap();
        let service = StoreService::new(opened, &owner);
        (owner, service)
    }
}
impl Drop for Postgres {
    fn drop(&mut self) {
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema));
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// The Branch this test's PostgreSQL fixtures fork.
fn pg_branch(stack: LayerStackId) -> BranchId {
    BranchId::named(stack, &HistoryName::new(BRANCH_NAME).unwrap())
}

/// One READDIRPLUS window over an Init'd PostgreSQL Store's root.
fn postgres_window(url: &str, files: usize, tag: &str, stack: [u8; 16]) -> Measured {
    let fixture = Postgres::new(tag, url, files, stack);
    let (owner, service) = fixture.open();
    let measured = measure(&service, &owner, files, tag, pg_branch(fixture.stack));
    drop(service);
    owner.stop().unwrap();
    measured
}

/// The same window over a SQLite Store built from the same tree, through the
/// installed-Store fixture the Store's own tests use.
fn sqlite_window(files: usize, tag: &str) -> Measured {
    let host = installed::Fixture::built(tag, |source| {
        build(source, files);
        files + 2
    });
    let config = OwnerConfig::default();
    let opened = open_store_observed(
        host.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        config.namespaces,
        CACHE_BYTES,
        ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &host.directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        config,
    )
    .unwrap();
    let service = StoreService::new(opened, &owner);
    let measured = measure(
        &service,
        &owner,
        files,
        tag,
        BranchId::from_authority(INSTALLED_BRANCH),
    );
    drop(service);
    owner.stop().unwrap();
    host.cleanup();
    measured
}

#[test]
fn a_postgresql_window_reads_its_facts_in_batches() {
    let Some(url) = url() else {
        return;
    };
    let small = postgres_window(&url, 8, "small", [81; 16]);
    let large = postgres_window(&url, 200, "large", [82; 16]);
    // One length demand carries the whole window, and it names each ordinary
    // file once: no entry asks the provider for its own length.
    for (measured, files) in [(&small, 8u64), (&large, 200)] {
        assert_eq!(measured.counts.length_batches, 1, "one length demand");
        assert_eq!(
            measured.counts.length_ids, files,
            "the one length demand names each file once"
        );
    }
    // The window's object demands and length demands do not grow with the
    // names it lists: one batch length demand carries the window's ordinary
    // files, and one batch inode demand carries its inherited serials for each
    // tree level the cache does not already hold, so the names add no batch
    // while a deeper tree adds one. Here the visits before the window read the
    // tree's upper level into the same cache, leaving one level: these are not
    // a fully cold window's counts. This
    // tree's metadata roots are inline and its subdirectory's child count was
    // built with the Commit, so no entry here costs a read of its own; an
    // entry whose metadata root is an object, a subdirectory counted by a scan
    // and a symlink's target are still read one at a time.
    assert_eq!(
        large.counts.object_batches, small.counts.object_batches,
        "8 names: {:?}, 200 names: {:?}",
        small.counts, large.counts
    );
    for (measured, files) in [(&small, 8u64), (&large, 200)] {
        assert_eq!(
            measured.counts.object_batches, 1,
            "{files} names: one batch inode demand"
        );
    }
    assert_eq!(small.facts.len(), 9);
    assert_eq!(large.facts.len(), 201);
    assert!(small.exhausted && large.exhausted);
}

#[test]
fn a_sqlite_window_makes_the_same_batches_and_answers_the_same_facts() {
    let sqlite = sqlite_window(40, "sqlite");
    assert_eq!(
        (sqlite.counts.object_batches, sqlite.counts.length_batches),
        (1, 1),
        "one batch inode demand and one length demand: {:?}",
        sqlite.counts
    );
    assert_eq!(sqlite.counts.length_ids, 40);
    assert_eq!(sqlite.facts.len(), 41);
    let Some(url) = url() else {
        return;
    };
    // The same tree on a PostgreSQL Store, mounted and listed the same way.
    let postgres = postgres_window(&url, 40, "compare", [83; 16]);
    assert_eq!(
        (
            postgres.counts.object_batches,
            postgres.counts.length_batches,
            postgres.counts.length_ids
        ),
        (
            sqlite.counts.object_batches,
            sqlite.counts.length_batches,
            sqlite.counts.length_ids
        ),
        "the same demands on both Stores: PostgreSQL {:?}, SQLite {:?}",
        postgres.counts,
        sqlite.counts
    );
    // What both Stores must answer identically. The mtime of each fixture's
    // own source tree differs between the two runs; it is compared there with
    // that tree's own LOOKUP, not across Stores.
    assert_eq!(
        postgres.facts.iter().map(Fact::shape).collect::<Vec<_>>(),
        sqlite.facts.iter().map(Fact::shape).collect::<Vec<_>>(),
        "the same facts on both Stores"
    );
    // The offset a reply starts at is the handle's own counter, not a cross
    // Store contract; that it lists the whole directory in one window is.
    assert!(postgres.exhausted && sqlite.exhausted);
}
