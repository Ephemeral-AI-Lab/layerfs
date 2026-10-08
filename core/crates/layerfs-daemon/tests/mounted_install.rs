//! Real kernel mounts: the known local install of a control Commit on a live
//! mount, and the overwrite-only Branch head with its captured parent.
//!
//! No notification is sent at install, so every proof here reads the mount
//! after install the way an ordinary process does: through descriptors that
//! were open before the Commit, through the kernel's caches, around them with
//! `O_DIRECT`, and with attributes the kernel fetches from the daemon for that
//! call. Counted work is printed; nothing is timed.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::control::{ReadyMount, Reply, Request};
use layerfs_content::ObjectId;
use layerfs_daemon::control::Failure;
use layerfs_history::{
    CommitHistoryRequest, CommitId, CommitRecord, CommitStagedOutcome, HistoryError,
};
use model::{assert_same, Flat, Seen, DIRECTORY, FILE};
use mounted::{mount_entry, until, COMMAND};
use mutating::{c, direct, frames, path_only};
use nix::{errno::Errno, libc};
use rig::{
    assert_kernel, bash, bash_in, kernel, native, passed, pattern, receipt, root, Background,
    Kernel, Rig, Stat,
};
use std::{
    collections::BTreeMap,
    ffi::CStr,
    fmt::Debug,
    fs::{self, File, OpenOptions},
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Differences of one table that are printed in full.
const SHOWN: usize = 24;

fn at<'a>(flat: &'a Flat, path: &str) -> &'a Seen {
    flat.get(path.as_bytes())
        .unwrap_or_else(|| panic!("missing {path}"))
}
fn file<'a>(flat: &'a Flat, path: &str) -> &'a [u8] {
    let seen = at(flat, path);
    assert_eq!(seen.kind, FILE, "{path}");
    &seen.payload
}
fn absent(flat: &Flat, paths: &[&str]) {
    for path in paths {
        assert!(!flat.contains_key(path.as_bytes()), "{path} is still there");
    }
}
/// Where two byte strings part, not the strings.
fn parted(actual: &[u8], expected: &[u8]) -> Option<String> {
    if actual == expected {
        return None;
    }
    let first = actual
        .iter()
        .zip(expected)
        .position(|(a, b)| a != b)
        .unwrap_or(actual.len().min(expected.len()));
    Some(format!(
        "{} bytes against {} expected, first difference at {first}",
        actual.len(),
        expected.len()
    ))
}
fn same_bytes(actual: &[u8], expected: &[u8], what: &str) {
    if let Some(difference) = parted(actual, expected) {
        panic!("{what}: {difference}");
    }
}

/// Comparisons that must not stop a test at the first difference: each one is
/// printed where it is found, the test goes on to its other observations and
/// its drained unmounts, and fails with all of them at its end.
#[derive(Default)]
struct Findings(Vec<String>);
impl Findings {
    fn note(&mut self, what: &str, detail: String) {
        println!("DIFFERENCE {what}: {detail}");
        self.0.push(format!("{what}: {detail}"));
    }
    fn same<T: PartialEq + Debug>(&mut self, what: &str, before: &T, after: &T) {
        if before != after {
            self.note(what, format!("{before:?} -> {after:?}"));
        }
    }
    fn bytes(&mut self, what: &str, actual: &[u8], expected: &[u8]) {
        if let Some(difference) = parted(actual, expected) {
            self.note(what, difference);
        }
    }
    /// Entry by entry, both ways; returns the number of differing keys.
    fn table<K: Ord + Debug, V: PartialEq + Debug>(
        &mut self,
        what: &str,
        before: &BTreeMap<K, V>,
        after: &BTreeMap<K, V>,
    ) -> usize {
        let mut wrong = Vec::new();
        for (key, value) in before {
            match after.get(key) {
                Some(other) if other == value => {}
                other => wrong.push(format!("{key:?}: {value:?} -> {other:?}")),
            }
        }
        for (key, value) in after {
            if !before.contains_key(key) {
                wrong.push(format!("{key:?}: absent -> {value:?}"));
            }
        }
        for line in wrong.iter().take(SHOWN) {
            self.note(what, line.clone());
        }
        if wrong.len() > SHOWN {
            self.note(
                what,
                format!(
                    "{} differing entries of {}; the first {SHOWN} are shown",
                    wrong.len(),
                    before.len()
                ),
            );
        }
        wrong.len()
    }
    /// Two complete trees: kind, mode, modification time, link count, bytes
    /// or target, and alias classes of every path.
    fn flat(&mut self, what: &str, expected: &Flat, actual: &Flat) -> usize {
        let named = |flat: &Flat| -> BTreeMap<String, Seen> {
            flat.iter()
                .map(|(path, seen)| (String::from_utf8_lossy(path).into_owned(), seen.clone()))
                .collect()
        };
        self.table(what, &named(expected), &named(actual))
    }
    fn finish(self, what: &str) {
        assert!(
            self.0.is_empty(),
            "{what}: {} differences: {:#?}",
            self.0.len(),
            self.0
        );
    }
}

/// One inode's attributes fetched from the daemon for this call
/// (`AT_STATX_FORCE_SYNC`), never the kernel's cached copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Forced {
    ino: u64,
    /// Type and permission bits.
    mode: u32,
    nlink: u64,
    size: u64,
    blocks: u64,
    mtime: (i64, u32),
    ctime: (i64, u32),
    uid: u32,
    gid: u32,
}
impl Forced {
    /// The fields an ordinary `lstat` table holds.
    fn stat(&self) -> Stat {
        Stat {
            ino: self.ino,
            mode: self.mode,
            nlink: self.nlink,
            size: self.size,
            mtime_sec: self.mtime.0,
            mtime_nsec: i64::from(self.mtime.1),
            uid: self.uid,
            gid: self.gid,
        }
    }
}
fn statx(descriptor: libc::c_int, path: &CStr, flags: libc::c_int) -> Forced {
    let mut raw = std::mem::MaybeUninit::<libc::statx>::zeroed();
    Errno::result(unsafe {
        libc::statx(
            descriptor,
            path.as_ptr(),
            flags | libc::AT_STATX_FORCE_SYNC,
            libc::STATX_BASIC_STATS,
            raw.as_mut_ptr(),
        )
    })
    .unwrap_or_else(|error| panic!("statx {path:?}: {error}"));
    let raw = unsafe { raw.assume_init() };
    Forced {
        ino: raw.stx_ino,
        mode: u32::from(raw.stx_mode),
        nlink: u64::from(raw.stx_nlink),
        size: raw.stx_size,
        blocks: raw.stx_blocks,
        mtime: (raw.stx_mtime.tv_sec, raw.stx_mtime.tv_nsec),
        ctime: (raw.stx_ctime.tv_sec, raw.stx_ctime.tv_nsec),
        uid: raw.stx_uid,
        gid: raw.stx_gid,
    }
}
/// Forced attributes of a path, not following a symlink.
fn forced_path(path: &Path) -> Forced {
    statx(libc::AT_FDCWD, &c(path), libc::AT_SYMLINK_NOFOLLOW)
}
/// Forced attributes of the inode a descriptor refers to.
fn forced_open(file: &File) -> Forced {
    statx(file.as_raw_fd(), c"", libc::AT_EMPTY_PATH)
}
type ForcedTable = BTreeMap<PathBuf, Forced>;
/// Forced attributes of every path of an `lstat` table.
fn forced_table(mount: &Path, paths: &Kernel) -> ForcedTable {
    paths
        .keys()
        .map(|path| (path.clone(), forced_path(&mount.join(path))))
        .collect()
}
fn stat_of(metadata: &fs::Metadata) -> Stat {
    Stat {
        ino: metadata.ino(),
        mode: metadata.mode(),
        nlink: metadata.nlink(),
        size: metadata.size(),
        mtime_sec: metadata.mtime(),
        mtime_nsec: metadata.mtime_nsec(),
        uid: metadata.uid(),
        gid: metadata.gid(),
    }
}
/// The ordinary and the forced stat table of a mount, the ordinary one first
/// so that it is not a copy the forced pass has just refreshed.
fn tables(mount: &Path) -> (Kernel, ForcedTable) {
    let cached = kernel(mount);
    let forced = forced_table(mount, &cached);
    (cached, forced)
}

/// Every byte from offset 0 by `pread` on the descriptor itself.
fn whole(file: &File) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buffer = vec![0_u8; 1 << 16];
    loop {
        let count = file.read_at(&mut buffer, bytes.len() as u64).unwrap();
        if count == 0 {
            return bytes;
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}
/// `whole` with `O_DIRECT` set on the same open descriptor for the reads:
/// every byte is a READ the daemon answers through the retained handle.
fn whole_direct(file: &File) -> Vec<u8> {
    let descriptor = file.as_raw_fd();
    let flags = Errno::result(unsafe { libc::fcntl(descriptor, libc::F_GETFL) }).unwrap();
    Errno::result(unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_DIRECT) })
        .unwrap();
    let bytes = whole(file);
    Errno::result(unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags) }).unwrap();
    bytes
}

/// A descriptor of the test process opened before the Commit and kept open.
struct Kept {
    what: &'static str,
    name: &'static str,
    path: PathBuf,
    /// False once the name was unlinked under the open descriptor.
    linked: bool,
    file: File,
    /// What the descriptor reads from offset 0.
    bytes: Vec<u8>,
    cached: Stat,
    forced: Forced,
}
impl Kept {
    /// Opens and reads the file completely once, so its pages are cached.
    fn open(
        mount: &Path,
        what: &'static str,
        name: &'static str,
        writable: bool,
        expected: Vec<u8>,
    ) -> Self {
        let path = mount.join(name);
        let file = OpenOptions::new()
            .read(true)
            .write(writable)
            .open(&path)
            .unwrap();
        same_bytes(&whole(&file), &expected, what);
        let (cached, forced) = (stat_of(&file.metadata().unwrap()), forced_open(&file));
        Self {
            what,
            name,
            path,
            linked: true,
            file,
            bytes: expected,
            cached,
            forced,
        }
    }
    /// Records the attributes later observations must find again.
    fn observe(&mut self) {
        self.cached = stat_of(&self.file.metadata().unwrap());
        self.forced = forced_open(&self.file);
    }
    /// The name is gone; the descriptor still reads the file.
    fn unlinked(&mut self) {
        assert_eq!(
            fs::symlink_metadata(&self.path).unwrap_err().kind(),
            io::ErrorKind::NotFound,
            "{}",
            self.what
        );
        self.linked = false;
        self.observe();
        assert_eq!(self.forced.nlink, 0, "{}", self.what);
        same_bytes(&whole(&self.file), &self.bytes, self.what);
    }
    fn write(&mut self, offset: usize, bytes: &[u8]) {
        self.file.write_all_at(bytes, offset as u64).unwrap();
        if self.bytes.len() < offset + bytes.len() {
            self.bytes.resize(offset + bytes.len(), 0);
        }
        self.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.observe();
    }
    /// Through the page cache: the kept descriptor, and a fresh open by name.
    fn cached_reads(&self, findings: &mut Findings, when: &str) {
        findings.bytes(
            &format!("{when}: {}: kept descriptor, page cache", self.what),
            &whole(&self.file),
            &self.bytes,
        );
        if self.linked {
            findings.bytes(
                &format!("{when}: {}: fresh open, page cache", self.what),
                &fs::read(&self.path).unwrap(),
                &self.bytes,
            );
        }
    }
    /// Around the page cache: the kept descriptor, and a fresh open by name.
    fn direct_reads(&self, findings: &mut Findings, when: &str) {
        findings.bytes(
            &format!("{when}: {}: kept descriptor, O_DIRECT", self.what),
            &whole_direct(&self.file),
            &self.bytes,
        );
        if self.linked {
            findings.bytes(
                &format!("{when}: {}: fresh open, O_DIRECT", self.what),
                &direct(&self.path),
                &self.bytes,
            );
        }
    }
    fn stats(&self, findings: &mut Findings, when: &str) {
        findings.same(
            &format!("{when}: {}: fstat", self.what),
            &self.cached,
            &stat_of(&self.file.metadata().unwrap()),
        );
        findings.same(
            &format!("{when}: {}: forced fstat", self.what),
            &self.forced,
            &forced_open(&self.file),
        );
    }
}

/// The Branch as History holds it now, read from the Store.
fn head(rig: &Rig) -> (Option<CommitId>, ObjectId) {
    let snapshot = rig
        .store
        .history()
        .branch_snapshot(rig.harness.branch)
        .unwrap()
        .expect("the Branch");
    (snapshot.branch.head_commit, snapshot.effective_root)
}
/// The Branch's current ancestry, head first.
fn ancestry(rig: &Rig) -> Vec<CommitId> {
    let page = rig.history();
    assert!(page.continuation.is_none(), "one page holds the ancestry");
    page.records.iter().map(|record| record.id).collect()
}
/// Any stored root, the Branch's current one or not, walked completely
/// through the client of a short bind that is closed again.
fn stored(rig: &Rig, tag: u8, root: ObjectId) -> Flat {
    let token = rig.harness.bind(tag);
    let operation = rig.harness.service.operation(token).unwrap();
    let flat = model::canonical(operation.client(), root);
    assert!(operation.ports().failure().unwrap().is_none());
    drop(operation);
    let closed = rig.harness.try_unmount(token).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(token));
    flat
}

/// A mix of ordinary changes made before the descriptors are opened.
const BEFORE_INSTALL: &str = r#"
set -euo pipefail
umask 022
# New files, a directory to be removed under a process, a symlink.
mkdir -p made/sub made/cwd-gone
printf 'created before the Commit\n' > made/top.txt
head -c 20000 /dev/zero | tr '\0' 'N' > made/new.bin
head -c 9000 /dev/zero | tr '\0' 'U' > made/unlinked-new.bin
ln -s ../top.txt made/sub/rel-link
# Changed base files: inside without truncating, appended, replaced whole.
printf 'CHANGED' | dd of=docs/guide.md bs=1 seek=2000 conv=notrunc status=none
printf 'appended before the Commit\n' >> docs/guide.md
printf 'fn main() { changed(); }\n' > src/main.rs
# Renames: a file in place and a populated directory.
mv -T src/deep/a/b/c/leaf.txt src/deep/a/b/c/leaf-renamed.txt
mv -T spare/inner spare/inner-renamed
# Hard links: of a base file and of a new file.
ln .git/index .git/index.bak
ln made/top.txt made/sub/top-alias.txt
# Unlinks: a file, a symlink, one of three aliases.
rm -f spare/gone.txt readme-link .cache/tool/object-alias
# Mode and modification time.
chmod 0600 src/lib.rs
chmod 0700 docs
touch -d '2001-09-09 01:46:40.123456789 UTC' node_modules/pkg/package.json
touch -d '2004-01-10 13:37:04 UTC' src/deep/a
"#;

/// R5-5: descriptors, a removed working directory and the kernel's caches
/// across the known install, on the same mount.
#[test]
fn retained_descriptors_and_kernel_caches_are_unchanged_by_install() {
    let rig = Rig::new("r5-5a");
    let ready = rig.mount(1);
    let mount = root(&ready);
    let bound = rig.harness.status(ready.token).binding;
    let mut findings = Findings::default();
    passed(
        &bash_in(COMMAND, mount, BEFORE_INSTALL),
        "changes before the Commit",
    );

    // Descriptors of this process, opened before the Commit and kept.
    let mut guide = pattern(10, 5_000);
    guide[2_000..2_007].copy_from_slice(b"CHANGED");
    guide.extend_from_slice(b"appended before the Commit\n");
    let mut kept = vec![
        Kept::open(
            mount,
            "unchanged base file",
            "target/debug/app",
            true,
            pattern(7, 300_017),
        ),
        Kept::open(
            mount,
            "unchanged base file, read-only",
            ".git/objects/pack/pack-0001.pack",
            false,
            pattern(3, 70_000),
        ),
        Kept::open(mount, "changed base file", "docs/guide.md", true, guide),
        Kept::open(
            mount,
            "created file",
            "made/new.bin",
            true,
            vec![b'N'; 20_000],
        ),
        Kept::open(
            mount,
            "unlinked base file",
            "dist/bundle.js",
            true,
            pattern(9, 33_000),
        ),
        Kept::open(
            mount,
            "unlinked created file",
            "made/unlinked-new.bin",
            true,
            vec![b'U'; 9_000],
        ),
    ];
    passed(
        &bash_in(COMMAND, mount, "rm -f dist/bundle.js made/unlinked-new.bin"),
        "unlink under the open descriptors",
    );
    for descriptor in &mut kept[4..] {
        descriptor.unlinked();
    }

    // A process with an open descriptor whose working directory is removed
    // from outside, and a path-only reference of this process to it.
    let gone = mount.join("made/cwd-gone");
    let log = mount.join("made/held.log");
    let mut background = Background::spawn(
        COMMAND,
        &gone,
        "exec 3>>../held.log; printf 'before the Commit\\n' >&3; read -r _; stat -c 'cwd links=%h kind=%F' . >&3; printf 'after install\\n' >&3",
    );
    until("the background process wrote", || {
        fs::read(&log).is_ok_and(|bytes| bytes == b"before the Commit\n")
    });
    let reference = path_only(&gone);
    passed(
        &bash_in(COMMAND, mount, "rmdir made/cwd-gone"),
        "remove the working directory",
    );
    assert!(background.alive());
    let held = bash(
        COMMAND,
        &format!("readlink /proc/{0}/fd/3 /proc/{0}/cwd", background.pid()),
    );
    passed(&held, "descriptor and working directory of the process");
    assert_eq!(
        String::from_utf8(held.stdout).unwrap(),
        format!("{}\n{} (deleted)\n", log.display(), gone.display())
    );
    let removed = forced_open(&reference);
    assert_eq!(
        (removed.nlink, removed.mode & libc::S_IFMT),
        (0, libc::S_IFDIR),
        "{removed:?}"
    );

    // The tree before the Commit. Reading the model reads every file once
    // through the page cache.
    let expected = native(mount);
    same_bytes(
        file(&expected, "docs/guide.md"),
        &kept[2].bytes,
        "changed base file in the model",
    );
    assert_eq!(
        file(&expected, "src/main.rs"),
        b"fn main() { changed(); }\n"
    );
    assert_eq!(
        file(&expected, "src/deep/a/b/c/leaf-renamed.txt"),
        b"leaf\n"
    );
    assert_eq!(file(&expected, "spare/inner-renamed/keep.txt"), b"keep\n");
    assert_eq!(file(&expected, "made/held.log"), b"before the Commit\n");
    assert_eq!(at(&expected, ".git/index").links, 2);
    assert_eq!(at(&expected, "made/top.txt").links, 2);
    assert_eq!(at(&expected, ".git/objects/aa/bbccdd").links, 2);
    assert_eq!(at(&expected, "src/lib.rs").mode, 0o600);
    assert_eq!(at(&expected, "docs").mode, 0o700);
    assert_eq!(
        at(&expected, "node_modules/pkg/package.json").mtime,
        (1_000_000_000, 123_456_789)
    );
    assert_eq!(at(&expected, "src/deep/a").mtime, (1_073_741_824, 0));
    absent(
        &expected,
        &[
            "spare/gone.txt",
            "spare/inner",
            "readme-link",
            ".cache/tool/object-alias",
            "src/deep/a/b/c/leaf.txt",
            "dist/bundle.js",
            "made/unlinked-new.bin",
            "made/cwd-gone",
        ],
    );
    let (before_cached, before_forced) = tables(mount);
    let projected: Kernel = before_forced
        .iter()
        .map(|(path, forced)| (path.clone(), forced.stat()))
        .collect();
    findings.table(
        "before the Commit: lstat table against forced table",
        &before_cached,
        &projected,
    );
    for descriptor in &mut kept {
        descriptor.observe();
    }

    let record = rig.committed(ready.token, "R5-5a");
    assert_eq!(record.parent, bound.branch.head_commit);
    assert_ne!(record.root, bound.effective_root);
    assert!(background.alive(), "the process outlived the Commit");

    // After install, on the same mount. The ordinary table first, then the
    // page cache, then everything that asks the daemon.
    let after_cached = kernel(mount);
    let cached_changes = findings.table("install: lstat table", &before_cached, &after_cached);
    let start = frames(&rig.harness, ready.token);
    for descriptor in &kept {
        descriptor.cached_reads(&mut findings, "after install");
    }
    let cached_requests = frames(&rig.harness, ready.token) - start;
    let start = frames(&rig.harness, ready.token);
    for descriptor in &kept {
        descriptor.direct_reads(&mut findings, "after install");
    }
    let direct_requests = frames(&rig.harness, ready.token) - start;
    assert!(
        direct_requests >= kept.len() as u64,
        "every O_DIRECT read reaches the daemon: {direct_requests} requests"
    );
    for descriptor in &kept {
        descriptor.stats(&mut findings, "after install");
    }
    let after_forced = forced_table(mount, &after_cached);
    let forced_changes =
        findings.table("install: forced stat table", &before_forced, &after_forced);
    findings.same(
        "install: forced attributes of the removed directory",
        &removed,
        &forced_open(&reference),
    );
    let same_mount = findings.flat("install: the same mount", &expected, &native(mount));
    let published = rig.published(record.root);
    let fresh_bind = findings.flat("the published root, fresh bind", &expected, &published);
    println!(
        "R5-5a install paths={} lstat_changes={cached_changes} forced_stat_changes={forced_changes} same_mount_model_differences={same_mount} published_model_differences={fresh_bind} descriptors={} unlinked={} cached_read_requests={cached_requests} direct_read_requests={direct_requests} background_pid={} removed_directory={removed:?}",
        before_forced.len(),
        kept.len(),
        kept.iter().filter(|descriptor| !descriptor.linked).count(),
        background.pid()
    );

    // Writes through the retained descriptors after install are live.
    kept[0].write(100_000, b"written after install");
    kept[2].write(10, b"AFTER");
    kept[3].write(20_000, b"tail after install");
    kept[4].write(5, b"orphan write");
    kept[5].write(9_000, b"orphan tail");
    for descriptor in &kept {
        descriptor.cached_reads(&mut findings, "after a write through the descriptor");
        descriptor.direct_reads(&mut findings, "after a write through the descriptor");
    }
    // The process is still alive; it states its removed working directory
    // and writes through its descriptor, then exits.
    assert!(background.alive());
    let status = background.release();
    assert!(status.success(), "{status:?}");
    findings.bytes(
        "the process after install",
        &fs::read(&log).unwrap(),
        b"before the Commit\ncwd links=0 kind=directory\nafter install\n",
    );

    // The next Commit captures the later writes, and installs again under
    // the same descriptors.
    let later = native(mount);
    for descriptor in kept.iter().filter(|descriptor| descriptor.linked) {
        findings.bytes(
            &format!("live view: {}", descriptor.what),
            file(&later, descriptor.name),
            &descriptor.bytes,
        );
    }
    absent(
        &later,
        &["dist/bundle.js", "made/unlinked-new.bin", "made/cwd-gone"],
    );
    assert!(later != expected, "the later writes changed the tree");
    for descriptor in &mut kept {
        descriptor.observe();
    }
    let (before_cached, before_forced) = tables(mount);
    let second = rig.committed(ready.token, "R5-5a later writes");
    assert_eq!(second.parent, Some(record.id));
    let (after_cached, after_forced) = tables(mount);
    let cached_changes =
        findings.table("second install: lstat table", &before_cached, &after_cached);
    let forced_changes = findings.table(
        "second install: forced stat table",
        &before_forced,
        &after_forced,
    );
    for descriptor in &kept {
        descriptor.cached_reads(&mut findings, "after the second install");
        descriptor.direct_reads(&mut findings, "after the second install");
        descriptor.stats(&mut findings, "after the second install");
    }
    findings.flat("second install: the same mount", &later, &native(mount));
    let published = rig.published(second.root);
    findings.flat("the second published root, fresh bind", &later, &published);
    println!(
        "R5-5a second install paths={} lstat_changes={cached_changes} forced_stat_changes={forced_changes} published_paths={}",
        before_forced.len(),
        published.len()
    );

    // Close everything, drain, and read the Branch on a fresh mount.
    drop(kept);
    drop(reference);
    rig.unmount(&ready);
    let fresh = rig.mount(2);
    assert_eq!(
        rig.harness.status(fresh.token).binding.effective_root,
        second.root
    );
    let again = native(root(&fresh));
    findings.flat("the fresh mount", &later, &again);
    let table = kernel(root(&fresh));
    rig.unmount(&fresh);
    rig.finish();
    findings.finish("R5-5a");
    assert_kernel(&later, &table, "R5-5a fresh mount");
}

const BULK: usize = 300;
/// Directories prepared for the sharded layout; the writer fails beyond them.
const SHARDS: usize = 40;
/// Where the writer puts its numbered files.
#[derive(Clone, Copy)]
struct Layout {
    label: &'static str,
    /// Names per directory `seq/d<nnnn>/`; 0 puts every file in `seq/` itself.
    shard: usize,
}
impl Layout {
    fn path(self, number: usize) -> String {
        if self.shard == 0 {
            format!("seq/{number:06}")
        } else {
            format!("seq/d{:04}/{number:06}", (number - 1) / self.shard)
        }
    }
    fn script(self, body: &str) -> String {
        format!("shard={}\nshards={SHARDS}\n{body}", self.shard)
    }
}
/// A few hundred changed files, so that construction takes a while.
const PREPARE: &str = r#"
set -euo pipefail
umask 022
mkdir -p seq work bulk/d0 bulk/d1 bulk/d2 bulk/d3 bulk/d4 bulk/d5
: > seq.log
i=0
while [ "$i" -lt 300 ]; do
  i=$((i+1))
  printf 'bulk file %04d\n' "$i" > "bulk/d$((i % 6))/f-$i"
done
printf 'changed with the bulk\n' >> README.md
rm -f spare/gone.txt
d=0
while [ "$shard" -gt 0 ] && [ "$d" -lt "$shards" ]; do
  printf -v name 'seq/d%04d' "$d"
  mkdir "$name"
  d=$((d+1))
done
"#;
/// Numbered files until standard input is closed. Each number is written to
/// a pending name, renamed into its directory and only then logged, so one
/// rename publishes one complete numbered file:
///
/// ```text
/// A(i) create work/next.tmp    B(i) write its content
/// C(i) rename it to its name   D(i) append <i> to seq.log
/// ```
const WRITER: &str = r#"
umask 022
i=0
d=seq
while :; do
  i=$((i+1))
  printf -v n '%06d' "$i"
  [ "$shard" -eq 0 ] || printf -v d 'seq/d%04d' "$(( (i - 1) / shard ))"
  printf 'sequence file %s\n' "$n" > work/next.tmp || exit 3
  mv -T work/next.tmp "$d/$n" || exit 4
  printf '%s\n' "$n" >> seq.log || exit 5
  read -r -t 0.002 _
  [ "$?" -gt 128 ] || break
done
"#;
fn numbered(number: usize) -> Vec<u8> {
    format!("sequence file {number:06}\n").into_bytes()
}
fn logged(count: usize) -> Vec<u8> {
    (1..=count)
        .flat_map(|number| format!("{number:06}\n").into_bytes())
        .collect()
}
/// Numbers the writer has logged so far, read from the live mount.
fn logged_now(mount: &Path) -> io::Result<usize> {
    Ok(fs::read(mount.join("seq.log"))?.len() / 7)
}
/// Bounded wait until the writer has logged `least` numbers. A failed read
/// or three seconds without reaching it ends the wait with that error.
fn wait_logged(mount: &Path, least: usize) -> io::Result<usize> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let logged = logged_now(mount)?;
        if logged >= least {
            return Ok(logged);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other(format!(
                "{logged} numbers logged, {least} awaited for three seconds"
            )));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
/// The live mount stopped answering. Prints what the daemon reports about
/// it, reaps the writer, makes the one normal unmount attempt for its
/// receipt, and fails the test. Nothing here is retried or settled.
fn stopped(
    rig: &Rig,
    ready: &ReadyMount,
    writer: Option<Background>,
    when: &str,
    error: &dyn std::fmt::Display,
) -> ! {
    let work = rig
        .harness
        .status(ready.token)
        .native
        .and_then(|native| native.work);
    println!("MOUNT STOPPED {when}: {error}");
    println!("MOUNT STOPPED work={work:?}");
    if let Some(mut writer) = writer {
        println!("MOUNT STOPPED writer_alive={}", writer.alive());
        drop(writer);
    }
    match rig.harness.try_unmount(ready.token) {
        Ok(done) => println!(
            "MOUNT STOPPED unmount: {:?} native={:?}",
            done.reply, done.native
        ),
        Err(failure) => println!("MOUNT STOPPED unmount refused: {failure:?}"),
    }
    match rig.harness.service.retained_native(ready.token) {
        Ok(kept) => println!("MOUNT STOPPED retained owner: {kept:?}"),
        Err(failure) => println!("MOUNT STOPPED retained owner unavailable: {failure:?}"),
    }
    panic!("{when}: the live mount stopped serving: {error}");
}
/// The numbered files of a tree: exactly the files 1 to k of the layout, each
/// complete, and exactly the layout's directories. Returns k.
fn sequence(flat: &Flat, layout: Layout, what: &str) -> usize {
    let (mut files, mut directories) = (0, 0);
    for (path, seen) in flat.iter().filter(|(path, _)| path.starts_with(b"seq/")) {
        let path = String::from_utf8_lossy(path);
        if seen.kind == DIRECTORY {
            assert_eq!(path, format!("seq/d{directories:04}"), "{what}");
            assert_eq!(seen.mode, 0o755, "{what}: {path}");
            directories += 1;
            continue;
        }
        files += 1;
        assert_eq!(
            path,
            layout.path(files),
            "{what}: the numbered files are not a contiguous prefix at {files}"
        );
        assert_eq!(
            (seen.kind, seen.mode, seen.links),
            (FILE, 0o644, 1),
            "{what}: {path}"
        );
        same_bytes(
            &seen.payload,
            &numbered(files),
            &format!("{what}: content of {path}"),
        );
    }
    assert_eq!(
        directories,
        if layout.shard == 0 { 0 } else { SHARDS },
        "{what}: directories under seq/"
    );
    files
}
/// Number of log lines; the log is exactly the numbers 1 to that count.
fn log_lines(flat: &Flat, what: &str) -> usize {
    let log = file(flat, "seq.log");
    let lines = log.len() / 7;
    same_bytes(log, &logged(lines), &format!("{what}: seq.log"));
    lines
}
/// Everything the writer does not touch.
fn rest(flat: &Flat) -> Flat {
    flat.iter()
        .filter(|(path, _)| !(path.starts_with(b"seq") || path.starts_with(b"work")))
        .map(|(path, seen)| (path.clone(), seen.clone()))
        .collect()
}
/// Names of one directory by `getdents`, or the error that ended the listing
/// with the number of names read before it.
fn listed(directory: &Path) -> Result<usize, (usize, io::Error)> {
    let mut count = 0;
    for entry in fs::read_dir(directory).map_err(|error| (0, error))? {
        entry.map_err(|error| (count, error))?;
        count += 1;
    }
    Ok(count)
}

/// What one Commit under the writer observed around its call.
struct Under {
    record: CommitRecord,
    /// Numbers logged when the Commit was called and when it returned.
    before_call: usize,
    at_return: usize,
}
/// A root captured while the writer ran holds a prefix of its sequence:
/// every published mutation is wholly before or wholly after the capture.
/// Returns k, the number of numbered files in the root.
fn prefix(
    captured: &Flat,
    live: &Flat,
    prepared: &Flat,
    layout: Layout,
    under: &Under,
    what: &str,
) -> usize {
    let k = sequence(captured, layout, what);
    let lines = log_lines(captured, what);
    // C(k) precedes D(k), and D(k) precedes C(k+1).
    assert!(
        lines == k || lines + 1 == k,
        "{what}: the log has {lines} lines beside {k} numbered files"
    );
    // The pending name exists only between A(k+1) and C(k+1), after D(k).
    let pending = match captured.get(b"work/next.tmp".as_slice()) {
        None => "absent",
        Some(seen) => {
            assert_eq!(seen.kind, FILE, "{what}");
            assert_eq!(
                lines, k,
                "{what}: a pending file before its predecessor is logged"
            );
            if seen.payload.is_empty() {
                "empty"
            } else {
                same_bytes(&seen.payload, &numbered(k + 1), "the pending file");
                "complete"
            }
        }
    };
    // A numbered file never changes after its rename.
    for number in 1..=k {
        let path = layout.path(number);
        assert_eq!(at(captured, &path), at(live, &path), "{what}: {path}");
    }
    for path in ["seq", "work"] {
        let seen = at(captured, path);
        assert_eq!((seen.kind, seen.mode), (DIRECTORY, 0o755), "{what}: {path}");
    }
    assert_same(
        &rest(prepared),
        &rest(captured),
        &format!("{what}, the rest"),
    );
    println!(
        "{what}: k={k} log_lines={lines} pending={pending} logged_before_call={} logged_at_return={} logged_during_call={} paths={}",
        under.before_call,
        under.at_return,
        under.at_return - under.before_call,
        captured.len()
    );
    // A number logged before the call was renamed into place before the
    // capture. The capture preceded the return, and its log lacks at most
    // the number of its last file.
    assert!(
        (under.before_call..=under.at_return + 1).contains(&k),
        "{what}: {} logged before the call, {k} captured, {} logged at return",
        under.before_call,
        under.at_return
    );
    k
}

/// Two Commits under continuous external writes, then one after them.
fn live_writer(layout: Layout) {
    let what = layout.label;
    let rig = Rig::new(layout.label);
    let ready = rig.mount(1);
    let mount = root(&ready);
    let bound = rig.harness.status(ready.token).binding;
    passed(&bash_in(COMMAND, mount, &layout.script(PREPARE)), "prepare");
    let prepared = native(mount);
    assert_eq!(
        prepared
            .keys()
            .filter(|path| path.starts_with(b"bulk/d") && path.len() > 7)
            .count(),
        BULK
    );
    assert_eq!(sequence(&prepared, layout, "prepared"), 0);

    let mut writer = Background::spawn(COMMAND, mount, &layout.script(WRITER));
    let mut before_call = match wait_logged(mount, 5) {
        Ok(logged) => logged,
        Err(error) => stopped(&rig, &ready, Some(writer), "before the Commit", &error),
    };
    // The first Commit is called just after a number was logged. The second
    // is called when the first returns, wherever the writer then is.
    let mut under = Vec::new();
    for number in ["first", "second"] {
        let record = rig.committed(ready.token, &format!("{what} {number} under the writer"));
        let at_return = match logged_now(mount) {
            Ok(logged) => logged,
            Err(error) => stopped(&rig, &ready, Some(writer), "at a Commit's return", &error),
        };
        assert!(writer.alive(), "the writer outlived the {number} Commit");
        under.push(Under {
            record,
            before_call,
            at_return,
        });
        before_call = at_return;
    }
    let (first, second) = (&under[0], &under[1]);
    assert_eq!(first.record.parent, bound.branch.head_commit);
    assert_eq!(second.record.parent, Some(first.record.id));
    // A number logged from here on was renamed into place after the second
    // Commit returned, so after its capture: at least two such files follow.
    if let Err(error) = wait_logged(mount, second.at_return + 3) {
        stopped(
            &rig,
            &ready,
            Some(writer),
            "after the Commits returned",
            &error,
        );
    }
    let status = writer.release();
    assert!(status.success(), "{status:?}");

    // The same mount, after two installs, shows everything the writer made.
    if let Err((read, error)) = listed(&mount.join("seq")) {
        stopped(
            &rig,
            &ready,
            None,
            "listing seq/ after the writer stopped",
            &format!(
                "{error} after {read} names (see a_directory_of_more_than_one_name_window_is_listed)"
            ),
        );
    }
    let live = native(mount);
    let n = sequence(&live, layout, "the mount");
    assert_eq!(log_lines(&live, "the mount"), n);
    absent(&live, &["work/next.tmp"]);
    assert_same(&rest(&prepared), &rest(&live), "the mount, the rest");

    // Both captured roots: the displaced first one read from the Store, the
    // second one as the Branch's root from a fresh bind.
    let k1 = prefix(
        &stored(&rig, 10, first.record.root),
        &live,
        &prepared,
        layout,
        first,
        &format!("{what} first root"),
    );
    let k2 = prefix(
        &rig.published(second.record.root),
        &live,
        &prepared,
        layout,
        second,
        &format!("{what} second root"),
    );
    println!(
        "{what} k1={k1} k2={k2} n={n} live_after_first_capture={} live_after_second_capture={} bulk={BULK} names_per_directory={} live_paths={}",
        n.saturating_sub(k1),
        n.saturating_sub(k2),
        if layout.shard == 0 { n } else { layout.shard },
        live.len()
    );
    assert!(k1 <= k2, "{k1} then {k2} captured");
    assert!(
        n > k2,
        "no write followed the capture ({k2} captured, {n} made): the row is not demonstrated"
    );

    // The next Commit captures the rest, and installs under the same mount.
    let mut findings = Findings::default();
    let (before_cached, before_forced) = tables(mount);
    let last = rig.committed(ready.token, &format!("{what} the rest"));
    assert_eq!(last.parent, Some(second.record.id));
    let (after_cached, after_forced) = tables(mount);
    let cached_changes = findings.table("last install: lstat table", &before_cached, &after_cached);
    let forced_changes = findings.table(
        "last install: forced stat table",
        &before_forced,
        &after_forced,
    );
    let all = rig.published(last.root);
    assert_eq!(sequence(&all, layout, "the last published root"), n);
    assert_eq!(log_lines(&all, "the last published root"), n);
    findings.flat("the last published root", &live, &all);
    findings.flat(
        "the same mount after the last install",
        &live,
        &native(mount),
    );
    println!(
        "{what} last commit files={n} paths={} lstat_changes={cached_changes} forced_stat_changes={forced_changes}",
        all.len()
    );
    rig.unmount(&ready);

    let fresh = rig.mount(2);
    assert_eq!(
        rig.harness.status(fresh.token).binding.effective_root,
        last.root
    );
    findings.flat("the fresh mount", &live, &native(root(&fresh)));
    let table = kernel(root(&fresh));
    rig.unmount(&fresh);
    rig.finish();
    findings.finish(what);
    assert_kernel(&live, &table, &format!("{what} fresh mount"));
}

/// R5-5, the live writer, as the row states it: numbered files in one
/// directory.
#[test]
fn writes_during_a_commit_stay_live_and_reach_the_next_commit() {
    live_writer(Layout {
        label: "r5-5b",
        shard: 0,
    });
}
/// The same proof with the numbered files fifty to a directory, so that no
/// directory holds more names than one listing window.
#[test]
fn writes_during_a_commit_stay_live_and_reach_the_next_commit_fifty_to_a_directory() {
    live_writer(Layout {
        label: "r5-5b-50",
        shard: 50,
    });
}

const IN_A: &str = r#"
set -euo pipefail
umask 022
printf 'only in A\n' > only-a.txt
printf 'A changed the readme\n' >> README.md
mkdir -p from-a/sub
head -c 50000 /dev/zero | tr '\0' 'a' > from-a/sub/big.bin
ln from-a/sub/big.bin from-a/big-alias.bin
rm -f debug.log
mv -T src/main.rs src/main-a.rs
chmod 0600 docs/guide.md
"#;
const IN_B: &str = r#"
set -euo pipefail
umask 022
printf 'only in B\n' > only-b.txt
printf 'B changed the readme differently\n' >> README.md
mkdir -p from-b
printf 'b' > from-b/small
ln -s only-b.txt link-b
rm -rf spare
mv -T src/lib.rs src/lib-b.rs
printf 'seed changed by B\n' > seed.txt
touch -d '2012-12-12 12:12:12.5 UTC' docs
"#;

/// R5-4: two Workspaces of one Branch mounted at once on one daemon.
#[test]
fn a_later_commit_overwrites_the_branch_head_and_keeps_its_captured_parent() {
    let rig = Rig::new("r5-4");
    // A first Commit, so the head both Workspaces capture is a Commit.
    let seed = rig.mount(3);
    passed(
        &bash_in(COMMAND, root(&seed), "printf 'seeded\\n' > seed.txt"),
        "seed",
    );
    let seeded = native(root(&seed));
    let h0 = rig.committed(seed.token, "R5-4 seed");
    rig.unmount(&seed);
    assert_eq!(head(&rig), (Some(h0.id), h0.root));

    let a = rig.mount(1);
    let b = rig.mount(2);
    let (mount_a, mount_b) = (root(&a), root(&b));
    for ready in [&a, &b] {
        let binding = rig.harness.status(ready.token).binding;
        assert_eq!(
            (binding.branch.head_commit, binding.effective_root),
            (Some(h0.id), h0.root)
        );
    }
    // Two kernel mounts.
    assert_ne!(a.directory, b.directory);
    let (entry_a, entry_b) = (
        mount_entry(&a.directory).expect("mount A"),
        mount_entry(&b.directory).expect("mount B"),
    );
    let (device_a, device_b) = (
        fs::metadata(mount_a).unwrap().dev(),
        fs::metadata(mount_b).unwrap().dev(),
    );
    assert_ne!(device_a, device_b, "one kernel superblock each");
    assert_same(&seeded, &native(mount_a), "A at the head");
    assert_same(&seeded, &native(mount_b), "B at the head");

    // Each changed differently; neither sees the other's change.
    passed(&bash_in(COMMAND, mount_a, IN_A), "changes in A");
    passed(&bash_in(COMMAND, mount_b, IN_B), "changes in B");
    let tree_a = native(mount_a);
    let tree_b = native(mount_b);
    let isolated = |when: &str| {
        for (mount, missing) in [(mount_a, "only-b.txt"), (mount_b, "only-a.txt")] {
            assert_eq!(
                fs::symlink_metadata(mount.join(missing))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::NotFound,
                "{when}: {missing} under {mount:?}"
            );
        }
        assert_same(&tree_a, &native(mount_a), &format!("{when}: mount A"));
        assert_same(&tree_b, &native(mount_b), &format!("{when}: mount B"));
    };
    assert_eq!(file(&tree_a, "only-a.txt"), b"only in A\n");
    assert_eq!(file(&tree_b, "only-b.txt"), b"only in B\n");
    assert_eq!(
        file(&tree_a, "README.md"),
        b"# base\nA changed the readme\n"
    );
    assert_eq!(
        file(&tree_b, "README.md"),
        b"# base\nB changed the readme differently\n"
    );
    assert_eq!(file(&tree_a, "seed.txt"), b"seeded\n");
    assert_eq!(file(&tree_b, "seed.txt"), b"seed changed by B\n");
    absent(&tree_a, &["only-b.txt", "from-b", "link-b", "debug.log"]);
    absent(&tree_b, &["only-a.txt", "from-a", "spare"]);
    isolated("before either Commit");

    // A commits.
    let record_a = rig.committed(a.token, "R5-4 A");
    assert_eq!(record_a.parent, Some(h0.id));
    assert_eq!(head(&rig), (Some(record_a.id), record_a.root));
    assert_eq!(ancestry(&rig), [record_a.id, h0.id]);
    assert_same(&tree_a, &rig.published(record_a.root), "A's published root");
    // B is still bound at the earlier head.
    let stale = rig.harness.status(b.token).binding;
    assert_eq!(
        (stale.branch.head_commit, stale.effective_root),
        (Some(h0.id), h0.root)
    );
    isolated("after A's Commit");

    // B commits: the head is overwritten, the parent is the captured head.
    let record_b = rig.committed(b.token, "R5-4 B");
    assert_eq!(record_b.parent, Some(h0.id), "the captured parent");
    assert_ne!(record_b.parent, Some(record_a.id));
    assert_ne!(record_b.id, record_a.id);
    assert_ne!(record_b.root, record_a.root);
    assert_eq!(head(&rig), (Some(record_b.id), record_b.root));
    assert_eq!(ancestry(&rig), [record_b.id, h0.id]);
    assert_same(&tree_b, &rig.published(record_b.root), "B's published root");
    // A's Commit is displaced from the ancestry, and still stored whole.
    assert_eq!(
        rig.store.history().commit(record_a.id).unwrap(),
        Some(record_a.clone())
    );
    assert_same(
        &tree_a,
        &stored(&rig, 10, record_a.root),
        "A's displaced root",
    );
    let anchored = rig
        .harness
        .service
        .execute_control(&Request::History(CommitHistoryRequest {
            branch: rig.harness.branch,
            start: Some(record_a.id),
            cursor: None,
            limit: rig::HISTORY,
        }));
    let anchored = match anchored {
        Err(Failure::History(HistoryError::NotInHistory(what))) => what,
        Err(other) => panic!("ancestry from the displaced Commit: {other:?}"),
        Ok(done) => panic!("ancestry from the displaced Commit: {:?}", done.reply),
    };
    // Each mounted Workspace keeps its own installed view.
    for (ready, record) in [(&a, &record_a), (&b, &record_b)] {
        let binding = rig.harness.status(ready.token).binding;
        assert_eq!(
            (binding.branch.head_commit, binding.effective_root),
            (Some(record.id), record.root)
        );
    }
    isolated("after B's Commit");
    println!(
        "R5-4 h0={:?} a={{id={:?} parent={:?}}} b={{id={:?} parent={:?}}} head_after_a=a head_after_b=b ancestry_after_b=[b,h0] a_stored=true a_root_readable_paths={} ancestry_from_a=NotInHistory({anchored}) mounts={{a={} {} dev={device_a}, b={} {} dev={device_b}}}",
        h0.id,
        record_a.id,
        record_a.parent,
        record_b.id,
        record_b.parent,
        tree_a.len(),
        entry_a.filesystem,
        entry_a.source,
        entry_b.filesystem,
        entry_b.source
    );

    // A third Workspace, bound and mounted now, shows B's tree exactly.
    let third = rig.mount(4);
    let mount_c = root(&third);
    let binding = rig.harness.status(third.token).binding;
    assert_eq!(
        (binding.branch.head_commit, binding.effective_root),
        (Some(record_b.id), record_b.root)
    );
    assert_same(&tree_b, &native(mount_c), "the third mount");
    assert_kernel(&tree_b, &kernel(mount_c), "R5-4 third mount");

    // The stale unchanged candidate: A, installed at its own root, changes
    // nothing and commits while the head is B's.
    let mut findings = Findings::default();
    let (before_cached, before_forced) = tables(mount_a);
    let done = rig
        .commit(a.token)
        .unwrap_or_else(|failure| panic!("R5-4 stale unchanged A: {failure:?}"));
    receipt(&done, "R5-4 stale unchanged A");
    let again: CommitRecord = match &done.reply {
        Reply::Committed(CommitStagedOutcome::Committed(record)) => record.clone(),
        other => panic!(
            "R5-4 stale unchanged A: the Branch overwrite decision says Committed; observed {other:?}"
        ),
    };
    drop(done);
    println!(
        "R5-4 stale_unchanged observed=Committed id={:?} parent={:?} root_is_a={} head_before=b",
        again.id,
        again.parent,
        again.root == record_a.root
    );
    assert_eq!(again.root, record_a.root, "the unchanged root");
    assert_eq!(again.parent, Some(record_a.id), "A's captured head");
    assert!(again.id != record_a.id && again.id != record_b.id);
    assert_eq!(head(&rig), (Some(again.id), record_a.root));
    assert_eq!(ancestry(&rig), [again.id, record_a.id, h0.id]);
    assert_same(&tree_a, &rig.published(record_a.root), "the head again");
    let (after_cached, after_forced) = tables(mount_a);
    findings.table(
        "stale unchanged install: lstat table of A",
        &before_cached,
        &after_cached,
    );
    findings.table(
        "stale unchanged install: forced stat table of A",
        &before_forced,
        &after_forced,
    );
    // B's Commit is displaced in turn, and still stored whole.
    assert_eq!(
        rig.store.history().commit(record_b.id).unwrap(),
        Some(record_b.clone())
    );
    assert_same(
        &tree_b,
        &stored(&rig, 11, record_b.root),
        "B's displaced root",
    );
    // Every mounted Workspace still shows its own tree.
    for (ready, head_commit, effective) in [
        (&a, again.id, record_a.root),
        (&b, record_b.id, record_b.root),
        (&third, record_b.id, record_b.root),
    ] {
        let binding = rig.harness.status(ready.token).binding;
        assert_eq!(
            (binding.branch.head_commit, binding.effective_root),
            (Some(head_commit), effective)
        );
    }
    isolated("after the stale unchanged Commit");
    assert_same(&tree_b, &native(mount_c), "the third mount, still B's tree");
    // Immediately again, A describes the head.
    assert_eq!(
        rig.up_to_date(a.token, "R5-4 A again"),
        (Some(again.id), record_a.root)
    );
    let fourth = rig.mount(5);
    assert_same(&tree_a, &native(root(&fourth)), "the fourth mount");
    assert_kernel(&tree_a, &kernel(root(&fourth)), "R5-4 fourth mount");
    println!(
        "R5-4 head_after_stale_unchanged=a2 ancestry=[a2,a,h0] b_stored=true mounts_now={{a=A's tree, b=B's tree, third=B's tree, fourth=A's tree}} paths_a={} paths_b={}",
        tree_a.len(),
        tree_b.len()
    );
    for ready in [&a, &b, &third, &fourth] {
        rig.unmount(ready);
    }
    rig.finish();
    findings.finish("R5-4");
}

/// Directories of 65 names: one of names too long for 64 of them to fit one
/// kernel reply, and two of six-byte names, 64 and 65 of them.
const MANY_NAMES: &str = r#"
set -euo pipefail
umask 022
mkdir long-65 short-64 short-65
i=0
while [ "$i" -lt 65 ]; do
  i=$((i+1))
  printf -v n '%06d' "$i"
  : > "long-65/a-name-of-forty-three-bytes-in-all-01234-$n"
  : > "short-65/$n"
  [ "$i" -gt 64 ] || : > "short-64/$n"
done
"#;

/// Not an R5 row: the smallest reproduction of what stops the live-writer
/// proof with one directory. No Commit and no concurrent process: a directory
/// whose first 64 names all fit one kernel READDIR reply and that has more
/// names.
#[test]
fn a_directory_of_more_than_one_name_window_is_listed() {
    let rig = Rig::new("r5-list");
    let ready = rig.mount(1);
    let mount = root(&ready);
    passed(&bash_in(COMMAND, mount, MANY_NAMES), "many names");
    for (name, expected) in [("long-65", 65), ("short-64", 64), ("short-65", 65)] {
        let observed = listed(&mount.join(name));
        println!("LISTING {name}: expected={expected} observed={observed:?}");
        match observed {
            Ok(count) => assert_eq!(count, expected, "{name}"),
            Err((read, error)) => stopped(
                &rig,
                &ready,
                None,
                &format!("listing {name}, {expected} names"),
                &format!("{error} after {read} names"),
            ),
        }
    }
    rig.unmount(&ready);
    rig.finish();
}
