//! Rig for mounted live Commit proofs: one sealed Disposable Store built from a
//! native base tree, opened once and kept open, one real native serving
//! assembly over it, commands launched outside the daemon, the product control
//! Commit and three independent readings of a tree (ordinary syscalls, the
//! published canonical root from a fresh bind, the kernel's stat table).
//!
//! The including test declares these modules beside this one, by these names:
//!
//! ```text
//! #[path = "support/installed_store.rs"]  mod installed;
//! #[path = "support/mounted.rs"]          mod mounted;
//! #[path = "support/namespace_model.rs"]  mod model;
//! #[path = "support/mounted_commit.rs"]   mod rig;
//! ```
//!
//! Workspace tags below 128 belong to the test; `Rig::published` uses tags
//! from 128 upward for its own short binds.
use super::{
    installed,
    model::{self, Flat, Model, Stamp, DIRECTORY, FILE, SYMLINK},
    mounted::{until, Harness, COMMAND},
};
use layerfs_bridge::control::{ReadyMount, Reply, Request, WorkspaceToken};
use layerfs_content::ObjectId;
use layerfs_daemon::{
    bootstrap::open_store,
    control::{Failure, Success},
    store::{CapturedConstruction, ReleasedOwner, Store},
};
use layerfs_history::{
    CommitHistoryRequest, CommitId, CommitRecord, CommitStagedOutcome, PageResult, WorkspaceId,
};
use layerfs_storage::ReservationBlocks;
use nix::{
    fcntl::AT_FDCWD,
    sys::{
        stat::{utimensat, UtimensatFlags},
        time::TimeSpec,
    },
};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{lchown, symlink, MetadataExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// The marker `clock_masked` puts in place of a time the clock chose.
pub const CLOCK: Stamp = (i64::MAX, 0);
/// The Branch history page size the rig asks for; the control window is 32.
pub const HISTORY: u16 = 32;

pub struct Rig {
    pub fixture: installed::Fixture,
    /// The one Store handle of the test: opened once, never sealed or reopened.
    pub store: Arc<Store>,
    pub harness: Harness,
    /// The base tree as the builder left it, read before Project Init.
    pub base: Flat,
    /// Two seconds before the rig was built. Every time the standard builder
    /// or a script's `touch -d` sets is far earlier; every time the clock
    /// chose during the test is at or after it.
    pub started: i64,
    build: Box<dyn Fn(&Path)>,
    fresh: Cell<u8>,
    replays: Cell<u32>,
}
impl Rig {
    /// The standard base tree, `base_tree`.
    pub fn new(label: &str) -> Self {
        Self::built(label, base_tree)
    }
    /// `build` fills an empty native directory. It runs once for the Store's
    /// base and once more for every `replay`, so it must be deterministic.
    pub fn built(label: &str, build: impl Fn(&Path) + 'static) -> Self {
        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - 2;
        let mut base = None;
        let fixture = installed::Fixture::built(label, |source| {
            build(source);
            let (model, names) = Model::native(source);
            base = Some(model.flat());
            names
        });
        let store = open_store(
            fixture.config.clone(),
            installed::BINDING,
            installed::CURSOR,
            2,
            2 * 1024 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        Self {
            fixture,
            store,
            harness,
            base: base.unwrap(),
            started,
            build: Box::new(build),
            fresh: Cell::new(128),
            replays: Cell::new(0),
        }
    }
    /// Binds a new Workspace of the Branch and mounts it; Ready on return.
    pub fn mount(&self, tag: u8) -> ReadyMount {
        self.harness.mount(tag)
    }
    /// One product control Commit, attempted once.
    pub fn commit(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        self.harness.try_commit(token)
    }
    /// A Commit that must publish a new record. Its receipt is checked and
    /// printed by `receipt`.
    pub fn committed(&self, token: WorkspaceToken, what: &str) -> CommitRecord {
        let done = self
            .commit(token)
            .unwrap_or_else(|failure| panic!("{what}: {failure:?}"));
        receipt(&done, what);
        match &done.reply {
            Reply::Committed(CommitStagedOutcome::Committed(record)) => record.clone(),
            other => panic!("{what}: {other:?}"),
        }
    }
    /// A Commit that must find the Branch already describing the Workspace.
    pub fn up_to_date(&self, token: WorkspaceToken, what: &str) -> (Option<CommitId>, ObjectId) {
        let done = self
            .commit(token)
            .unwrap_or_else(|failure| panic!("{what}: {failure:?}"));
        receipt(&done, what);
        match &done.reply {
            Reply::Committed(CommitStagedOutcome::UpToDate { head, root }) => (*head, *root),
            other => panic!("{what}: {other:?}"),
        }
    }
    /// The Branch's current root, which must be `root`, walked completely
    /// through a fresh bind of a new Workspace that is closed again. Reads the
    /// Store only: no mount and no Workspace of the test is consulted.
    pub fn published(&self, root: ObjectId) -> Flat {
        let tag = self.fresh.get();
        self.fresh.set(tag.checked_add(1).expect("fresh bind tags"));
        let bound = self
            .harness
            .service
            .execute_control(&Request::Mount {
                workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
                branch: self.harness.branch,
            })
            .unwrap();
        let token = match &bound.reply {
            Reply::Bound { token, binding } => {
                assert_eq!(
                    binding.effective_root, root,
                    "a fresh bind selects the published root"
                );
                *token
            }
            other => panic!("{other:?}"),
        };
        drop(bound);
        let operation = self.harness.service.operation(token).unwrap();
        let flat = model::canonical(operation.client(), root);
        assert!(operation.ports().failure().unwrap().is_none());
        drop(operation);
        let closed = self.harness.try_unmount(token).unwrap();
        assert_eq!(closed.reply, Reply::Unmounted(token));
        flat
    }
    /// The newest page of the Branch's Commit ancestry, head first.
    pub fn history(&self) -> PageResult<CommitRecord> {
        match self
            .harness
            .service
            .execute_control(&Request::History(CommitHistoryRequest {
                branch: self.harness.branch,
                start: None,
                cursor: None,
                limit: HISTORY,
            }))
            .unwrap()
            .reply
        {
            Reply::History(page) => page,
            other => panic!("{other:?}"),
        }
    }
    /// A plain native copy of the base tree, made by running the builder
    /// again, owned by the command identity. Never read by the daemon.
    pub fn replay(&self) -> PathBuf {
        let number = self.replays.get();
        self.replays.set(number + 1);
        let root = self
            .fixture
            .directory
            .join(format!("native-replay-{number}"));
        fs::create_dir(&root).unwrap();
        (self.build)(&root);
        for (path, _) in walk(&root) {
            lchown(root.join(path), Some(COMMAND), Some(COMMAND)).unwrap();
        }
        root
    }
    /// Normal terminal unmount after the connection is quiet, with a complete
    /// drain; returns the drain receipt text.
    pub fn unmount(&self, ready: &ReadyMount) -> String {
        until("connection quiescent", || {
            let work = self
                .harness
                .status(ready.token)
                .native
                .unwrap()
                .work
                .unwrap();
            work.received == 0 && work.admitted == 0
        });
        format!("{:?}", self.harness.unmount(ready).native)
    }
    /// Stops the serving assembly (no mount may remain), then drops the Store
    /// handle and removes every file of the test.
    pub fn finish(self) {
        let Self {
            fixture,
            store,
            harness,
            ..
        } = self;
        harness.stop();
        println!(
            "MOUNTED_COMMIT_RIG store_opened=1 sealed=0 reopened=0 handles_after_stop={}",
            Arc::strong_count(&store)
        );
        drop(store);
        fixture.cleanup();
    }
}

/// The mount directory of a Ready mount.
pub fn root(ready: &ReadyMount) -> &Path {
    Path::new(&ready.directory)
}
/// The product constructor's receipt of a successful Commit: both owners
/// released by two original completions, nothing retained. Prints the
/// producer's counted work as evidence.
pub fn receipt<'a>(done: &'a Success, what: &str) -> &'a CapturedConstruction {
    let commit = done
        .commit
        .as_ref()
        .unwrap_or_else(|| panic!("{what}: no Commit receipt"));
    let namespace = commit
        .namespace
        .as_ref()
        .unwrap_or_else(|| panic!("{what}: the product constructor did not run"));
    assert!(!namespace.retained(), "{what}: {namespace:?}");
    assert_eq!(
        namespace.released,
        [ReleasedOwner::Reader, ReleasedOwner::Operation],
        "{what}: reader, then owner"
    );
    assert!(
        namespace.release_error.is_none()
            && namespace.release_failure.is_none()
            && namespace.custody.is_none(),
        "{what}: {namespace:?}"
    );
    let outcome = match &commit.history {
        CommitStagedOutcome::Committed(_) => "Committed",
        CommitStagedOutcome::UpToDate { .. } => "UpToDate",
    };
    println!(
        "MOUNTED_COMMIT {what}: outcome={outcome} released={} retained=false work={:?} counters={:?}",
        namespace.released.len(),
        namespace.work,
        namespace.counters
    );
    namespace
}

fn command(identity: u32, directory: Option<&Path>, script: &str) -> Command {
    let mut command = Command::new("bash");
    command
        .args(["-c", script])
        .uid(identity)
        .gid(identity)
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .stdin(Stdio::null());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    command
}
/// Real `bash -c`, launched here by plain exec under the given uid and gid
/// and waited for here. Nothing registers it with the daemon.
pub fn bash(identity: u32, script: &str) -> Output {
    command(identity, None, script).output().unwrap()
}
/// `bash` with its working directory set before exec.
pub fn bash_in(identity: u32, directory: &Path, script: &str) -> Output {
    command(identity, Some(directory), script).output().unwrap()
}
/// A script that must exit 0; its output is shown otherwise.
pub fn passed(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what}: {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
/// A bash process that stays alive until its standard input is closed (the
/// script blocks in `read`). Killed and reaped on drop, so none is left.
pub struct Background(Option<Child>);
impl Background {
    pub fn spawn(identity: u32, directory: &Path, script: &str) -> Self {
        Self(Some(
            command(identity, Some(directory), script)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        ))
    }
    pub fn pid(&self) -> u32 {
        self.0.as_ref().unwrap().id()
    }
    /// True while the process has not exited.
    pub fn alive(&mut self) -> bool {
        self.0.as_mut().unwrap().try_wait().unwrap().is_none()
    }
    /// Closes its standard input and waits, bounded, for it to exit.
    pub fn release(mut self) -> ExitStatus {
        let mut child = self.0.take().unwrap();
        drop(child.stdin.take());
        let mut status = None;
        until("background process exit", || {
            status = child.try_wait().unwrap();
            status.is_some()
        });
        status.unwrap()
    }
}
impl Drop for Background {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A native directory tree read through ordinary syscalls: kind, mode,
/// modification time, names per inode, whole bytes or symlink target, and
/// hard-link alias classes. Works on a mount and on a plain directory alike.
pub fn native(root: &Path) -> Flat {
    Model::native(root).0.flat()
}
/// One `lstat` of the kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stat {
    pub ino: u64,
    /// `st_mode`: type and permission bits.
    pub mode: u32,
    pub nlink: u64,
    pub size: u64,
    pub mtime_sec: i64,
    pub mtime_nsec: i64,
    pub uid: u32,
    pub gid: u32,
}
/// Path relative to the root; the root itself is the empty path.
pub type Kernel = BTreeMap<PathBuf, Stat>;
fn walk(root: &Path) -> Vec<(PathBuf, fs::Metadata)> {
    let mut found = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(root.join(&path)).unwrap();
        if metadata.is_dir() {
            for entry in fs::read_dir(root.join(&path)).unwrap() {
                pending.push(path.join(entry.unwrap().file_name()));
            }
        }
        found.push((path, metadata));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}
/// The kernel's attributes of every path of a tree, not following symlinks.
pub fn kernel(root: &Path) -> Kernel {
    walk(root)
        .into_iter()
        .map(|(path, m)| {
            let stat = Stat {
                ino: m.ino(),
                mode: m.mode(),
                nlink: m.nlink(),
                size: m.size(),
                mtime_sec: m.mtime(),
                mtime_nsec: m.mtime_nsec(),
                uid: m.uid(),
                gid: m.gid(),
            };
            (path, stat)
        })
        .collect()
}
/// What the model does not read, checked against it for a whole mounted tree:
/// every owner is the command identity; a regular file's `st_nlink` is its
/// alias count and its `st_size` its byte length; two paths share `st_ino`
/// exactly when they are aliases of one inode; a directory's count is the
/// projected 2. Symlink sizes are reported, not asserted.
pub fn assert_kernel(expected: &Flat, table: &Kernel, what: &str) {
    assert_eq!(expected.len(), table.len(), "{what}: paths");
    let mut by_identity = BTreeMap::<&[u8], u64>::new();
    let mut inodes = BTreeSet::new();
    let (mut files, mut aliased, mut directories, mut symlinks, mut sized) = (0, 0, 0, 0, 0);
    for (path, stat) in table {
        let name = path.as_os_str().as_bytes();
        let seen = expected
            .get(name)
            .unwrap_or_else(|| panic!("{what}: unexpected {path:?}"));
        assert_eq!(
            (stat.uid, stat.gid),
            (COMMAND, COMMAND),
            "{what}: owner of {path:?}"
        );
        let kind = match stat.mode & 0o170_000 {
            0o100_000 => FILE,
            0o040_000 => DIRECTORY,
            0o120_000 => SYMLINK,
            other => panic!("{what}: {path:?} has type {other:o}"),
        };
        assert_eq!(kind, seen.kind, "{what}: kind of {path:?}");
        match kind {
            FILE => {
                files += 1;
                aliased += usize::from(seen.links > 1);
                assert_eq!(stat.nlink, seen.links, "{what}: st_nlink of {path:?}");
                assert_eq!(
                    stat.size,
                    seen.payload.len() as u64,
                    "{what}: st_size of {path:?}"
                );
            }
            DIRECTORY => {
                directories += 1;
                assert_eq!(stat.nlink, 2, "{what}: directory count of {path:?}");
            }
            _ => {
                symlinks += 1;
                sized += usize::from(stat.size == seen.payload.len() as u64);
            }
        }
        match by_identity.get(seen.identity.as_slice()) {
            Some(inode) => assert_eq!(*inode, stat.ino, "{what}: aliases share st_ino: {path:?}"),
            None => {
                assert!(
                    inodes.insert(stat.ino),
                    "{what}: {path:?} shares st_ino {} with another inode",
                    stat.ino
                );
                by_identity.insert(seen.identity.as_slice(), stat.ino);
            }
        }
    }
    println!(
        "MOUNTED_KERNEL {what}: paths={} inodes={} files={files} alias_names={aliased} directories={directories} symlinks={symlinks} symlink_size_is_target_length={sized} owner={COMMAND}:{COMMAND}",
        table.len(),
        inodes.len()
    );
}
/// The same tree with every modification time at or after `since` replaced by
/// `CLOCK`. Two runs of one script then compare equal exactly when every
/// explicit time is identical and every other time was chosen by the clock in
/// both; which instant the clock chose is not compared.
pub fn clock_masked(flat: &Flat, since: i64) -> Flat {
    flat.iter()
        .map(|(path, seen)| {
            let mut seen = seen.clone();
            if seen.mtime.0 >= since {
                seen.mtime = CLOCK;
            }
            (path.clone(), seen)
        })
        .collect()
}
/// Paths whose time the clock chose, by `clock_masked`'s rule.
pub fn clocked(flat: &Flat, since: i64) -> usize {
    flat.values().filter(|seen| seen.mtime.0 >= since).count()
}

/// Deterministic bytes that are not a repeated short period.
pub fn pattern(seed: u8, length: usize) -> Vec<u8> {
    (0..length)
        .map(|n| {
            (n as u32)
                .wrapping_mul(31)
                .wrapping_add((n as u32) >> 9)
                .wrapping_add(u32::from(seed)) as u8
        })
        .collect()
}
/// Sets one path's modification time without following a symlink.
pub fn stamp(path: &Path, seconds: i64, nanoseconds: u32) {
    let time = TimeSpec::new(seconds, nanoseconds.into());
    utimensat(
        AT_FDCWD,
        path,
        &time,
        &time,
        UtimensatFlags::NoFollowSymlink,
    )
    .unwrap();
}
/// Gives every path of a tree, its root and its symlinks included, a fixed
/// and distinct modification time in 2014 to 2015, by sorted path order.
pub fn stamp_tree(root: &Path) {
    for (index, (path, _)) in walk(root).into_iter().enumerate() {
        let n = index as u32;
        stamp(
            &root.join(path),
            1_400_000_000 + i64::from(n) * 100_003,
            n.wrapping_mul(123_456_789) % 1_000_000_000,
        );
    }
}
/// The standard base: a `.git` directory with its index, objects and refs, an
/// ignore file with ignored files, a dependency directory, cache and
/// build-output files, files from empty to about 300 KiB, nested directories,
/// relative, dangling and directory symlinks, hard links of two and three
/// names, varied modes and fixed modification times.
pub fn base_tree(source: &Path) {
    for directory in [
        ".git/objects/aa",
        ".git/objects/pack",
        ".git/refs/heads",
        ".git/hooks",
        "ignored/deep",
        "node_modules/pkg/lib",
        "node_modules/.bin",
        "__pycache__",
        ".cache/tool",
        "target/debug/build",
        "dist/assets",
        "src/deep/a/b/c",
        "docs",
        "empty-dir",
        "spare/inner",
    ] {
        fs::create_dir_all(source.join(directory)).unwrap();
    }
    let files: Vec<(&str, Vec<u8>, u32)> = vec![
        (".git/HEAD", b"ref: refs/heads/main\n".to_vec(), 0o644),
        (".git/config", b"[core]\n\tbare = false\n".to_vec(), 0o644),
        (".git/index", pattern(1, 4_321), 0o644),
        (".git/objects/aa/bbccdd", pattern(2, 900), 0o444),
        (
            ".git/objects/pack/pack-0001.pack",
            pattern(3, 70_000),
            0o444,
        ),
        (
            ".git/refs/heads/main",
            b"0123456789abcdef0123456789abcdef01234567\n".to_vec(),
            0o644,
        ),
        (
            ".git/hooks/pre-commit",
            b"#!/bin/sh\nexit 0\n".to_vec(),
            0o755,
        ),
        (
            ".gitignore",
            b"node_modules/\n__pycache__/\n.cache/\ntarget/\ndist/\nignored/\n*.log\n".to_vec(),
            0o644,
        ),
        (
            "ignored/secret.env",
            b"TOKEN=not-a-secret\n".to_vec(),
            0o600,
        ),
        ("ignored/deep/notes.log", b"ignored notes\n".to_vec(), 0o640),
        ("debug.log", b"debug line 1\n".to_vec(), 0o644),
        (
            "node_modules/pkg/index.js",
            b"module.exports = 42;\n".to_vec(),
            0o644,
        ),
        (
            "node_modules/pkg/package.json",
            b"{\"name\":\"pkg\",\"version\":\"1.0.0\"}\n".to_vec(),
            0o644,
        ),
        ("node_modules/pkg/lib/util.js", pattern(4, 12_000), 0o644),
        ("__pycache__/app.cpython-312.pyc", pattern(5, 3_000), 0o644),
        (".cache/tool/data.bin", pattern(6, 20_000), 0o600),
        (
            ".cache/CACHEDIR.TAG",
            b"Signature: 8a477f597d28d172789f06886806bc55\n".to_vec(),
            0o644,
        ),
        ("target/debug/app", pattern(7, 300_017), 0o755),
        ("target/debug/build/out.o", pattern(8, 9_000), 0o644),
        ("dist/bundle.js", pattern(9, 33_000), 0o644),
        ("dist/assets/logo.svg", b"<svg/>\n".to_vec(), 0o644),
        ("src/main.rs", b"fn main() {}\n".to_vec(), 0o644),
        ("src/lib.rs", b"pub fn lib() {}\n".to_vec(), 0o644),
        ("src/empty.txt", Vec::new(), 0o644),
        ("src/deep/a/b/c/leaf.txt", b"leaf\n".to_vec(), 0o640),
        ("README.md", b"# base\n".to_vec(), 0o644),
        ("docs/guide.md", pattern(10, 5_000), 0o644),
        ("docs/zero", Vec::new(), 0o600),
        ("spare/inner/keep.txt", b"keep\n".to_vec(), 0o644),
    ];
    for (path, bytes, mode) in files {
        let path = source.join(path);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    }
    fs::write(source.join("spare/gone.txt"), b"gone\n").unwrap();
    for (target, link) in [
        ("../pkg/index.js", "node_modules/.bin/pkg"),
        ("README.md", "readme-link"),
        ("missing/target", "dangling"),
        ("src/deep", "deep-link"),
        ("..", "docs/up"),
    ] {
        symlink(target, source.join(link)).unwrap();
    }
    for (existing, alias) in [
        ("src/lib.rs", "src/lib-alias.rs"),
        ("README.md", "docs/README-alias.md"),
        (".git/objects/aa/bbccdd", ".cache/tool/object-alias"),
        (".git/objects/aa/bbccdd", "ignored/object-alias-2"),
    ] {
        fs::hard_link(source.join(existing), source.join(alias)).unwrap();
    }
    for (directory, mode) in [("spare", 0o750), ("ignored", 0o700), ("empty-dir", 0o755)] {
        fs::set_permissions(source.join(directory), fs::Permissions::from_mode(mode)).unwrap();
    }
    stamp_tree(source);
}
