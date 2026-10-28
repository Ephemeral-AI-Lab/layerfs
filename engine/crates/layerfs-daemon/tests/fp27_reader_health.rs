//! FP-27, two halves a real mount can present that no test staged before
//! (R8b track T-I). Specification section 12: "Cold demand fails | `EIO` for
//! that request | the original `PortError` in that request's failure scope
//! and in a bounded daemon diagnostic" and "A reader session quarantined |
//! `EIO` for the demand that observed it | reader excluded and reported".
//!
//! 1. **The failed-demand marker at the READ step.** `mounted_failure_scope.rs`
//!    reads a cold file whole, so the request that observes the quarantined
//!    reader is whichever of LOOKUP, OPEN and READ makes the first demand.
//!    Here the file is looked up and opened with `O_DIRECT` while both
//!    readers are healthy; then the test takes the other read session through
//!    the public Store API and puts the remaining one into an uncertain state
//!    the way `store_read_service.rs` does it, through the provider's public
//!    port with the provider's own answer. The next request of the mount is
//!    one synchronous READ, and its own base demand is the one that observes
//!    the reader.
//! 2. **`Capacity` is not a failed base demand.** The test takes every
//!    read-admission ticket of the mounted Workspace's own lane
//!    (`ReadLimits::requests_per_namespace`) through `Store::read_ticket`.
//!    The next mounted READ's admission is refused `Capacity`. By source
//!    (`service/filesystem_port.rs`, `scoped`) a table bound is not a base
//!    read of one request: the caller is answered `EIO`
//!    (`layerfs-fuse` `request/terminal.rs`), the request is retained with
//!    its original cause, no failed demand is recorded, the retention makes
//!    the lane terminal so later requests of the mount are answered
//!    `ENOTCONN` at admission (`layerfs-fuse` `dispatch/task.rs`,
//!    `request/state.rs`), and the Workspace's later Unmount stops
//!    `Retained` at `Requests` (`layerfs-fuse` `session/drain.rs`).
//!
//! The Store is composed from the public pieces `open_store` composes, with
//! no object cache, so that the test keeps each read session's provider and
//! every base demand reaches it. Every call is forwarded to the real
//! provider; nothing is injected. Global Store profile: Disposable, selected
//! explicitly by the fixture and asserted.
//!
//! NOT in scope here (plan section 6): a poisoned reader, a release after a
//! failed demand, a concurrent demand and a foreign reader.
//!
//! Limit, by source: a retained request's original failure is kept in its
//! dispatcher slot, and the daemon exposes no observation of it
//! (`MountRequests::inspect_retained` has no caller in daemon source). The
//! second test therefore shows the cause by what the public counters can
//! and cannot be: the lane was at its bound by the Store's own answer, the
//! READ was granted no reader, and no failed demand was recorded.
#![cfg(target_os = "linux")]
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
#[path = "support/mounted_commit.rs"]
mod rig;
use layerfs_bridge::control::{MountWork, WorkspaceToken};
use layerfs_content::ObjectId;
use layerfs_daemon::store::{PortError, Store, StoreReader};
use layerfs_persistence::{Handles, SqlitePersistenceProfile, StorageProvider};
use layerfs_storage::{
    location::PackInfo,
    port::{PackPersistence, PackReadChoice, PackReadPlan, PersistenceError},
    ReservationBlocks, Storage,
};
use mounted::{mount_entry, Harness};
use nix::{errno::Errno, libc};
use rig::{pattern, root, stamp_tree};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command as Process,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the tests.
const WAIT: Duration = Duration::from_secs(8);
/// The largest window of one READ: specification section 8.1.
const WINDOW: usize = 128 * 1024;

/// Files none of which a bind reads: every first read is a cold demand.
const COLD: [(&str, u8, usize); 4] = [
    ("cold-0.bin", 20, 48_000),
    ("cold-1.bin", 21, 49_000),
    ("cold-2.bin", 22, 50_000),
    ("cold-3.bin", 23, 51_000),
];
fn expected(index: usize) -> Vec<u8> {
    pattern(COLD[index].1, COLD[index].2)
}

/// Never leaves a kernel mount behind: if the test unwinds, or the product
/// did not unmount, one lazy umount(8) launched and reaped by the test
/// detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "FP-27 GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// A bounded observation loop that reports instead of unwinding.
fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn work(harness: &Harness, token: WorkspaceToken) -> MountWork {
    harness.status(token).kernel.unwrap().work.unwrap()
}
/// Two consecutive observations of a connection with nothing received, no
/// request admitted beyond those retained, and none completed in between.
fn quiet(harness: &Harness, token: WorkspaceToken) -> bool {
    let mut last = None;
    eventually(|| {
        let now = work(harness, token);
        let seen = (now.active, now.completed);
        let settled = now.active == 0 && last == Some(seen);
        last = Some(seen);
        thread::sleep(Duration::from_millis(5));
        settled
    })
}

/// One installed Disposable Store, opened once and kept open, with no object
/// cache, and one real kernel serving assembly over it. The test keeps the
/// provider of each read session.
struct Fx {
    fixture: installed::Fixture,
    store: Arc<Store>,
    harness: Harness,
    readers: Arc<Mutex<Vec<Arc<StorageProvider>>>>,
}
impl Fx {
    fn new(label: &str) -> Self {
        let fixture = installed::Fixture::built(label, |source| {
            fs::write(source.join("README.md"), b"# base\n").unwrap();
            for (name, seed, length) in COLD {
                fs::write(source.join(name), pattern(seed, length)).unwrap();
            }
            for entry in fs::read_dir(source).unwrap() {
                fs::set_permissions(entry.unwrap().path(), fs::Permissions::from_mode(0o644))
                    .unwrap();
            }
            stamp_tree(source);
            1 + COLD.len()
        });
        assert!(
            matches!(
                fixture.config.sqlite_profile,
                SqlitePersistenceProfile::Disposable
            ),
            "the global Store profile is Disposable, selected explicitly"
        );
        let config = || fixture.config.clone();
        let writer =
            Handles::open_writable(config(), installed::BINDING, installed::CURSOR).unwrap();
        let readers = Arc::new(Mutex::new(Vec::new()));
        let reader_config = fixture.config.clone();
        let observed_readers = readers.clone();
        let factory: layerfs_daemon::store::ReaderFactory = Arc::new(move |_| {
            let read = Handles::open_read_only(
                reader_config.clone(),
                installed::BINDING,
                installed::CURSOR,
            )
            .unwrap();
            observed_readers.lock().unwrap().push(read.storage.clone());
            Ok(StoreReader::new(
                Storage::new(read.storage).unwrap(),
                Arc::new(read.history),
            ))
        });
        let control = factory(0).unwrap();
        let pack: Arc<dyn PackPersistence> = writer.storage;
        let store = Arc::new(
            Store::new(
                pack,
                Arc::new(writer.history),
                control,
                factory,
                0,
                ReservationBlocks::default(),
                16,
            )
            .unwrap(),
        );
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        Self {
            fixture,
            store,
            harness,
            readers,
        }
    }
    /// `clean`: no mount and no custody remains, so the serving assembly is
    /// stopped with its checks. Otherwise the registry still owns what the
    /// product kept, and the assembly is dropped with it.
    fn finish(self, clean: bool) {
        let Self {
            fixture,
            store,
            harness,
            readers,
        } = self;
        if clean {
            harness.stop();
        } else {
            let Harness {
                owner,
                serving,
                service,
                ..
            } = harness;
            drop(service);
            drop(serving);
            println!(
                "FP-27 RIG owner stopped with retained custody: {:?}",
                owner.stop()
            );
        }
        drop(readers);
        println!(
            "FP-27 RIG store_opened=1 sealed=0 reopened=0 profile=Disposable clean_stop={clean} handles_after_stop={}",
            Arc::strong_count(&store)
        );
        drop(store);
        fixture.cleanup();
    }
}

/// A descriptor whose READs are synchronous kernel requests served by the
/// daemon: no page cache and no readahead stand between `pread` and READ.
fn direct(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
        .unwrap()
}
type Answer = Result<Vec<u8>, Option<Errno>>;
/// One `pread` of a file's head on its own thread, which ends when that
/// system call returns.
fn pread_apart(file: Arc<File>) -> mpsc::Receiver<Answer> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = vec![0; WINDOW];
        let answer = file
            .read_at(&mut bytes, 0)
            .map(|length| bytes[..length].to_vec())
            .map_err(|error| error.raw_os_error().map(Errno::from_raw));
        let _ = sender.send(answer);
    });
    receiver
}
/// One whole-file read by ordinary syscalls on its own thread.
fn read_apart(path: PathBuf) -> mpsc::Receiver<Answer> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let answer = fs::read(path).map_err(|error| error.raw_os_error().map(Errno::from_raw));
        let _ = sender.send(answer);
    });
    receiver
}

// ------------------------------------------------- marker at the READ step

/// The read session the test puts out of service.
const QUARANTINED: usize = 1;
struct Uncertain;
impl PackReadPlan for Uncertain {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        Err(PersistenceError::Uncertain)
    }
}
/// As `store_read_service.rs`: one scoped pack read on the session's own
/// provider whose plan answers `Uncertain`, which the provider keeps.
fn quarantine(provider: &StorageProvider, root: ObjectId) {
    let mut rows = Vec::new();
    provider.locate(&[root], &mut rows).unwrap();
    assert!(matches!(
        provider.read_scoped_pack(rows[0].location.pack_id, &mut Uncertain),
        Err(PersistenceError::Uncertain)
    ));
}

/// FP-27, the marker at the READ step. The READ whose own demand observes
/// the quarantined reader is answered `EIO` and ends by itself; the same
/// descriptor is then read exactly; the mount's failed-demand record counts
/// one with that original cause; nothing is retained and the mount keeps
/// serving, with no further demand on the quarantined reader.
#[test]
fn fp27_a_quarantined_workspace_reader_is_never_reopened_or_borrowed_by_its_sibling() {
    let fx = Fx::new("fp27-owned-reader");
    let ready = fx.harness.mount(1);
    let mounted = Mounted(ready.directory.clone());
    let sibling = fx.harness.mount(2);
    let sibling_mounted = Mounted(sibling.directory.clone());
    let base = fx
        .store
        .history()
        .branch_snapshot(fx.fixture.branch)
        .unwrap()
        .unwrap()
        .effective_root;
    let file = Arc::new(direct(&root(&ready).join(COLD[0].0)));
    assert!(quiet(&fx.harness, ready.token));
    assert_eq!(fx.store.read_work().readers, 3); // control and two mounted readers
    quarantine(&fx.readers.lock().unwrap()[QUARANTINED], base);
    let observed = pread_apart(file.clone()).recv_timeout(WAIT).unwrap();
    assert_eq!(observed, Err(Some(Errno::EIO)));
    let failures = fx.store.reader_failures();
    assert_eq!(failures.len(), 1);
    let first = failures[0].1.clone();
    assert_eq!(failures[0].0, QUARANTINED);
    assert!(matches!(first.as_ref(), PortError::Storage(error) if error.is_unknown_outcome()));
    let stopped_at = fx.readers.lock().unwrap()[QUARANTINED]
        .diagnostics()
        .unwrap()
        .statements;
    assert_eq!(
        pread_apart(file.clone()).recv_timeout(WAIT).unwrap(),
        Err(Some(Errno::EIO))
    );
    for (index, (name, _, _)) in COLD.iter().enumerate() {
        assert_eq!(
            read_apart(root(&sibling).join(name))
                .recv_timeout(WAIT)
                .unwrap(),
            Ok(expected(index))
        );
    }
    assert_eq!(
        fx.readers.lock().unwrap()[QUARANTINED]
            .diagnostics()
            .unwrap()
            .statements,
        stopped_at
    );
    assert!(Arc::ptr_eq(&fx.store.reader_failures()[0].1, &first));
    drop(file);
    assert!(quiet(&fx.harness, ready.token));
    fx.harness.unmount(&sibling);
    drop(sibling_mounted);
    let failed_close = fx.harness.try_unmount(ready.token);
    assert!(matches!(
        failed_close,
        Err(layerfs_daemon::control::Failure::Retained(_))
    ));
    assert!(mount_entry(&ready.directory).is_none()); // known detach, unknown reader retained
    assert!(Arc::ptr_eq(&fx.store.reader_failures()[0].1, &first));
    drop((failed_close, first, failures, mounted));
    fx.finish(false);
}

// ------------------------------------------------------- Capacity retained
