//! Real kernel mounts: ordinary mutation through plain filesystem syscalls,
//! with cached reads, uncached reads and attributes agreeing at once.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use mounted::COMMAND;
use mutating::{direct, errno, harness, quiet, shell, unmounted};
use nix::errno::Errno;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{symlink, FileExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    time::{Duration, SystemTime},
};

/// Cached read, a fresh open, an uncached read served by the daemon and the
/// reported size must all describe the same bytes.
fn agree(path: &Path, expected: &[u8], what: &str) {
    assert_eq!(fs::read(path).unwrap(), expected, "{what}: cached read");
    assert_eq!(direct(path), expected, "{what}: daemon read");
    let stat = fs::metadata(path).unwrap();
    assert_eq!(stat.len(), expected.len() as u64, "{what}: size");
    assert_eq!(
        (stat.ctime(), stat.ctime_nsec()),
        (stat.mtime(), stat.mtime_nsec()),
        "{what}: ctime is reported equal to mtime"
    );
}

#[test]
fn write_append_and_truncate_are_exact_in_cache_and_daemon() {
    let (f, h) = harness("-write");
    let ready = h.mount(2);
    let root = Path::new(&ready.directory);

    // FP-10. A created file written through one descriptor, read back cached
    // through the same descriptor, a fresh open and the daemon.
    let path = root.join("created");
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o640)
        .open(&path)
        .unwrap();
    assert_eq!(file.metadata().unwrap().len(), 0);
    assert_eq!(file.metadata().unwrap().mode() & 0o7777, 0o640);
    file.write_all(b"hello native").unwrap();
    let mut cached = [0; 12];
    file.read_exact_at(&mut cached, 0).unwrap();
    assert_eq!(&cached, b"hello native");
    agree(&path, b"hello native", "create and write");
    // An overwrite inside the file and a write past its end with a hole.
    file.write_all_at(b"HELLO", 0).unwrap();
    file.write_all_at(b"tail", 20).unwrap();
    let mut expected = b"HELLO native".to_vec();
    expected.resize(20, 0);
    expected.extend_from_slice(b"tail");
    agree(&path, &expected, "overwrite and sparse extension");

    // O_APPEND through two descriptors: every write lands at the size the
    // kernel resolved, so neither overlaps the other.
    let mut first = OpenOptions::new().append(true).open(&path).unwrap();
    let mut second = OpenOptions::new().append(true).open(&path).unwrap();
    for round in 0..8_u8 {
        first.write_all(&[b'a', b'0' + round]).unwrap();
        second.write_all(&[b'b', b'0' + round]).unwrap();
        expected.extend_from_slice(&[b'a', b'0' + round, b'b', b'0' + round]);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected.len() as u64);
    }
    agree(&path, &expected, "interleaved O_APPEND");
    drop((first, second));

    // A window larger than one request: the kernel splits at 128 KiB and each
    // part is one published job replied in full.
    let big = bytes_of(300_000, 7);
    let large = root.join("large");
    fs::write(&large, &big).unwrap();
    agree(&large, &big, "multi-window write");

    // A file inherited from the committed root: bytes this write does not
    // cover stay inherited, and the file's other name sees the same inode.
    let inherited = root.join(".git/index");
    let alias = root.join(".cache/index-alias");
    let base = fixture::FILES[1].1;
    assert_eq!(fs::read(&alias).unwrap(), base, "alias cached before");
    let writer = OpenOptions::new().write(true).open(&inherited).unwrap();
    writer.write_all_at(b"GIT", 0).unwrap();
    let mut changed = base.to_vec();
    changed[..3].copy_from_slice(b"GIT");
    agree(&inherited, &changed, "inherited overwrite");
    // FP-14: read through the other name, from the shared page cache.
    agree(&alias, &changed, "hard-link alias");
    assert_eq!(
        fs::metadata(&inherited).unwrap().ino(),
        fs::metadata(&alias).unwrap().ino()
    );
    drop(writer);

    // FP-13. Shrink, then regrow: the discarded tail is zero in cached pages
    // and in the daemon, never the old bytes.
    file.set_len(5).unwrap();
    agree(&path, b"HELLO", "shrink");
    file.set_len(4096 + 9).unwrap();
    let mut regrown = b"HELLO".to_vec();
    regrown.resize(4096 + 9, 0);
    agree(&path, &regrown, "regrow");
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(b"end").unwrap();
    regrown.extend_from_slice(b"end");
    agree(&path, &regrown, "write after regrow");
    // Truncate by path, with no descriptor.
    OpenOptions::new()
        .write(true)
        .open(&large)
        .unwrap()
        .set_len(100_000)
        .unwrap();
    agree(&large, &big[..100_000], "ftruncate of a large file");
    nix::unistd::truncate(&large, 70_000).unwrap();
    agree(&large, &big[..70_000], "truncate by path");
    // O_TRUNC of a cached inherited file: OPEN then a size-0 SETATTR.
    let ignored = root.join("ignored.bin");
    assert_eq!(fs::read(&ignored).unwrap(), fixture::FILES[6].1);
    let mut truncated = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&ignored)
        .unwrap();
    agree(&ignored, b"", "O_TRUNC");
    truncated.write_all(b"new").unwrap();
    agree(&ignored, b"new", "write after O_TRUNC");
    drop((truncated, file));

    quiet(&h, ready.token);
    let work = h.status(ready.token).native.unwrap().work.unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    println!("NATIVE_WRITE work={work:?}");
    unmounted(&h, &ready);
    h.stop();
    f.cleanup();
}

#[test]
fn namespace_mutations_are_visible_at_once_and_refusals_stay_refused() {
    let (f, h) = harness("-names");
    let ready = h.mount(2);
    let root = Path::new(&ready.directory);
    let missing = |path: &Path, what: &str| {
        assert_eq!(
            errno(&fs::symlink_metadata(path).unwrap_err()),
            Errno::ENOENT,
            "{what}"
        );
    };

    // FP-11. A name looked up as absent is not cached as absent.
    let name = root.join("fresh");
    missing(&name, "before create");
    fs::write(&name, b"one").unwrap();
    assert_eq!(fs::read(&name).unwrap(), b"one");
    fs::remove_file(&name).unwrap();
    missing(&name, "after unlink");
    fs::write(&name, b"two").unwrap();
    assert_eq!(fs::read(&name).unwrap(), b"two", "recreated at once");
    assert_eq!(
        errno(
            &File::options()
                .write(true)
                .create_new(true)
                .open(&name)
                .unwrap_err()
        ),
        Errno::EEXIST
    );

    // Directories, with exact emptiness.
    let dir = root.join("made");
    fs::create_dir(&dir).unwrap();
    assert!(fs::metadata(&dir).unwrap().is_dir());
    assert_eq!(errno(&fs::create_dir(&dir).unwrap_err()), Errno::EEXIST);
    fs::write(dir.join("child"), b"c").unwrap();
    assert_eq!(errno(&fs::remove_dir(&dir).unwrap_err()), Errno::ENOTEMPTY);
    assert_eq!(errno(&fs::remove_file(&dir).unwrap_err()), Errno::EISDIR);
    assert_eq!(
        errno(&fs::remove_dir(dir.join("child")).unwrap_err()),
        Errno::ENOTDIR
    );
    let listed: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(listed, ["child"]);

    // Replacement rename: the destination binds the moved inode at once and
    // the source name is gone. A descriptor of the replaced file keeps its
    // content (FP-28, rotation).
    let log = root.join("log");
    fs::write(&log, b"generation-1").unwrap();
    let old = File::open(&log).unwrap();
    let old_inode = old.metadata().unwrap().ino();
    let next = root.join("log.next");
    fs::write(&next, b"generation-2").unwrap();
    let next_inode = fs::metadata(&next).unwrap().ino();
    fs::rename(&next, &log).unwrap();
    missing(&next, "renamed source");
    assert_eq!(fs::read(&log).unwrap(), b"generation-2");
    assert_eq!(fs::metadata(&log).unwrap().ino(), next_inode);
    let mut retained = Vec::new();
    (&old).read_to_end(&mut retained).unwrap();
    assert_eq!(
        retained, b"generation-1",
        "replaced file through its descriptor"
    );
    let replaced = old.metadata().unwrap();
    assert_eq!((replaced.ino(), replaced.nlink()), (old_inode, 0));
    drop(old);
    // No-replace rename refuses an existing destination with no effect.
    fs::write(&next, b"generation-3").unwrap();
    assert_eq!(
        Errno::result(unsafe {
            nix::libc::renameat2(
                nix::libc::AT_FDCWD,
                mutating::c(&next).as_ptr(),
                nix::libc::AT_FDCWD,
                mutating::c(&log).as_ptr(),
                nix::libc::RENAME_NOREPLACE,
            )
        })
        .unwrap_err(),
        Errno::EEXIST
    );
    assert_eq!(fs::read(&log).unwrap(), b"generation-2");
    assert_eq!(fs::read(&next).unwrap(), b"generation-3");

    // A directory moved to another parent, then moved again; a move beneath
    // itself is refused.
    fs::create_dir_all(root.join("made/deep")).unwrap();
    fs::rename(&dir, root.join("output/moved")).unwrap();
    missing(&dir, "moved directory source");
    assert_eq!(fs::read(root.join("output/moved/child")).unwrap(), b"c");
    assert_eq!(
        errno(&fs::rename(root.join("output"), root.join("output/moved/deep/inside")).unwrap_err()),
        Errno::EINVAL
    );
    fs::rename(root.join("output/moved/deep"), root.join("deep")).unwrap();
    fs::rename(root.join("output/moved"), root.join("deep/moved")).unwrap();
    assert_eq!(fs::read(root.join("deep/moved/child")).unwrap(), b"c");
    // Replacing a directory requires an empty one.
    fs::create_dir(root.join("target")).unwrap();
    fs::rename(root.join("deep/moved"), root.join("target")).unwrap();
    assert_eq!(fs::read(root.join("target/child")).unwrap(), b"c");
    fs::create_dir(root.join("another")).unwrap();
    assert_eq!(
        errno(&fs::rename(root.join("another"), root.join("target")).unwrap_err()),
        Errno::ENOTEMPTY
    );
    fs::remove_file(root.join("target/child")).unwrap();
    fs::remove_dir(root.join("target")).unwrap();
    missing(&root.join("target"), "rmdir");
    // An inherited directory and file removed from the committed view.
    fs::remove_file(root.join("node_modules/pkg/index.js")).unwrap();
    fs::remove_dir(root.join("node_modules/pkg")).unwrap();
    missing(&root.join("node_modules/pkg"), "inherited rmdir");
    assert_eq!(fs::read_dir(root.join("node_modules")).unwrap().count(), 0);

    // Hard links: one inode, exact link count, ctime equal to mtime.
    let origin = root.join("origin");
    fs::write(&origin, b"shared").unwrap();
    let before = fs::metadata(&origin).unwrap();
    fs::hard_link(&origin, root.join("link")).unwrap();
    let linked = fs::metadata(root.join("link")).unwrap();
    let after = fs::metadata(&origin).unwrap();
    assert_eq!((linked.ino(), linked.nlink()), (before.ino(), 2));
    assert_eq!(
        after.nlink(),
        2,
        "link count through the other name at once"
    );
    assert_eq!(
        (after.mtime(), after.mtime_nsec()),
        (before.mtime(), before.mtime_nsec())
    );
    assert_eq!(
        (after.ctime(), after.ctime_nsec()),
        (after.mtime(), after.mtime_nsec())
    );
    fs::write(root.join("link"), b"through-link").unwrap();
    assert_eq!(fs::read(&origin).unwrap(), b"through-link", "FP-14 alias");
    fs::remove_file(&origin).unwrap();
    assert_eq!(fs::metadata(root.join("link")).unwrap().nlink(), 1);
    // Renaming one name of an inode onto its other name changes nothing.
    fs::hard_link(root.join("link"), root.join("link2")).unwrap();
    fs::rename(root.join("link"), root.join("link2")).unwrap();
    assert_eq!(fs::metadata(root.join("link")).unwrap().nlink(), 2);

    // Symlinks round-trip raw bytes up to one page minus one.
    symlink("log", root.join("to-log")).unwrap();
    assert_eq!(
        fs::read_link(root.join("to-log")).unwrap(),
        Path::new("log")
    );
    assert_eq!(fs::read(root.join("to-log")).unwrap(), b"generation-2");
    let longest = "t".repeat(4095);
    symlink(&longest, root.join("longest")).unwrap();
    assert_eq!(
        fs::read_link(root.join("longest"))
            .unwrap()
            .as_os_str()
            .len(),
        4095
    );
    assert_eq!(
        fs::symlink_metadata(root.join("longest")).unwrap().len(),
        4095
    );
    assert_eq!(
        errno(&symlink("t".repeat(4096), root.join("too-long")).unwrap_err()),
        Errno::ENAMETOOLONG
    );
    missing(&root.join("too-long"), "refused symlink");

    // FP-17. chmod takes effect at once, also for a file whose pages are
    // cached; times are exact.
    let private = root.join("private");
    fs::write(&private, b"secret").unwrap();
    fs::set_permissions(&private, fs::Permissions::from_mode(0o644)).unwrap();
    let reader = |path: &Path| shell(COMMAND, &format!("cat {}", path.display()));
    assert_eq!(reader(&private).stdout, b"secret", "cached for the command");
    fs::set_permissions(&private, fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(fs::metadata(&private).unwrap().mode() & 0o7777, 0);
    assert!(!reader(&private).status.success(), "mode 000 from cache");
    fs::set_permissions(&private, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(reader(&private).stdout, b"secret", "readable again at once");
    let stamp = SystemTime::UNIX_EPOCH + Duration::new(1_600_000_000, 123_456_789);
    File::options()
        .write(true)
        .open(&private)
        .unwrap()
        .set_modified(stamp)
        .unwrap();
    let stat = fs::metadata(&private).unwrap();
    assert_eq!(
        (stat.mtime(), stat.mtime_nsec()),
        (1_600_000_000, 123_456_789)
    );
    assert_eq!(
        (stat.ctime(), stat.ctime_nsec()),
        (stat.mtime(), stat.mtime_nsec())
    );
    fs::set_permissions(&private, fs::Permissions::from_mode(0o640)).unwrap();
    let stat = fs::metadata(&private).unwrap();
    assert_eq!(
        (
            stat.mtime(),
            stat.mtime_nsec(),
            stat.ctime(),
            stat.ctime_nsec()
        ),
        (1_600_000_000, 123_456_789, 1_600_000_000, 123_456_789),
        "chmod leaves the one stored time"
    );
    fs::rename(&private, root.join("private2")).unwrap();
    let stat = fs::metadata(root.join("private2")).unwrap();
    assert_eq!(
        (stat.ctime(), stat.ctime_nsec()),
        (1_600_000_000, 123_456_789)
    );
    fs::create_dir(root.join("sticky")).unwrap();
    fs::set_permissions(root.join("sticky"), fs::Permissions::from_mode(0o1777)).unwrap();
    assert_eq!(
        fs::metadata(root.join("sticky")).unwrap().mode() & 0o7777,
        0o1777
    );

    mutating::refusals(root);

    // Every mutation above by an ordinary nonroot process that was never
    // registered with anything: the same semantics through the same mount.
    let script = format!(
        "cd {root} && echo data > cmd && echo more >> cmd && mv cmd cmd2 && ln cmd2 cmd3 && \
         mkdir cmddir && mv cmd3 cmddir/in && ln -s cmddir/in cmdlink && cat cmdlink && \
         rm cmd2 && chmod 600 cmddir/in && truncate -s 2 cmddir/in && cat cmddir/in",
        root = root.display()
    );
    let ran = shell(COMMAND, &script);
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(ran.stdout, b"data\nmore\nda");
    let made = fs::metadata(root.join("cmddir/in")).unwrap();
    assert_eq!((made.uid(), made.gid()), (COMMAND, COMMAND));
    assert_eq!(
        (made.len(), made.nlink(), made.mode() & 0o7777),
        (2, 1, 0o600)
    );
    assert_eq!(direct(&root.join("cmddir/in")), b"da");

    quiet(&h, ready.token);
    let work = h.status(ready.token).native.unwrap().work.unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    println!("NATIVE_NAMES work={work:?}");
    unmounted(&h, &ready);
    h.stop();
    f.cleanup();
}
fn bytes_of(length: usize, seed: u8) -> Vec<u8> {
    (0..length)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}
