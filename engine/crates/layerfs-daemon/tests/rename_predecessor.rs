//! #352 F-063: a regular file renamed over another offers the replaced file
//! to its Commit as the delta base of its chunks. git, editors and package
//! managers save by writing a new file and renaming it over the old one, so
//! the result is a fresh inode with no base file of its own.
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

/// Bytes that do not compress: every chunk's FULL record is its own size.
fn noise(seed: u64, length: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}
/// A rewrite that changes four bytes every 6 KiB, as git refreshing the
/// stat data of an index's entries does: every chunk changes a little.
fn refreshed(data: &[u8], tag: u8) -> Vec<u8> {
    let mut out = data.to_vec();
    for at in (100..out.len()).step_by(6 * 1024) {
        for byte in &mut out[at..at + 4] {
            *byte ^= tag;
        }
    }
    out
}
/// `path` written whole as a new file, then renamed over `target`.
fn save_over(workspace: &BoundWorkspace, path: &str, target: &str, data: &[u8]) {
    put(workspace, path, 0o644, data);
    let (from, from_name) = split(path);
    let (to, to_name) = split(target);
    let (from, to) = (serial(workspace, from), serial(workspace, to));
    mutate(workspace, |_| Operation::Rename {
        parent: from,
        name: name(from_name),
        new_parent: to,
        new_name: name(to_name),
        replace: true,
        destination_path: None,
    });
}
/// Chunks the Commit stored as PREFIX records and as FULL ones.
fn chunk_forms(done: &CommitSuccess) -> (u64, u64) {
    let depths = &done.saved.delta.chunk_depths;
    (depths[1..].iter().sum(), depths[0])
}
/// The content root a plain fresh write of `data` commits, in a Store of its own.
fn plain_root(label: &str, data: &[u8]) -> InodeValue {
    let s = setup(label);
    put(&s.workspace, "plain", 0o644, data);
    let (done, _) = commit(&s.workspace);
    let value = committed(&s.store, &s.workspace, "plain").1;
    drop(done);
    s.finish();
    value
}

const LENGTH: usize = 400 * 1024;

#[test]
fn a_rename_over_from_another_directory_deltas_its_chunks() {
    let s = setup("predecessor-rename");
    let w = &s.workspace;
    mkdir(w, "repo");
    mkdir(w, "tmp");
    let first = noise(1, LENGTH);
    put(w, "repo/index", 0o644, &first);
    let (base, _) = commit(w);
    let second = refreshed(&first, 0x5A);
    save_over(w, "tmp/index.lock", "repo/index", &second);
    let (done, work) = commit(w);
    assert_eq!(work.files_constructed, 1);
    let (prefix, full) = chunk_forms(&done);
    assert!(prefix > 0 && full == 0, "prefix {prefix} full {full}");
    // Every chunk changed, and each is stored as a small delta.
    assert!(
        done.saved.stored_bytes < (LENGTH / 4) as u64,
        "{} bytes stored for {LENGTH}",
        done.saved.stored_bytes
    );
    assert_eq!(content(w, "repo/index", LENGTH), second);
    // The hint changes how chunks are stored, never what the file is.
    let value = committed(&s.store, w, "repo/index").1;
    assert_eq!(
        value.content_root,
        plain_root("predecessor-rename-plain", &second).content_root
    );
    drop((base, done));
    s.finish();
}

/// git writes the index twice in one Workspace (add, then commit): the second
/// rename replaces the first, which inherits the base file as predecessor.
#[test]
fn a_chain_of_renames_before_a_commit_ends_at_the_base_file() {
    let s = setup("predecessor-chain");
    let w = &s.workspace;
    let first = noise(2, LENGTH);
    put(w, "index", 0o644, &first);
    let (base, _) = commit(w);
    save_over(w, "index.lock", "index", &refreshed(&first, 0x11));
    let last = refreshed(&first, 0x22);
    save_over(w, "index.lock", "index", &last);
    let (done, _) = commit(w);
    let (prefix, full) = chunk_forms(&done);
    assert!(prefix > 0 && full == 0, "prefix {prefix} full {full}");
    assert_eq!(content(w, "index", LENGTH), last);
    // The next Commit's predecessor is the file the last one installed, not
    // the one before it, which the base no longer holds.
    let next = refreshed(&last, 0x33);
    save_over(w, "index.lock", "index", &next);
    let (again, _) = commit(w);
    let (prefix, full) = chunk_forms(&again);
    assert!(prefix > 0 && full == 0, "prefix {prefix} full {full}");
    assert_eq!(content(w, "index", LENGTH), next);
    drop((base, done, again));
    s.finish();
}

/// A predecessor the base does not hold offers nothing, and a file renamed
/// in place and then removed builds nothing: neither is an error.
#[test]
fn a_predecessor_the_base_does_not_hold_offers_nothing() {
    let s = setup("predecessor-missing");
    let w = &s.workspace;
    put(w, "kept", 0o644, b"base");
    let (base, _) = commit(w);
    // Replaces a file created since the base, which held no predecessor.
    let unsaved = noise(3, LENGTH);
    put(w, "fresh", 0o644, &unsaved);
    let replacement = refreshed(&unsaved, 0x44);
    save_over(w, "fresh.lock", "fresh", &replacement);
    // Replaces a base file, then is itself removed.
    save_over(w, "gone.lock", "kept", &noise(4, LENGTH));
    named(w, "kept", |parent, name| Operation::Unlink { parent, name });
    let (done, work) = commit(w);
    assert_eq!(work.files_constructed, 1);
    let (prefix, full) = chunk_forms(&done);
    assert_eq!(prefix, 0);
    assert!(full > 0);
    assert_eq!(content(w, "fresh", LENGTH), replacement);
    drop((base, done));
    s.finish();
}

/// A hard-linked file renamed back over the file that replaced its other
/// name would be its own predecessor: the rename records nothing and succeeds.
#[test]
fn a_file_renamed_back_over_its_replacement_is_not_its_own_predecessor() {
    let s = setup("predecessor-self");
    let w = &s.workspace;
    let data = noise(5, LENGTH);
    let linked = put(w, "a", 0o644, &data);
    named(w, "b", |parent, name| Operation::Link {
        serial: linked,
        parent,
        name,
    });
    let (base, _) = commit(w);
    save_over(w, "a.lock", "a", &refreshed(&data, 0x66));
    // "b" still names the replaced file; it goes back over its replacement.
    let root = serial(w, "");
    mutate(w, |_| Operation::Rename {
        parent: root,
        name: name("b"),
        new_parent: root,
        new_name: name("a"),
        replace: true,
        destination_path: None,
    });
    let (done, _) = commit(w);
    assert_eq!(content(w, "a", LENGTH), data);
    drop((base, done));
    s.finish();
}
