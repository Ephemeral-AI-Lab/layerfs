//! The exact owner and Store cost of READ, OPEN, RELEASE and READLINK through
//! the daemon's Fuse port: owner jobs by class, statement attempts and
//! executions by family, write transactions, Store reader grants and length
//! batches. The real owner and the real Store with its shared canonical
//! cache, without a kernel mount. Every request is measured at two file
//! sizes and again beside unrelated local rows, and its bytes are checked.
//! A READ records nothing in the engine: the tests after the costs show
//! what that leaves true of an unlinked open file, of a window whose base
//! is replaced before its bytes are read, and of a daemon with no cache.
//! Public API only; every wait is bounded and the test spawns no thread.
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::filesystem::PathName;
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, Store},
    Command, Completion, NativeJob, NativeReply, Owner, OwnerClient, OwnerConfig, Response,
    ServiceClass,
};
use layerfs_fuse::{
    operations::{
        create::{create, symlink},
        remove::unlink,
        MutationInput, MutationRequest, NativeData, NativeMutation, NativeRead, ReadDataInput,
    },
    ports::{Fence, MountServices, RequestServices},
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{
    DatabaseWork, NativeMount, ProfileConfig, StatementKind, StatementWork, READ_WINDOW,
};
use layerfs_workspace::{NativeReadOperation, NativeWindow, Refusal, Time};
use std::{
    cell::Cell,
    future::Future,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);
const CACHE: usize = 8 * 1024 * 1024;
const WINDOW: u32 = READ_WINDOW as u32;
/// A file of one short window and one of three windows and a tail.
const SMALL: usize = 29;
const LARGE: usize = 3 * READ_WINDOW + 1000;
const NOW: Time = Time {
    seconds: 1_700_000_000,
    nanoseconds: 0,
};
/// Local files created between the two measured passes.
const UNRELATED: usize = 48;

/// Statement families in the order a receipt prints them.
const FAMILIES: [(StatementKind, &str); 14] = [
    (StatementKind::Startup, "Startup"),
    (StatementKind::Begin, "Begin"),
    (StatementKind::Commit, "Commit"),
    (StatementKind::Rollback, "Rollback"),
    (StatementKind::Workspace, "Workspace"),
    (StatementKind::Inode, "Inode"),
    (StatementKind::DirectoryEntry, "DirectoryEntry"),
    (StatementKind::Payload, "Payload"),
    (StatementKind::Frontier, "Frontier"),
    (StatementKind::Capture, "Capture"),
    (StatementKind::OperationRecord, "OperationRecord"),
    (StatementKind::Lease, "Lease"),
    (StatementKind::Explain, "Explain"),
    (StatementKind::Reclaim, "Reclaim"),
];
const READ: usize = ServiceClass::Read as usize;
const MUTATION: usize = ServiceClass::Mutation as usize;
const LIFECYCLE: usize = ServiceClass::Lifecycle as usize;
const SOURCE: usize = ServiceClass::Source as usize;

struct Event(mpsc::SyncSender<()>);
impl Wake for Event {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
/// Polls with a real wakeup channel; a future that never completes fails the
/// test at its deadline instead of hanging it.
fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let (send, woken) = mpsc::sync_channel(1);
    let waker = Waker::from(Arc::new(Event(send)));
    let deadline = Instant::now() + WAIT;
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut Context::from_waker(&waker)) {
            return value;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if woken.recv_timeout(remaining).is_err() {
            panic!("a port call did not complete within {WAIT:?}");
        }
    }
}
fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "bounded observation: {what}");
        std::thread::yield_now();
    }
}
fn finish(client: &OwnerClient, route: layerfs_overlay::Route, command: Command) -> Completion {
    let pending = match client.try_submit(Some(route), command) {
        Ok(pending) => pending,
        Err((error, command)) => panic!("admission of {command:?}: {error:?}"),
    };
    wait(pending).unwrap()
}
fn pattern(length: usize, seed: u8) -> Vec<u8> {
    (0..length)
        .map(|at| (at as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}
fn name(text: &str) -> PathName {
    PathName::new(text).unwrap()
}

/// What one request cost the owner and the Store.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Cost {
    /// Completed owner jobs by class: Read, Mutation, Capture, Lifecycle,
    /// OperationRecord, Source.
    jobs: [u64; 6],
    grants: u64,
    length_batches: u64,
    /// (family, attempts, executions) of every family the request touched.
    /// A write transaction is one Startup, one Begin and one Commit.
    sql: Vec<(&'static str, u64, u64)>,
}
impl Cost {
    fn transactions(&self) -> u64 {
        self.family("Begin").0
    }
    fn family(&self, label: &str) -> (u64, u64) {
        self.sql
            .iter()
            .find(|(family, ..)| *family == label)
            .map_or((0, 0), |(_, attempts, executions)| (*attempts, *executions))
    }
    fn statements(&self) -> (u64, u64) {
        self.sql
            .iter()
            .fold((0, 0), |sum, (_, a, e)| (sum.0 + a, sum.1 + e))
    }
}
#[derive(Clone, Copy)]
struct Snapshot {
    completed: [u64; 6],
    admitted: u64,
    grants: u64,
    length_batches: u64,
    sql: DatabaseWork,
}

/// One bound Workspace over a Store with one reader and a canonical cache,
/// and a native mount group. No kernel connection and no dispatcher lane.
struct Rig {
    fixture: support::Fixture,
    store: Arc<Store>,
    owner: Owner,
    client: OwnerClient,
    bound: BoundWorkspace,
    root: u64,
    mount: NativeMount,
    next: Cell<u64>,
}
impl Rig {
    fn new(label: &str, authority: u8, cache: usize) -> Self {
        let fixture = support::Fixture::built(label, |source| {
            std::fs::write(source.join("small"), pattern(SMALL, 1)).unwrap();
            std::fs::write(source.join("large"), pattern(LARGE, 2)).unwrap();
            std::fs::write(source.join("mixed-small"), pattern(SMALL, 3)).unwrap();
            std::fs::write(source.join("mixed-large"), pattern(LARGE, 4)).unwrap();
            std::os::unix::fs::symlink("small", source.join("link-short")).unwrap();
            std::os::unix::fs::symlink("d/".repeat(100), source.join("link-long")).unwrap();
            6
        });
        let store = open_store(
            fixture.config.clone(),
            support::BINDING,
            support::CURSOR,
            1,
            cache,
            Default::default(),
        )
        .unwrap();
        let owner = Owner::start(
            &fixture.directory.join("overlay"),
            ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let client = owner.client();
        let identity = WorkspaceId::from_authority([authority; 32]).unwrap();
        let bound = store
            .bind(
                client.clone(),
                BindRequest {
                    branch: fixture.branch,
                    workspace: identity,
                },
            )
            .unwrap()
            .workspace;
        let root = bound
            .operation()
            .unwrap()
            .workspace()
            .base()
            .unwrap()
            .root()
            .root_inode()
            .serial();
        let done = finish(
            &client,
            bound.route(),
            Command::Native(NativeJob::Mount { root }),
        );
        let mount = match done.result() {
            Ok(Response::Native(NativeReply::Mount(mount))) => *mount,
            other => panic!("mount: {other:?}"),
        };
        drop(done);
        Self {
            fixture,
            store,
            owner,
            client,
            bound,
            root,
            mount,
            next: Cell::new(1),
        }
    }
    /// Fresh services, as every kernel request has, and its request number.
    fn request(&self) -> (Arc<dyn RequestServices>, u64) {
        let request = self.next.get();
        self.next.set(request + 1);
        (self.bound.request(&Fence::default()).unwrap(), request)
    }
    /// Taken once every result of the request under observation is returned.
    fn snapshot(&self) -> Snapshot {
        until("owner results returned", || {
            self.client.diagnostics().unwrap().outstanding == 0
        });
        let work = self.client.diagnostics().unwrap();
        let readers = self.store.read_work();
        assert_eq!((readers.outstanding, readers.waiting), (0, 0));
        Snapshot {
            completed: work.completed,
            admitted: work.admitted,
            grants: readers.grants,
            length_batches: self.store.work().length_batches,
            sql: work.sql_foreground,
        }
    }
    /// One request's cost. It must contain no scan, sort, automatic index or
    /// second preparation, and admit exactly the jobs it completed.
    fn measured<T>(&self, request: impl FnOnce() -> T) -> (T, Cost) {
        let before = self.snapshot();
        let value = request();
        let after = self.snapshot();
        let work = after.sql.since(&before.sql);
        let total: StatementWork = work.total();
        assert_eq!(
            (
                total.fullscan_steps,
                total.sorts,
                total.autoindex_rows,
                total.reprepares
            ),
            (0, 0, 0, 0),
            "{work:?}"
        );
        let mut jobs = [0; 6];
        for (class, count) in jobs.iter_mut().enumerate() {
            *count = after.completed[class] - before.completed[class];
        }
        assert_eq!(after.admitted - before.admitted, jobs.iter().sum::<u64>());
        let sql = FAMILIES
            .iter()
            .map(|(kind, label)| (*label, work.statements[*kind as usize]))
            .filter(|(_, family)| family.attempts != 0)
            .map(|(label, family)| (label, family.attempts, family.executions))
            .collect();
        let cost = Cost {
            jobs,
            grants: after.grants - before.grants,
            length_batches: after.length_batches - before.length_batches,
            sql,
        };
        (value, cost)
    }
    /// LOOKUP of a name in the root: the kernel's reference and the serial.
    fn lookup(&self, child: &str) -> (u64, u64) {
        let (services, request) = self.request();
        let found = wait(NativeRead::prepare(
            services,
            self.mount,
            request,
            self.root,
            None,
            NativeReadOperation::Lookup {
                parent: self.root,
                name: name(child),
            },
        ))
        .unwrap_or_else(|failure| panic!("lookup {child}: {failure:?}"));
        let stat = &found.value().unwrap().stat;
        let answer = (stat.serial, stat.logical_len);
        wait(found.dispose()).unwrap();
        answer
    }
    fn getattr(&self, serial: u64) -> u64 {
        let (services, request) = self.request();
        let found = wait(NativeRead::prepare(
            services,
            self.mount,
            request,
            serial,
            None,
            NativeReadOperation::Getattr { serial },
        ))
        .unwrap_or_else(|failure| panic!("getattr {serial}: {failure:?}"));
        let length = found.value().unwrap().stat.logical_len;
        wait(found.dispose()).unwrap();
        length
    }
    /// OPEN: the descriptor's handle.
    fn open(&self, serial: u64, writable: bool) -> u64 {
        let (services, request) = self.request();
        let opened = wait(NativeRead::prepare(
            services,
            self.mount,
            request,
            serial,
            None,
            NativeReadOperation::Open { serial, writable },
        ))
        .unwrap_or_else(|failure| panic!("open {serial}: {failure:?}"));
        let handle = opened.value().unwrap().file.unwrap().owner_id();
        wait(opened.dispose()).unwrap();
        handle
    }
    fn release(&self, serial: u64, handle: u64) {
        let (services, _) = self.request();
        drop(wait(services.close_file(self.mount, serial, handle)).unwrap());
    }
    /// READ of one window through a descriptor.
    fn read(&self, serial: u64, handle: u64, offset: u64, length: u32) -> Vec<u8> {
        self.data(serial, Some(handle), ReadDataInput::File { offset, length })
    }
    fn readlink(&self, serial: u64) -> Vec<u8> {
        self.data(serial, None, ReadDataInput::Link)
    }
    /// The reply's bytes. Nothing follows the reply: no release is owed.
    fn data(&self, serial: u64, handle: Option<u64>, input: ReadDataInput) -> Vec<u8> {
        let (services, request) = self.request();
        wait(NativeData::read(
            services, self.mount, request, serial, handle, input,
        ))
        .unwrap_or_else(|failure| panic!("read {serial} {input:?}: {failure:?}"))
        .unwrap_or_else(|refusal| panic!("read {serial} {input:?}: refused {refusal:?}"))
        .bytes()
        .to_vec()
    }
    /// One published mutation and its reply attempt: the changed inode's
    /// serial and, for a create, its open descriptor's handle.
    fn mutate(
        &self,
        protected: u64,
        handle: Option<u64>,
        input: MutationInput,
        open: Option<bool>,
    ) -> (u64, Option<u64>) {
        let (services, request) = self.request();
        let done = wait(NativeMutation::perform(
            services,
            MutationRequest {
                mount: self.mount,
                request,
                protected,
                handle,
                input,
                open,
                now: NOW,
            },
        ))
        .unwrap_or_else(|failure| panic!("mutation: {failure:?}"));
        let published = done.value().unwrap();
        assert!(published.changed);
        let serial = published.stat.as_ref().unwrap().serial;
        let file = published.file.map(|file| file.owner_id());
        wait(done.replied()).unwrap();
        (serial, file)
    }
    /// A local file with `bytes`, written window by window; its descriptor
    /// stays open.
    fn local_file(&self, child: &str, bytes: &[u8]) -> (u64, u64) {
        let (serial, handle) = self.mutate(
            self.root,
            None,
            MutationInput::Named(create(self.root, name(child), 0o644)),
            Some(true),
        );
        let handle = handle.expect("the created file is open");
        self.write(serial, handle, 0, bytes);
        (serial, handle)
    }
    fn write(&self, serial: u64, handle: u64, offset: u64, bytes: &[u8]) {
        for (index, window) in bytes.chunks(READ_WINDOW).enumerate() {
            self.mutate(
                serial,
                Some(handle),
                MutationInput::Write {
                    offset: offset + (index * READ_WINDOW) as u64,
                    data: window.into(),
                    cached: false,
                },
                None,
            );
        }
    }
    fn stop(self) {
        let Self {
            fixture,
            store,
            owner,
            client,
            bound,
            mount,
            ..
        } = self;
        let done = finish(
            &client,
            bound.route(),
            Command::Native(NativeJob::Revoke(mount)),
        );
        assert!(
            matches!(done.result(), Ok(Response::Native(NativeReply::Done))),
            "revoke: {:?}",
            done.result()
        );
        drop(done);
        until("owner credits returned", || {
            client.diagnostics().unwrap().outstanding == 0
        });
        assert_eq!(store.read_work().outstanding, 0);
        drop(bound);
        owner.stop().unwrap();
        drop(store);
        fixture.cleanup();
    }
}

/// The files one pass reads: serials and open descriptors.
struct Subjects {
    small: (u64, u64),
    large: (u64, u64),
    local_small: (u64, u64),
    local_large: (u64, u64),
    mixed_small: (u64, u64),
    mixed_large: (u64, u64),
    link_short: u64,
    link_long: u64,
    link_local: u64,
}
/// Where the mixed files were overwritten.
const MIXED_SMALL_AT: usize = 3;
const MIXED_LARGE_AT: usize = 4096;
const MIXED_LARGE_BYTES: usize = 8192;

fn subjects(rig: &Rig) -> Subjects {
    let base = |child: &str, length: usize, writable: bool| {
        let (serial, seen) = rig.lookup(child);
        assert_eq!(seen, length as u64, "{child}");
        (serial, rig.open(serial, writable))
    };
    let small = base("small", SMALL, false);
    let large = base("large", LARGE, false);
    let mixed_small = base("mixed-small", SMALL, true);
    let mixed_large = base("mixed-large", LARGE, true);
    rig.write(mixed_small.0, mixed_small.1, MIXED_SMALL_AT as u64, b"L");
    rig.write(
        mixed_large.0,
        mixed_large.1,
        MIXED_LARGE_AT as u64,
        &pattern(MIXED_LARGE_BYTES, 9),
    );
    let local_small = rig.local_file("local-small", b"1\n");
    let local_large = rig.local_file("local-large", &pattern(2 * READ_WINDOW, 5));
    let (link_local, _) = rig.mutate(
        rig.root,
        None,
        MutationInput::Named(symlink(rig.root, name("link-local"), b"local-small", 4096).unwrap()),
        None,
    );
    Subjects {
        small,
        large,
        local_small,
        local_large,
        mixed_small,
        mixed_large,
        link_short: rig.lookup("link-short").0,
        link_long: rig.lookup("link-long").0,
        link_local,
    }
}

/// Every measured request once, with its bytes checked. The order is fixed.
fn pass(rig: &Rig, s: &Subjects) -> Vec<(&'static str, Cost)> {
    let mut costs = Vec::new();
    let window = READ_WINDOW as u64;

    // LOOKUP and GETATTR of base files this daemon has already seen.
    let ((serial, length), cost) = rig.measured(|| rig.lookup("small"));
    assert_eq!((serial, length), (s.small.0, SMALL as u64));
    costs.push(("lookup base small", cost));
    let ((serial, length), cost) = rig.measured(|| rig.lookup("large"));
    assert_eq!((serial, length), (s.large.0, LARGE as u64));
    costs.push(("lookup base large", cost));
    let (length, cost) = rig.measured(|| rig.getattr(s.small.0));
    assert_eq!(length, SMALL as u64);
    costs.push(("getattr base small", cost));
    let (length, cost) = rig.measured(|| rig.getattr(s.large.0));
    assert_eq!(length, LARGE as u64);
    costs.push(("getattr base large", cost));

    // READ of base bytes.
    let (bytes, cost) = rig.measured(|| rig.read(s.small.0, s.small.1, 0, WINDOW));
    assert_eq!(bytes, pattern(SMALL, 1));
    costs.push(("read base small", cost));
    let large = pattern(LARGE, 2);
    let (bytes, cost) = rig.measured(|| rig.read(s.large.0, s.large.1, 0, WINDOW));
    assert_eq!(bytes, large[..READ_WINDOW]);
    costs.push(("read base large first", cost));
    let (bytes, cost) = rig.measured(|| rig.read(s.large.0, s.large.1, 2 * window, WINDOW));
    assert_eq!(bytes, large[2 * READ_WINDOW..3 * READ_WINDOW]);
    costs.push(("read base large third", cost));
    let (bytes, cost) = rig.measured(|| rig.read(s.large.0, s.large.1, 3 * window, WINDOW));
    assert_eq!(bytes, large[3 * READ_WINDOW..]);
    costs.push(("read base large tail", cost));

    // READ at and beyond the end of a base file whose length is remembered.
    let (bytes, cost) = rig.measured(|| rig.read(s.small.0, s.small.1, SMALL as u64, WINDOW));
    assert!(bytes.is_empty());
    costs.push(("read base small end", cost));
    let (bytes, cost) = rig.measured(|| rig.read(s.large.0, s.large.1, LARGE as u64 + 7, WINDOW));
    assert!(bytes.is_empty());
    costs.push(("read base large beyond", cost));

    // READ of local bytes.
    let (bytes, cost) = rig.measured(|| rig.read(s.local_small.0, s.local_small.1, 0, WINDOW));
    assert_eq!(bytes, b"1\n");
    costs.push(("read local small", cost));
    let (bytes, cost) = rig.measured(|| rig.read(s.local_large.0, s.local_large.1, window, WINDOW));
    assert_eq!(bytes, pattern(2 * READ_WINDOW, 5)[READ_WINDOW..]);
    costs.push(("read local large", cost));

    let (bytes, cost) = rig.measured(|| rig.read(s.local_small.0, s.local_small.1, 2, WINDOW));
    assert!(bytes.is_empty());
    costs.push(("read local small end", cost));

    // READ of a window that is partly local and partly inherited.
    let mut mixed = pattern(SMALL, 3);
    mixed[MIXED_SMALL_AT] = b'L';
    let (bytes, cost) = rig.measured(|| rig.read(s.mixed_small.0, s.mixed_small.1, 0, WINDOW));
    assert_eq!(bytes, mixed);
    costs.push(("read mixed small", cost));
    let mut mixed = pattern(LARGE, 4);
    mixed[MIXED_LARGE_AT..MIXED_LARGE_AT + MIXED_LARGE_BYTES]
        .copy_from_slice(&pattern(MIXED_LARGE_BYTES, 9));
    let (bytes, cost) = rig.measured(|| rig.read(s.mixed_large.0, s.mixed_large.1, 0, WINDOW));
    assert_eq!(bytes, mixed[..READ_WINDOW]);
    costs.push(("read mixed large", cost));

    // OPEN and RELEASE.
    for (label, closing, serial) in [
        ("open base small", "release base small", s.small.0),
        ("open base large", "release base large", s.large.0),
        ("open local small", "release local small", s.local_small.0),
        ("open local large", "release local large", s.local_large.0),
    ] {
        let (handle, cost) = rig.measured(|| rig.open(serial, false));
        costs.push((label, cost));
        let ((), cost) = rig.measured(|| rig.release(serial, handle));
        costs.push((closing, cost));
    }

    // READLINK.
    let (target, cost) = rig.measured(|| rig.readlink(s.link_short));
    assert_eq!(target, b"small");
    costs.push(("readlink base short", cost));
    let (target, cost) = rig.measured(|| rig.readlink(s.link_long));
    assert_eq!(target, "d/".repeat(100).as_bytes());
    costs.push(("readlink base long", cost));
    let (target, cost) = rig.measured(|| rig.readlink(s.link_local));
    assert_eq!(target, b"local-small");
    costs.push(("readlink local", cost));
    costs
}
fn of<'a>(costs: &'a [(&'static str, Cost)], label: &str) -> &'a Cost {
    &costs
        .iter()
        .find(|(name, _)| *name == label)
        .unwrap_or_else(|| panic!("no measured request {label}"))
        .1
}
fn jobs(read: u64, lifecycle: u64, source: u64) -> [u64; 6] {
    let mut jobs = [0; 6];
    jobs[READ] = read;
    jobs[LIFECYCLE] = lifecycle;
    jobs[SOURCE] = source;
    jobs
}

#[test]
fn read_open_and_readlink_cost_exactly_this_at_any_size_and_beside_unrelated_rows() {
    let rig = Rig::new("read-cost", 171, CACHE);
    let subjects = subjects(&rig);
    let first = pass(&rig, &subjects);
    for (label, cost) in &first {
        println!(
            "READ_COST {label}: jobs={:?} grants={} length_batches={} transactions={} statements={:?} sql={:?}",
            cost.jobs,
            cost.grants,
            cost.length_batches,
            cost.transactions(),
            cost.statements(),
            cost.sql
        );
    }

    // Unrelated local rows: files created, written and closed in the same
    // directory. No measured request reads or changes them.
    for index in 0..UNRELATED {
        let (serial, handle) = rig.local_file(&format!("unrelated-{index:03}"), &pattern(5000, 7));
        rig.release(serial, handle);
    }
    let second = pass(&rig, &subjects);
    assert_eq!(first, second, "beside {UNRELATED} unrelated local files");

    // Equal work at both sizes and at every window of one file.
    for (one, other) in [
        ("lookup base small", "lookup base large"),
        ("getattr base small", "getattr base large"),
        ("read base small", "read base large first"),
        ("read base small", "read base large third"),
        ("read base small", "read base large tail"),
        ("read base small end", "read base large beyond"),
        ("read base small end", "read local small end"),
        ("read local small", "read local large"),
        ("read mixed small", "read mixed large"),
        ("open base small", "open base large"),
        ("open local small", "open local large"),
        ("release base small", "release base large"),
        ("release base small", "release local small"),
        ("release base small", "release local large"),
        ("readlink base short", "readlink base long"),
    ] {
        assert_eq!(of(&first, one), of(&first, other), "{one} and {other}");
    }

    let expected: [(&str, Cost); 10] = [
        ("lookup base small", LOOKUP_BASE.cost()),
        ("getattr base small", GETATTR_BASE.cost()),
        ("read base small", READ_BASE.cost()),
        ("read base small end", READ_END.cost()),
        ("read local small", READ_LOCAL.cost()),
        ("read mixed small", READ_MIXED.cost()),
        ("open base small", OPEN_BASE.cost()),
        ("open local small", OPEN_LOCAL.cost()),
        ("release base small", RELEASE.cost()),
        ("readlink base short", READLINK_BASE.cost()),
    ];
    for (label, wanted) in &expected {
        assert_eq!(of(&first, label), wanted, "{label}");
    }
    assert_eq!(of(&first, "readlink local"), &READLINK_LOCAL.cost());

    let Subjects {
        small,
        large,
        local_small,
        local_large,
        mixed_small,
        mixed_large,
        ..
    } = subjects;
    for (serial, handle) in [
        small,
        large,
        local_small,
        local_large,
        mixed_small,
        mixed_large,
    ] {
        rig.release(serial, handle);
    }
    rig.stop();
}

#[test]
fn a_base_file_length_is_one_store_answer_per_daemon_and_costs_a_visit_when_not_remembered() {
    // With the cache: the first touch asks the Store once, inside the one
    // reader grant of its second visit; every later touch is one visit.
    let rig = Rig::new("read-cost-first", 172, CACHE);
    let ((serial, length), cost) = rig.measured(|| rig.lookup("small"));
    assert_eq!(length, SMALL as u64);
    assert_eq!(cost.jobs, jobs(2, 0, 0));
    assert_eq!((cost.grants, cost.length_batches), (1, 1));
    assert_eq!(rig.store.cache_work().unwrap().file_lengths, 1);
    let ((again, length), cost) = rig.measured(|| rig.lookup("small"));
    assert_eq!((again, length), (serial, SMALL as u64));
    assert_eq!(cost, LOOKUP_BASE.cost());
    let (length, cost) = rig.measured(|| rig.getattr(serial));
    assert_eq!(length, SMALL as u64);
    assert_eq!(cost, GETATTR_BASE.cost());
    rig.stop();

    // Without an allowance nothing is remembered: every touch is the
    // indexed path of the first one, with the same answer. Nothing fails.
    let rig = Rig::new("read-cost-uncached", 173, 0);
    let (serial, _) = rig.lookup("small");
    for _ in 0..3 {
        let ((again, length), cost) = rig.measured(|| rig.lookup("small"));
        assert_eq!((again, length), (serial, SMALL as u64));
        assert_eq!(cost, LOOKUP_UNSEEN.cost());
        let (length, cost) = rig.measured(|| rig.getattr(serial));
        assert_eq!(length, SMALL as u64);
        assert_eq!(cost.jobs, jobs(2, 0, 0));
        assert_eq!((cost.grants, cost.length_batches), (1, 1));
    }
    assert_eq!(rig.store.cache_work().unwrap().file_lengths, 0);
    rig.stop();
}

#[test]
fn an_unlinked_open_file_still_reads_its_own_bytes_in_one_visit() {
    let rig = Rig::new("read-cost-unlinked", 174, CACHE);
    let s = subjects(&rig);
    let remove = |child: &str| {
        let (services, request) = rig.request();
        let done = wait(NativeMutation::perform(
            services,
            MutationRequest {
                mount: rig.mount,
                request,
                protected: rig.root,
                handle: None,
                input: MutationInput::Named(unlink(rig.root, name(child))),
                open: None,
                now: NOW,
            },
        ))
        .unwrap_or_else(|failure| panic!("unlink {child}: {failure:?}"));
        assert!(done.value().unwrap().changed, "{child}");
        wait(done.replied()).unwrap();
    };
    // A base file, a partly overwritten base file and a local file, each
    // unlinked while its descriptor is open.
    for child in ["small", "mixed-large", "local-large"] {
        remove(child);
    }
    let (bytes, base) = rig.measured(|| rig.read(s.small.0, s.small.1, 0, WINDOW));
    assert_eq!(bytes, pattern(SMALL, 1));
    let mut mixed = pattern(LARGE, 4);
    mixed[MIXED_LARGE_AT..MIXED_LARGE_AT + MIXED_LARGE_BYTES]
        .copy_from_slice(&pattern(MIXED_LARGE_BYTES, 9));
    let (bytes, partly) = rig.measured(|| rig.read(s.mixed_large.0, s.mixed_large.1, 0, WINDOW));
    assert_eq!(bytes, mixed[..READ_WINDOW]);
    let window = READ_WINDOW as u64;
    let (bytes, local) =
        rig.measured(|| rig.read(s.local_large.0, s.local_large.1, window, WINDOW));
    assert_eq!(bytes, pattern(2 * READ_WINDOW, 5)[READ_WINDOW..]);
    for (label, cost, grants) in [
        ("base", &base, 1),
        ("mixed", &partly, 1),
        ("local", &local, 0),
    ] {
        println!(
            "READ_UNLINKED {label}: jobs={:?} grants={} statements={:?} sql={:?}",
            cost.jobs,
            cost.grants,
            cost.statements(),
            cost.sql
        );
        // One read-only visit: nothing is written, acquired or released.
        // Inherited bytes come from the root the orphan retains: one reader.
        assert_eq!(cost.jobs, jobs(1, 0, 0), "{label}");
        assert_eq!((cost.grants, cost.length_batches), (grants, 0), "{label}");
        // The fence, the orphan's row, and its own and its retained layers.
        let families: Vec<_> = cost.sql.iter().map(|(family, ..)| *family).collect();
        assert_eq!(
            families,
            ["Workspace", "Inode", "Payload", "Lease"],
            "{label}"
        );
        assert_eq!(cost.family("Workspace"), (1, 1), "{label}");
        assert_eq!(cost.family("Inode"), (2, 2), "{label}");
        assert_eq!(cost.family("Lease"), (1, 1), "{label}");
        // One cell range for each layer that still holds the file's cells:
        // the orphan's own, and the retained one below it until background
        // reclamation has moved its cells up. Never more than these two here.
        let ranges = cost.family("Payload");
        assert!(ranges == (1, 1) || ranges == (2, 2), "{label}: {ranges:?}");
    }

    // It is written through its descriptor and read back, still unlinked.
    rig.write(s.mixed_large.0, s.mixed_large.1, 0, b"after-unlink");
    mixed[..12].copy_from_slice(b"after-unlink");
    assert_eq!(
        rig.read(s.mixed_large.0, s.mixed_large.1, 0, WINDOW),
        mixed[..READ_WINDOW]
    );
    // A file that still has its name pays one probe for the orphan.
    let (bytes, cost) = rig.measured(|| rig.read(s.large.0, s.large.1, 0, WINDOW));
    assert_eq!(bytes, pattern(LARGE, 2)[..READ_WINDOW]);
    assert_eq!(cost, READ_BASE_BESIDE_ORPHAN.cost());

    for (serial, handle) in [
        s.small,
        s.large,
        s.local_small,
        s.local_large,
        s.mixed_small,
        s.mixed_large,
    ] {
        rig.release(serial, handle);
    }
    rig.stop();
}

#[test]
fn a_window_reads_the_base_its_visit_named_whatever_is_installed_before_its_bytes() {
    let rig = Rig::new("read-cost-install", 175, CACHE);
    let s = subjects(&rig);
    let (serial, handle) = s.mixed_large;
    let mut before = pattern(LARGE, 4);
    before[MIXED_LARGE_AT..MIXED_LARGE_AT + MIXED_LARGE_BYTES]
        .copy_from_slice(&pattern(MIXED_LARGE_BYTES, 9));

    // The request's visit, exactly as the READ driver makes it: the local
    // part is copied and the base root of the rest is named. Nothing is held.
    let (services, _) = rig.request();
    let visit = wait(services.read_visit(rig.mount, serial, Some(handle), 0, WINDOW)).unwrap();
    let window = NativeWindow::clone(visit.get());
    drop(visit);
    let old = rig.bound.operation().unwrap().workspace().base().unwrap();
    assert_eq!(window.root, old.identity().0.to_bytes());
    assert!(window.inherits(0, false) && window.whole(false).is_none());
    assert_eq!(rig.client.diagnostics().unwrap().outstanding, 0);

    // Before its bytes are read, the inherited part of the same window is
    // overwritten and the Workspace is committed: the installed base now
    // holds other bytes there, and the install waited for no read.
    rig.write(serial, handle, 0, &pattern(MIXED_LARGE_AT, 11));
    let committed = rig
        .bound
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert!(matches!(committed.installed.result(), Ok(Response::Done)));
    drop(committed);
    let new = wait(services.base()).unwrap();
    assert_ne!(new.identity(), old.identity());

    // The window is finished over the Workspace's base of now, as the driver
    // does it, and answers the file as it stood at its visit: every
    // inherited byte from the root the visit named, none from the new one.
    let bytes = window
        .finish(Some(&new), serial, 0, WINDOW, false)
        .unwrap()
        .unwrap();
    drop(new);
    assert_eq!(bytes, before[..READ_WINDOW]);

    // A read that visits now sees the file of now.
    let mut after = before;
    after[..MIXED_LARGE_AT].copy_from_slice(&pattern(MIXED_LARGE_AT, 11));
    let (bytes, cost) = rig.measured(|| rig.read(serial, handle, 0, WINDOW));
    assert_eq!(bytes, after[..READ_WINDOW]);
    // Its window is wholly inherited from the installed base.
    assert_eq!(cost, READ_BASE.cost());
    println!(
        "READ_INSTALL visit_root=old installed_between=1 finished_over=new answer=old_file(exact) next_read=new_file(exact) cost={cost:?}"
    );

    drop(services);
    for (serial, handle) in [
        s.small,
        s.large,
        s.local_small,
        s.local_large,
        s.mixed_small,
        s.mixed_large,
    ] {
        rig.release(serial, handle);
    }
    rig.stop();
}

#[test]
fn without_a_cache_a_read_is_still_one_visit_and_one_reader_and_refuses_nothing() {
    let rig = Rig::new("read-cost-uncached-read", 176, 0);
    let s = subjects(&rig);
    let large = pattern(LARGE, 2);
    let window = READ_WINDOW as u64;
    for round in 0..2 {
        let (bytes, cost) = rig.measured(|| rig.read(s.large.0, s.large.1, 2 * window, WINDOW));
        assert_eq!(bytes, large[2 * READ_WINDOW..3 * READ_WINDOW], "{round}");
        assert_eq!(cost, READ_BASE.cost(), "{round}");
        // Nothing is remembered about the base: the end of the file is
        // found by the one base read instead of by the visit.
        let (bytes, cost) = rig.measured(|| rig.read(s.large.0, s.large.1, LARGE as u64, WINDOW));
        assert!(bytes.is_empty(), "{round}");
        assert_eq!(cost, READ_BASE.cost(), "{round}");
        let (bytes, cost) =
            rig.measured(|| rig.read(s.local_large.0, s.local_large.1, window, WINDOW));
        assert_eq!(bytes, pattern(2 * READ_WINDOW, 5)[READ_WINDOW..], "{round}");
        assert_eq!(cost, READ_LOCAL.cost(), "{round}");
        let (target, cost) = rig.measured(|| rig.readlink(s.link_short));
        assert_eq!(target, b"small", "{round}");
        assert_eq!(cost, READLINK_BASE.cost(), "{round}");
    }
    // A READ of a directory and a READLINK of a file are refused by kind.
    let (services, request) = rig.request();
    let refused = |serial: u64, handle: Option<u64>, input: ReadDataInput| match wait(
        NativeData::read(services.clone(), rig.mount, request, serial, handle, input),
    ) {
        Ok(Err(refusal)) => refusal,
        Ok(Ok(_)) => panic!("served {input:?} of {serial}"),
        Err(failure) => panic!("{failure:?}"),
    };
    assert_eq!(
        refused(
            rig.root,
            None,
            ReadDataInput::File {
                offset: 0,
                length: WINDOW
            }
        ),
        Refusal::IsDirectory
    );
    assert_eq!(
        refused(s.large.0, None, ReadDataInput::Link),
        Refusal::Invalid
    );
    assert_eq!(
        refused(s.local_large.0, None, ReadDataInput::Link),
        Refusal::Invalid
    );
    drop(services);
    assert_eq!(rig.store.cache_work().unwrap().file_lengths, 0);
    for (serial, handle) in [
        s.small,
        s.large,
        s.local_small,
        s.local_large,
        s.mixed_small,
        s.mixed_large,
    ] {
        rig.release(serial, handle);
    }
    rig.stop();
}

/// A pinned cost: jobs (Read, Lifecycle, Source), reader grants, length
/// batches and the statement families.
struct Pinned {
    jobs: (u64, u64, u64),
    grants: u64,
    length_batches: u64,
    sql: &'static [(&'static str, u64, u64)],
}
impl Pinned {
    fn cost(&self) -> Cost {
        let cost = Cost {
            jobs: jobs(self.jobs.0, self.jobs.1, self.jobs.2),
            grants: self.grants,
            length_batches: self.length_batches,
            sql: self.sql.to_vec(),
        };
        assert_eq!(cost.jobs[MUTATION], 0);
        cost
    }
}
/// LOOKUP of a base regular file this daemon has seen: one visit. Its
/// length is remembered, so the visit decides over resident facts and adds
/// one to the kernel's count in one transaction. No reader, no length batch.
const LOOKUP_BASE: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 0,
    length_batches: 0,
    sql: &[
        ("Startup", 1, 1),
        ("Begin", 1, 1),
        ("Commit", 1, 1),
        ("Workspace", 1, 1),
        ("Inode", 3, 3),
        ("DirectoryEntry", 2, 2),
        ("Lease", 2, 2),
    ],
};
/// GETATTR of the same file: one visit, and nothing written.
const GETATTR_BASE: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 0,
    length_batches: 0,
    sql: &[("Workspace", 1, 1), ("Inode", 2, 2)],
};
/// The first LOOKUP of a base regular file in a daemon, and every one where
/// no length can be remembered: two visits around one reader, whose one
/// length batch is the Store's answer.
const LOOKUP_UNSEEN: Pinned = Pinned {
    jobs: (2, 0, 0),
    grants: 1,
    length_batches: 1,
    sql: &[
        ("Startup", 1, 1),
        ("Begin", 1, 1),
        ("Commit", 1, 1),
        ("Workspace", 2, 2),
        ("Inode", 3, 3),
        ("DirectoryEntry", 2, 2),
        ("Lease", 2, 2),
    ],
};
/// READ of base bytes: one read-only visit (the mount, the descriptor and
/// the Workspace row in one statement; the inode's local layers in another)
/// and one reader for the bytes. No transaction, no source, no release.
const READ_BASE: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 1,
    length_batches: 0,
    sql: &[("Workspace", 1, 1), ("Inode", 1, 1)],
};
/// READ at or beyond the end of a file: the same visit and nothing else. A
/// base file's end is the length the daemon remembers; a local file's is its
/// own row.
const READ_END: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 0,
    length_batches: 0,
    sql: &[("Workspace", 1, 1), ("Inode", 1, 1)],
};
/// READ of local bytes: the visit copies them (one cell-range statement of
/// the one local layer). No reader.
const READ_LOCAL: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 0,
    length_batches: 0,
    sql: &[("Workspace", 1, 1), ("Inode", 1, 1), ("Payload", 1, 1)],
};
/// READ of a window that is partly local: the visit of a local one, and one
/// reader for the inherited span.
const READ_MIXED: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 1,
    length_batches: 0,
    sql: &[("Workspace", 1, 1), ("Inode", 1, 1), ("Payload", 1, 1)],
};
/// OPEN of a base file: a source, two observations around one reader (no
/// length batch), and the release of the processing read and source.
const OPEN_BASE: Pinned = Pinned {
    jobs: (2, 2, 1),
    grants: 1,
    length_batches: 0,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 12, 12),
        ("Inode", 2, 2),
        ("Lease", 38, 56),
    ],
};
/// OPEN of a local file: one observation decides.
const OPEN_LOCAL: Pinned = Pinned {
    jobs: (1, 2, 1),
    grants: 0,
    length_batches: 0,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 11, 11),
        ("Inode", 1, 1),
        ("Lease", 34, 52),
    ],
};
/// RELEASE: one job, one transaction.
const RELEASE: Pinned = Pinned {
    jobs: (0, 1, 0),
    grants: 0,
    length_batches: 0,
    sql: &[
        ("Startup", 1, 1),
        ("Begin", 1, 1),
        ("Commit", 1, 1),
        ("Workspace", 1, 1),
        ("Lease", 3, 7),
    ],
};
/// READLINK of a base link: the visit, under the kernel's lookup reference,
/// and one reader for the target.
const READLINK_BASE: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 1,
    length_batches: 0,
    sql: &[("Workspace", 1, 1), ("Inode", 1, 1)],
};
/// READLINK of a local link: the visit copies the target. No reader.
const READLINK_LOCAL: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 0,
    length_batches: 0,
    sql: &[("Workspace", 1, 1), ("Inode", 1, 1), ("Payload", 1, 1)],
};
/// READ of base bytes once this engine has made an orphan: the visit also
/// asks whether its inode is one.
const READ_BASE_BESIDE_ORPHAN: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 1,
    length_batches: 0,
    sql: &[("Workspace", 1, 1), ("Inode", 1, 1), ("Lease", 1, 1)],
};
