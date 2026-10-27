//! Replays kernel request sequences through the daemon's Fuse port and
//! attributes the owner's work to each kind of request: owner jobs by class,
//! statement attempts and executions, write transactions and VM steps from
//! the engine's own diagnostics, and the statements by template from the
//! connection observer of `statement_trace.rs`.
//!
//! The real owner and the real Store, without a kernel mount: each request is
//! the operation the kernel callback of that opcode runs, with fresh request
//! services, one after the other on the test's thread. Every wait is bounded.
use super::{
    statement_trace::{self, Trace},
    support,
};
use layerfs_content::filesystem::PathName;
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, Store}, KernelReply, Owner, OwnerClient, OwnerConfig, Response, ServiceClass,
};
use layerfs_fuse::{
    operations::{
        create::{create, mkdir},
        remove::unlink,
        DirectoryStep, DirectoryStream, MutationInput, MutationRequest, KernelData, KernelMutation,
        KernelRead, ReadDataInput,
    },
    ports::{Fence, MountServices, RequestServices},
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{DatabaseWork, KernelMount, ProfileConfig, StatementKind, StatementWork};
use layerfs_workspace::{KernelReadOperation, Operation, Refusal, Time};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt::Write as _,
    future::Future,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);
const CACHE: usize = 8 * 1024 * 1024;
pub const NOW: Time = Time {
    seconds: 1_700_000_000,
    nanoseconds: 0,
};
/// A later processing time, for a request that changes a time.
const LATER: Time = Time {
    seconds: 1_700_000_001,
    nanoseconds: 0,
};
/// The reply buffer the kernel offers one READDIR.
const READDIR_BUFFER: usize = 4096;

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
fn finish<T>(
    client: &OwnerClient,
    route: layerfs_workspace::Route,
    command: impl FnOnce(
        &layerfs_daemon::OwnerClient,
        layerfs_workspace::Route,
    ) -> Result<T, layerfs_daemon::OwnerError>,
) -> T {
    command(client, route).unwrap_or_else(|error| panic!("{error:?}"))
}
pub fn name(text: &str) -> PathName {
    PathName::new(text).unwrap()
}

/// The kinds of request a replay attributes work to.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Kind {
    LookupMiss,
    Lookup,
    Getattr,
    Create,
    Mkdir,
    Write,
    Setattr,
    Release,
    Unlink,
    Forget,
    Open,
    Read,
    Opendir,
    Readdir,
    Releasedir,
    /// The control plane's state job, once per measured command.
    Status,
}
/// What every request of one kind cost together.
#[derive(Default)]
pub struct Row {
    pub requests: u64,
    /// Completed owner jobs by class.
    pub jobs: [u64; 6],
    pub sql: DatabaseWork,
    /// Statement template: runs and VM steps.
    names: Shape,
    /// Requests the owner thread served: their templates are those of a
    /// request of the same kind and signature that this thread served.
    elsewhere: u64,
}
/// The statements of one request: template, runs and VM steps.
type Shape = BTreeMap<String, (u64, u64)>;
/// Attempts, executions and VM steps of each statement family.
type Signature = Vec<(u64, u64, u64)>;
#[derive(Default)]
struct Shapes {
    known: BTreeMap<(Kind, Signature), Shape>,
    elsewhere: Vec<(Kind, Signature)>,
}
impl Row {
    fn add(&mut self, shape: &Shape) {
        for (name, (runs, steps)) in shape {
            let entry = self.names.entry(name.clone()).or_default();
            entry.0 += runs;
            entry.1 += steps;
        }
    }
    pub fn owner_jobs(&self) -> u64 {
        self.jobs.iter().sum()
    }
    pub fn attempts(&self) -> u64 {
        self.sql.total().attempts
    }
    pub fn executions(&self) -> u64 {
        self.sql.total().executions
    }
    pub fn vm_steps(&self) -> u64 {
        self.sql.total().vm_steps
    }
    pub fn transactions(&self) -> u64 {
        self.sql.statements[StatementKind::Begin as usize].attempts
    }
}
#[derive(Clone, Copy)]
struct Snapshot {
    completed: [u64; 6],
    admitted: u64,
    foreground: DatabaseWork,
}

/// One bound Workspace over a Store and a kernel mount group. No kernel
/// connection and no dispatcher lane.
pub struct Rig {
    fixture: support::Fixture,
    store: Arc<Store>,
    owner: Owner,
    client: OwnerClient,
    bound: BoundWorkspace,
    pub root: u64,
    pub mount: KernelMount,
    next: Cell<u64>,
    trace: Arc<Trace>,
    table: RefCell<BTreeMap<Kind, Row>>,
    shapes: RefCell<Shapes>,
    /// Requests made while set are not attributed to any kind.
    muted: Cell<bool>,
}
impl Rig {
    /// `build` fills the committed base and returns its entry count.
    pub fn new(label: &str, authority: u8, build: impl FnOnce(&std::path::Path) -> usize) -> Self {
        let fixture = support::Fixture::built(label, build);
        let store = open_store(
            fixture.config.clone(),
            support::BINDING,
            support::CURSOR,
            1,
            CACHE,
            Default::default(),
        )
        .unwrap();
        let overlay = fixture.directory.join("overlay");
        // The one Workspace bound below has its own file beside the owner's.
        let suffix = format!(
            "{}/1.sqlite",
            fixture.directory.file_name().unwrap().to_str().unwrap()
        );
        let trace = statement_trace::watch(&suffix);
        let owner =
            Owner::start(&overlay, ProfileConfig::default(), OwnerConfig::default()).unwrap();
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
            |job_client: &layerfs_daemon::OwnerClient, job_route| {
                job_client.kernel_mount(job_route, root)
            },
        );
        let mount = done;
        drop(done);
        let rig = Self {
            fixture,
            store,
            owner,
            client,
            bound,
            root,
            mount,
            next: Cell::new(1),
            trace,
            table: RefCell::default(),
            shapes: RefCell::default(),
            muted: Cell::new(false),
        };
        rig.snapshot();
        assert!(
            !rig.trace.take().is_empty(),
            "the engine connection is not observed"
        );
        rig
    }
    fn services(&self) -> (Arc<dyn RequestServices>, u64) {
        let request = self.next.get();
        self.next.set(request + 1);
        (layerfs_daemon::mount_request(&self.bound, &Fence::default()).unwrap(), request)
    }
    fn snapshot(&self) -> Snapshot {
        let work = self.client.diagnostics().unwrap();
        Snapshot {
            completed: work.jobs,
            admitted: work.jobs.iter().sum::<u64>(),
            foreground: work.sql_foreground,
        }
    }
    /// One request, attributed to its kind. It must contain no scan, sort,
    /// automatic index or second preparation, and admit exactly the jobs it
    /// completed.
    fn observed<T>(&self, kind: Kind, request: impl FnOnce() -> T) -> T {
        if self.muted.get() {
            return request();
        }
        let before = self.snapshot();
        drop(self.trace.take());
        let value = request();
        let after = self.snapshot();
        let runs = self.trace.take();
        let work = after.foreground.since(&before.foreground);
        let total: StatementWork = work.total();
        assert_eq!(
            (
                total.fullscan_steps,
                total.sorts,
                total.autoindex_rows,
                total.reprepares
            ),
            (0, 0, 0, 0),
            "{kind:?}: {work:?}"
        );
        let mut table = self.table.borrow_mut();
        let row = table.entry(kind).or_default();
        row.requests += 1;
        let mut jobs = 0;
        for (class, count) in row.jobs.iter_mut().enumerate() {
            *count += after.completed[class] - before.completed[class];
            jobs += after.completed[class] - before.completed[class];
        }
        assert_eq!(after.admitted - before.admitted, jobs, "{kind:?}");
        row.sql.accumulate(work);
        // The engine's own counts of this request, family by family: two
        // requests of one kind with the same signature ran the same work.
        let signature: Signature = work
            .statements
            .iter()
            .map(|family| (family.attempts, family.executions, family.vm_steps))
            .collect();
        // A job runs every statement on its calling thread; maintenance
        // steps are counted apart from the foreground work.
        let mine: Vec<&statement_trace::Run> = runs.iter().filter(|run| run.own).collect();
        let mut shapes = self.shapes.borrow_mut();
        if mine.len() as u64 == total.attempts {
            let steps: u64 = mine.iter().map(|run| run.vm_steps).sum();
            assert_eq!(steps, total.vm_steps, "{kind:?}");
            let mut shape: Shape = BTreeMap::new();
            for run in mine {
                let entry = shape
                    .entry(statement_name(&self.trace.template(run.sql)))
                    .or_default();
                entry.0 += 1;
                entry.1 += run.vm_steps;
            }
            row.add(&shape);
            shapes.known.entry((kind, signature)).or_insert(shape);
        } else {
            // A maintenance step ran inside this request's window: the
            // templates are those of a request of the same kind and
            // signature, resolved at the end.
            shapes.elsewhere.push((kind, signature));
        }
        drop(shapes);
        value
    }
    fn read_visit(
        &self,
        kind: Kind,
        protected: u64,
        handle: Option<u64>,
        operation: KernelReadOperation,
    ) -> Result<(u64, Option<u64>, Option<u64>), Refusal> {
        self.observed(kind, || {
            let (services, request) = self.services();
            let found = wait(KernelRead::prepare(
                services, self.mount, request, protected, handle, operation,
            ))
            .unwrap_or_else(|failure| panic!("{kind:?}: {failure:?}"));
            let answer = found.value().map(|value| {
                (
                    value.stat.serial,
                    value.file.map(|file| file.owner_id()),
                    value.directory.map(|directory| directory.owner_id()),
                )
            });
            wait(found.dispose()).unwrap();
            answer
        })
    }
    /// LOOKUP of a name that does not exist.
    pub fn lookup_miss(&self, parent: u64, child: &str) {
        let operation = KernelReadOperation::Lookup {
            parent,
            name: name(child),
        };
        let answer = self.read_visit(Kind::LookupMiss, parent, None, operation);
        assert_eq!(answer, Err(Refusal::Missing), "{child}");
    }
    /// LOOKUP of an existing name: the kernel takes one more reference.
    pub fn lookup(&self, parent: u64, child: &str) -> u64 {
        let operation = KernelReadOperation::Lookup {
            parent,
            name: name(child),
        };
        self.read_visit(Kind::Lookup, parent, None, operation)
            .unwrap_or_else(|refusal| panic!("lookup {child}: {refusal:?}"))
            .0
    }
    pub fn getattr(&self, serial: u64) {
        let operation = KernelReadOperation::Getattr { serial };
        self.read_visit(Kind::Getattr, serial, None, operation)
            .unwrap_or_else(|refusal| panic!("getattr {serial}: {refusal:?}"));
    }
    pub fn open(&self, serial: u64, writable: bool) -> u64 {
        let operation = KernelReadOperation::Open { serial, writable };
        self.read_visit(Kind::Open, serial, None, operation)
            .unwrap_or_else(|refusal| panic!("open {serial}: {refusal:?}"))
            .1
            .expect("an opened file has a descriptor")
    }
    pub fn opendir(&self, serial: u64) -> u64 {
        let operation = KernelReadOperation::Opendir { serial };
        self.read_visit(Kind::Opendir, serial, None, operation)
            .unwrap_or_else(|refusal| panic!("opendir {serial}: {refusal:?}"))
            .2
            .expect("an opened directory has a descriptor")
    }
    fn mutate(
        &self,
        kind: Kind,
        protected: u64,
        handle: Option<u64>,
        input: MutationInput,
        open: Option<bool>,
        now: Time,
    ) -> (Option<u64>, Option<u64>) {
        self.observed(kind, || {
            let (services, request) = self.services();
            let done = wait(KernelMutation::perform(
                services,
                MutationRequest {
                    mount: self.mount,
                    request,
                    protected,
                    handle,
                    input,
                    open,
                    now,
                },
            ))
            .unwrap_or_else(|failure| panic!("{kind:?}: {failure:?}"));
            let published = done
                .value()
                .unwrap_or_else(|declined| panic!("{kind:?}: {declined:?}"));
            assert!(published.changed, "{kind:?}");
            let serial = published.stat.as_ref().map(|stat| stat.serial);
            let file = published.file.map(|file| file.owner_id());
            wait(done.replied()).unwrap();
            (serial, file)
        })
    }
    /// CREATE: the new file's serial and its writable descriptor.
    pub fn create(&self, parent: u64, child: &str) -> (u64, u64) {
        let input = MutationInput::Named(create(parent, name(child), 0o644));
        let (serial, file) = self.mutate(Kind::Create, parent, None, input, Some(true), NOW);
        (serial.unwrap(), file.expect("the created file is open"))
    }
    pub fn mkdir(&self, parent: u64, child: &str) -> u64 {
        let input = MutationInput::Named(mkdir(parent, name(child), 0o755));
        self.mutate(Kind::Mkdir, parent, None, input, None, NOW)
            .0
            .unwrap()
    }
    pub fn write(&self, serial: u64, handle: u64, offset: u64, bytes: &[u8]) {
        let input = MutationInput::Write {
            offset,
            data: bytes.into(),
            cached: false,
        };
        self.mutate(Kind::Write, serial, Some(handle), input, None, NOW);
    }
    /// SETATTR of the modification time alone, as `futimens(fd, NULL)` sends.
    pub fn touch(&self, serial: u64) {
        let input = MutationInput::Named(Operation::SetAttributes {
            serial,
            mode: None,
            mtime: Some(LATER),
            size: None,
        });
        self.mutate(Kind::Setattr, serial, None, input, None, LATER);
    }
    pub fn unlink(&self, parent: u64, child: &str) {
        let input = MutationInput::Named(unlink(parent, name(child)));
        self.mutate(Kind::Unlink, parent, None, input, None, NOW);
    }
    pub fn release(&self, serial: u64, handle: u64) {
        self.observed(Kind::Release, || {
            let (services, _) = self.services();
            drop(wait(services.close_file(self.mount, serial, handle)).unwrap());
        })
    }
    pub fn forget(&self, serial: u64, count: u64) {
        self.observed(Kind::Forget, || {
            let (services, _) = self.services();
            drop(wait(services.forget(self.mount, serial, count)).unwrap());
        })
    }
    /// READ of one window through a descriptor: the reply's length.
    pub fn read(&self, serial: u64, handle: u64, offset: u64, length: u32) -> usize {
        self.observed(Kind::Read, || {
            let (services, request) = self.services();
            let input = ReadDataInput::File { offset, length };
            wait(KernelData::read(
                services,
                self.mount,
                request,
                serial,
                Some(handle),
                input,
            ))
            .unwrap_or_else(|failure| panic!("read {serial} at {offset}: {failure:?}"))
            .unwrap_or_else(|refusal| panic!("read {serial} at {offset}: {refusal:?}"))
            .bytes()
            .len()
        })
    }
    /// One READDIR at `offset`, filled as the kernel request fills its reply.
    pub fn readdir(&self, serial: u64, handle: u64, offset: u64) -> Vec<(Vec<u8>, u64)> {
        let size = |name: &[u8]| (24 + name.len()).next_multiple_of(8);
        self.observed(Kind::Readdir, || {
            let (services, request) = self.services();
            let mut stream = wait(DirectoryStream::prepare(
                services, self.mount, request, serial, handle, offset,
            ))
            .unwrap_or_else(|failure| panic!("readdir {serial} at {offset}: {failure:?}"));
            let mut listed: Vec<(Vec<u8>, u64)> = stream
                .dots()
                .map(|(name, _, cookie)| (name.as_bytes().to_vec(), cookie))
                .collect();
            let mut used: usize = listed.iter().map(|(name, _)| size(name)).sum();
            loop {
                let batch = match wait(stream.next()).unwrap() {
                    DirectoryStep::End(_) => return listed,
                    DirectoryStep::Batch(batch) => batch,
                };
                let mut accepted = 0;
                for (entry, cookie) in batch.entries() {
                    if size(&entry.name) > READDIR_BUFFER - used {
                        break;
                    }
                    used += size(&entry.name);
                    listed.push((entry.name.clone(), cookie));
                    accepted += 1;
                }
                stream = wait(batch.accept(accepted)).unwrap();
            }
        })
    }
    pub fn releasedir(&self, serial: u64, handle: u64) {
        self.observed(Kind::Releasedir, || {
            let (services, _) = self.services();
            drop(wait(services.close_directory(self.mount, serial, handle)).unwrap());
        })
    }
    /// The state job the control plane runs once around a measured command.
    pub fn status(&self) {
        self.observed(Kind::Status, || {
            drop(finish(
                &self.client,
                self.bound.route(),
                |job_client: &layerfs_daemon::OwnerClient, job_route| job_client.state(job_route),
            ));
        })
    }
    /// Requests made before a measured command, attributed to no kind: what
    /// a mount's first requests leave in the daemon's memory of the base.
    pub fn unmeasured<T>(&self, requests: impl FnOnce() -> T) -> T {
        self.muted.set(true);
        let value = requests();
        self.muted.set(false);
        value
    }
    /// Waits, outside every measured request, until the owner's automatic
    /// maintenance has nothing due: the retirement a RELEASEDIR queued then
    /// runs between requests, as it does between a mount's requests, and no
    /// measured window holds one of its turns.
    pub fn settle(&self) {
        until("maintenance idle", || {
            let done = finish(
                &self.client,
                self.bound.route(),
                |job_client: &layerfs_daemon::OwnerClient, job_route| {
                    job_client.maintenance_idle(job_route)
                },
            );
            matches!(done.result(), Ok(Response::MaintenanceIdle(true)))
        });
    }
    /// The requests replayed so far, by kind, and nothing after it.
    pub fn table(&self) -> BTreeMap<Kind, Row> {
        let mut table = std::mem::take(&mut *self.table.borrow_mut());
        let shapes = std::mem::take(&mut *self.shapes.borrow_mut());
        for (kind, signature) in shapes.elsewhere {
            let row = table.get_mut(&kind).unwrap();
            let shape = shapes.known.get(&(kind, signature)).unwrap_or_else(|| {
                panic!("{kind:?}: no request of this shape ran its statements on the caller")
            });
            row.add(shape);
            row.elsewhere += 1;
        }
        table
    }
    pub fn stop(self) {
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
            |job_client: &layerfs_daemon::OwnerClient, job_route| {
                job_client.kernel_revoke(job_route, mount)
            },
        );
        assert!(
            matches!(done.result(), Ok(Response::Kernel(KernelReply::Done))),
            "revoke: {:?}",
            done.result()
        );
        drop(done);
        assert_eq!(store.read_work().outstanding, 0);
        drop(bound);
        owner.stop().unwrap();
        drop(store);
        fixture.cleanup();
    }
}

/// The counts of one replayed command.
pub struct Anatomy {
    pub rows: BTreeMap<Kind, Row>,
}
impl Anatomy {
    pub fn of(rig: &Rig) -> Self {
        Self { rows: rig.table() }
    }
    fn sum(&self, of: impl Fn(&Row) -> u64) -> u64 {
        self.rows.values().map(of).sum()
    }
    /// Kernel requests, owner jobs and statement executions: the three
    /// numbers a benchmark cell compares.
    pub fn headline(&self) -> (u64, u64, u64) {
        (
            self.sum(|row| row.requests)
                - self.rows.get(&Kind::Status).map_or(0, |row| row.requests),
            self.sum(Row::owner_jobs),
            self.sum(Row::executions),
        )
    }
    pub fn attempts(&self) -> u64 {
        self.sum(Row::attempts)
    }
    pub fn transactions(&self) -> u64 {
        self.sum(Row::transactions)
    }
    pub fn vm_steps(&self) -> u64 {
        self.sum(Row::vm_steps)
    }
    /// One line per kind and one per statement template of that kind: the
    /// exact counts a test pins. VM steps are not part of it.
    pub fn counts(&self) -> String {
        let mut text = String::new();
        for (kind, row) in &self.rows {
            let classes = [
                ServiceClass::Read,
                ServiceClass::Mutation,
                ServiceClass::Lifecycle,
            ]
            .map(|class| row.jobs[class as usize]);
            writeln!(
                text,
                "{kind:?}: requests={} jobs(read,mutation,lifecycle)={classes:?} transactions={} attempts={} executions={}",
                row.requests,
                row.transactions(),
                row.attempts(),
                row.executions(),
            )
            .unwrap();
            assert_eq!(row.owner_jobs(), classes.iter().sum::<u64>(), "{kind:?}");
            let named: u64 = row.names.values().map(|(runs, _)| runs).sum();
            assert_eq!(
                named,
                row.attempts(),
                "{kind:?}: a template for every statement"
            );
            for (name, (runs, _)) in &row.names {
                writeln!(text, "  {runs} {name}").unwrap();
            }
        }
        text
    }
    /// The same rows with what the pinned counts leave out: VM steps per
    /// kind and per template, and the families of each kind.
    pub fn report(&self, label: &str) -> String {
        let mut text = String::new();
        let (requests, jobs, executions) = self.headline();
        writeln!(
            text,
            "REQUEST_PATH {label}: requests={requests} owner_jobs={jobs} statements(executions)={executions} attempts={} transactions={} vm_steps={}",
            self.attempts(),
            self.transactions(),
            self.vm_steps()
        )
        .unwrap();
        for (kind, row) in &self.rows {
            writeln!(
                text,
                "REQUEST_PATH {label} {kind:?}: requests={} jobs={} transactions={} attempts={} executions={} vm_steps={} served_by_owner_thread={}",
                row.requests,
                row.owner_jobs(),
                row.transactions(),
                row.attempts(),
                row.executions(),
                row.vm_steps(),
                row.elsewhere,
            )
            .unwrap();
            let families: Vec<String> = FAMILIES
                .iter()
                .map(|(family, label)| (label, row.sql.statements[*family as usize]))
                .filter(|(_, work)| work.attempts != 0)
                .map(|(label, work)| {
                    format!(
                        "{label}={}/{}/{}",
                        work.attempts, work.executions, work.rows_changed
                    )
                })
                .collect();
            writeln!(
                text,
                "REQUEST_PATH {label} {kind:?}: families(attempts/executions/rows changed) {}",
                families.join(" ")
            )
            .unwrap();
            for (name, (runs, steps)) in &row.names {
                writeln!(
                    text,
                    "REQUEST_PATH {label} {kind:?}:   {runs} {name} vm={steps}"
                )
                .unwrap();
            }
        }
        text
    }
}

/// The name of a statement template: the engine's constant where it has one,
/// otherwise the statement's own text on one line.
fn statement_name(sql: &str) -> String {
    let flat = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    for (needle, name) in NAMES {
        if flat.contains(needle) {
            return (*name).to_owned();
        }
    }
    flat
}
/// A distinguishing part of each template and its name: the engine's
/// constant in `database/statements.rs` where it has one.
const NAMES: &[(&str, &str)] = &[
    ("BEGIN IMMEDIATE", "begin"),
    ("COMMIT", "commit"),
    ("PRAGMA main.freelist_count", "freelist-count"),
    // The fences that also read the named inode's row come first: each
    // holds the text of its plain fence.
    ("AND l.serial=?4),i.serial", "FENCE_LOOKUP_ROW"),
    (
        "l.owner,l.nlookup,l.implicit FROM workspace w",
        "FENCE_FORGET",
    ),
    ("SELECT ?3<=w.installed OR EXISTS", "LAYER_SUPERSEDED"),
    ("AND f.serial=?4),i.serial", "FENCE_FILE_ROW"),
    ("THEN 1 END),i.serial", "FENCE_HANDLE_ROW"),
    ("(SELECT 1 FROM kernel_lookup l", "FENCE_LOOKUP"),
    ("(SELECT f.writable FROM file_handle f", "FENCE_FILE"),
    (
        "(CASE WHEN EXISTS(SELECT 1 FROM file_handle f",
        "FENCE_HANDLE",
    ),
    ("(SELECT d.floor FROM kernel_directory d", "FENCE_DIRECTORY"),
    (
        "SELECT active,captured,revision,base_root",
        "workspace-state",
    ),
    ("SELECT incarnation FROM workspace", "workspace-incarnation"),
    ("UPDATE workspace SET revision=?2", "FRONTIER_ADVANCE"),
    (
        "FROM inode WHERE ns=?1 AND serial=?2 AND gen<=?3 AND gen>?4 ORDER BY gen DESC LIMIT 1",
        "INODE_LOOKUP",
    ),
    (
        "FROM inode WHERE ns=?1 AND serial=?2 AND gen<=?3 AND gen>?4 ORDER BY gen DESC LIMIT 4",
        "LAYERS",
    ),
    ("INSERT INTO inode VALUES", "INODE_INSERT"),
    ("UPDATE inode SET kind=?4", "INODE_UPDATE"),
    (
        "DELETE FROM inode WHERE ns=?1 AND serial=?2 AND gen=-1",
        "orphan-inode-drop",
    ),
    (
        "SELECT gen,serial,inherited FROM directory_entry",
        "NAME_LAYERS",
    ),
    (
        "FROM directory_entry e INDEXED BY directory_entry_capture",
        "SOURCE_NAMES_KINDS",
    ),
    ("INSERT INTO directory_entry VALUES", "DIRECTORY_ENTRY_PUT"),
    (
        "DELETE FROM directory_entry WHERE ns=?1 AND parent=?2 AND name=?3 AND gen=?4",
        "DIRECTORY_ENTRY_DROP",
    ),
    (
        "CASE WHEN cell_offset=?5 AND length(data)<=4096 THEN data END",
        "CELL_COVER",
    ),
    (
        "SELECT rowid,cell_offset,epoch,length(data),length(validity) FROM payload",
        "CELL_SHAPES",
    ),
    (
        "INSERT INTO payload(ns,serial,gen,cell_offset,epoch,data,validity)",
        "CELL_PUT",
    ),
    (
        "DELETE FROM payload WHERE ns=?1 AND serial=?2 AND gen=?3 AND cell_offset=?4",
        "CELL_DROP",
    ),
    (
        "FROM payload WHERE ns=?1 AND serial=?2 AND gen=-1 ORDER BY cell_offset LIMIT 14",
        "orphan-payload-page",
    ),
    (
        "FROM payload WHERE ns=?1 AND serial=?2 AND gen=?3 ORDER BY cell_offset LIMIT 14",
        "layer-payload-page",
    ),
    (
        "FROM shrink WHERE ns=?1 AND serial=?2 AND gen=-1",
        "orphan-shrink-page",
    ),
    (
        "FROM shrink WHERE ns=?1 AND serial=?2 AND gen=?3 ORDER BY depth LIMIT 64",
        "layer-shrink-page",
    ),
    (
        "INSERT INTO file_custody(ns,serial,opens,lookups)",
        "FILE_OPEN_LOOKUP_ADD",
    ),
    (
        "INSERT INTO file_custody(ns,serial,opens)",
        "FILE_OPENS_ADD",
    ),
    (
        "INSERT INTO file_custody(ns,serial,lookups)",
        "FILE_LOOKUPS_ADD",
    ),
    ("UPDATE file_custody SET opens=opens-1", "FILE_OPENS_DROP"),
    (
        "UPDATE file_custody SET lookups=lookups-1",
        "FILE_LOOKUPS_DROP",
    ),
    (
        "SELECT opens+lookups+readers,opens+readers FROM file_custody",
        "file-custody-refs",
    ),
    ("DELETE FROM file_custody", "file-custody-drop"),
    ("INSERT INTO file_handle", "file-handle-insert"),
    ("DELETE FROM file_handle", "file-handle-drop"),
    (
        "SELECT owner,root,revoked FROM kernel_mount",
        "kernel-mount-state",
    ),
    (
        "SELECT owner,nlookup,implicit FROM kernel_lookup",
        "kernel-lookup-row",
    ),
    ("INSERT INTO kernel_lookup", "kernel-lookup-insert"),
    ("UPDATE kernel_lookup SET nlookup", "kernel-lookup-count"),
    ("DELETE FROM kernel_lookup", "kernel-lookup-drop"),
    ("SELECT parent FROM kernel_parent", "kernel-parent"),
    ("INSERT INTO kernel_parent", "kernel-parent-put"),
    ("INSERT INTO kernel_directory", "kernel-directory-insert"),
    (
        "UPDATE kernel_directory SET closed=1",
        "kernel-directory-close",
    ),
    ("DELETE FROM kernel_directory", "kernel-directory-drop"),
    ("INSERT INTO kernel_cookie", "kernel-cookie-insert"),
    (
        "FROM kernel_cookie INDEXED BY kernel_cookie_after",
        "COOKIE_PAGE_AFTER",
    ),
    (
        "SELECT first_cookie,names FROM kernel_cookie",
        "COOKIE_PAGE",
    ),
    (
        "SELECT first_cookie,length(names) FROM kernel_cookie",
        "COOKIE_PAGES",
    ),
    (
        "SELECT base_root,lower_top,lower_floor FROM orphan",
        "ORPHAN_LOOKUP",
    ),
    ("INSERT INTO orphan VALUES", "orphan-insert"),
    ("UPDATE orphan SET lower_top", "orphan-lower-top"),
    ("DELETE FROM orphan_wait", "orphan-wait-drop"),
    ("DELETE FROM orphan WHERE", "orphan-drop"),
    ("SELECT orphan_rows FROM accounting", "orphan-rows"),
    (
        "SELECT 1 FROM lease INDEXED BY lease_resource",
        "GENERATION_HELD",
    ),
    ("INSERT INTO maintenance", "maintenance-enqueue"),
    ("DELETE FROM maintenance", "maintenance-drop"),
];
