//! #364 item 2 (b): a captured Commit records the Store digest of each fresh
//! file the overlay followed from creation, and a fresh file whose digest and
//! length a saved root already has reuses that object: its own row, serial
//! and metadata stay (owner decision, object reuse only).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod support;
use layerfs_content::filesystem::{FilesystemRead, FilesystemRootId, PathName};
use layerfs_content::object::inode_leaf::InodeValue;
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, CommitSuccess, Store},
    Owner, OwnerConfig,
};
use layerfs_history::{CommitOutcome, WorkspaceId};
use layerfs_overlay::ProfileConfig;
use layerfs_storage::ReservationBlocks;
use layerfs_workspace::{CapturedNamespaceWork, Operation, Outcome, Position, Time};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

fn name(text: &str) -> PathName {
    PathName::new(text).unwrap()
}
/// One mutation under its own base source; the published reply is attempted.
fn mutate(workspace: &BoundWorkspace, operation: impl FnOnce(u64) -> Operation) {
    static NEXT: AtomicU64 = AtomicU64::new(100);
    let held = workspace.operation().unwrap();
    let route = workspace.route();
    let source = held
        .overlay()
        .acquire_base_source(route, NEXT.fetch_add(1, Ordering::Relaxed))
        .unwrap();
    let view = held.workspace().view_for_source(source).unwrap();
    let outcome = held
        .workspace()
        .mutate(
            held.overlay(),
            held.ports(),
            &view,
            operation(view.root_serial()),
            Time {
                seconds: 7,
                nanoseconds: 0,
            },
        )
        .unwrap();
    let Outcome::Applied { publication, .. } = outcome else {
        panic!("{outcome:?}");
    };
    held.overlay().reply_attempted(publication).unwrap();
    drop(view);
    held.overlay().release_base_source(route, source).unwrap();
}
/// The live serial at `path`: "" is the root, else names joined by '/'.
fn serial(workspace: &BoundWorkspace, path: &str) -> u64 {
    let held = workspace.operation().unwrap();
    let route = workspace.route();
    let source = held.overlay().acquire_base_source(route, 99).unwrap();
    let view = held.workspace().view_for_source(source).unwrap();
    let mut serial = view.root_serial();
    for step in path.split('/').filter(|step| !step.is_empty()) {
        serial = view
            .lookup(held.overlay(), serial, &name(step))
            .unwrap()
            .serial;
    }
    drop(view);
    held.overlay().release_base_source(route, source).unwrap();
    serial
}
/// The live bytes at `path`.
fn content(workspace: &BoundWorkspace, path: &str, length: usize) -> Vec<u8> {
    let at = serial(workspace, path);
    let held = workspace.operation().unwrap();
    let route = workspace.route();
    let source = held.overlay().acquire_base_source(route, 98).unwrap();
    let view = held.workspace().view_for_source(source).unwrap();
    let mut bytes = Vec::new();
    while bytes.len() < length {
        let before = bytes.len();
        view.read(held.overlay(), at, before as u64, 65_536, &mut bytes)
            .unwrap();
        assert!(bytes.len() > before, "{path}");
    }
    drop(view);
    held.overlay().release_base_source(route, source).unwrap();
    bytes
}
fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}
fn named(
    workspace: &BoundWorkspace,
    path: &str,
    operation: impl FnOnce(u64, PathName) -> Operation,
) {
    let (parent, child) = split(path);
    let parent = serial(workspace, parent);
    mutate(workspace, |_| operation(parent, name(child)));
}
fn create(workspace: &BoundWorkspace, path: &str, mode: u32) -> u64 {
    named(workspace, path, |parent, name| Operation::Create {
        parent,
        name,
        mode,
    });
    serial(workspace, path)
}
fn mkdir(workspace: &BoundWorkspace, path: &str) {
    named(workspace, path, |parent, name| Operation::Mkdir {
        parent,
        name,
        mode: 0o755,
    });
}
fn write(workspace: &BoundWorkspace, serial: u64, offset: u64, data: &[u8]) {
    mutate(workspace, |_| Operation::Write {
        serial,
        position: Position::At(offset),
        data: data.into(),
    });
}
/// A fresh file written as appended chunks, as an SDK write sends them.
fn put(workspace: &BoundWorkspace, path: &str, mode: u32, data: &[u8]) -> u64 {
    let serial = create(workspace, path, mode);
    let mut offset = 0;
    for piece in data.chunks(64 * 1024) {
        write(workspace, serial, offset, piece);
        offset += piece.len() as u64;
    }
    serial
}
/// The committed serial and value at `path` under the Branch's head.
fn committed(store: &Store, workspace: &BoundWorkspace, path: &str) -> (u64, InodeValue) {
    let storage = store.producer().unwrap();
    let reader = storage.reader().unwrap();
    let snapshot = workspace.snapshot().unwrap();
    let mut read = FilesystemRead::new(&reader, FilesystemRootId(snapshot.effective_root)).unwrap();
    let mut serial = read.root().root_inode().serial();
    let mut value = None;
    for step in path.split('/') {
        let resolved = read.resolve_child(serial, &name(step)).unwrap();
        serial = resolved.serial;
        value = Some(resolved.value);
    }
    (serial, value.unwrap())
}
fn commit(workspace: &BoundWorkspace) -> (CommitSuccess, CapturedNamespaceWork) {
    let done = workspace.commit_captured(&Default::default()).unwrap();
    assert!(
        matches!(done.history, Some(CommitOutcome::Committed { .. })),
        "{:?}",
        done.history
    );
    let work = done.namespace.as_ref().unwrap().work.unwrap();
    (done, work)
}
fn bytes(tag: u8, length: usize) -> Vec<u8> {
    (0..length).map(|at| tag ^ (at % 251) as u8).collect()
}

struct Setup {
    f: support::Fixture,
    store: Arc<Store>,
    owner: Owner,
    workspace: Arc<BoundWorkspace>,
}
fn setup(label: &str) -> Setup {
    let f = support::Fixture::new(8, label);
    let store = open_store(
        f.config.clone(),
        support::BINDING,
        support::CURSOR,
        OwnerConfig::default().namespaces,
        2 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &f.directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let workspace = Arc::new(
        store
            .bind(
                owner.client(),
                BindRequest {
                    branch: f.branch,
                    workspace: WorkspaceId::from_authority([31; 32]).unwrap(),
                },
            )
            .unwrap()
            .workspace,
    );
    Setup {
        f,
        store,
        owner,
        workspace,
    }
}
impl Setup {
    fn finish(self) {
        self.workspace.close().unwrap();
        drop((self.workspace, self.store));
        self.owner.stop().unwrap();
        self.f.cleanup();
    }
}

#[test]
fn a_commit_records_digests_of_followed_fresh_files_only() {
    let s = setup("file-digests");
    put(&s.workspace, "appended", 0o644, b"hello digest");
    // Rewritten at offset 0 after its first bytes: not followed.
    let rewritten = create(&s.workspace, "rewritten", 0o644);
    write(&s.workspace, rewritten, 0, b"other bytes");
    write(&s.workspace, rewritten, 0, b"O");
    let (done, work) = commit(&s.workspace);
    assert_eq!(work.digests_recorded, 1);
    let appended = committed(&s.store, &s.workspace, "appended").1.content_root;
    let digest = |bytes: &[u8]| *blake3::hash(bytes).as_bytes();
    let roots = s
        .store
        .history()
        .file_roots(&[(digest(b"hello digest"), 12), (digest(b"Other bytes"), 11)])
        .unwrap();
    assert_eq!(roots[0], Some(appended.to_bytes()));
    assert_eq!(roots[1], None);
    drop(done);
    s.finish();
}

/// A clean relink over files a Commit built: the directory and every file
/// are removed and written again, and one copy lands at a new path. A file
/// whose bytes a saved root holds reuses that object and keeps its own
/// serial and metadata; every other file takes the full path.
#[test]
fn a_relinked_file_with_saved_bytes_reuses_the_object() {
    let s = setup("file-digests-relink");
    let w = &s.workspace;
    mkdir(w, "nm");
    let cases = [
        ("nm/same", 0o644, bytes(1, 300_000)),
        ("nm/mode", 0o644, bytes(2, 5_000)),
        ("nm/byte", 0o644, bytes(3, 5_000)),
        ("nm/linked", 0o644, bytes(4, 5_000)),
        ("top", 0o755, bytes(5, 70_000)),
    ];
    for (path, mode, data) in &cases {
        put(w, path, *mode, data);
    }
    let (first, work) = commit(w);
    assert_eq!(work.digests_recorded, 5);
    assert_eq!(work.files_reused, 0);
    let before: Vec<(u64, InodeValue)> = cases
        .iter()
        .map(|(path, _, _)| committed(&s.store, w, path))
        .collect();
    // rm -rf nm; rm top; then everything written again.
    for (path, _, _) in &cases {
        named(w, path, |parent, name| Operation::Unlink { parent, name });
    }
    named(w, "nm", |parent, name| Operation::Rmdir { parent, name });
    mkdir(w, "nm");
    put(w, "nm/same", 0o644, &cases[0].2);
    put(w, "nm/mode", 0o600, &cases[1].2);
    let mut changed = cases[2].2.clone();
    changed[4_000] ^= 1;
    put(w, "nm/byte", 0o644, &changed);
    let linked = put(w, "nm/linked", 0o644, &cases[3].2);
    named(w, "nm/second", |parent, name| Operation::Link {
        serial: linked,
        parent,
        name,
    });
    put(w, "top", 0o755, &cases[4].2);
    mkdir(w, "elsewhere");
    put(w, "elsewhere/copy", 0o644, &cases[4].2);
    let (second, work) = commit(w);
    assert_eq!(
        work.files_reused, 4,
        "nm/same, nm/mode, top, elsewhere/copy"
    );
    let after: Vec<(u64, InodeValue)> = cases
        .iter()
        .map(|(path, _, _)| committed(&s.store, w, path))
        .collect();
    for (at, (path, _, _)) in cases.iter().enumerate() {
        // A fresh inode in every case: the serial is new.
        assert_ne!(after[at].0, before[at].0, "{path}");
    }
    // Reused: the same object. Each file's metadata is built from its own
    // row, which here has the same mode and time as before.
    assert_eq!(after[0].1, before[0].1);
    assert_eq!(after[4].1, before[4].1);
    // A mode change reuses the object; the record is the new file's own.
    assert_eq!(after[1].1.content_root, before[1].1.content_root);
    assert_ne!(after[1].1.metadata_root, before[1].1.metadata_root);
    // One byte different: a new object.
    assert_ne!(after[2].1.content_root, before[2].1.content_root);
    // Hard link: no digest, built on the full path, two names.
    assert_eq!(after[3].1.namespace_ref_count, 2);
    // At a new path: the same object.
    let copy = committed(&s.store, w, "elsewhere/copy").1;
    assert_eq!(copy.content_root, before[4].1.content_root);
    // The live files still read their own bytes after the install.
    assert_eq!(content(w, "nm/same", cases[0].2.len()), cases[0].2);
    assert_eq!(content(w, "elsewhere/copy", cases[4].2.len()), cases[4].2);
    drop((first, second));
    s.finish();
}

/// A row left by a Commit that never finished names a root the Store does
/// not hold: the file is built in full and the row takes the new root.
#[test]
fn a_recorded_root_the_store_does_not_hold_is_not_reused() {
    let s = setup("file-digests-missing");
    let data = bytes(9, 40_000);
    let digest = *blake3::hash(&data).as_bytes();
    s.store
        .history()
        .record_file_digests(&[layerfs_history::FileDigest {
            root: [0xAB; 32],
            length: data.len() as u64,
            digest,
        }])
        .unwrap();
    put(&s.workspace, "lost", 0o644, &data);
    let (done, work) = commit(&s.workspace);
    assert_eq!((work.files_reused, work.digests_recorded), (0, 1));
    let root = committed(&s.store, &s.workspace, "lost").1.content_root;
    assert_eq!(content(&s.workspace, "lost", data.len()), data);
    let rows = s
        .store
        .history()
        .file_roots(&[(digest, data.len() as u64)])
        .unwrap();
    assert_eq!(rows[0], Some(root.to_bytes()));
    drop(done);
    s.finish();
}
