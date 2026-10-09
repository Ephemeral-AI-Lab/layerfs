//! The exact owner and Store cost of OPENDIR, READDIR and RELEASEDIR through
//! the daemon's Fuse port: owner jobs by class, statement attempts and
//! executions by family, write transactions and Store reader grants. The
//! real owner and the real Store with its shared canonical cache, without a
//! kernel mount, on the fixture of `read_cost.rs`. Every request is measured
//! on a directory of 10 names and on one of 4000 names, local and inherited,
//! and again beside unrelated local rows; every listing is checked.
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
        create::{create, mkdir},
        DirectoryStep, DirectoryStream, MutationInput, MutationRequest, NativeMutation, NativeRead,
    },
    ports::{Fence, MountServices, RequestServices},
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{
    DatabaseWork, NativeMount, ProfileConfig, StatementKind, StatementWork, READ_WINDOW,
};
use layerfs_workspace::{NativeReadOperation, Refusal, Time};
use std::{
    cell::Cell,
    future::Future,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);
const CACHE: usize = 8 * 1024 * 1024;
/// A directory listed in one reply and one that takes many.
const SMALL: usize = 10;
const LARGE: usize = 4000;
/// Names of one reply of the large directories: one owner window.
const WINDOW: usize = layerfs_overlay::PAGE_ROWS;
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
fn name(text: &str) -> PathName {
    PathName::new(text).unwrap()
}
fn child(index: usize) -> String {
    format!("n-{index:04}")
}
/// The bytes one entry takes in a READDIR reply, as the request counts them.
fn entry_bytes(name: &[u8]) -> usize {
    (24 + name.len()).next_multiple_of(8)
}

/// What one request cost the owner and the Store.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Cost {
    /// Completed owner jobs by class: Read, Mutation, Capture, Lifecycle,
    /// OperationRecord, Source.
    jobs: [u64; 6],
    grants: u64,
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
            for (directory, count) in [("base-small", SMALL), ("base-large", LARGE)] {
                let directory = source.join(directory);
                std::fs::create_dir(&directory).unwrap();
                for index in 0..count {
                    std::fs::write(directory.join(child(index)), b"").unwrap();
                }
            }
            std::fs::write(source.join("file"), b"file").unwrap();
            SMALL + LARGE + 3
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
            sql,
        };
        (value, cost)
    }
    /// LOOKUP of a name: the kernel's reference and the serial.
    fn lookup(&self, parent: u64, child: &str) -> u64 {
        let (services, request) = self.request();
        let found = wait(NativeRead::prepare(
            services,
            self.mount,
            request,
            parent,
            None,
            NativeReadOperation::Lookup {
                parent,
                name: name(child),
            },
        ))
        .unwrap_or_else(|failure| panic!("lookup {child}: {failure:?}"));
        let serial = found.value().unwrap().stat.serial;
        wait(found.dispose()).unwrap();
        serial
    }
    /// OPENDIR: the descriptor's handle, or the refusal.
    fn try_opendir(&self, serial: u64) -> Result<u64, Refusal> {
        let (services, request) = self.request();
        let opened = wait(NativeRead::prepare(
            services,
            self.mount,
            request,
            serial,
            None,
            NativeReadOperation::Opendir { serial },
        ))
        .unwrap_or_else(|failure| panic!("opendir {serial}: {failure:?}"));
        let handle = opened
            .value()
            .map(|value| value.directory.unwrap().owner_id());
        wait(opened.dispose()).unwrap();
        handle
    }
    fn opendir(&self, serial: u64) -> u64 {
        self.try_opendir(serial)
            .unwrap_or_else(|refusal| panic!("opendir {serial}: refused {refusal:?}"))
    }
    /// READDIR at `offset`, filled as the kernel request fills its reply:
    /// every entry that fits one reply window, with its offset.
    fn readdir(&self, serial: u64, handle: u64, offset: u64) -> Vec<(Vec<u8>, u64)> {
        let (services, request) = self.request();
        let mut stream = wait(DirectoryStream::prepare(
            services, self.mount, request, serial, handle, offset,
        ))
        .unwrap_or_else(|failure| panic!("readdir {serial} at {offset}: {failure:?}"));
        let mut listed: Vec<(Vec<u8>, u64)> = stream
            .dots()
            .map(|(name, _, cookie)| (name.as_bytes().to_vec(), cookie))
            .collect();
        let mut used: usize = listed.iter().map(|(name, _)| entry_bytes(name)).sum();
        loop {
            let batch = match wait(stream.next())
                .unwrap_or_else(|failure| panic!("readdir {serial} at {offset}: {failure:?}"))
            {
                DirectoryStep::End(stream) => {
                    wait(stream.dispose()).unwrap();
                    return listed;
                }
                DirectoryStep::Batch(batch) => batch,
            };
            let mut accepted = 0;
            for (entry, cookie) in batch.entries() {
                let size = entry_bytes(&entry.name);
                if size > READ_WINDOW - used {
                    break;
                }
                used += size;
                listed.push((entry.name.clone(), cookie));
                accepted += 1;
            }
            stream = wait(batch.accept(accepted))
                .unwrap_or_else(|failure| panic!("readdir {serial} at {offset}: {failure:?}"));
        }
    }
    fn releasedir(&self, serial: u64, handle: u64) {
        let (services, _) = self.request();
        drop(wait(services.close_directory(self.mount, serial, handle)).unwrap());
    }
    /// One published mutation and its reply attempt: the changed inode.
    fn mutate(&self, protected: u64, input: MutationInput) -> u64 {
        let (services, request) = self.request();
        let done = wait(NativeMutation::perform(
            services,
            MutationRequest {
                mount: self.mount,
                request,
                protected,
                handle: None,
                input,
                open: None,
                now: NOW,
            },
        ))
        .unwrap_or_else(|failure| panic!("mutation: {failure:?}"));
        let published = done.value().unwrap();
        assert!(published.changed);
        let serial = published.stat.as_ref().unwrap().serial;
        wait(done.replied()).unwrap();
        serial
    }
    /// A local directory of `count` empty files.
    fn local_directory(&self, label: &str, count: usize) -> u64 {
        let serial = self.mutate(
            self.root,
            MutationInput::Named(mkdir(self.root, name(label), 0o755)),
        );
        for index in 0..count {
            self.mutate(
                serial,
                MutationInput::Named(create(serial, name(&child(index)), 0o644)),
            );
        }
        serial
    }
    /// Every name of a directory through one descriptor, reply by reply:
    /// the names in the order returned and the number of data replies.
    fn enumerate(&self, serial: u64) -> (Vec<Vec<u8>>, usize) {
        let handle = self.opendir(serial);
        let (mut names, mut offset, mut replies) = (Vec::new(), 0, 0);
        loop {
            let page = self.readdir(serial, handle, offset);
            let Some((_, last)) = page.last() else { break };
            offset = *last;
            replies += 1;
            names.extend(page.into_iter().map(|(name, _)| name));
        }
        self.releasedir(serial, handle);
        (names, replies)
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

/// The directories one pass lists, each with the number of its names.
struct Subjects {
    local_small: u64,
    local_large: u64,
    base_small: u64,
    base_large: u64,
}
fn subjects(rig: &Rig) -> Subjects {
    let subjects = Subjects {
        local_small: rig.local_directory("local-small", SMALL),
        local_large: rig.local_directory("local-large", LARGE),
        base_small: rig.lookup(rig.root, "base-small"),
        base_large: rig.lookup(rig.root, "base-large"),
    };
    // Every directory is listed once whole before anything is measured, so
    // what the daemon's cache holds of the base is the same in every pass.
    for (serial, count) in [
        (subjects.local_small, SMALL),
        (subjects.local_large, LARGE),
        (subjects.base_small, SMALL),
        (subjects.base_large, LARGE),
    ] {
        let (names, replies) = rig.enumerate(serial);
        let wanted: Vec<Vec<u8>> = [".".to_string(), "..".to_string()]
            .into_iter()
            .chain((0..count).map(child))
            .map(String::into_bytes)
            .collect();
        assert_eq!(names, wanted, "every name exactly once, in order");
        assert_eq!(replies, count.div_ceil(WINDOW));
    }
    subjects
}

/// Every measured request once, with its listing checked. The order is fixed.
fn pass(rig: &Rig, s: &Subjects) -> Vec<(String, Cost)> {
    let mut costs = Vec::new();
    let names = |from: usize, to: usize| -> Vec<Vec<u8>> {
        (from..to).map(|index| child(index).into_bytes()).collect()
    };
    for (label, serial, count) in [
        ("local small", s.local_small, SMALL),
        ("local large", s.local_large, LARGE),
        ("base small", s.base_small, SMALL),
        ("base large", s.base_large, LARGE),
    ] {
        let (handle, cost) = rig.measured(|| rig.opendir(serial));
        costs.push((format!("opendir {label}"), cost));
        // The first reply: both dots and the first names.
        let (page, cost) = rig.measured(|| rig.readdir(serial, handle, 0));
        let first = count.min(WINDOW);
        assert_eq!(page[0], (b".".to_vec(), 1), "{label}");
        assert_eq!(page[1], (b"..".to_vec(), 2), "{label}");
        let listed: Vec<Vec<u8>> = page[2..].iter().map(|(name, _)| name.clone()).collect();
        assert_eq!(listed, names(0, first), "{label}");
        costs.push((format!("readdir first {label}"), cost));
        let mut offset = page.last().unwrap().1;
        if count > WINDOW {
            // A reply from the middle of the directory: names only.
            let (page, cost) = rig.measured(|| rig.readdir(serial, handle, offset));
            let listed: Vec<Vec<u8>> = page.iter().map(|(name, _)| name.clone()).collect();
            assert_eq!(listed, names(first, first + WINDOW), "{label}");
            costs.push((format!("readdir next {label}"), cost));
            // The same offset again, as a second reader of the handle would
            // ask: the same names at the same offsets.
            let (again, cost) = rig.measured(|| rig.readdir(serial, handle, offset));
            assert_eq!(again, page, "{label}");
            costs.push((format!("readdir again {label}"), cost));
            offset = page.last().unwrap().1;
        } else {
            let (page, cost) = rig.measured(|| rig.readdir(serial, handle, offset));
            assert!(page.is_empty(), "{label}: {page:?}");
            costs.push((format!("readdir end {label}"), cost));
        }
        let _ = offset;
        let ((), cost) = rig.measured(|| rig.releasedir(serial, handle));
        costs.push((format!("releasedir {label}"), cost));
    }
    costs
}
fn of<'a>(costs: &'a [(String, Cost)], label: &str) -> &'a Cost {
    &costs
        .iter()
        .find(|(name, _)| name == label)
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
fn opendir_readdir_and_releasedir_cost_exactly_this_at_any_size_and_beside_unrelated_rows() {
    let rig = Rig::new("directory-cost", 181, CACHE);
    let subjects = subjects(&rig);
    let first = pass(&rig, &subjects);
    for (label, cost) in &first {
        println!(
            "DIR_COST {label}: jobs={:?} grants={} transactions={} statements={:?} sql={:?}",
            cost.jobs,
            cost.grants,
            cost.transactions(),
            cost.statements(),
            cost.sql
        );
    }

    // Unrelated local rows: files created in the root and in another local
    // directory. No measured request reads or changes them.
    let other = rig.local_directory("unrelated", UNRELATED);
    for index in 0..UNRELATED {
        rig.mutate(
            rig.root,
            MutationInput::Named(create(
                rig.root,
                name(&format!("unrelated-{index:03}")),
                0o644,
            )),
        );
    }
    let _ = other;
    let second = pass(&rig, &subjects);
    assert_eq!(first, second, "beside {UNRELATED} unrelated local files");

    // Equal work at both sizes.
    for (one, other) in [
        ("opendir local small", "opendir local large"),
        ("opendir base small", "opendir base large"),
        ("releasedir local small", "releasedir local large"),
        ("releasedir local small", "releasedir base small"),
        ("releasedir local small", "releasedir base large"),
    ] {
        assert_eq!(of(&first, one), of(&first, other), "{one} and {other}");
    }
    let expected: [(&str, Cost); 13] = [
        ("opendir local small", OPENDIR_LOCAL.cost()),
        ("opendir base small", OPENDIR_BASE.cost()),
        (
            "readdir first local small",
            READDIR_FIRST_LOCAL_SMALL.cost(),
        ),
        (
            "readdir first local large",
            READDIR_FIRST_LOCAL_LARGE.cost(),
        ),
        ("readdir next local large", READDIR_NEXT_LOCAL.cost()),
        ("readdir again local large", READDIR_AGAIN_LOCAL.cost()),
        ("readdir end local small", READDIR_END.cost()),
        ("readdir first base small", READDIR_FIRST_BASE_SMALL.cost()),
        ("readdir first base large", READDIR_FIRST_BASE_LARGE.cost()),
        ("readdir next base large", READDIR_NEXT_BASE.cost()),
        ("readdir again base large", READDIR_AGAIN_BASE.cost()),
        ("readdir end base small", READDIR_END.cost()),
        ("releasedir local small", RELEASEDIR.cost()),
    ];
    for (label, wanted) in &expected {
        assert_eq!(of(&first, label), wanted, "{label}");
    }

    // The statements one more listed name costs: the difference between a
    // first reply of WINDOW names and one of SMALL names.
    for (kind, wanted) in [("local", PER_NAME_LOCAL), ("base", PER_NAME_BASE)] {
        let small = of(&first, &format!("readdir first {kind} small")).statements();
        let large = of(&first, &format!("readdir first {kind} large")).statements();
        let more = (WINDOW - SMALL) as u64;
        assert_eq!(
            (large.0 - small.0, large.1 - small.1),
            (wanted.0 * more, wanted.1 * more),
            "{kind}: {more} more names"
        );
    }

    // OPENDIR of a regular file is refused by kind and leaves nothing.
    let file = rig.lookup(rig.root, "file");
    let (refused, cost) = rig.measured(|| rig.try_opendir(file));
    assert_eq!(refused, Err(Refusal::NotDirectory));
    assert_eq!(cost.jobs, jobs(1, 0, 0));
    assert_eq!((cost.grants, cost.transactions()), (0, 0));
    rig.stop();
}

/// A pinned cost: jobs (Read, Lifecycle, Source), reader grants and the
/// statement families.
struct Pinned {
    jobs: (u64, u64, u64),
    grants: u64,
    sql: &'static [(&'static str, u64, u64)],
}
impl Pinned {
    fn cost(&self) -> Cost {
        let cost = Cost {
            jobs: jobs(self.jobs.0, self.jobs.1, self.jobs.2),
            grants: self.grants,
            sql: self.sql.to_vec(),
        };
        assert_eq!(cost.jobs[MUTATION], 0);
        cost
    }
}
const TRANSACTION: [(&str, u64, u64); 3] = [("Startup", 1, 1), ("Begin", 1, 1), ("Commit", 1, 1)];
/// OPENDIR of a local directory: one visit. The fence on the kernel's lookup
/// reference, the directory's row, and in its one transaction the parent it
/// was reached through, the handle row, its lease and the open count.
const OPENDIR_LOCAL: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 0,
    sql: &[
        TRANSACTION[0],
        TRANSACTION[1],
        TRANSACTION[2],
        ("Workspace", 1, 1),
        ("Inode", 1, 1),
        ("Lease", 4, 6),
    ],
};
/// OPENDIR of a base directory this daemon has seen: the same visit, decided
/// over the resident inode. No reader.
const OPENDIR_BASE: Pinned = Pinned {
    jobs: (1, 0, 0),
    grants: 0,
    sql: &[
        TRANSACTION[0],
        TRANSACTION[1],
        TRANSACTION[2],
        ("Workspace", 1, 1),
        ("Inode", 1, 1),
        ("Lease", 4, 6),
    ],
};
/// RELEASEDIR: one job. The fence on the open descriptor, then the handle
/// closed, its lease and open count released and its cookie retirement
/// queued, in one transaction.
const RELEASEDIR: Pinned = Pinned {
    jobs: (0, 1, 0),
    grants: 0,
    sql: &[
        TRANSACTION[0],
        TRANSACTION[1],
        TRANSACTION[2],
        ("Workspace", 1, 1),
        ("Lease", 4, 5),
        ("Reclaim", 1, 2),
    ],
};
/// READDIR, first reply of a local directory of SMALL names: six jobs (the
/// handle, the request source, the page, the cookie plan, its publication,
/// and the release after the reply), four transactions and one reader.
const READDIR_FIRST_LOCAL_SMALL: Pinned = Pinned {
    jobs: (3, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 25, 25),
        ("Inode", 11, 11),
        ("DirectoryEntry", 1, 1),
        ("Lease", 65, 84),
    ],
};
/// The same reply with WINDOW names.
const READDIR_FIRST_LOCAL_LARGE: Pinned = Pinned {
    jobs: (3, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 79, 79),
        ("Inode", 65, 65),
        ("DirectoryEntry", 1, 1),
        ("Lease", 227, 300),
    ],
};
/// A reply of WINDOW names from the middle: the offset's name is one more
/// statement in each of the four jobs that check the request's read.
const READDIR_NEXT_LOCAL: Pinned = Pinned {
    jobs: (3, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 79, 79),
        ("Inode", 65, 65),
        ("DirectoryEntry", 1, 1),
        ("Lease", 231, 304),
    ],
};
/// The same offset again: every name's offset exists and is not written.
const READDIR_AGAIN_LOCAL: Pinned = Pinned {
    jobs: (3, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 79, 79),
        ("Inode", 65, 65),
        ("DirectoryEntry", 1, 1),
        ("Lease", 166, 175),
    ],
};
/// READDIR after the last name: four jobs, two transactions and one reader.
const READDIR_END: Pinned = Pinned {
    jobs: (1, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 2, 2),
        ("Begin", 2, 2),
        ("Commit", 2, 2),
        ("Workspace", 11, 11),
        ("Inode", 1, 1),
        ("DirectoryEntry", 1, 1),
        ("Lease", 25, 34),
    ],
};
/// The replies of a base directory: its names have no local row.
const READDIR_FIRST_BASE_SMALL: Pinned = Pinned {
    jobs: (3, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 15, 15),
        ("Inode", 1, 1),
        ("DirectoryEntry", 1, 1),
        ("Lease", 55, 74),
    ],
};
const READDIR_FIRST_BASE_LARGE: Pinned = Pinned {
    jobs: (3, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 15, 15),
        ("Inode", 1, 1),
        ("DirectoryEntry", 1, 1),
        ("Lease", 163, 236),
    ],
};
const READDIR_NEXT_BASE: Pinned = Pinned {
    jobs: (3, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 15, 15),
        ("Inode", 1, 1),
        ("DirectoryEntry", 1, 1),
        ("Lease", 167, 240),
    ],
};
const READDIR_AGAIN_BASE: Pinned = Pinned {
    jobs: (3, 2, 1),
    grants: 1,
    sql: &[
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 15, 15),
        ("Inode", 1, 1),
        ("DirectoryEntry", 1, 1),
        ("Lease", 102, 111),
    ],
};
/// Statements (attempts, executions) one more listed name costs in a first
/// reply: a local name's kind and its offset's lookup and row; a base name's
/// offset alone.
const PER_NAME_LOCAL: (u64, u64) = (5, 6);
const PER_NAME_BASE: (u64, u64) = (2, 3);
