//! Actual mounted mknod/link/unlink/rmdir/rename/setattr and post-Commit durability.
#[cfg(target_os = "linux")]
#[path = "support/native_workspace.rs"]
mod support;
#[cfg(target_os = "linux")]
mod linux {
    use super::support::*;
    use layerfs_bridge::contract::*;
    use layerfs_workspace::*;
    use std::{
        fs::{self, File, OpenOptions},
        os::unix::fs::{FileExt, MetadataExt},
        path::{Path, PathBuf},
        process::Command,
        time::{Duration, Instant},
    };

    fn check(id: &str) {
        println!("MOUNTED_NAMESPACE_CHECK {id} PASS");
    }
    fn fixture(gate: Gate) -> Fixture {
        let native = Native::new_fresh(gate);
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: PathBuf::from(std::env::var("LAYERFS_STAGE_TEST_ROOT").unwrap()),
                max_count: 3,
                memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
                disk_budget_bytes: Some(64 * 1024 * 1024),
            },
            native.delivery(),
        )
        .unwrap();
        let workspace = host
            .attach(
                Fixture::options("stage", 31, WorkspaceAccess::LocalEdit),
                deadline(),
            )
            .unwrap();
        assert_eq!(workspace.root().uid, 0);
        Fixture {
            host,
            workspace,
            native,
        }
    }
    fn kernel(f: &Fixture, script: &str) {
        let out = Command::new("python3")
            .args(["-c", script])
            .arg(f.workspace.mount_path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        print!("{}", String::from_utf8_lossy(&out.stdout));
    }
    fn mounted(f: &Fixture) -> bool {
        fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| {
                line.split(' ').nth(4) == Some(f.workspace.mount_path().to_str().unwrap())
                    && (line.contains(" - fuse layerfs ")
                        || line.contains(" - fuse.layerfs layerfs "))
            })
    }
    fn writable_mount(f: &Fixture) -> layerfs_fuse::MountHandle {
        let mount = layerfs_fuse::mount_writable(&f.workspace, deadline()).unwrap();
        let mountinfo = fs::read_to_string("/proc/self/mountinfo").unwrap();
        let row = mountinfo
            .lines()
            .find(|line| {
                line.split(' ').nth(4) == Some(f.workspace.mount_path().to_str().unwrap())
                    && (line.contains(" - fuse layerfs ")
                        || line.contains(" - fuse.layerfs layerfs "))
            })
            .expect("actual FUSE mount must exist");
        let options = row.split(' ').nth(5).unwrap();
        assert!(options.split(',').any(|option| option == "rw"), "{row}");
        assert!(!options.split(',').any(|option| option == "ro"), "{row}");
        println!(
            "NAMESPACE_MOUNTED_RESOURCE mount_profile=writable actual_mount_options={options}"
        );
        mount
    }
    fn quiescent(f: &Fixture) {
        let end = deadline();
        while f.workspace.status().unwrap().projection_replies != 0 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn drained(f: &Fixture) {
        let end = deadline();
        loop {
            let s = f.workspace.status().unwrap();
            if s.projection_handles == 0 && s.projection_replies == 0 {
                break;
            }
            assert!(Instant::now() < end, "{s:?}");
            std::thread::yield_now();
        }
    }
    fn snapshot(f: &Fixture) -> BranchSnapshotWire {
        let Response::History(value) = f.branch() else {
            panic!("history")
        };
        let HistoryResult::BranchSnapshot(value) = *value else {
            panic!("branch")
        };
        value
    }
    fn count(f: &Fixture, reserves: bool) -> usize {
        f.native
            .observations
            .lock()
            .unwrap()
            .operations
            .iter()
            .filter(|op| {
                if reserves {
                    matches!(
                        op,
                        Operation::HistoryCommand(HistoryCommand::ReserveInodes { .. })
                    )
                } else {
                    matches!(
                        op,
                        Operation::HistoryCommand(
                            HistoryCommand::Commit(_) | HistoryCommand::CommitStaged { .. }
                        )
                    )
                }
            })
            .count()
    }
    fn lookup(f: &Fixture, parent: u64, name: &[u8]) -> NodeAttributes {
        f.workspace
            .lookup(parent, name, ReferenceScope::Local, deadline())
            .unwrap()
    }
    fn commit(f: &Fixture) -> Root {
        let report = f.workspace.commit(deadline()).unwrap();
        assert!(f.workspace.status().unwrap().submission.is_none());
        let CommitOutcomeWire::Committed(commit) = report.outcome else {
            panic!("new commit")
        };
        assert_eq!(snapshot(f).effective_root, commit.root);
        commit.root
    }
    fn kernel_file(path: &Path, a: &NodeAttributes) {
        let m = fs::symlink_metadata(path)
            .unwrap_or_else(|error| panic!("kernel lstat {path:?}: {error}"));
        assert!(m.is_file());
        assert_eq!(
            (
                m.ino(),
                m.len(),
                m.mode() & 0o7777,
                m.mtime(),
                m.mtime_nsec()
            ),
            (
                a.serial,
                a.size,
                a.mode,
                a.mtime_seconds,
                i64::from(a.mtime_nanoseconds)
            )
        );
    }
    /// The kernel-side identity of one mounted path: inode, namespace link
    /// count, mode, mtime and size exactly as the real mount answers lstat.
    fn identity(path: &Path) -> (u64, u64, u32, (i64, i64), u64) {
        let m = fs::symlink_metadata(path)
            .unwrap_or_else(|error| panic!("kernel lstat {path:?}: {error}"));
        (
            m.ino(),
            m.nlink(),
            m.mode() & 0o7777,
            (m.mtime(), m.mtime_nsec()),
            m.len(),
        )
    }
    fn names(path: &Path) -> Vec<String> {
        let mut rows: Vec<_> = fs::read_dir(path)
            .unwrap_or_else(|error| panic!("kernel listdir {path:?}: {error}"))
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        rows.sort();
        rows
    }
    fn bytes(file: &File, count: usize) -> Vec<u8> {
        let mut out = vec![0; count];
        file.read_exact_at(&mut out, 0).unwrap();
        out
    }
    fn saved_file(
        f: &Fixture,
        root: Root,
        path: &[u8],
        expected: &[u8],
        serial: u64,
        references: u64,
        mode: u32,
        mtime: (i64, u32),
    ) {
        let Response::Attributes {
            serial: saved_serial,
            kind,
            references: saved_references,
            mode: saved_mode,
            mtime: saved_mtime,
            nanoseconds: saved_nanos,
            size,
            content,
            ..
        } = f.native.attributes(root, path)
        else {
            panic!("attributes")
        };
        assert_eq!(
            (
                saved_serial,
                kind,
                saved_references,
                saved_mode,
                saved_mtime,
                saved_nanos,
                size
            ),
            (
                serial,
                1,
                references,
                mode,
                mtime.0,
                mtime.1,
                expected.len() as u64
            )
        );
        assert_eq!(f.native.bytes(content, 0, expected.len()), expected);
    }
    fn saved_directory(f: &Fixture, root: Root, path: &[u8], serial: u64, mode: u32) {
        let Response::Attributes {
            serial: saved_serial,
            kind,
            mode: saved_mode,
            ..
        } = f.native.attributes(root, path)
        else {
            panic!("attributes")
        };
        assert_eq!((saved_serial, kind, saved_mode), (serial, 2, mode));
    }
    fn missing(f: &Fixture, root: Root, name: &[u8]) {
        assert_eq!(
            f.native
                .request(
                    Operation::Inspect {
                        root,
                        query: Inspect::Attributes {
                            path: name.to_vec()
                        }
                    },
                    0,
                    &mut std::io::sink()
                )
                .unwrap_err()
                .code,
            Code::PathNotFound
        );
    }
    fn close(f: &Fixture, local: &[(u64, u64)]) {
        assert!(!mounted(f));
        for (serial, count) in local {
            f.workspace.forget(*serial, *count, ReferenceScope::Local);
            assert_eq!(f.workspace.getattr(*serial), Err(WorkspaceError::NotFound));
        }
        println!(
            "NAMESPACE_MOUNTED_CLOSE {:?}",
            f.workspace.status().unwrap()
        );
        f.workspace.close_clean().unwrap();
        check("native-clean-close");
    }

    #[test]
    #[ignore = "requires actual kernel namespace syscalls on a privileged FUSE mount"]
    fn mounted_namespace_kernel() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let mut mount = writable_mount(&f);
        kernel(
            &f,
            r#"
import errno, os, stat, sys
p = sys.argv[1]
os.umask(0o027)
os.mkdir(p + '/left', 0o777)
os.mkdir(p + '/right', 0o777)
assert stat.S_IMODE(os.lstat(p + '/left').st_mode) == 0o750
assert stat.S_IMODE(os.lstat(p + '/right').st_mode) == 0o750
os.mknod(p + '/left/node.bin', 0o666 | stat.S_IFREG)
st = os.lstat(p + '/left/node.bin')
assert stat.S_ISREG(st.st_mode) and st.st_size == 0
assert stat.S_IMODE(st.st_mode) == 0o640
try:
    os.mknod(p + '/left/node.bin', 0o644 | stat.S_IFREG)
except OSError as e:
    assert e.errno == errno.EEXIST
else:
    raise AssertionError('duplicate mknod accepted')
print('NAMESPACE_MOUNTED_MKNOD umask=027 requested=0666 stored=0640 size=0 duplicate=EEXIST')
"#,
        );
        quiescent(&f);
        // mknod published its file without opening a handle.
        assert_eq!(f.workspace.status().unwrap().handles, 0);
        let top = f.workspace.root().serial;
        let left = lookup(&f, top, b"left");
        let right = lookup(&f, top, b"right");
        assert_eq!(
            (left.kind, right.kind),
            (NodeKind::Directory, NodeKind::Directory)
        );
        assert_eq!((left.mode, right.mode), (0o750, 0o750));
        let node = lookup(&f, left.serial, b"node.bin");
        assert_eq!(node.kind, NodeKind::File);
        assert_eq!((node.mode, node.size, node.references), (0o640, 0, 1));
        kernel_file(&f.workspace.mount_path().join("left/node.bin"), &node);
        kernel(
            &f,
            r#"
import ctypes, errno, os, stat, sys

LIBC = ctypes.CDLL(None, use_errno=True)
AT_FDCWD, UTIME_OMIT = -100, (1 << 30) - 2

class Timespec(ctypes.Structure):
    _fields_ = [('seconds', ctypes.c_long), ('nanos', ctypes.c_long)]

def set_mtime(path, mtime_ns):
    times = (Timespec * 2)()
    times[0].seconds, times[0].nanos = 0, UTIME_OMIT
    times[1].seconds, times[1].nanos = divmod(mtime_ns, 10 ** 9)
    ctypes.set_errno(0)
    rc = LIBC.utimensat(AT_FDCWD, path.encode(), ctypes.byref(times), 0)
    assert rc == 0, 'utimensat %s -> %s(%d)' % (path, errno.errorcode.get(ctypes.get_errno(), '?'), ctypes.get_errno())

p = sys.argv[1]
left, right = p + '/left', p + '/right'

# link: two names share one inode and one live link count
fd = os.open(left + '/node.bin', os.O_WRONLY)
assert os.write(fd, b'shared-body') == 11
os.close(fd)
node_ino = os.lstat(left + '/node.bin').st_ino
os.link(left + '/node.bin', left + '/alias.bin')
a, b = os.lstat(left + '/node.bin'), os.lstat(left + '/alias.bin')
assert a.st_ino == b.st_ino == node_ino, (a.st_ino, b.st_ino, node_ino)
assert a.st_nlink == 2 and b.st_nlink == 2, (a.st_nlink, b.st_nlink)
fd = os.open(left + '/alias.bin', os.O_WRONLY | os.O_APPEND)
assert os.write(fd, b'!') == 1
os.close(fd)
assert open(left + '/node.bin', 'rb').read() == b'shared-body!'
assert open(left + '/alias.bin', 'rb').read() == b'shared-body!'

# rename: cross-directory move replaces a name whose FD stays held
os.mknod(right + '/victim.txt', 0o644 | stat.S_IFREG)
victim_ino = os.lstat(right + '/victim.txt').st_ino
held = os.open(right + '/victim.txt', os.O_RDWR)
assert os.write(held, b'victim-body') == 11
os.rename(left + '/node.bin', right + '/victim.txt')
replaced = os.fstat(held)
assert replaced.st_ino == victim_ino
assert replaced.st_nlink == 0
assert os.pread(held, 16, 0) == b'victim-body'
moved = os.lstat(right + '/victim.txt')
assert moved.st_ino == node_ino and moved.st_nlink == 2
assert open(right + '/victim.txt', 'rb').read() == b'shared-body!'
assert os.lstat(left + '/alias.bin').st_ino == node_ino
assert sorted(os.listdir(left)) == ['alias.bin']

# RENAME_NOREPLACE collision refuses with both trees unchanged
renameat2 = getattr(LIBC, 'renameat2', None)
assert renameat2 is not None, 'renameat2 unavailable'
renameat2.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
before_left, before_right = sorted(os.listdir(left)), sorted(os.listdir(right))
ctypes.set_errno(0)
rc = renameat2(AT_FDCWD, (left + '/alias.bin').encode(), AT_FDCWD, (right + '/victim.txt').encode(), 1)
assert rc != 0 and ctypes.get_errno() == errno.EEXIST, (rc, ctypes.get_errno())
assert sorted(os.listdir(left)) == before_left and sorted(os.listdir(right)) == before_right
assert os.lstat(left + '/alias.bin').st_ino == node_ino
assert os.lstat(right + '/victim.txt').st_ino == node_ino

# rmdir: cycle refusal, nonempty refusal, fresh-directory move, empty removal
os.mkdir(right + '/inner', 0o755)
os.mkdir(right + '/inner/child', 0o755)
inner_ino = os.lstat(right + '/inner').st_ino
try:
    os.rename(right + '/inner', right + '/inner/nested')
except OSError as e:
    assert e.errno == errno.EINVAL
else:
    raise AssertionError('directory-cycle rename accepted')
try:
    os.rmdir(right + '/inner')
except OSError as e:
    assert e.errno == errno.ENOTEMPTY
else:
    raise AssertionError('nonempty rmdir accepted')
assert sorted(os.listdir(right + '/inner')) == ['child']
os.rename(right + '/inner', left + '/inner')
assert os.lstat(left + '/inner').st_ino == inner_ino
assert 'inner' not in os.listdir(right)
os.rmdir(left + '/inner/child')
os.rmdir(left + '/inner')
assert 'inner' not in os.listdir(left)

# unlink with a held FD: last-name removal keeps the inode readable and writable
os.mknod(left + '/orphan.bin', 0o600 | stat.S_IFREG)
orphan_ino = os.lstat(left + '/orphan.bin').st_ino
fd = os.open(left + '/orphan.bin', os.O_RDWR)
assert os.write(fd, b'orphan-body') == 11
os.unlink(left + '/orphan.bin')
assert 'orphan.bin' not in os.listdir(left)
orphan = os.fstat(fd)
assert orphan.st_ino == orphan_ino
assert orphan.st_nlink == 0
assert os.pread(fd, 16, 0) == b'orphan-body'
assert os.pwrite(fd, b'ORPHAN', 0) == 6
assert os.pread(fd, 16, 0) == b'ORPHAN-body'
os.close(fd)

# setattr: chmod, UTIME_OMIT mtime, then mode preservation by a mtime-only set
os.chmod(left + '/alias.bin', 0o600)
set_mtime(left + '/alias.bin', 1700000123456000000)
st = os.lstat(left + '/alias.bin')
assert stat.S_IMODE(st.st_mode) == 0o600
assert st.st_mtime_ns == 1700000123456000000
set_mtime(left + '/alias.bin', 1700000456789000000)
st = os.lstat(left + '/alias.bin')
assert stat.S_IMODE(st.st_mode) == 0o600, 'mtime-only setattr reverted the mode'
assert st.st_mtime_ns == 1700000456789000000

assert sorted(os.listdir(left)) == ['alias.bin']
assert sorted(os.listdir(right)) == ['victim.txt']
final = os.lstat(right + '/victim.txt')
assert final.st_ino == node_ino and final.st_nlink == 2
assert stat.S_IMODE(final.st_mode) == 0o600
print('NAMESPACE_MOUNTED_KERNEL link=nlink2 rename=replace-held-fd-nlink0-noreplace-EEXIST cycle=EINVAL rmdir=empty-nonempty unlink=held-fd-nlink0 setattr=chmod-utime-omit')
os.close(held)
"#,
        );
        drained(&f);
        // The moved identity owns both surviving names with one live link count.
        let victim = lookup(&f, right.serial, b"victim.txt");
        let alias = lookup(&f, left.serial, b"alias.bin");
        assert_eq!((victim.serial, alias.serial), (node.serial, node.serial));
        assert_eq!((victim.references, alias.references), (2, 2));
        assert_eq!(victim.size, 12);
        assert_eq!(
            (victim.mode, victim.mtime_seconds, victim.mtime_nanoseconds),
            (0o600, 1_700_000_456, 789_000_000)
        );
        assert_eq!(f.workspace.getattr(node.serial).unwrap().references, 2);
        let path = f.workspace.mount_path();
        kernel_file(&path.join("right/victim.txt"), &victim);
        kernel_file(&path.join("left/alias.bin"), &alias);
        assert_eq!(names(&path.join("left")), vec!["alias.bin"]);
        assert_eq!(names(&path.join("right")), vec!["victim.txt"]);
        assert_eq!(count(&f, true), 7);
        assert_eq!(count(&f, false), 0);
        assert_eq!(snapshot(&f), before);
        let root = commit(&f);
        saved_file(
            &f,
            root,
            b"right/victim.txt",
            b"shared-body!",
            node.serial,
            2,
            0o600,
            (1_700_000_456, 789_000_000),
        );
        saved_file(
            &f,
            root,
            b"left/alias.bin",
            b"shared-body!",
            node.serial,
            2,
            0o600,
            (1_700_000_456, 789_000_000),
        );
        saved_directory(&f, root, b"left", left.serial, 0o750);
        saved_directory(&f, root, b"right", right.serial, 0o750);
        missing(&f, root, b"left/node.bin");
        missing(&f, root, b"left/orphan.bin");
        missing(&f, root, b"left/inner");
        missing(&f, root, b"left/inner/child");
        missing(&f, root, b"right/inner");
        missing(&f, before.effective_root, b"left");
        assert_eq!(count(&f, false), 1);
        mount.unmount(deadline()).unwrap();
        check("actual-kernel-mknod-link-unlink-rmdir-rename-setattr-errno-identity-nlink-listings");
        close(&f, &[(left.serial, 1), (right.serial, 1), (node.serial, 3)]);
    }

    #[test]
    #[ignore = "requires a mounted tree, one explicit Commit and a fresh kernel read"]
    fn mounted_namespace_durability() {
        let f = fixture(Gate::None);
        let before = snapshot(&f);
        let mut mount = writable_mount(&f);
        kernel(
            &f,
            r#"
import os, stat, sys
p = sys.argv[1]
left, right = p + '/left', p + '/right'
os.umask(0o027)
os.mkdir(left, 0o777)
os.mkdir(right, 0o777)
os.mknod(left + '/node.bin', 0o666 | stat.S_IFREG)
fd = os.open(left + '/node.bin', os.O_WRONLY)
assert os.write(fd, b'shared-body') == 11
os.close(fd)
os.link(left + '/node.bin', left + '/alias.bin')
a, b = os.lstat(left + '/node.bin'), os.lstat(left + '/alias.bin')
assert a.st_ino == b.st_ino, (a.st_ino, b.st_ino)
assert a.st_nlink == 2 and b.st_nlink == 2, (a.st_nlink, b.st_nlink)
fd = os.open(left + '/alias.bin', os.O_WRONLY | os.O_APPEND)
assert os.write(fd, b'!') == 1
os.close(fd)
assert open(left + '/node.bin', 'rb').read() == b'shared-body!'
os.mknod(right + '/victim.txt', 0o644 | stat.S_IFREG)
fd = os.open(right + '/victim.txt', os.O_WRONLY)
assert os.write(fd, b'victim-body') == 11
os.close(fd)
os.mknod(left + '/orphan.bin', 0o600 | stat.S_IFREG)
fd = os.open(left + '/orphan.bin', os.O_WRONLY)
assert os.write(fd, b'orphan-body') == 11
os.close(fd)
print('NAMESPACE_MOUNTED_BUILD identities=3 names=node.bin,alias.bin,victim.txt,orphan.bin link_nlink=2')
"#,
        );
        quiescent(&f);
        let top = f.workspace.root().serial;
        let left = lookup(&f, top, b"left");
        let right = lookup(&f, top, b"right");
        let node = lookup(&f, left.serial, b"node.bin");
        let victim = lookup(&f, right.serial, b"victim.txt");
        let orphan = lookup(&f, left.serial, b"orphan.bin");
        assert_eq!((left.mode, right.mode), (0o750, 0o750));
        assert_eq!((node.mode, node.size), (0o640, 12));
        assert_eq!(node.references, 2);
        // FDs the Commit must not disturb: the replaced destination and the
        // last-name-unlinked orphan keep addressing their own inodes.
        let path = f.workspace.mount_path();
        let held_victim = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.join("right/victim.txt"))
            .unwrap();
        let held_orphan = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.join("left/orphan.bin"))
            .unwrap();
        kernel(
            &f,
            r#"
import ctypes, os, stat, sys

LIBC = ctypes.CDLL(None, use_errno=True)
AT_FDCWD, UTIME_OMIT = -100, (1 << 30) - 2

class Timespec(ctypes.Structure):
    _fields_ = [('seconds', ctypes.c_long), ('nanos', ctypes.c_long)]

def set_mtime(path, mtime_ns):
    times = (Timespec * 2)()
    times[0].seconds, times[0].nanos = 0, UTIME_OMIT
    times[1].seconds, times[1].nanos = divmod(mtime_ns, 10 ** 9)
    ctypes.set_errno(0)
    assert LIBC.utimensat(AT_FDCWD, path.encode(), ctypes.byref(times), 0) == 0

p = sys.argv[1]
left, right = p + '/left', p + '/right'
os.rename(left + '/node.bin', right + '/victim.txt')
assert os.lstat(right + '/victim.txt').st_nlink == 2
os.unlink(left + '/orphan.bin')
assert 'orphan.bin' not in os.listdir(left)
os.chmod(left + '/alias.bin', 0o600)
set_mtime(left + '/alias.bin', 1700000123456000000)
set_mtime(left + '/alias.bin', 1700000456789000000)
st = os.lstat(left + '/alias.bin')
assert stat.S_IMODE(st.st_mode) == 0o600
assert st.st_mtime_ns == 1700000456789000000
os.mkdir(left + '/empty', 0o755)
os.rmdir(left + '/empty')
assert 'empty' not in os.listdir(left)
assert sorted(os.listdir(left)) == ['alias.bin']
assert sorted(os.listdir(right)) == ['victim.txt']
print('NAMESPACE_MOUNTED_EDIT rename_replaced=true unlink_last_name=true chmod=0600 utime_omit_mtime=exact rmdir_empty=true')
"#,
        );
        quiescent(&f);
        // The pre-Commit kernel view: identity, link count and metadata.
        let alias_before = identity(&path.join("left/alias.bin"));
        let victim_before = identity(&path.join("right/victim.txt"));
        let expected = (
            node.serial,
            2u64,
            0o600u32,
            (1_700_000_456i64, 789_000_000i64),
            12u64,
        );
        assert_eq!(alias_before, expected);
        assert_eq!(victim_before, expected);
        assert_eq!(
            fs::read(path.join("left/alias.bin")).unwrap(),
            b"shared-body!"
        );
        assert_eq!(names(&path.join("left")), vec!["alias.bin"]);
        assert_eq!(names(&path.join("right")), vec!["victim.txt"]);
        assert_eq!(count(&f, true), 6);
        assert_eq!(count(&f, false), 0);
        assert_eq!(snapshot(&f), before);
        let root = commit(&f);
        // A fresh read of the mounted tree: names, inode identities, contents,
        // link counts and metadata are unchanged by the explicit Commit.
        assert_eq!(names(&path.join("left")), vec!["alias.bin"]);
        assert_eq!(names(&path.join("right")), vec!["victim.txt"]);
        assert_eq!(identity(&path.join("left/alias.bin")), alias_before);
        assert_eq!(identity(&path.join("right/victim.txt")), victim_before);
        assert_eq!(
            fs::read(path.join("left/alias.bin")).unwrap(),
            b"shared-body!"
        );
        assert_eq!(
            fs::read(path.join("right/victim.txt")).unwrap(),
            b"shared-body!"
        );
        assert_eq!(held_victim.metadata().unwrap().ino(), victim.serial);
        assert_eq!(held_orphan.metadata().unwrap().ino(), orphan.serial);
        assert_eq!(bytes(&held_victim, 11), b"victim-body");
        assert_eq!(bytes(&held_orphan, 11), b"orphan-body");
        assert_eq!(lookup(&f, left.serial, b"alias.bin").serial, node.serial);
        assert_eq!(lookup(&f, right.serial, b"victim.txt").serial, node.serial);
        saved_file(
            &f,
            root,
            b"right/victim.txt",
            b"shared-body!",
            node.serial,
            2,
            0o600,
            (1_700_000_456, 789_000_000),
        );
        saved_file(
            &f,
            root,
            b"left/alias.bin",
            b"shared-body!",
            node.serial,
            2,
            0o600,
            (1_700_000_456, 789_000_000),
        );
        saved_directory(&f, root, b"left", left.serial, 0o750);
        saved_directory(&f, root, b"right", right.serial, 0o750);
        missing(&f, root, b"left/node.bin");
        missing(&f, root, b"left/orphan.bin");
        missing(&f, root, b"left/empty");
        missing(&f, before.effective_root, b"left");
        assert_eq!(count(&f, false), 1);
        println!("NAMESPACE_MOUNTED_DURABILITY commit_root=published names_unchanged=true identities_unchanged=true nlink_unchanged=2 metadata_unchanged=true held_fds_readable=true");
        drop(held_victim);
        drop(held_orphan);
        drained(&f);
        mount.unmount(deadline()).unwrap();
        check("mounted-tree-after-explicit-Commit-fresh-read-names-inodes-contents-nlink-metadata");
        close(
            &f,
            &[
                (left.serial, 1),
                (right.serial, 1),
                (node.serial, 3),
                (victim.serial, 1),
                (orphan.serial, 1),
            ],
        );
    }
}
