//! Content whole-root qualification over the real overlay scratch records:
//! every record read and guarded batch is one short original owner job.
#[path = "../../layerfs-content/tests/support/mod.rs"]
mod content_support;
use content_support::filesystem::{name_of, synthetic, value, Session, TreeStore};
use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::{encode_inode_page, InodePage};
use layerfs_content::filesystem::{
    profile_id, qualify_root, DirectoryUpdate, FilesystemRoot, FilesystemRootId, InodeUpdate,
    QualificationWork, RootContext,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ObjectId, ObjectRole};
use layerfs_daemon::{Command, Completion, Owner, OwnerClient, OwnerConfig, Response};
use layerfs_overlay::{IndexedScope, ProfileConfig};
use layerfs_workspace::IndexedEditRecords;
use std::time::{Duration, Instant};

const DIRECTORIES: u64 = 20;
const FILES: u64 = 100;

fn job(
    client: &OwnerClient,
    route: Option<layerfs_overlay::Route>,
    command: Command,
) -> Completion {
    let pending = client
        .try_submit(route, command)
        .unwrap_or_else(|(cause, command)| panic!("{cause:?}: {command:?}"));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "bounded original owner result");
        std::thread::yield_now();
    }
}
fn scope(client: &OwnerClient, tag: u8) -> IndexedScope {
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
        other => panic!("{other:?}"),
    };
    drop(opened);
    let acquired = job(
        client,
        Some(route),
        Command::AcquireOperation { request: 1 },
    );
    let owner = match acquired.result() {
        Ok(Response::Operation(Some(owner))) => *owner,
        other => panic!("{other:?}"),
    };
    IndexedScope {
        owner,
        file_scope: u64::MAX,
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
fn context(session: &Session, root: ObjectId) -> RootContext {
    RootContext {
        root: FilesystemRootId(root),
        scope: session.scope,
        profile: profile_id(),
        root_serial: Some(session.root_serial),
    }
}

/// A real tree of 20 directories of 100 files each, with one file bound in
/// every directory as well.
fn built() -> (Session, u64, u64) {
    let mut session = Session::new(1).expect("empty");
    let shared = session.allocate();
    let mut inodes = vec![InodeUpdate {
        serial: shared,
        value: value(
            InodeKind::RegularFile,
            synthetic("r2/shared"),
            synthetic("r2/meta"),
        ),
    }];
    let mut root = Vec::new();
    let mut directories = Vec::new();
    for directory in 0..DIRECTORIES {
        let serial = session.allocate();
        root.push((name_of(&format!("dir-{directory:02}")), Some(serial)));
        inodes.push(InodeUpdate {
            serial,
            value: value(
                InodeKind::Directory,
                synthetic("r2/unused"),
                synthetic("r2/meta"),
            ),
        });
        let mut changes = Vec::new();
        for file in 0..FILES {
            let child = session.allocate();
            changes.push((name_of(&format!("file-{file:03}")), Some(child)));
            inodes.push(InodeUpdate {
                serial: child,
                value: value(
                    InodeKind::RegularFile,
                    synthetic(&format!("r2/{child}")),
                    synthetic("r2/meta"),
                ),
            });
        }
        changes.push((name_of("shared"), Some(shared)));
        directories.push(DirectoryUpdate {
            parent: serial,
            changes,
        });
    }
    directories.push(DirectoryUpdate {
        parent: 1,
        changes: root,
    });
    directories.sort_by_key(|update| update.parent);
    inodes.sort_by_key(|update| update.serial);
    let new_inodes: Vec<u64> = inodes.iter().map(|update| update.serial).collect();
    session
        .apply(&directories, &inodes, &new_inodes)
        .expect("built tree");
    (
        session,
        2 + DIRECTORIES * (FILES + 1),
        DIRECTORIES * (FILES + 2),
    )
}

/// Two names for one directory, stored as real canonical pages.
fn aliased(session: &Session) -> (TreeStore, ObjectId) {
    let mut store = TreeStore::new();
    let mut listing = |entries: Vec<(&str, u64)>| {
        let entries = entries
            .into_iter()
            .map(|(name, serial)| (name_of(name), serial))
            .collect();
        let page = encode_directory_page(&DirectoryPage::Leaf { entries }).expect("listing");
        store.insert(ObjectRole::DirectoryLeaf, page)
    };
    let entries = vec![
        (1, (0, listing(vec![("a", 2), ("b", 2)]))),
        (2, (1, listing(Vec::new()))),
    ]
    .into_iter()
    .map(|(serial, (namespace_ref_count, content_root))| {
        (
            serial,
            InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count,
                content_root,
                metadata_root: synthetic("r2/meta"),
            },
        )
    })
    .collect();
    let table = encode_inode_page(&InodePage::Leaf { entries }).expect("table");
    let table = store.insert(ObjectRole::InodeLeaf, table);
    let root = FilesystemRoot::new(profile_id(), session.scope, 1, table).expect("root");
    let root = store.insert(ObjectRole::FilesystemRoot, root.encode().expect("bytes"));
    (store, root)
}

#[test]
fn a_real_root_qualifies_through_short_original_scratch_jobs() {
    let path = std::env::temp_dir().join(format!("layerfs-r2-qualify-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let owner = Owner::start(
        &path.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let (session, inodes, bindings) = built();
    let own = context(&session, session.root);

    let first = scope(&client, 21);
    let before = client.diagnostics().unwrap();
    let mut records = IndexedEditRecords::new(&client, first);
    let mut work = QualificationWork::default();
    let started = Instant::now();
    let proof = qualify_root(&session.store, &mut records, &own, &mut work).expect("qualifies");
    let elapsed = started.elapsed();
    assert_eq!(
        (proof.inodes(), proof.directories(), proof.bindings()),
        (inodes, DIRECTORIES + 1, bindings)
    );
    assert_eq!(proof.admit(&own), Ok(session.value));
    assert!(records.failure().is_none());
    let provider = records.work();
    assert_eq!(provider.calls, work.record_reads + work.record_batches);
    assert_eq!(provider.converted_changes, work.record_changes);
    assert_eq!((provider.terminal_calls, provider.saturated), (0, false));
    let after = client.diagnostics().unwrap();
    assert_eq!(after.admitted - before.admitted, provider.calls);
    assert_eq!(after.outstanding, before.outstanding);
    println!(
        "S9_R2_QUALIFY_REAL_SCRATCH inodes={inodes} bindings={bindings} inode_pages={} \
         directory_pages={} record_reads={} record_batches={} record_changes={} \
         peak_batch_changes={} owner_jobs={} peak_conversion_heap_bytes={} \
         peak_reply_capacity_bytes={} diagnostic_wall_ms={}",
        work.inode.pages_read,
        work.directory.pages_read,
        work.record_reads,
        work.record_batches,
        work.record_changes,
        work.peak_batch_changes,
        provider.calls,
        provider.peak_conversion_heap_bytes,
        provider.peak_reply_capacity_bytes,
        elapsed.as_millis()
    );

    // The same scope is not a fresh pass: the original guarded refusal ends it.
    assert_eq!(
        qualify_root(&session.store, &mut records, &own, &mut work),
        Err(ContentError::ProviderFailure {
            what: "filesystem backing precondition"
        })
    );
    assert!(records.failure().is_some());
    drop(records.into_custody());

    // A refused graph is Content's refusal; the provider stays healthy.
    let (store, root) = aliased(&session);
    let second = scope(&client, 22);
    let mut records = IndexedEditRecords::new(&client, second);
    assert_eq!(
        qualify_root(
            &store,
            &mut records,
            &context(&session, root),
            &mut QualificationWork::default()
        ),
        Err(ContentError::InvalidRecord(
            "qualified directory has more than one binding"
        ))
    );
    assert!(records.failure().is_none());
    drop(records);

    release(&client, first);
    release(&client, second);
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    owner.stop().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
