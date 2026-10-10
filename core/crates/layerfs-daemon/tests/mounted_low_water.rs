//! Real kernel mounts: the low-water early serial refill of owner ruling P-1
//! (R8b track P-A).
//!
//! `mounted_failure_scope.rs` holds the exhaustion half: a create that finds
//! no local range under a held Store writer is `EAGAIN` with no effect. This
//! file holds the other half. With an explicit low-water just below the
//! refill window, the second create of a Workspace is already below it:
//!
//! - it makes exactly one early reservation attempt and succeeds;
//! - while a real external process holds the Store's SQLite writer, that one
//!   attempt is refused, the create still succeeds with its exact effect and
//!   makes no second attempt; a later create makes its own single attempt,
//!   and once the peer has released it succeeds and extends the range;
//! - with low-water 0 on the same shape no early attempt is made at all: the
//!   next reservation is the one of the create that finds the range
//!   exhausted.
//!
//! As in `mounted_failure_scope.rs`, the Store is composed from the same
//! public pieces `open_store` composes, so that the test keeps the writable
//! session and counts reservations at the public History port. Every call is
//! forwarded to the real provider; nothing is injected. The low-water is set
//! on the Store through the same public setter application assembly calls.
//! A created name's kernel inode number is its serial plus one, so the
//! serial a create consumed is read back with `stat`.
//!
//! Product expectations are collected and reported after the mount has been
//! torn down, so a deviation still leaves its whole evidence. Counts only.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/held_writer.rs"]
mod held_writer;
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
use held_writer::HeldWriter;
use layerfs_bridge::control::{Reply, WorkspaceToken};
use layerfs_daemon::store::{PortError, ReadLimits, Store, StoreReader};
use layerfs_history::{HistoryCatalog, HistoryError};
use layerfs_persistence::{Handles, SqlitePersistenceProfile};
use layerfs_storage::{port::PackPersistence, ReservationBlocks, Storage};
use layerfs_workspace::{WorkspaceError, SERIAL_REFILL};
use model::Model;
use mounted::{mount_entry, Harness, COMMAND};
use rig::{bash_in, root, stamp_tree};
use std::{
    fmt::Debug,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    process::{Command as Process, Output},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(8);
/// The bound of a wait for a quiet connection while the external writer is
/// held. It ends well inside the holder's own five seconds.
const HELD_WAIT: Duration = Duration::from_secs(2);
/// Just below the refill window: one serial taken from a fresh window of
/// 1,024 leaves 1,023, which is not below it; the next leaves 1,022.
const LOW: u64 = SERIAL_REFILL - 1;

/// The real catalog, with every serial reservation and its answer recorded.
/// Copied from `mounted_failure_scope.rs`.
mod counted {
    use layerfs_history::*;
    use layerfs_persistence::Handles;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub struct Attempt {
        /// The reserved range as `(start, count)`.
        pub reserved: Option<(u64, u64)>,
        /// The provider answered `Busy`: no write effect.
        pub busy: bool,
        /// The provider's original answer.
        pub answer: String,
    }
    impl std::fmt::Debug for Attempt {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.answer)
        }
    }
    pub struct CountedHistory {
        pub handles: Arc<Handles>,
        pub attempts: Mutex<Vec<Attempt>>,
    }
    macro_rules! forward {
        ($($name:ident($($arg:ident: $ty:ty),*) -> $out:ty;)*) => {$(
            fn $name(&self, $($arg: $ty),*) -> $out { self.handles.history.$name($($arg),*) }
        )*};
    }
    impl HistoryCatalog for CountedHistory {
        forward! {
            catalog_id() -> CatalogId;
            incarnation() -> u64;
            layer_stack(id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>>;
            layer_stacks(page: &Page) -> HistoryResult<PageResult<LayerStackRecord>>;
            branch(id: BranchId) -> HistoryResult<Option<BranchRecord>>;
            branch_snapshot(id: BranchId) -> HistoryResult<Option<BranchSnapshot>>;
            branches(stack: LayerStackId, page: &Page) -> HistoryResult<PageResult<BranchRecord>>;
            commit(id: CommitId) -> HistoryResult<Option<CommitRecord>>;
            layer(id: LayerId) -> HistoryResult<Option<LayerRecord>>;
            stage(workspace: WorkspaceId) -> HistoryResult<Option<StageRecord>>;
            stages(branch: BranchId, page: &Page) -> HistoryResult<PageResult<StageRecord>>;
            commit_history(request: &CommitHistoryRequest) -> HistoryResult<PageResult<CommitRecord>>;
            layer_history(request: &LayerHistoryRequest) -> HistoryResult<PageResult<LayerRecord>>;
            initialize_layerstack(request: &StackInitialization) -> HistoryResult<LayerStackRecord>;
            fork(request: &ForkRequest) -> HistoryResult<BranchSnapshot>;
            stage_changes(request: &StageRequest) -> HistoryResult<StageRecord>;
            commit_staged(request: &CommitStagedRequest) -> HistoryResult<CommitStagedOutcome>;
            stage_and_commit(request: &StageRequest) -> HistoryResult<CommitStagedOutcome>;
            add_layer(request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome>;
            discard_stage(request: &DiscardRequest) -> HistoryResult<DiscardOutcome>;
        }
        fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation> {
            let result = self.handles.history.reserve_inodes(request);
            self.attempts.lock().unwrap().push(Attempt {
                reserved: result.as_ref().ok().map(|range| (range.start, range.count)),
                busy: matches!(result, Err(HistoryError::Busy)),
                answer: match &result {
                    Ok(range) => format!("Ok(start={} count={})", range.start, range.count),
                    Err(error) => format!("Err({error:?})"),
                },
            });
            result
        }
    }
}
use counted::{Attempt, CountedHistory};

/// A bounded description of an original value for an evidence line.
fn brief(value: &impl Debug) -> String {
    let mut text = format!("{value:?}");
    let mut end = text.len().min(480);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}
/// Product expectations that did not hold. Reported after teardown.
#[derive(Default)]
struct Checks(Vec<String>);
impl Checks {
    fn that(&mut self, holds: bool, what: impl FnOnce() -> String) {
        if !holds {
            let what = what();
            println!("PA_DEVIATION {what}");
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
macro_rules! expect {
    ($checks:expr, $holds:expr, $($what:tt)+) => {
        $checks.that($holds, || format!($($what)+))
    };
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
                "PA_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// A bounded observation loop that reports instead of unwinding.
fn within(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
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
/// Two consecutive observations of a connection with nothing received or
/// admitted and no request completed in between.
fn quiet_within(harness: &Harness, token: WorkspaceToken, limit: Duration) -> bool {
    let mut last = None;
    within(limit, || {
        let work = harness.status(token).native.unwrap().work.unwrap();
        let now = (work.received, work.admitted, work.completed);
        let settled = work.received == 0 && work.admitted == 0 && last == Some(now);
        last = Some(now);
        settled
    })
}
/// The engine's revision on a quiet connection, and whether the connection
/// kept anything retained or terminal.
fn engine(fx: &Fx, token: WorkspaceToken, limit: Duration) -> (bool, Option<i64>, bool) {
    let quiet = quiet_within(&fx.harness, token, limit);
    let status = fx.harness.status(token);
    (
        quiet,
        status.local.map(|local| local.revision),
        status
            .native
            .and_then(|native| native.work)
            .is_some_and(|work| work.retained == 0 && work.terminal == 0),
    )
}

/// One installed Disposable Store, opened once and kept open, with the given
/// explicit serial low-water, and one real native serving assembly over it.
/// The test keeps the writable session and the counted catalog.
struct Fx {
    fixture: installed::Fixture,
    store: Arc<Store>,
    harness: Harness,
    writer: Arc<Handles>,
    history: Arc<CountedHistory>,
}
impl Fx {
    fn new(label: &str, low_water: u64) -> Self {
        let fixture = installed::Fixture::built(label, |source| {
            fs::write(source.join("README.md"), b"# base\n").unwrap();
            fs::create_dir(source.join("dir")).unwrap();
            fs::write(source.join("dir/inherited.txt"), b"inherited\n").unwrap();
            for path in ["README.md", "dir/inherited.txt"] {
                fs::set_permissions(source.join(path), fs::Permissions::from_mode(0o644)).unwrap();
            }
            fs::set_permissions(source.join("dir"), fs::Permissions::from_mode(0o755)).unwrap();
            stamp_tree(source);
            Model::native(source).1
        });
        assert!(
            matches!(
                fixture.config.sqlite_profile,
                SqlitePersistenceProfile::Disposable
            ),
            "the global Store profile is Disposable, selected explicitly"
        );
        let config = || fixture.config.clone();
        let writer = Arc::new(
            Handles::open_writable(config(), installed::BINDING, installed::CURSOR).unwrap(),
        );
        let history = Arc::new(CountedHistory {
            handles: writer.clone(),
            attempts: Mutex::new(Vec::new()),
        });
        let sessions = (0..2)
            .map(|_| {
                let read = Handles::open_read_only(config(), installed::BINDING, installed::CURSOR)
                    .unwrap();
                StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
            })
            .collect();
        let pack: Arc<dyn PackPersistence> = writer.storage.clone();
        let catalog: Arc<dyn HistoryCatalog> = history.clone();
        let store = Arc::new(
            Store::new(
                pack,
                catalog,
                sessions,
                2 * 1024 * 1024,
                ReservationBlocks::default(),
                ReadLimits::default(),
            )
            .unwrap(),
        );
        // Explicit configuration, before any Workspace is bound.
        store.set_serial_low_water(low_water);
        let harness = Harness::new(store.clone(), &fixture.directory, fixture.branch);
        Self {
            fixture,
            store,
            harness,
            writer,
            history,
        }
    }
    fn attempts(&self) -> Vec<Attempt> {
        self.history.attempts.lock().unwrap().clone()
    }
    /// Reservation attempts at the History port and the Store's own count.
    fn counts(&self) -> (usize, u64) {
        (self.attempts().len(), self.store.work().serial_reservations)
    }
    fn write_transactions(&self) -> u64 {
        self.writer.diagnostics().unwrap().write_transactions
    }
    /// One normal Unmount of a connection the test left quiet. True when it
    /// answered `Unmounted`; its answer is printed either way.
    fn unmount(&self, token: WorkspaceToken, what: &str) -> bool {
        let quiet = quiet_within(&self.harness, token, WAIT);
        let closed = self.harness.try_unmount(token);
        let clean = matches!(&closed, Ok(done) if done.reply == Reply::Unmounted(token));
        println!(
            "PA_UNMOUNT {what}: quiet_before={quiet} unmounted={clean} answer={}",
            match &closed {
                Ok(done) => brief(&done.reply),
                Err(failure) => brief(failure),
            }
        );
        clean
    }
    /// `clean`: no mount and no custody remains, so the serving assembly is
    /// stopped with its checks. Otherwise the registry still owns what the
    /// product kept, and the assembly is dropped with it.
    fn finish(self, clean: bool) {
        let Self {
            fixture,
            store,
            harness,
            writer,
            history,
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
                "PA_RIG owner stopped without a clean drain: {:?}",
                owner.stop()
            );
        }
        drop((history, writer));
        println!(
            "PA_RIG store_opened=1 sealed=0 reopened=0 profile=Disposable clean_stop={clean} handles_after_stop={}",
            Arc::strong_count(&store)
        );
        drop(store);
        fixture.cleanup();
    }
}

fn names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
/// One create by an ordinary process launched outside the daemon: the shell
/// opens the name with `O_CREAT` and closes it.
fn create(mount: &Path, name: &str) -> Output {
    bash_in(COMMAND, mount, &format!(": > {name}"))
}
/// What a create left: `(serial, regular, length, uid)` of the name.
type Made = Option<(u64, bool, u64, u32)>;
fn made(mount: &Path, name: &str) -> Made {
    fs::symlink_metadata(mount.join(name))
        .ok()
        .map(|made| (made.ino() - 1, made.is_file(), made.len(), made.uid()))
}
/// The exact effect of one successful create that consumed `serial`.
fn created(checks: &mut Checks, what: &str, output: &Output, made: Made, serial: u64) {
    expect!(
        checks,
        output.status.success() && made == Some((serial, true, 0, COMMAND)),
        "{what}: status={:?} stderr={:?} (serial, regular, length, uid)={made:?}, expected serial {serial}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
/// The allocator's immediate admission refusal, as the request port reads it.
fn contended(error: &WorkspaceError) -> bool {
    let WorkspaceError::Service(error) = error else {
        return false;
    };
    matches!(
        error.downcast_ref::<Arc<PortError>>().map(Arc::as_ref),
        Some(PortError::History(HistoryError::Busy))
    )
}
/// The first window, reserved by the Workspace's first create.
fn first_window(checks: &mut Checks, fx: &Fx, mount: &Path) -> u64 {
    let opened = fx.counts();
    assert_eq!(opened, (0, 0), "the Workspace starts with no serial range");
    let first = create(mount, "first.txt");
    let attempts = fx.attempts();
    let start = attempts
        .first()
        .and_then(|attempt| attempt.reserved)
        .map_or(0, |(start, _)| start);
    println!(
        "PA first create (no local range): attempts={attempts:?} store_serial_reservations={}",
        fx.store.work().serial_reservations
    );
    expect!(
        checks,
        attempts.len() == 1
            && attempts[0].reserved.is_some_and(|(_, count)| count == SERIAL_REFILL)
            && fx.counts() == (1, 1),
        "the first create did not make exactly one successful reservation of {SERIAL_REFILL}: {attempts:?}"
    );
    created(
        checks,
        "first create",
        &first,
        made(mount, "first.txt"),
        start,
    );
    start
}

#[test]
fn the_second_create_below_an_explicit_low_water_makes_one_early_attempt_and_succeeds() {
    let fx = Fx::new("low-a", LOW);
    let ready = fx.harness.mount(1);
    let mounted = Mounted(ready.directory.clone());
    let mount = root(&ready).to_path_buf();
    let token = ready.token;
    let mut checks = Checks::default();
    let names_before = names(&mount);
    let start = first_window(&mut checks, &fx, &mount);

    // 1,022 serials are left after this create's own: below 1,023.
    let transactions_before = fx.write_transactions();
    let second = create(&mount, "second.txt");
    let attempts_second = fx.attempts();
    let counted_second = fx.store.work().serial_reservations;
    let transactions_second = fx.write_transactions();
    // 2,045 are left: the next creates are above the low-water.
    let third = create(&mount, "third.txt");
    let fourth = create(&mount, "dir/fourth.txt");
    let counts_after = fx.counts();
    println!(
        "PA low_water={LOW} second create: attempts={attempts_second:?} store_serial_reservations={counted_second} writer write_transactions {transactions_before} -> {transactions_second} | after two more creates: attempts={} store_serial_reservations={}",
        counts_after.0, counts_after.1
    );
    expect!(
        checks,
        attempts_second.len() == 2 && counted_second == 2,
        "the second create did not make exactly one early attempt: {attempts_second:?}, Store count {counted_second}"
    );
    expect!(
        checks,
        attempts_second.get(1).and_then(|attempt| attempt.reserved)
            == Some((start + SERIAL_REFILL, SERIAL_REFILL)),
        "the early attempt did not reserve the next window after {start}: {attempts_second:?}"
    );
    // The create used the range it found; the new window waits behind it.
    created(
        &mut checks,
        "second create",
        &second,
        made(&mount, "second.txt"),
        start + 1,
    );
    created(
        &mut checks,
        "third create",
        &third,
        made(&mount, "third.txt"),
        start + 2,
    );
    created(
        &mut checks,
        "fourth create",
        &fourth,
        made(&mount, "dir/fourth.txt"),
        start + 3,
    );
    expect!(
        checks,
        counts_after == (2, 2),
        "a create above the low-water made a reservation attempt: {counts_after:?}"
    );
    let mut expected = names_before.clone();
    expected.extend(["first.txt", "second.txt", "third.txt"].map(str::to_owned));
    expected.sort();
    let names_after = names(&mount);
    expect!(
        checks,
        names_after == expected && names(&mount.join("dir")) == ["fourth.txt", "inherited.txt"],
        "the directories after four creates: {names_after:?} {:?}",
        names(&mount.join("dir"))
    );

    let clean = fx.unmount(token, "low-water early attempt");
    drop(mounted);
    fx.finish(clean);
    checks.done("P-1 early attempt below the low-water");
}

#[test]
fn an_early_attempt_refused_under_a_held_store_writer_leaves_the_create_successful() {
    let fx = Fx::new("low-b", LOW);
    let ready = fx.harness.mount(1);
    let mounted = Mounted(ready.directory.clone());
    let mount = root(&ready).to_path_buf();
    let token = ready.token;
    let mut checks = Checks::default();
    let names_before = names(&mount);
    let start = first_window(&mut checks, &fx, &mount);
    let (quiet_before, revision_before, _) = engine(&fx, token, WAIT);

    // A real external process holds the Store's writer.
    let held = HeldWriter::acquire(&fx.fixture.config.path);
    let transactions_before = fx.write_transactions();
    // Below the low-water: one early attempt, refused; the create succeeds.
    let second = create(&mount, "held.txt");
    let attempts_second = fx.attempts();
    let counted_second = fx.store.work().serial_reservations;
    let made_second = made(&mount, "held.txt");
    // A later create is a new operation with its own single attempt.
    let third = create(&mount, "dir/held-again.txt");
    let attempts_third = fx.attempts();
    let made_third = made(&mount, "dir/held-again.txt");
    // The same call at the public request port, on one operation's own
    // custody: the early refusal is returned beside the serial and is not
    // retained as the operation's failure, so its next Store demand is
    // admitted.
    let operation = fx.harness.service.operation(token).unwrap();
    let taken = operation.ports().take_serial(operation.workspace());
    let kept = operation.ports().failure().map(|kept| kept.is_some());
    let admitted = operation.ports().read_ticket().is_ok();
    let attempts_port = fx.attempts();
    // Control: the early attempt made on the operation's own custody
    // instead. Its refusal is retained and refuses the next demand.
    let shared = fx.harness.service.operation(token).unwrap();
    let shared_taken =
        shared
            .workspace()
            .next_serial_with_low_water(shared.ports(), shared.ports(), LOW);
    let shared_kept = shared.ports().failure().map(|kept| kept.is_some());
    let shared_admitted = shared.ports().read_ticket().is_ok();
    drop((operation, shared));
    let attempts_held = fx.attempts();
    let counted_held = fx.store.work().serial_reservations;
    let transactions_held = fx.write_transactions();
    let (quiet_held, revision_held, undamaged_held) = engine(&fx, token, HELD_WAIT);
    // The peer still held the writer here: its release is asserted.
    held.release();
    println!(
        "PA low_water={LOW} held writer: second create attempts={attempts_second:?} store_serial_reservations={counted_second} made={made_second:?} | later create attempts={attempts_third:?} made={made_third:?} | port take_serial={} failure_retained={kept:?} next_demand_admitted={admitted} attempts={} | control on shared custody={} failure_retained={shared_kept:?} next_demand_admitted={shared_admitted} | attempts={attempts_held:?} store_serial_reservations={counted_held} writer write_transactions {transactions_before} -> {transactions_held} | engine revision {revision_before:?} -> {revision_held:?} quiet before={quiet_before} held={quiet_held} undamaged={undamaged_held}",
        brief(&taken),
        attempts_port.len(),
        brief(&shared_taken)
    );
    expect!(
        checks,
        attempts_second.len() == 2
            && attempts_second[1].busy
            && attempts_second[1].reserved.is_none()
            && counted_second == 2,
        "the create below the low-water did not make exactly one early attempt answered Busy: {attempts_second:?}, Store count {counted_second}"
    );
    created(
        &mut checks,
        "create under the held writer",
        &second,
        made_second,
        start + 1,
    );
    expect!(
        checks,
        attempts_third.len() == 3 && attempts_third[2].busy && attempts_third[2].reserved.is_none(),
        "the later create did not make its own single attempt answered Busy: {attempts_third:?}"
    );
    created(
        &mut checks,
        "later create under the held writer",
        &third,
        made_third,
        start + 2,
    );
    expect!(
        checks,
        matches!(&taken, Ok((serial, Some(early))) if *serial == start + 3 && contended(early))
            && attempts_port.len() == 4
            && attempts_port[3].busy,
        "the port did not return the serial beside one contended early refusal: {} {attempts_port:?}",
        brief(&taken)
    );
    expect!(
        checks,
        matches!(kept, Ok(false)) && admitted,
        "the early refusal became the operation's failure: retained={kept:?} admitted={admitted}"
    );
    expect!(
        checks,
        matches!(&shared_taken, Ok((serial, Some(early))) if *serial == start + 4 && contended(early))
            && matches!(shared_kept, Ok(true))
            && !shared_admitted
            && attempts_held.len() == 5
            && attempts_held[4].busy,
        "the control on shared custody did not retain its refusal: {} retained={shared_kept:?} admitted={shared_admitted} {attempts_held:?}",
        brief(&shared_taken)
    );
    expect!(
        checks,
        counted_held == 5 && attempts_held.iter().skip(1).all(|attempt| attempt.busy),
        "an attempt under the held writer was not refused, or was made twice: {attempts_held:?}, Store count {counted_held}"
    );
    expect!(
        checks,
        transactions_held == transactions_before,
        "the writer session ran a write transaction under the held writer: {transactions_before} -> {transactions_held}"
    );
    expect!(
        checks,
        quiet_before && quiet_held && undamaged_held,
        "the connection was not quiet and undamaged: before {quiet_before}, within {HELD_WAIT:?} under the held writer {quiet_held}, undamaged {undamaged_held}"
    );
    expect!(
        checks,
        revision_before.is_some() && revision_held > revision_before,
        "the creates under the held writer did not publish: engine revision {revision_before:?} -> {revision_held:?}"
    );

    // The writer is released: the next create's own single attempt
    // succeeds and extends the range; the one after it makes none.
    let released = create(&mount, "released.txt");
    let attempts_released = fx.attempts();
    let after = create(&mount, "after.txt");
    let counts_after = fx.counts();
    let transactions_after = fx.write_transactions();
    println!(
        "PA low_water={LOW} released: create attempts={attempts_released:?} | next create attempts={} store_serial_reservations={} | writer write_transactions {transactions_held} -> {transactions_after}",
        counts_after.0, counts_after.1
    );
    expect!(
        checks,
        attempts_released.len() == 6
            && attempts_released[5].reserved == Some((start + SERIAL_REFILL, SERIAL_REFILL)),
        "the create after the release did not make one successful reservation of the next window: {attempts_released:?}"
    );
    created(
        &mut checks,
        "create after the release",
        &released,
        made(&mount, "released.txt"),
        start + 5,
    );
    created(
        &mut checks,
        "create above the low-water",
        &after,
        made(&mount, "after.txt"),
        start + 6,
    );
    expect!(
        checks,
        counts_after == (6, 6),
        "the create above the low-water made a reservation attempt: {counts_after:?}"
    );
    let mut expected = names_before.clone();
    expected.extend(["after.txt", "first.txt", "held.txt", "released.txt"].map(str::to_owned));
    expected.sort();
    let names_after = names(&mount);
    expect!(
        checks,
        names_after == expected && names(&mount.join("dir")) == ["held-again.txt", "inherited.txt"],
        "the directories after the creates: {names_after:?} {:?}",
        names(&mount.join("dir"))
    );

    let clean = fx.unmount(token, "low-water under a held writer");
    drop(mounted);
    fx.finish(clean);
    checks.done("P-1 early attempt refused by a held writer");
}

#[test]
fn zero_low_water_makes_no_early_attempt_until_the_range_is_exhausted() {
    let fx = Fx::new("low-d", 0);
    let ready = fx.harness.mount(1);
    let mounted = Mounted(ready.directory.clone());
    let mount = root(&ready).to_path_buf();
    let token = ready.token;
    let mut checks = Checks::default();
    let names_before = names(&mount);
    let start = first_window(&mut checks, &fx, &mount);

    // The same shape as above: a held writer and creates inside the range.
    let held = HeldWriter::acquire(&fx.fixture.config.path);
    let second = create(&mount, "held.txt");
    let made_second = made(&mount, "held.txt");
    let operation = fx.harness.service.operation(token).unwrap();
    let taken = operation.ports().take_serial(operation.workspace());
    drop(operation);
    let counts_held = fx.counts();
    held.release();
    println!(
        "PA low_water=0 held writer: create made={made_second:?} port take_serial={} | attempts={} store_serial_reservations={}",
        brief(&taken),
        counts_held.0,
        counts_held.1
    );
    created(
        &mut checks,
        "create under the held writer",
        &second,
        made_second,
        start + 1,
    );
    expect!(
        checks,
        matches!(&taken, Ok((serial, None)) if *serial == start + 2) && counts_held == (1, 1),
        "an early attempt was made with low-water 0: {} {counts_held:?}",
        brief(&taken)
    );

    // Three serials are consumed and `mkdir bulk` consumes the fourth. The
    // rest of the window, to its last serial, is consumed without one
    // further reservation attempt.
    let rest = SERIAL_REFILL - 4;
    let bulk = bash_in(
        COMMAND,
        &mount,
        &format!("mkdir bulk && cd bulk && for n in $(seq 1 {rest}); do : > $n || exit 1; done"),
    );
    let counts_bulk = fx.counts();
    let last = made(&mount, &format!("bulk/{rest}"));
    let listed = names(&mount.join("bulk")).len() as u64;
    // The window is exhausted: this create makes the one reservation.
    let exhausted = create(&mount, "exhausted.txt");
    let attempts_after = fx.attempts();
    println!(
        "PA low_water=0 window: {rest} further creates status={:?} names={listed} last={last:?} attempts={} store_serial_reservations={} | create on the exhausted range attempts={attempts_after:?}",
        bulk.status.code(),
        counts_bulk.0,
        counts_bulk.1
    );
    expect!(
        checks,
        bulk.status.success()
            && listed == rest
            && last == Some((start + SERIAL_REFILL - 1, true, 0, COMMAND)),
        "the creates to the end of the window: {:?} {:?} names={listed} last={last:?}",
        bulk.status,
        String::from_utf8_lossy(&bulk.stderr)
    );
    expect!(
        checks,
        counts_bulk == (1, 1),
        "a reservation attempt was made before the range was exhausted: {counts_bulk:?}"
    );
    expect!(
        checks,
        attempts_after.len() == 2
            && attempts_after[1].reserved == Some((start + SERIAL_REFILL, SERIAL_REFILL))
            && fx.counts() == (2, 2),
        "the create on the exhausted range did not make exactly one successful reservation: {attempts_after:?}"
    );
    created(
        &mut checks,
        "create on the exhausted range",
        &exhausted,
        made(&mount, "exhausted.txt"),
        start + SERIAL_REFILL,
    );
    let mut expected = names_before.clone();
    expected.extend(["bulk", "exhausted.txt", "first.txt", "held.txt"].map(str::to_owned));
    expected.sort();
    let names_after = names(&mount);
    expect!(
        checks,
        names_after == expected,
        "the root after the creates: {names_after:?}"
    );

    let clean = fx.unmount(token, "low-water 0");
    drop(mounted);
    fx.finish(clean);
    checks.done("P-1 with low-water 0");
}
