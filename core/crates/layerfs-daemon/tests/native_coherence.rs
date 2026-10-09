//! Real kernel mounts: shared mappings, removed references, lookups racing
//! directory mutation, size ordering under concurrent append, refusals that
//! stay refused and mutation racing the reversible unmount probe.
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
use layerfs_bridge::control::{Activity, ControlCode, NativePhase};
use layerfs_daemon::control::Failure;
use layerfs_fuse::request::Opcode;
use mounted::{until, COMMAND};
use mutating::{
    bytes_of, direct, errno, forced, frames, harness, opcodes, path_only, quiet, shell, unmounted,
    Mapping,
};
use nix::{errno::Errno, libc};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileExt, MetadataExt},
            process::CommandExt,
        },
    },
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

const MAP_STORE: &str = "LAYERFS_R3_MAP_CLOSE_STORE_EXIT";
const PAGE: usize = 4096;

/// Ends the concurrent side of a case when its driver returns or panics, so
/// no thread's exit depends on another finishing without panicking.
struct Stop<'a>(&'a AtomicBool);
impl Drop for Stop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
fn agree(path: &Path, expected: &[u8], what: &str) {
    assert_eq!(fs::read(path).unwrap(), expected, "{what}: cached read");
    assert_eq!(direct(path), expected, "{what}: daemon read");
    assert_eq!(
        fs::metadata(path).unwrap().len(),
        expected.len() as u64,
        "{what}: size"
    );
}

/// Not a proof by itself. Re-executed by the mapping case as a separate
/// nonroot process that was never registered with anything: it maps the file
/// shared, closes its descriptor, stores, and exits with the page dirty.
#[test]
fn map_close_store_exit_process() {
    let Some(path) = std::env::var_os(MAP_STORE) else {
        return;
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let map = Mapping::shared(&file, 2 * PAGE);
    drop(file);
    map.store(PAGE + 5, b"exit-store");
    unsafe { libc::_exit(0) }
}

#[test]
fn shared_mapping_stores_are_published_and_never_change_size() {
    let (f, h) = harness("-map");
    let ready = h.mount(2);
    let root = Path::new(&ready.directory);
    let path = root.join("mapped");
    let mut content = bytes_of(2 * PAGE + 100, 3);
    fs::write(&path, &content).unwrap();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let map = Mapping::shared(&file, 3 * PAGE);
    assert_eq!(map.load(0, content.len()), content, "mapped read");

    // msync: the store is in the cache at once and in the daemon after it.
    map.store(10, b"msync-store");
    content[10..21].copy_from_slice(b"msync-store");
    assert_eq!(fs::read(&path).unwrap(), content, "cached at once");
    map.sync();
    agree(&path, &content, "msync");

    // A store past the end, inside the last page: never published and never
    // an extension. A later extension reads zero there from cache and daemon.
    map.store(2 * PAGE + 200, b"beyond");
    map.sync();
    agree(&path, &content, "store past the end");
    file.set_len((2 * PAGE + 400) as u64).unwrap();
    content.resize(2 * PAGE + 400, 0);
    agree(&path, &content, "extension after a store past the old end");
    assert_eq!(
        map.load(2 * PAGE + 200, 6),
        [0; 6],
        "mapping after extension"
    );

    // A store while another descriptor appends: each lands at its own offset.
    let mut append = OpenOptions::new().append(true).open(&path).unwrap();
    map.store(PAGE, b"mapped-while-append");
    content[PAGE..PAGE + 19].copy_from_slice(b"mapped-while-append");
    append.write_all(b"APPENDED").unwrap();
    content.extend_from_slice(b"APPENDED");
    map.sync();
    agree(&path, &content, "store beside an append");

    // munmap without msync: the kernel writes the page before it lets go.
    map.store(100, b"unmapped-store");
    content[100..114].copy_from_slice(b"unmapped-store");
    map.unmap();
    drop((file, append));
    agree(&path, &content, "munmap without msync");

    // Map, close, store, exit, in an unregistered nonroot process.
    let exited = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "map_close_store_exit_process", "--nocapture"])
        .env(MAP_STORE, &path)
        .uid(COMMAND)
        .gid(COMMAND)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(exited.status.success(), "{exited:?}");
    content[PAGE + 5..PAGE + 15].copy_from_slice(b"exit-store");
    agree(&path, &content, "map, close, store, exit");

    quiet(&h, ready.token);
    let work = h.status(ready.token).native.unwrap().work.unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    let receipt = unmounted(&h, &ready);
    let counts = opcodes(&receipt);
    let stores = mutating::counter(&receipt, "store_units");
    println!(
        "NATIVE_MAPPED store_units={stores} write={} setattr={} release={} work={work:?}",
        counts[Opcode::Write as usize],
        counts[Opcode::Setattr as usize],
        counts[Opcode::Release as usize]
    );
    // Every mapped store arrived as a WRITE carrying the page-cache flag
    // while a writable handle was live; none produced a SETATTR. The one
    // SETATTR is the explicit extension above.
    assert!(stores >= 4, "{receipt}");
    assert_eq!(counts[Opcode::Setattr as usize], 1, "{receipt}");
    h.stop();
    f.cleanup();
}

#[test]
fn removed_references_keep_exact_state_without_open_custody() {
    let (f, h) = harness("-removed");
    let helper = h.bind(1);
    let ready = h.mount(2);
    let root = Path::new(&ready.directory);
    let missing = |path: &Path| {
        assert_eq!(
            errno(&fs::symlink_metadata(path).unwrap_err()),
            Errno::ENOENT
        )
    };

    // FP-28. Open-unlinked: the descriptor's content, size and mutations stay
    // exact with no name.
    let path = root.join("victim");
    fs::write(&path, b"keep-me-exact").unwrap();
    let before = fs::metadata(&path).unwrap();
    let handle = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let reader = File::open(&path).unwrap();
    let reference = path_only(&path);
    fs::remove_file(&path).unwrap();
    missing(&path);
    let removed = forced(&handle).unwrap();
    assert_eq!(
        (removed.inode, removed.links, removed.size),
        (before.ino(), 0, 13)
    );
    assert_eq!(removed.mode, before.mode());
    let mut bytes = [0; 13];
    reader.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(&bytes, b"keep-me-exact");
    handle.write_all_at(b"KEEP", 0).unwrap();
    handle.write_all_at(b"-tail", 13).unwrap();
    handle.set_len(15).unwrap();
    let expected = b"KEEP-me-exact-t";
    let read = |file: &File| {
        let mut bytes = vec![0; 15];
        file.read_exact_at(&mut bytes, 0).unwrap();
        bytes
    };
    assert_eq!(read(&reader), expected, "cached through the other handle");
    assert_eq!(forced(&reader).unwrap().size, 15);
    // Releasing one handle reclaims nothing the other still reads.
    drop(handle);
    quiet(&h, ready.token);
    assert_eq!(read(&reader), expected);
    assert_eq!(forced(&reader).unwrap().links, 0);
    // FP-29. The last open handle is released: only the O_PATH reference,
    // which has kernel lookup custody and no open-file custody, remains.
    drop(reader);
    quiet(&h, ready.token);
    let mut settled = h.engine(helper);
    until("maintenance after the last release settled", || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let now = h.engine(helper);
        let same = now == settled;
        settled = now;
        same
    });
    let kept = forced(&reference).unwrap();
    println!("NATIVE_REMOVED file={kept:?} engine={settled:?}");
    assert_eq!(
        (kept.inode, kept.links, kept.size, kept.mode),
        (before.ino(), 0, 15, before.mode()),
        "exact metadata through lookup custody alone"
    );

    // A removed directory held as a working directory and by O_PATH.
    let gone = root.join("gone");
    fs::create_dir(&gone).unwrap();
    let was = fs::metadata(&gone).unwrap();
    let directory = path_only(&gone);
    // An unregistered nonroot process that entered the directory before its
    // removal and stats its own working directory afterwards.
    let mut resident = Command::new("sh")
        .args(["-c", "read go; stat -c '%i %h %F' ."])
        .current_dir(&gone)
        .uid(COMMAND)
        .gid(COMMAND)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    fs::remove_dir(&gone).unwrap();
    missing(&gone);
    let held = forced(&directory).unwrap();
    assert_eq!((held.inode, held.mode), (was.ino(), was.mode()));
    assert_eq!(held.links, 0, "removed directory");
    // The name is free at once and binds a different inode.
    fs::create_dir(&gone).unwrap();
    assert_ne!(fs::metadata(&gone).unwrap().ino(), was.ino());
    assert_eq!(forced(&directory).unwrap(), held, "still the removed one");
    assert!(resident.try_wait().unwrap().is_none());
    resident.stdin.take().unwrap().write_all(b"go\n").unwrap();
    let resident = resident.wait_with_output().unwrap();
    assert!(resident.status.success(), "{resident:?}");
    assert_eq!(
        String::from_utf8(resident.stdout).unwrap(),
        format!("{} 0 directory\n", was.ino()),
        "the removed working directory, from inside it"
    );

    // Replacement: the replaced inode through O_PATH, and a hard-link alias
    // whose count follows each removal at once.
    let target = root.join("target");
    fs::write(&target, b"replaced-content").unwrap();
    let replaced = path_only(&target);
    let identity = forced(&replaced).unwrap();
    fs::write(root.join("incoming"), b"new").unwrap();
    fs::rename(root.join("incoming"), &target).unwrap();
    let after = forced(&replaced).unwrap();
    assert_eq!(
        (after.inode, after.links, after.size),
        (identity.inode, 0, 16)
    );
    assert_eq!(fs::read(&target).unwrap(), b"new");
    fs::hard_link(&target, root.join("alias")).unwrap();
    let alias = path_only(&target);
    assert_eq!(forced(&alias).unwrap().links, 2);
    fs::remove_file(&target).unwrap();
    assert_eq!(forced(&alias).unwrap().links, 1);
    assert_eq!(fs::metadata(root.join("alias")).unwrap().nlink(), 1);
    fs::remove_file(root.join("alias")).unwrap();
    assert_eq!(forced(&alias).unwrap().links, 0);

    // Rotation: a writer keeps appending to the inode it opened, through a
    // rename away and then through that name's replacement.
    let log = root.join("app.log");
    let rotated = root.join("app.log.1");
    let mut writer = OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(&log)
        .unwrap();
    writer.write_all(b"old-1\n").unwrap();
    fs::rename(&log, &rotated).unwrap();
    let mut fresh = OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(&log)
        .unwrap();
    fresh.write_all(b"new-1\n").unwrap();
    writer.write_all(b"old-2\n").unwrap();
    assert_eq!(fs::read(&rotated).unwrap(), b"old-1\nold-2\n");
    assert_eq!(fs::read(&log).unwrap(), b"new-1\n");
    let mut old = File::open(&rotated).unwrap();
    fs::write(root.join("compressed"), b"replacement").unwrap();
    fs::rename(root.join("compressed"), &rotated).unwrap();
    writer.write_all(b"old-3\n").unwrap();
    assert_eq!(fs::read(&rotated).unwrap(), b"replacement");
    let mut all = Vec::new();
    old.read_to_end(&mut all).unwrap();
    assert_eq!(all, b"old-1\nold-2\nold-3\n", "nameless inode, exact");
    assert_eq!(forced(&old).unwrap().links, 0);

    // Observation, not a proof row: reopening a removed file through its
    // descriptor's /proc name asks the daemon for a new OPEN of a nameless
    // inode. Either outcome is printed; content is checked when it opens.
    match File::open(format!("/proc/self/fd/{}", old.as_raw_fd())) {
        Ok(mut again) => {
            let mut bytes = Vec::new();
            again.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, all);
            println!("NATIVE_REOPEN_REMOVED opened");
        }
        Err(error) => println!("NATIVE_REOPEN_REMOVED errno={:?}", errno(&error)),
    }
    // fchmod names no handle to the daemon; the removed inode has no name.
    let mode = Errno::result(unsafe { libc::fchmod(old.as_raw_fd(), 0o600) });
    println!("NATIVE_FCHMOD_REMOVED result={mode:?}");

    drop((reference, directory, replaced, alias, writer, fresh, old));
    quiet(&h, ready.token);
    let work = h.status(ready.token).native.unwrap().work.unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    unmounted(&h, &ready);
    h.stop();
    f.cleanup();
}

#[test]
fn a_lookup_racing_create_unlink_and_rename_never_binds_a_stale_inode() {
    const ROUNDS: u64 = 240;
    let (f, h) = harness("-race");
    let ready = h.mount(2);
    let root = Path::new(&ready.directory);
    let name = root.join("contended");
    let staged = root.join("staged");
    fs::write(&name, b"0").unwrap();
    let done = AtomicBool::new(false);
    // The newest generation whose installing syscall has returned.
    let installed = AtomicU64::new(0);
    // Unlink-then-create windows: the only time the name may be absent.
    let gaps = AtomicU64::new(0);
    let absent = AtomicBool::new(false);
    let (found, missed, empty) = (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0));
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                while !done.load(Ordering::SeqCst) {
                    let gap = gaps.load(Ordering::SeqCst);
                    let was_absent = absent.load(Ordering::SeqCst);
                    let floor = installed.load(Ordering::SeqCst);
                    match fs::read(&name) {
                        Ok(bytes) if bytes.is_empty() => {
                            empty.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(bytes) => {
                            let seen: u64 = String::from_utf8(bytes).unwrap().parse().unwrap();
                            assert!(seen >= floor, "stale binding: {seen} below {floor}");
                            found.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(error) => {
                            assert_eq!(errno(&error), Errno::ENOENT);
                            assert!(
                                was_absent || gaps.load(Ordering::SeqCst) != gap,
                                "absent outside every unlink window"
                            );
                            missed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });
        }
        let _stop = Stop(&done);
        for round in 1..=ROUNDS {
            if round % 3 == 0 {
                // UNLINK then CREATE of the same name.
                absent.store(true, Ordering::SeqCst);
                gaps.fetch_add(1, Ordering::SeqCst);
                fs::remove_file(&name).unwrap();
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&name)
                    .unwrap();
                absent.store(false, Ordering::SeqCst);
                file.write_all(round.to_string().as_bytes()).unwrap();
            } else {
                // Replacement RENAME of a complete file onto the name.
                fs::write(&staged, round.to_string()).unwrap();
                fs::rename(&staged, &name).unwrap();
            }
            installed.store(round, Ordering::SeqCst);
            // The mutator itself, right after its own syscall returned.
            assert_eq!(fs::read(&name).unwrap(), round.to_string().as_bytes());
        }
    });
    let observed = (
        found.load(Ordering::Relaxed),
        missed.load(Ordering::Relaxed),
        empty.load(Ordering::Relaxed),
    );
    quiet(&h, ready.token);
    let work = h.status(ready.token).native.unwrap().work.unwrap();
    println!("NATIVE_LOOKUP_RACE rounds={ROUNDS} found_missed_empty={observed:?} work={work:?}");
    assert!(observed.0 > 0, "the readers observed nothing");
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    unmounted(&h, &ready);
    h.stop();
    f.cleanup();
}

#[test]
fn size_never_shrinks_under_concurrent_append_and_refusals_stay_refused() {
    const RECORDS: usize = 200;
    const RECORD: usize = 1000;
    let (f, h) = harness("-order");
    let ready = h.mount(2);
    let root = Path::new(&ready.directory);
    let path = root.join("growing");
    let all = bytes_of(RECORDS * RECORD, 9);
    fs::write(&path, b"").unwrap();
    let done = AtomicBool::new(false);
    let samples = AtomicU64::new(0);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            // FP-16, the executable half. Cached size, a size the kernel
            // fetched for this call and the length actually read form one
            // sequence that never decreases, and every byte read is exact.
            let reader = File::open(&path).unwrap();
            let mut least = 0;
            let mut step = |size: u64, what: &str| {
                assert!(size >= least, "{what}: {size} after {least}");
                least = size;
            };
            while !done.load(Ordering::SeqCst) {
                step(fs::metadata(&path).unwrap().len(), "cached attributes");
                step(forced(&reader).unwrap().size, "fetched attributes");
                let bytes = fs::read(&path).unwrap();
                assert_eq!(bytes, all[..bytes.len()], "concurrent read");
                step(bytes.len() as u64, "read length");
                samples.fetch_add(1, Ordering::Relaxed);
            }
        });
        let _stop = Stop(&done);
        let mut append = OpenOptions::new().append(true).open(&path).unwrap();
        for record in all.chunks(RECORD) {
            append.write_all(record).unwrap();
        }
    });
    assert_eq!(fs::read(&path).unwrap(), all);
    assert_eq!(direct(&path), all);
    assert!(samples.load(Ordering::Relaxed) > 0);

    // FP-10: a read of bytes the kernel already holds asks the daemon nothing.
    let cached = File::open(&path).unwrap();
    let mut bytes = vec![0; all.len()];
    cached.read_exact_at(&mut bytes, 0).unwrap();
    let before = frames(&h, ready.token);
    cached.read_exact_at(&mut bytes, 0).unwrap();
    assert_eq!(bytes, all);
    assert_eq!(
        frames(&h, ready.token),
        before,
        "a cached read sent a request"
    );
    drop(cached);

    // FP-18. The refusals, twice: nothing changed the first time, and the
    // second extended-attribute and preallocation calls never reach the daemon.
    mutating::refusals(root);
    mutating::refusals(root);
    // Many small writes, by root and by an unregistered nonroot process.
    let mut small = OpenOptions::new().append(true).open(&path).unwrap();
    for _ in 0..200 {
        small.write_all(b"x").unwrap();
    }
    drop(small);
    let script = format!(
        "i=0; while [ $i -lt 200 ]; do printf y >> {}/growing || exit 1; i=$((i+1)); done",
        root.display()
    );
    let ran = shell(COMMAND, &script);
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(fs::metadata(&path).unwrap().len(), (all.len() + 400) as u64);

    quiet(&h, ready.token);
    let work = h.status(ready.token).native.unwrap().work.unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    let receipt = unmounted(&h, &ready);
    let counts = opcodes(&receipt);
    let count = |opcode: Opcode| counts[opcode as usize];
    println!(
        "NATIVE_REFUSED write={} getxattr={} setxattr={} listxattr={} removexattr={} fallocate={} fsync={} samples={}",
        count(Opcode::Write),
        count(Opcode::Getxattr),
        count(Opcode::Setxattr),
        count(Opcode::Listxattr),
        count(Opcode::Removexattr),
        count(Opcode::Fallocate),
        count(Opcode::Fsync),
        samples.load(Ordering::Relaxed)
    );
    assert!(count(Opcode::Write) >= (RECORDS + 400) as u64);
    // One request each, ever, on this connection: the kernel stopped asking.
    assert_eq!(count(Opcode::Getxattr), 1, "{receipt}");
    assert_eq!(count(Opcode::Setxattr), 1, "{receipt}");
    assert_eq!(count(Opcode::Fallocate), 1, "{receipt}");
    assert_eq!(count(Opcode::Listxattr) + count(Opcode::Removexattr), 0);
    // Every close of the 200 appends succeeded, and only the first FLUSH of
    // the connection reached the daemon; likewise at most one FSYNC.
    assert_eq!(count(Opcode::Flush), 1, "{receipt}");
    assert!(count(Opcode::Fsync) <= 1, "{receipt}");
    h.stop();
    f.cleanup();
}

/// Appends, stats, reads, creates and unlinks until told to stop. The first
/// error of any kind ends it and is returned exactly.
fn mutate_until(done: &AtomicBool, appended: &AtomicU64, root: &Path) -> Result<(), String> {
    let path = root.join("journal");
    let scratch = root.join("scratch");
    let failed = |what: &str| {
        let what = what.to_owned();
        move |error: std::io::Error| format!("{what}: {error:?}")
    };
    while !done.load(Ordering::SeqCst) {
        let round = appended.load(Ordering::SeqCst);
        OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(format!("{round:08}\n").as_bytes()))
            .map_err(failed("append"))?;
        appended.store(round + 1, Ordering::SeqCst);
        fs::metadata(&path).map_err(failed("stat"))?;
        fs::read(root.join(".git/index")).map_err(failed("read"))?;
        fs::write(&scratch, b"s")
            .and_then(|()| fs::remove_file(&scratch))
            .map_err(failed("create and unlink"))?;
    }
    Ok(())
}

#[test]
fn mutations_racing_the_unmount_probe_keep_normal_results_and_persist() {
    const PROBES: usize = 6;
    let (f, h) = harness("-probe");
    let ready = h.mount(2);
    let token = ready.token;
    let root = Path::new(&ready.directory);
    let path = root.join("journal");
    fs::write(&path, b"").unwrap();
    fs::create_dir(root.join("held")).unwrap();
    // A kernel reference with no registration anywhere: a nonroot process
    // whose working directory is inside the Workspace.
    let mut resident = Command::new("sleep")
        .arg("60")
        .current_dir(root.join("held"))
        .uid(COMMAND)
        .gid(COMMAND)
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let done = AtomicBool::new(false);
    let appended = AtomicU64::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        scope.spawn(|| {
            if let Err(failure) = mutate_until(&done, &appended, root) {
                failures.lock().unwrap().push(failure);
            }
        });
        let _stop = Stop(&done);
        until("the mutator is running", || {
            appended.load(Ordering::SeqCst) > 0
        });
        for probe in 0..PROBES {
            let before = appended.load(Ordering::SeqCst);
            match h.try_unmount(token) {
                Err(Failure::Native(failure)) => assert_eq!(
                    (failure.code, failure.phase),
                    (ControlCode::Busy, "unmount:kernel"),
                    "probe {probe}"
                ),
                other => panic!("probe {probe}: {other:?}"),
            }
            let native = h.status(token).native.unwrap();
            assert_eq!(native.phase, NativePhase::Ready);
            assert!(!native.detached);
            // Requests keep being served after the refused probe.
            until("mutation continued after the probe", || {
                appended.load(Ordering::SeqCst) > before || !failures.lock().unwrap().is_empty()
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    assert!(failures.is_empty(), "probe-induced failures: {failures:?}");
    // Every acknowledged mutation persisted, in order.
    let records = appended.load(Ordering::SeqCst);
    let mut expected = String::new();
    for round in 0..records {
        expected.push_str(&format!("{round:08}\n"));
    }
    assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
    assert_eq!(direct(&path), expected.as_bytes());
    quiet(&h, token);
    let status = h.status(token);
    assert_eq!(
        status.activity,
        Activity::Idle,
        "control admission reopened"
    );
    let work = status.native.unwrap().work.unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    println!("NATIVE_PROBE_RACE probes={PROBES} records={records} work={work:?}");
    assert!(resident.try_wait().unwrap().is_none());
    resident.kill().unwrap();
    resident.wait().unwrap();
    unmounted(&h, &ready);
    h.stop();
    f.cleanup();
}
