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
//! (`MountQueue::inspect_retained` has no caller in daemon source). The
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
use layerfs_bridge::control::{
    Activity, NativePhase, NativeWork, Reply, Request, TeardownStage, WorkspaceToken,
};
use layerfs_content::ObjectId;
use layerfs_daemon::{
    control::Failure,
    store::{PortError, ReadAdmissionError, ReadLimits, ReadTicket, Store, StoreReader},
    Command, Completion, NativeJob, NativeReply, OwnerClient, OwnerError, Response,
};
use layerfs_fuse::request::Opcode;
use layerfs_overlay::{CleanupState, NativeMount, NativeMountState, Route};
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
    fmt::Debug,
    fs::{self, File, OpenOptions},
    os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command as Process,
    sync::{mpsc, Arc},
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

/// A bounded description of an original value for an evidence line.
fn brief(value: &impl Debug) -> String {
    let mut text = format!("{value:?}");
    let mut end = text.len().min(600);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}
/// Product expectations that did not hold. Reported after teardown, so a
/// deviation still leaves its whole evidence.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("FP-27 DEVIATION {what}");
            self.0.push(what);
        }
    }
    fn done(self, name: &str) {
        assert!(
            self.0.is_empty(),
            "{name}: {} product expectations did not hold: {:#?}",
            self.0.len(),
            self.0
        );
    }
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
fn work(harness: &Harness, token: WorkspaceToken) -> NativeWork {
    harness.status(token).native.unwrap().work.unwrap()
}
/// Two consecutive observations of a connection with nothing received, no
/// request admitted beyond those retained, and none completed in between.
fn quiet(harness: &Harness, token: WorkspaceToken) -> bool {
    let mut last = None;
    eventually(|| {
        let now = work(harness, token);
        let seen = (now.received, now.admitted, now.completed);
        let settled = now.received == 0 && now.admitted == now.retained && last == Some(seen);
        last = Some(seen);
        thread::sleep(Duration::from_millis(5));
        settled
    })
}
/// One owner job of the test, attempted once after a bounded wait for a free
/// slot of its class, and awaited within the same bound.
fn job(client: &OwnerClient, route: Route, mut command: Command) -> Completion {
    let deadline = Instant::now() + WAIT;
    let pending = loop {
        match client.try_submit(Some(route), command) {
            Ok(pending) => break pending,
            Err((OwnerError::AdmissionFull, back)) => {
                assert!(Instant::now() < deadline, "owner observation {back:?}");
                command = back;
                thread::sleep(Duration::from_millis(1));
            }
            Err((error, back)) => panic!("owner observation {back:?}: {error:?}"),
        }
    };
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < deadline, "owner observation incomplete");
        thread::sleep(Duration::from_millis(1));
    }
}
fn engine_mount(client: &OwnerClient, route: Route) -> Option<NativeMount> {
    let done = job(client, route, Command::Native(NativeJob::RetainedMount));
    match done.result() {
        Ok(Response::Native(NativeReply::RetainedMount(mount))) => *mount,
        other => panic!("{other:?}"),
    }
}
fn mount_state(client: &OwnerClient, route: Route, mount: NativeMount) -> NativeMountState {
    let done = job(client, route, Command::Native(NativeJob::State(mount)));
    match done.result() {
        Ok(Response::Native(NativeReply::State(state))) => *state,
        other => panic!("{other:?}"),
    }
}
fn cleanup(client: &OwnerClient, route: Route) -> CleanupState {
    let done = job(client, route, Command::CleanupState);
    match done.result() {
        Ok(Response::CleanupState(state)) => *state,
        other => panic!("{other:?}"),
    }
}
/// Per-opcode frame counts out of a Debug-rendered drain receipt.
fn opcodes(receipt: &str) -> Vec<u64> {
    let key = "OpcodeWork { opcodes: [";
    let at = receipt.find(key).unwrap_or_else(|| panic!("{receipt}")) + key.len();
    let end = at + receipt[at..].find(']').unwrap();
    receipt[at..end]
        .split(", ")
        .map(|count| count.parse().unwrap())
        .collect()
}
/// The text of a Debug-rendered receipt from its failed-demand record on.
fn failed_demands(receipt: &str) -> &str {
    let at = receipt
        .find("failed_demands: ")
        .unwrap_or_else(|| panic!("{receipt}"));
    &receipt[at..]
}

/// One installed Disposable Store, opened once and kept open, with no object
/// cache, and one real native serving assembly over it. The test keeps the
/// provider of each read session.
struct Fx {
    fixture: installed::Fixture,
    store: Arc<Store>,
    harness: Harness,
    readers: Vec<Arc<StorageProvider>>,
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
        let mut readers = Vec::new();
        let sessions = (0..2)
            .map(|_| {
                let read = Handles::open_read_only(config(), installed::BINDING, installed::CURSOR)
                    .unwrap();
                readers.push(read.storage.clone());
                StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
            })
            .collect();
        let pack: Arc<dyn PackPersistence> = writer.storage;
        let store = Arc::new(
            Store::new(
                pack,
                Arc::new(writer.history),
                sessions,
                0,
                ReservationBlocks::default(),
                ReadLimits::default(),
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
const QUARANTINED: usize = 0;
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
fn fp27_a_read_whose_own_demand_observes_the_quarantined_reader_fails_alone_with_eio() {
    let fx = Fx::new("fp27-read");
    let ready = fx.harness.mount(1);
    let mounted = Mounted(ready.directory.clone());
    let mount = root(&ready).to_path_buf();
    let token = ready.token;
    let mut checks = Checks::default();
    let base = fx
        .store
        .history()
        .branch_snapshot(fx.fixture.branch)
        .unwrap()
        .unwrap()
        .effective_root;
    let statements = |fx: &Fx| {
        fx.readers[QUARANTINED]
            .diagnostics()
            .map(|work| work.statements)
    };

    // Looked up and opened while both readers are healthy.
    let file = Arc::new(direct(&mount.join(COLD[0].0)));
    assert!(quiet(&fx.harness, token), "the open is answered");
    let opened = work(&fx.harness, token);
    let idle = fx.store.read_work();
    assert_eq!(
        (idle.readers, idle.quarantined, idle.outstanding),
        (2, 0, 0),
        "{idle:?}"
    );
    // The test takes the other read session, so the next demand of the
    // mount can only be given the one put out of service.
    let first = fx.store.read_ticket(None).unwrap().wait().unwrap();
    let healthy = if first.index() == QUARANTINED {
        let second = fx.store.read_ticket(None).unwrap().wait().unwrap();
        drop(first);
        second
    } else {
        first
    };
    assert_eq!(healthy.index(), 1 - QUARANTINED);
    let before_staging = statements(&fx).unwrap();
    quarantine(&fx.readers[QUARANTINED], base);
    let staged = statements(&fx).unwrap();
    let grants = fx.store.read_work().grants;

    // The observing request: one READ. It is answered while the test still
    // holds the healthy reader, so it had no second reader to ask.
    let observed = pread_apart(file.clone()).recv_timeout(WAIT).ok();
    let pool = fx.store.read_work();
    let at_failure = statements(&fx);
    drop(healthy);
    let quiet_after = quiet(&fx.harness, token);
    let failed = work(&fx.harness, token);
    let failures = fx.store.reader_failures();
    println!(
        "FP-27 read: reader={QUARANTINED} statements(before_staging={before_staging} staged={staged} at_failure={at_failure:?}) observing READ={} pool={pool:?} reader_grants={grants}->{}",
        match &observed {
            Some(Ok(bytes)) => format!("Ok({} bytes)", bytes.len()),
            other => format!("{other:?}"),
        },
        pool.grants
    );
    println!(
        "FP-27 read: native work {opened:?} -> {failed:?} reader_failures={}",
        brief(&failures)
    );
    checks.that(observed == Some(Err(Some(Errno::EIO))), || {
        format!(
            "the READ whose demand observed the quarantined reader was not answered EIO: {}",
            brief(&observed)
        )
    });
    checks.that(pool.quarantined == 1 && pool.grants == grants + 1, || {
        format!("one reader granted to the READ and quarantined by it: {grants} -> {pool:?}")
    });
    checks.that(
        failures.len() == 1
            && failures[0].0 == QUARANTINED
            && matches!(failures[0].1.as_ref(), PortError::Storage(error) if error.is_unknown_outcome()),
        || format!("the retired reader and its original cause: {}", brief(&failures)),
    );
    // One request since the open, completed, with nothing retained.
    checks.that(
        quiet_after
            && failed.handoffs == opened.handoffs + 1
            && failed.completed == opened.completed + 1
            && (failed.retained, failed.terminal, failed.admitted) == (0, 0, 0),
        || format!("the failed READ did not end by itself: {opened:?} -> {failed:?}"),
    );

    // A later read on the same descriptor is exact.
    let again = pread_apart(file.clone()).recv_timeout(WAIT).ok();
    checks.that(again.as_ref() == Some(&Ok(expected(0))), || {
        format!(
            "the later READ on the same descriptor was not exact: {}",
            match &again {
                Some(Ok(bytes)) => format!("Ok({} bytes)", bytes.len()),
                other => format!("{other:?}"),
            }
        )
    });
    // The mount keeps serving: every other cold file, by ordinary reads.
    let mut exact = 0;
    for (index, (name, _, _)) in COLD.iter().enumerate().skip(1) {
        let answer = read_apart(mount.join(name)).recv_timeout(WAIT).ok();
        if answer.as_ref() == Some(&Ok(expected(index))) {
            exact += 1;
        } else {
            checks.that(false, || {
                format!("a later read of {name} was not exact: {}", brief(&answer))
            });
            break;
        }
    }
    let at_end = statements(&fx);
    let end_pool = fx.store.read_work();
    let quiet_end = quiet(&fx.harness, token);
    let served = work(&fx.harness, token);
    println!(
        "FP-27 read: later READ on the same descriptor exact={} other cold files exact={exact} of {} quarantined reader statements {at_failure:?} -> {at_end:?} pool={end_pool:?} native work={served:?}",
        again.as_ref() == Some(&Ok(expected(0))),
        COLD.len() - 1
    );
    checks.that(
        matches!((&at_failure, &at_end), (Ok(then), Ok(now)) if then == now),
        || format!("the quarantined reader got a further demand: {at_failure:?} -> {at_end:?}"),
    );
    checks.that(
        end_pool.quarantined == 1 && end_pool.readers == 2 && end_pool.outstanding == 0,
        || format!("the read pool at the end: {end_pool:?}"),
    );
    checks.that(
        quiet_end && (served.retained, served.terminal, served.unadmitted) == (0, 0, 0),
        || format!("the mount retained or ended a request: {served:?}"),
    );

    // The drain receipt carries the one failed demand and its cause.
    drop(file);
    let quiet_last = quiet(&fx.harness, token);
    let closed = fx.harness.try_unmount(token);
    let clean = matches!(&closed, Ok(done) if done.reply == Reply::Unmounted(token));
    let receipt = match &closed {
        Ok(done) => format!("{:?}", done.native),
        Err(failure) => brief(failure),
    };
    println!(
        "FP-27 read: unmount quiet_before={quiet_last} unmounted={clean} {}",
        if clean {
            failed_demands(&receipt).to_owned()
        } else {
            receipt.clone()
        }
    );
    checks.that(clean, || format!("the Unmount answered {receipt}"));
    if clean {
        let record = failed_demands(&receipt);
        let cause = failures
            .first()
            .map(|(_, cause)| format!("{cause:?}"))
            .unwrap_or_default();
        checks.that(
            record.starts_with("failed_demands: FailedDemands { count: 1, first: Some("),
            || format!("the drain receipt does not count one failed demand: {record}"),
        );
        checks.that(!cause.is_empty() && record.contains(&cause), || {
            format!("the drain receipt does not carry the original cause {cause}: {record}")
        });
        let frames = opcodes(&receipt);
        println!(
            "FP-27 read: receipt_opcodes(lookup={} open={} read={} release={})",
            frames[Opcode::Lookup as usize],
            frames[Opcode::Open as usize],
            frames[Opcode::Read as usize],
            frames[Opcode::Release as usize]
        );
    }
    drop(closed);
    drop(mounted);
    fx.finish(clean);
    checks.done("FP-27 marker at the READ step");
}

// ------------------------------------------------------- Capacity retained

/// FP-27, `Capacity`. With every read-admission ticket of the mounted
/// Workspace's own lane taken by the test, the next READ's admission is
/// refused before anything is read. By source the caller is answered `EIO`
/// and the request is retained (not ended as a failed base demand); a later
/// READ on the same descriptor is refused `ENOTCONN` at admission, because a
/// retained request makes its lane terminal; returning the tickets settles
/// nothing; the later Unmount detaches, finds the retained request and stops
/// `Retained` at `Requests`, and later replies repeat that custody.
#[test]
fn fp27_a_read_refused_capacity_on_its_own_full_lane_is_retained_and_unmount_stops_at_requests() {
    let fx = Fx::new("fp27-capacity");
    let client = fx.harness.owner.client();
    let ready = fx.harness.mount(1);
    let mounted = Mounted(ready.directory.clone());
    let mount = root(&ready).to_path_buf();
    let token = ready.token;
    let route = fx
        .harness
        .service
        .operation(token)
        .unwrap()
        .workspace()
        .route();
    let mut checks = Checks::default();
    let engine = engine_mount(&client, route).expect("attached engine mount");

    // Looked up and opened while the lane is free.
    let file = Arc::new(direct(&mount.join(COLD[0].0)));
    assert!(quiet(&fx.harness, token), "the open is answered");
    let opened = work(&fx.harness, token);
    let idle = fx.store.read_work();
    assert_eq!((idle.quarantined, idle.outstanding), (0, 0), "{idle:?}");
    assert_eq!(fx.store.workspace_reads(token.workspace), Ok(0));

    // Every ticket of the mounted Workspace's own lane, through the public
    // Store API. A ticket that is not polled is an unstarted admission.
    let bound = ReadLimits::default().requests_per_namespace;
    let tickets: Vec<ReadTicket> = (0..bound)
        .map(|taken| {
            fx.store
                .read_ticket(Some(token.workspace))
                .unwrap_or_else(|error| panic!("ticket {taken} of {bound}: {error:?}"))
        })
        .collect();
    // The Store's own answer for one more on that lane.
    let one_more = fx.store.read_ticket(Some(token.workspace)).err();
    let held = fx.store.read_work();
    println!(
        "FP-27 capacity staged: lane_bound={bound} tickets_held={} workspace_reads={:?} one_more={one_more:?} pool={held:?}",
        tickets.len(),
        fx.store.workspace_reads(token.workspace)
    );
    assert_eq!(
        one_more,
        Some(ReadAdmissionError::Capacity),
        "NOT STAGED unless the lane is at its bound"
    );
    assert_eq!(fx.store.workspace_reads(token.workspace), Ok(bound));

    // The READ whose admission is refused.
    let refused = pread_apart(file.clone()).recv_timeout(WAIT).ok();
    let settled = eventually(|| {
        let now = work(&fx.harness, token);
        now.received == 0 && now.retained == 1 && now.admitted == 1
    });
    let kept = work(&fx.harness, token);
    let pool = fx.store.read_work();
    println!(
        "FP-27 capacity: READ={} native work {opened:?} -> {kept:?} pool={pool:?} reader_failures={}",
        match &refused {
            Some(Ok(bytes)) => format!("Ok({} bytes)", bytes.len()),
            other => format!("{other:?}"),
        },
        fx.store.reader_failures().len()
    );
    checks.that(refused == Some(Err(Some(Errno::EIO))), || {
        format!(
            "the READ refused Capacity was not answered EIO: {}",
            brief(&refused)
        )
    });
    // Retained with its cause, not ended: it stays admitted and uncompleted.
    checks.that(
        settled
            && kept.handoffs == opened.handoffs + 1
            && kept.completed == opened.completed
            && (kept.retained, kept.admitted, kept.terminal) == (1, 1, 0),
        || format!("the refused READ is not retained: {opened:?} -> {kept:?}"),
    );
    // It was granted no reader, quarantined none, and holds no ticket.
    checks.that(
        pool.grants == held.grants
            && pool.quarantined == 0
            && pool.outstanding == bound
            && fx.store.reader_failures().is_empty(),
        || format!("the refused READ reached a reader: {held:?} -> {pool:?}"),
    );

    // The tickets are returned. Nothing settles the retained request, and
    // by source a retained request makes its lane terminal
    // (`layerfs-fuse` `dispatch/task.rs`): a later READ on the same
    // descriptor is refused at admission with one `ENOTCONN` reply
    // (`request/state.rs`, `admit_reply`) and counted `terminal`. Attempt 1
    // of this test expected that READ to be served; that expectation was the
    // test's own and is not what source defines.
    drop(tickets);
    // Observed after the return and before the later READ: the pool counts
    // a grant whenever a reader is assigned to a waiting ticket, and each
    // cancelled ticket of the test passed its reader to the test's next one
    // (attempt 2 compared across the return and counted those).
    let returned = fx.store.read_work();
    checks.that(returned.outstanding == 0, || {
        format!("tickets after the return: {returned:?}")
    });
    let again = pread_apart(file.clone()).recv_timeout(WAIT).ok();
    let quiet_again = quiet(&fx.harness, token);
    let after = work(&fx.harness, token);
    let unasked = fx.store.read_work();
    println!(
        "FP-27 capacity: tickets returned: later READ on the same descriptor={} native work={after:?} pool {returned:?} -> {unasked:?}",
        match &again {
            Some(Ok(bytes)) => format!("Ok({} bytes)", bytes.len()),
            other => format!("{other:?}"),
        }
    );
    checks.that(again == Some(Err(Some(Errno::ENOTCONN))), || {
        format!(
            "the READ after the retention was not refused ENOTCONN at admission: {}",
            brief(&again)
        )
    });
    checks.that(
        quiet_again
            && (after.retained, after.admitted) == (1, 1)
            && after.terminal == kept.terminal + 1
            && after.completed == kept.completed
            && after.handoffs == kept.handoffs,
        || format!("the retained READ did not stay as it was, or the later READ was admitted: {kept:?} -> {after:?}"),
    );
    // The refused READ asked the Store for nothing.
    checks.that(
        unasked.grants == returned.grants && unasked.quarantined == 0 && unasked.outstanding == 0,
        || format!("the READ refused at admission reached the Store: {returned:?} -> {unasked:?}"),
    );

    // The later Unmount: detach, then the drain finds the retained request.
    drop(file);
    let quiet_last = quiet(&fx.harness, token);
    let before_unmount = work(&fx.harness, token);
    let custody = match fx.harness.try_unmount(token) {
        Err(Failure::Retained(custody)) => Some(*custody),
        other => {
            checks.that(false, || {
                format!(
                    "the Unmount with a retained request answered {}",
                    match &other {
                        Ok(done) => brief(&done.reply),
                        Err(failure) => brief(failure),
                    }
                )
            });
            None
        }
    };
    println!(
        "FP-27 capacity: unmount quiet_before={quiet_last} work_before={before_unmount:?} custody={custody:?}"
    );
    let mut clean = false;
    if let Some(custody) = &custody {
        checks.that(
            custody.token == token
                && custody.stage == TeardownStage::Requests
                && custody.detached
                && custody.forced.is_none(),
            || format!("not Retained at Requests after a known detach: {custody:?}"),
        );
        checks.that(
            custody.work.is_some_and(|work| {
                (work.received, work.admitted, work.retained) == (0, 1, 1)
                    && (work.loops_configured, work.loops_joined) == (1, 1)
            }),
            || format!("counters at the stop: {:?}", custody.work),
        );
        checks.that(mount_entry(&ready.directory).is_none(), || {
            "the mount is still in the table after a known detach".to_owned()
        });
        let status = fx.harness.status(token);
        let native = status.native.as_ref();
        checks.that(
            status.activity == Activity::Closing
                && native
                    .is_some_and(|native| native.phase == NativePhase::Retained && native.detached),
            || format!("status after the stop: {status:?}"),
        );
        let text = fx.harness.service.retained_native(token).unwrap();
        let text = text.unwrap_or_default();
        println!("FP-27 capacity: kept={}", brief(&text));
        checks.that(
            text.contains("stage: Requests")
                && text.contains("failed_demands: FailedDemands { count: 0, first: None"),
            || format!("the kept owner: {text}"),
        );
        // Later operations answer with the same custody and attempt nothing.
        let jobs = client.diagnostics().unwrap().admitted;
        for request in [Request::Unmount(token), Request::Attach(token)] {
            match fx.harness.service.execute_control(&request) {
                Err(Failure::Retained(again)) => checks.that(*again == *custody, || {
                    format!("{request:?} answered {again:?}, not the same custody")
                }),
                other => checks.that(false, || {
                    format!(
                        "{request:?} answered {}",
                        match &other {
                            Ok(done) => brief(&done.reply),
                            Err(failure) => brief(failure),
                        }
                    )
                }),
            }
        }
        checks.that(client.diagnostics().unwrap().admitted == jobs, || {
            "a later reply submitted an owner job".to_owned()
        });
        // Nothing was revoked or closed behind the retained request.
        let (state, closing) = (mount_state(&client, route, engine), cleanup(&client, route));
        println!(
            "FP-27 capacity: stage=Requests detached=true retained=1 later_replies_equal=2 engine_mount={state:?} cleanup={closing:?} serving={:?}",
            fx.harness.serving.work()
        );
        checks.that(
            state == NativeMountState::Live && closing == CleanupState::Live,
            || format!("revoked or closed behind a retained request: {state:?} {closing:?}"),
        );
    } else {
        clean = mount_entry(&ready.directory).is_none()
            && fx
                .harness
                .service
                .execute_control(&Request::Status(token))
                .is_err();
    }
    drop(mounted);
    fx.finish(clean);
    checks.done("FP-27 Capacity retained");
}
