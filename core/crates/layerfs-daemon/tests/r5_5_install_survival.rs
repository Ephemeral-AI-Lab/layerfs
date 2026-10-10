//! Real kernel mounts: what the known local install of a control Commit leaves
//! unchanged on a live mount, for the halves of row R5-5 that
//! `mounted_install.rs` names as not covered:
//!
//! - access times, in the checked attribute tables. The contract is that the
//!   access time is not stored, is reported equal to the modification time,
//!   and that the mount is `noatime`;
//! - an `O_DIRECT` read of every regular file of the model after install,
//!   with no descriptor of the test kept on the mount;
//! - shared mappings held across two installs;
//! - a capture inside one iteration of a writer: the writer is parked after
//!   each of its steps on a pipe, and one Commit is taken at each position.
//!
//! No notification is sent at install, so every observation reads the mount
//! the way an ordinary process does. Counted work is printed; nothing is
//! timed.
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
use model::{assert_same, Flat, Seen, FILE};
use mounted::{mount_entry, until, COMMAND};
use mutating::{c, direct, frames, Mapping};
use nix::{errno::Errno, libc};
use rig::{assert_kernel, bash_in, kernel, native, passed, pattern, root, Rig};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{ffi::OsStrExt, fs::FileExt, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
};

const PAGE: usize = 4096;

fn at<'a>(flat: &'a Flat, path: &str) -> &'a Seen {
    flat.get(path.as_bytes())
        .unwrap_or_else(|| panic!("missing {path}"))
}
fn file<'a>(flat: &'a Flat, path: &str) -> &'a [u8] {
    let seen = at(flat, path);
    assert_eq!(seen.kind, FILE, "{path}");
    &seen.payload
}
/// Whole-content equality that reports where two byte strings part.
fn same_bytes(actual: &[u8], expected: &[u8], what: &str) {
    let first = actual
        .iter()
        .zip(expected)
        .position(|(a, b)| a != b)
        .unwrap_or(actual.len().min(expected.len()));
    assert!(
        actual == expected,
        "{what}: {} bytes against {} expected, first difference at {first}",
        actual.len(),
        expected.len()
    );
}
/// The regular-file paths of a model, each alias name included.
fn files(flat: &Flat) -> Vec<(PathBuf, &[u8])> {
    flat.iter()
        .filter(|(_, seen)| seen.kind == FILE)
        .map(|(path, seen)| {
            (
                PathBuf::from(OsStr::from_bytes(path)),
                seen.payload.as_slice(),
            )
        })
        .collect()
}

const FIRST: &str = r#"
set -euo pipefail
umask 022
mkdir -p made/sub
printf 'created before the first Commit\n' > made/top.txt
head -c 20000 /dev/zero | tr '\0' 'N' > made/new.bin
: > made/empty
ln -s ../top.txt made/sub/rel-link
printf 'CHANGED' | dd of=docs/guide.md bs=1 seek=2000 conv=notrunc status=none
printf 'appended before the first Commit\n' >> README.md
mv -T spare/inner spare/inner-renamed
ln .git/index .git/index.bak
rm -f spare/gone.txt readme-link
chmod 0600 src/lib.rs
touch -d '2001-09-09 01:46:40.123456789 UTC' node_modules/pkg/package.json
"#;
const SECOND: &str = r#"
set -euo pipefail
umask 022
printf 'second round\n' >> made/top.txt
truncate -s 1000 .cache/tool/data.bin
truncate -s 70000 docs/guide.md
rm -f made/new.bin
mkdir made/later
head -c 150000 /dev/zero | tr '\0' 'L' > made/later/large.bin
ln made/later/large.bin made/large-alias.bin
touch -d '2010-10-10 10:10:10.000000001 UTC' src/main.rs
"#;

/// One path's attributes with its access time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Timed {
    ino: u64,
    /// Type and permission bits.
    mode: u32,
    nlink: u32,
    size: u64,
    blocks: u64,
    atime: (i64, u32),
    mtime: (i64, u32),
    ctime: (i64, u32),
    uid: u32,
    gid: u32,
}
/// `statx` of a path, not following a symlink. `sync` is
/// `AT_STATX_FORCE_SYNC` (the kernel fetches the attributes from the daemon
/// for this call) or `AT_STATX_SYNC_AS_STAT` (what `lstat` reports, which the
/// kernel may answer from its attribute cache).
fn statx(path: &Path, sync: libc::c_int) -> Timed {
    let mut raw = std::mem::MaybeUninit::<libc::statx>::zeroed();
    Errno::result(unsafe {
        libc::statx(
            libc::AT_FDCWD,
            c(path).as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW | sync,
            libc::STATX_BASIC_STATS,
            raw.as_mut_ptr(),
        )
    })
    .unwrap_or_else(|error| panic!("statx {path:?}: {error}"));
    let raw = unsafe { raw.assume_init() };
    assert_eq!(
        raw.stx_mask & (libc::STATX_ATIME | libc::STATX_MTIME),
        libc::STATX_ATIME | libc::STATX_MTIME,
        "{path:?}: the access and modification times were reported"
    );
    Timed {
        ino: raw.stx_ino,
        mode: u32::from(raw.stx_mode),
        nlink: raw.stx_nlink,
        size: raw.stx_size,
        blocks: raw.stx_blocks,
        atime: (raw.stx_atime.tv_sec, raw.stx_atime.tv_nsec),
        mtime: (raw.stx_mtime.tv_sec, raw.stx_mtime.tv_nsec),
        ctime: (raw.stx_ctime.tv_sec, raw.stx_ctime.tv_nsec),
        uid: raw.stx_uid,
        gid: raw.stx_gid,
    }
}
type Table = BTreeMap<PathBuf, Timed>;
/// The ordinary and the forced table of every path of a mount, the ordinary
/// one first so that it is not a copy the forced pass has just refreshed.
/// In both, every path's access time is its modification time.
fn tables(mount: &Path, what: &str) -> (Table, Table) {
    let paths: Vec<PathBuf> = kernel(mount).into_keys().collect();
    let read = |sync| -> Table {
        paths
            .iter()
            .map(|path| (path.clone(), statx(&mount.join(path), sync)))
            .collect()
    };
    let cached = read(libc::AT_STATX_SYNC_AS_STAT);
    let forced = read(libc::AT_STATX_FORCE_SYNC);
    for (label, table) in [("lstat", &cached), ("forced", &forced)] {
        for (path, row) in table {
            assert_eq!(
                row.atime, row.mtime,
                "{what}: {label} access time of {path:?} is not its modification time"
            );
        }
    }
    (cached, forced)
}
/// Entry by entry, both ways.
fn same_table(before: &Table, after: &Table, what: &str) {
    let mut wrong = Vec::new();
    for (path, row) in before {
        match after.get(path) {
            Some(other) if other == row => {}
            other => wrong.push(format!("{path:?}: {row:?} -> {other:?}")),
        }
    }
    for (path, row) in after {
        if !before.contains_key(path) {
            wrong.push(format!("{path:?}: absent -> {row:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{what}: {} of {} paths differ; first: {:#?}",
        wrong.len(),
        before.len(),
        &wrong[..wrong.len().min(8)]
    );
}
/// The forced table agrees with the model: the access time and the
/// modification time of every path are the model's modification time.
fn times_of_the_model(expected: &Flat, forced: &Table, what: &str) {
    assert_eq!(expected.len(), forced.len(), "{what}: paths");
    for (path, row) in forced {
        let seen = expected
            .get(path.as_os_str().as_bytes())
            .unwrap_or_else(|| panic!("{what}: unexpected {path:?}"));
        assert_eq!(row.mtime, seen.mtime, "{what}: mtime of {path:?}");
        assert_eq!(row.atime, seen.mtime, "{what}: atime of {path:?}");
    }
}

/// R5-5, access time. `stx_atime` is in both checked tables: equal to the
/// modification time for every path, and unchanged by two installs and by
/// reading every file through the page cache and around it.
#[test]
fn r5_5_access_time_is_the_modification_time_and_unchanged_by_reads_and_two_installs() {
    let rig = Rig::new("r5-5-atime");
    let ready = rig.mount(1);
    let mount = root(&ready);
    let entry = mount_entry(&ready.directory).expect("the mount");
    assert!(
        entry.options.split(',').any(|option| option == "noatime"),
        "{entry:?}"
    );
    let base = native(mount);
    let (_, forced) = tables(mount, "the mounted base");
    times_of_the_model(&base, &forced, "the mounted base");

    passed(&bash_in(COMMAND, mount, FIRST), "first changes");
    let expected = native(mount);
    assert_eq!(
        at(&expected, "node_modules/pkg/package.json").mtime,
        (1_000_000_000, 123_456_789)
    );
    let (before_cached, before_forced) = tables(mount, "before the first Commit");
    times_of_the_model(&expected, &before_forced, "before the first Commit");
    let explicit = before_forced[Path::new("node_modules/pkg/package.json")];
    assert_eq!(
        (explicit.atime, explicit.mtime),
        ((1_000_000_000, 123_456_789), (1_000_000_000, 123_456_789))
    );
    let first = rig.committed(ready.token, "R5-5 atime first");
    let (after_cached, after_forced) = tables(mount, "after the first install");
    same_table(&before_cached, &after_cached, "first install: lstat table");
    same_table(
        &before_forced,
        &after_forced,
        "first install: forced stat table",
    );

    // Reading every file, through the page cache and around it, changes no
    // access time in either table.
    assert_same(&expected, &native(mount), "the same mount after install");
    let mut direct_bytes = 0;
    for (path, bytes) in files(&expected) {
        same_bytes(&direct(&mount.join(&path)), bytes, "daemon read");
        direct_bytes += bytes.len();
    }
    let (read_cached, read_forced) = tables(mount, "after reading every file");
    same_table(&after_cached, &read_cached, "reads: lstat table");
    same_table(&after_forced, &read_forced, "reads: forced stat table");
    assert_same(&expected, &rig.published(first.root), "the first root");

    passed(&bash_in(COMMAND, mount, SECOND), "second changes");
    let later = native(mount);
    assert_eq!(at(&later, "src/main.rs").mtime, (1_286_705_410, 1));
    let (before_cached, before_forced) = tables(mount, "before the second Commit");
    times_of_the_model(&later, &before_forced, "before the second Commit");
    let second = rig.committed(ready.token, "R5-5 atime second");
    assert_eq!(second.parent, Some(first.id));
    let (after_cached, after_forced) = tables(mount, "after the second install");
    same_table(&before_cached, &after_cached, "second install: lstat table");
    same_table(
        &before_forced,
        &after_forced,
        "second install: forced stat table",
    );
    assert_same(&later, &rig.published(second.root), "the second root");
    rig.unmount(&ready);

    let fresh = rig.mount(2);
    let again = root(&fresh);
    assert_same(&later, &native(again), "the fresh mount");
    let (fresh_cached, fresh_forced) = tables(again, "the fresh mount");
    times_of_the_model(&later, &fresh_forced, "the fresh mount");
    // The times a fresh mount reports are the times the first mount reported.
    for (path, row) in &fresh_forced {
        let old = after_forced[path];
        assert_eq!((row.atime, row.mtime), (old.atime, old.mtime), "{path:?}");
        assert_eq!(fresh_cached[path].atime, old.atime, "{path:?}");
    }
    println!(
        "R5-5-ATIME noatime=true paths(base={} first={} second={}) tables=lstat,forced atime_equals_mtime=every_path installs=2 table_changes_at_install=0 table_changes_by_reads=0 files_read_o_direct={} direct_bytes={direct_bytes}",
        base.len(),
        expected.len(),
        later.len(),
        files(&expected).len()
    );
    rig.unmount(&fresh);
    rig.finish();
}

/// Every regular file of the model read with `O_DIRECT` by name and through
/// the page cache by name. No descriptor is kept. Returns the files, the
/// bytes and the requests the direct reads sent.
fn every_file_direct(
    rig: &Rig,
    ready: &layerfs_bridge::control::ReadyMount,
    expected: &Flat,
    when: &str,
) -> (usize, usize, u64) {
    let mount = root(ready);
    let start = frames(&rig.harness, ready.token);
    let (mut count, mut total) = (0, 0);
    for (path, bytes) in files(expected) {
        same_bytes(
            &direct(&mount.join(&path)),
            bytes,
            &format!("{when}: O_DIRECT read of {path:?}"),
        );
        count += 1;
        total += bytes.len();
    }
    let requests = frames(&rig.harness, ready.token) - start;
    // Each file is at least an OPEN the daemon answers.
    assert!(
        requests >= count as u64,
        "{when}: {requests} requests for {count} files"
    );
    for (path, bytes) in files(expected) {
        same_bytes(
            &fs::read(mount.join(&path)).unwrap(),
            bytes,
            &format!("{when}: cached read of {path:?}"),
        );
    }
    (count, total, requests)
}

/// R5-5, `O_DIRECT` without a kept descriptor: after each of two installs
/// every regular file of the model, changed or not, is read around the page
/// cache by a fresh open and equals the model and the cached read.
#[test]
fn r5_5_every_model_file_reads_exactly_with_o_direct_after_install_with_no_kept_descriptor() {
    let rig = Rig::new("r5-5-direct");
    let ready = rig.mount(1);
    let mount = root(&ready);
    passed(&bash_in(COMMAND, mount, FIRST), "first changes");
    // The model reads every file once through the page cache, so the cache
    // holds the bytes of before the install.
    let expected = native(mount);
    let mut guide = pattern(10, 5_000);
    guide[2_000..2_007].copy_from_slice(b"CHANGED");
    same_bytes(
        file(&expected, "docs/guide.md"),
        &guide,
        "changed base file",
    );
    same_bytes(
        file(&expected, "target/debug/app"),
        &pattern(7, 300_017),
        "unchanged base file",
    );
    same_bytes(
        file(&expected, "made/new.bin"),
        &[b'N'; 20_000],
        "created file",
    );
    let first = rig.committed(ready.token, "R5-5 direct first");
    let one = every_file_direct(&rig, &ready, &expected, "after the first install");
    assert_same(&expected, &rig.published(first.root), "the first root");

    passed(&bash_in(COMMAND, mount, SECOND), "second changes");
    let later = native(mount);
    guide.resize(70_000, 0);
    same_bytes(file(&later, "docs/guide.md"), &guide, "extended file");
    same_bytes(
        file(&later, ".cache/tool/data.bin"),
        &pattern(6, 1_000),
        "truncated file",
    );
    same_bytes(
        file(&later, "made/later/large.bin"),
        &[b'L'; 150_000],
        "large created file",
    );
    assert_eq!(at(&later, "made/large-alias.bin").links, 2);
    let second = rig.committed(ready.token, "R5-5 direct second");
    assert_eq!(second.parent, Some(first.id));
    let two = every_file_direct(&rig, &ready, &later, "after the second install");
    assert_same(&later, &rig.published(second.root), "the second root");
    assert_same(&later, &native(mount), "the same mount");
    rig.unmount(&ready);

    let fresh = rig.mount(2);
    assert_eq!(
        rig.harness.status(fresh.token).binding.effective_root,
        second.root
    );
    let three = every_file_direct(&rig, &fresh, &later, "the fresh mount");
    assert_same(&later, &native(root(&fresh)), "the fresh mount");
    println!(
        "R5-5-DIRECT kept_descriptors=0 after_first_install(files={} bytes={} requests={}) after_second_install(files={} bytes={} requests={}) fresh_mount(files={} bytes={} requests={})",
        one.0, one.1, one.2, two.0, two.1, two.2, three.0, three.1, three.2
    );
    rig.unmount(&fresh);
    rig.finish();
}

/// One file mapped shared by this process, with its writable descriptor.
struct Held {
    name: &'static str,
    path: PathBuf,
    handle: File,
    map: Mapping,
    length: usize,
    /// What the file holds, as this test stored it.
    bytes: Vec<u8>,
}
impl Held {
    fn open(mount: &Path, name: &'static str, pages: usize, bytes: Vec<u8>) -> Self {
        let path = mount.join(name);
        let handle = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let length = pages * PAGE;
        assert!(
            length <= bytes.len(),
            "{name}: the mapping is inside the file"
        );
        let map = Mapping::shared(&handle, length);
        let held = Self {
            name,
            path,
            handle,
            map,
            length,
            bytes,
        };
        held.coherent("mapped");
        held
    }
    /// A store through the mapping, written back by `msync`.
    fn store(&mut self, offset: usize, bytes: &[u8]) {
        self.map.store(offset, bytes);
        self.bytes[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.map.sync();
    }
    /// The mapping, the page cache through the kept descriptor and through a
    /// fresh open, and the daemon around the cache all hold the same bytes.
    fn coherent(&self, when: &str) {
        let name = self.name;
        same_bytes(
            &self.map.load(0, self.length),
            &self.bytes[..self.length],
            &format!("{when}: {name}: mapping"),
        );
        let mut through = vec![0; self.bytes.len()];
        self.handle.read_exact_at(&mut through, 0).unwrap();
        same_bytes(
            &through,
            &self.bytes,
            &format!("{when}: {name}: kept descriptor"),
        );
        same_bytes(
            &fs::read(&self.path).unwrap(),
            &self.bytes,
            &format!("{when}: {name}: cached read"),
        );
        same_bytes(
            &direct(&self.path),
            &self.bytes,
            &format!("{when}: {name}: daemon read"),
        );
        assert_eq!(
            self.handle.metadata().unwrap().len(),
            self.bytes.len() as u64,
            "{when}: {name}: size"
        );
    }
}

/// R5-5, mappings across install. Two shared mappings of this process are
/// held across two installs: one of a file nothing changes, one of a file
/// stored through before and after each install. After every install each
/// holds its exact content and agrees with the page cache and the daemon.
#[test]
fn r5_5_shared_mappings_held_across_two_installs_keep_exact_content_and_coherence() {
    let rig = Rig::new("r5-5-map");
    let ready = rig.mount(1);
    let mount = root(&ready);
    let untouched = Held::open(mount, "dist/bundle.js", 8, pattern(9, 33_000));
    let mut stored = Held::open(mount, "target/debug/app", 64, pattern(7, 300_017));

    // Before the first install: a store, written back, and changes beside.
    stored.store(3 * PAGE + 17, b"stored-before-the-first-install");
    passed(&bash_in(COMMAND, mount, FIRST), "first changes");
    let expected = native(mount);
    same_bytes(
        file(&expected, "target/debug/app"),
        &stored.bytes,
        "model of the stored file",
    );
    same_bytes(
        file(&expected, "dist/bundle.js"),
        &untouched.bytes,
        "model of the untouched file",
    );
    let first = rig.committed(ready.token, "R5-5 map first");
    untouched.coherent("after the first install");
    stored.coherent("after the first install");
    assert_same(&expected, &rig.published(first.root), "the first root");
    assert_same(&expected, &native(mount), "the same mount, first install");

    // The same mappings after the first install: a store across a page
    // boundary, written back, and more changes beside.
    stored.store(40 * PAGE - 4, b"stored-after-the-first-install");
    stored.coherent("a store after the first install");
    passed(&bash_in(COMMAND, mount, SECOND), "second changes");
    let later = native(mount);
    same_bytes(
        file(&later, "target/debug/app"),
        &stored.bytes,
        "model of the stored file",
    );
    assert_ne!(later, expected);
    let second = rig.committed(ready.token, "R5-5 map second");
    assert_eq!(second.parent, Some(first.id));
    untouched.coherent("after the second install");
    stored.coherent("after the second install");
    let published = rig.published(second.root);
    assert_same(&later, &published, "the second root");
    same_bytes(
        file(&published, "target/debug/app"),
        &stored.bytes,
        "second published bytes of the stored file",
    );
    same_bytes(
        file(&published, "dist/bundle.js"),
        &untouched.bytes,
        "second published bytes of the untouched file",
    );
    assert_same(&later, &native(mount), "the same mount, second install");

    // Still live after two installs: one more store reaches the daemon, and
    // the next Commit publishes it.
    stored.store(10 * PAGE, b"stored-after-the-second-install");
    stored.coherent("a store after the second install");
    untouched.coherent("at the end");
    let last = native(mount);
    let third = rig.committed(ready.token, "R5-5 map third");
    assert_eq!(third.parent, Some(second.id));
    assert_same(&last, &rig.published(third.root), "the third root");
    stored.coherent("after the third install");
    let Held {
        map: untouched_map,
        handle: untouched_handle,
        ..
    } = untouched;
    let Held {
        map: stored_map,
        handle: stored_handle,
        bytes: stored_bytes,
        ..
    } = stored;
    untouched_map.unmap();
    stored_map.unmap();
    drop((untouched_handle, stored_handle));
    let drained = rig.unmount(&ready);
    let stores = mutating::counter(&drained, "store_units");
    // Page 3, pages 39 and 40 (possibly together) and page 10.
    assert!((3..=4).contains(&stores), "{drained}");

    let fresh = rig.mount(2);
    let again = root(&fresh);
    assert_same(&last, &native(again), "the fresh mount");
    same_bytes(
        &direct(&again.join("target/debug/app")),
        &stored_bytes,
        "fresh mount, daemon read",
    );
    assert_kernel(&last, &kernel(again), "R5-5 map fresh mount");
    println!(
        "R5-5-MAP mappings=2 installs_under_the_mappings=3 stores=3 store_units={stores} coherent(mapping,kept_descriptor,cached,o_direct)=after_every_install paths={}",
        last.len()
    );
    rig.unmount(&fresh);
    rig.finish();
}

/// A writer that parks on a pipe after each of its four steps:
///
/// ```text
/// A(i) create work/next.tmp, kept open   B(i) write its content
/// C(i) close it, rename it to its name   D(i) append <i> to seq.log
/// ```
///
/// After a step it prints the step's label and blocks in `read`; one line on
/// its standard input lets it take the next step.
const STEPPED: &str = r#"
umask 022
park() { printf '%s\n' "$1"; IFS= read -r _ || exit 9; }
i=0
while [ "$i" -lt "$rounds" ]; do
  i=$((i+1))
  printf -v n '%06d' "$i"
  exec 4> work/next.tmp || exit 3
  park "A$i"
  printf 'sequence file %s\n' "$n" >&4 || exit 4
  park "B$i"
  exec 4>&-
  mv -T work/next.tmp "seq/$n" || exit 5
  park "C$i"
  printf '%s\n' "$n" >> seq.log || exit 6
  park "D$i"
done
"#;
const ROUNDS: usize = 2;

/// The stepped writer: an ordinary bash process of the command identity that
/// nothing registered. Killed and reaped on drop, so none is left and no
/// wait here depends on it.
struct Stepper {
    child: Option<Child>,
    input: Option<ChildStdin>,
    output: ChildStdout,
    seen: Vec<u8>,
}
impl Stepper {
    fn spawn(directory: &Path) -> Self {
        let mut child = Command::new("bash")
            .args(["-c", &format!("rounds={ROUNDS}\n{STEPPED}")])
            .uid(COMMAND)
            .gid(COMMAND)
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        // Reads of the label pipe never block: every wait below is bounded.
        let descriptor = output.as_raw_fd();
        let flags = Errno::result(unsafe { libc::fcntl(descriptor, libc::F_GETFL) }).unwrap();
        Errno::result(unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) })
            .unwrap();
        Self {
            child: Some(child),
            input,
            output,
            seen: Vec::new(),
        }
    }
    fn alive(&mut self) -> bool {
        self.child.as_mut().unwrap().try_wait().unwrap().is_none()
    }
    /// Bounded wait until the writer has printed exactly `label` as its next
    /// line: it has finished that step and is blocked in `read`.
    fn parked(&mut self, label: &str) {
        let wanted = format!("{label}\n").into_bytes();
        until(&format!("the writer parked at {label}"), || {
            let mut buffer = [0_u8; 64];
            match self.output.read(&mut buffer) {
                Ok(0) => panic!("the writer closed its output before {label}"),
                Ok(count) => self.seen.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("label pipe: {error}"),
            }
            assert!(
                wanted.starts_with(&self.seen),
                "the writer printed {:?} where {label} was awaited",
                String::from_utf8_lossy(&self.seen)
            );
            self.seen == wanted
        });
        self.seen.clear();
        assert!(self.alive(), "the writer is alive at {label}");
    }
    /// Lets the writer take its next step.
    fn go(&mut self) {
        self.input.as_mut().unwrap().write_all(b"\n").unwrap();
    }
    /// Closes its input and waits, bounded, for it to exit.
    fn finish(mut self) -> ExitStatus {
        drop(self.input.take());
        let mut child = self.child.take().unwrap();
        let mut status = None;
        until("the writer exited", || {
            status = child.try_wait().unwrap();
            status.is_some()
        });
        status.unwrap()
    }
}
impl Drop for Stepper {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn numbered(number: usize) -> Vec<u8> {
    format!("sequence file {number:06}\n").into_bytes()
}
/// What the writer's own files are in a tree: the numbered files (exactly 1
/// to that count, each complete), the lines of the log (exactly 1 to that
/// count) and the pending file's bytes if it exists.
fn writer_state(flat: &Flat, what: &str) -> (usize, usize, Option<Vec<u8>>) {
    let mut count = 0;
    for (path, seen) in flat.iter().filter(|(path, _)| path.starts_with(b"seq/")) {
        count += 1;
        assert_eq!(
            String::from_utf8_lossy(path),
            format!("seq/{count:06}"),
            "{what}: the numbered files are not a contiguous prefix"
        );
        assert_eq!((seen.kind, seen.mode, seen.links), (FILE, 0o644, 1));
        same_bytes(&seen.payload, &numbered(count), what);
    }
    let log = file(flat, "seq.log");
    let lines = log.len() / 7;
    let wanted: Vec<u8> = (1..=lines)
        .flat_map(|number| format!("{number:06}\n").into_bytes())
        .collect();
    same_bytes(log, &wanted, &format!("{what}: seq.log"));
    let pending = flat.get(b"work/next.tmp".as_slice()).map(|seen| {
        assert_eq!(seen.kind, FILE, "{what}: the pending file");
        seen.payload.clone()
    });
    (count, lines, pending)
}

/// R5-5, a capture inside one iteration of a writer. The writer is parked
/// after each of its steps and one control Commit is taken at each position,
/// so every capture here lands between two steps of one iteration by
/// construction, not by timing. Each published root holds exactly the state
/// of that position, read from a fresh bind, from the same mount after
/// install and from a fresh mount of a new Workspace of the Branch.
#[test]
fn r5_5_a_commit_at_each_parked_step_of_a_writer_is_exact_on_the_same_and_a_fresh_mount() {
    let rig = Rig::new("r5-5-steps");
    let ready = rig.mount(1);
    let mount = root(&ready);
    let mut parent = rig.harness.status(ready.token).binding.branch.head_commit;
    passed(
        &bash_in(
            COMMAND,
            mount,
            "set -euo pipefail; umask 022; mkdir seq work; : > seq.log",
        ),
        "prepare",
    );
    let mut writer = Stepper::spawn(mount);
    let mut positions = Vec::new();
    let mut tag = 2;
    for round in 1..=ROUNDS {
        for step in ["A", "B", "C", "D"] {
            let what = format!("R5-5 step {step}{round}");
            writer.parked(&format!("{step}{round}"));
            // The tree at this position; the writer is blocked in `read`.
            let expected = native(mount);
            let state = writer_state(&expected, &what);
            let wanted = match step {
                // Created and still open in the writer, nothing written.
                "A" => (round - 1, round - 1, Some(Vec::new())),
                // Written through the open descriptor, not yet renamed.
                "B" => (round - 1, round - 1, Some(numbered(round))),
                // Renamed into place, not yet logged.
                "C" => (round, round - 1, None),
                _ => (round, round, None),
            };
            assert_eq!(state, wanted, "{what}: the parked position");
            let record = rig.committed(ready.token, &what);
            assert_eq!(record.parent, parent, "{what}: parent is the previous head");
            parent = Some(record.id);
            assert!(writer.alive(), "{what}: the writer outlived the Commit");
            let published = rig.published(record.root);
            assert_same(&expected, &published, &format!("{what}: published root"));
            assert_eq!(
                writer_state(&published, &what),
                wanted,
                "{what}: the captured position"
            );
            assert_same(&expected, &native(mount), &format!("{what}: same mount"));
            // A new Workspace of the Branch, mounted beside the first.
            let fresh = rig.mount(tag);
            tag += 1;
            assert_eq!(
                rig.harness.status(fresh.token).binding.effective_root,
                record.root
            );
            let again = native(root(&fresh));
            assert_same(&expected, &again, &format!("{what}: fresh mount"));
            assert_eq!(writer_state(&again, &what), wanted);
            assert_kernel(&expected, &kernel(root(&fresh)), &what);
            rig.unmount(&fresh);
            println!(
                "{what}: numbered_files={} log_lines={} pending={} paths={} oracles=published,same_mount,fresh_mount",
                wanted.0,
                wanted.1,
                match &wanted.2 {
                    None => "absent",
                    Some(bytes) if bytes.is_empty() => "empty",
                    Some(_) => "complete",
                },
                expected.len()
            );
            positions.push(record);
            writer.go();
        }
    }
    let status = writer.finish();
    assert!(status.success(), "{status:?}");
    // Nothing followed the last position.
    let last = positions.last().unwrap();
    assert_eq!(
        rig.up_to_date(ready.token, "R5-5 steps after the writer"),
        (Some(last.id), last.root)
    );
    let newest: Vec<_> = rig
        .history()
        .records
        .iter()
        .take(positions.len())
        .map(|record| record.id)
        .collect();
    let made: Vec<_> = positions.iter().rev().map(|record| record.id).collect();
    assert_eq!(newest, made, "one record per position, newest first");
    println!(
        "R5-5-STEPS rounds={ROUNDS} positions={} commits={} fresh_mounts={} positions_inside_an_iteration={}",
        positions.len(),
        positions.len(),
        positions.len(),
        3 * ROUNDS
    );
    rig.unmount(&ready);
    rig.finish();
}
