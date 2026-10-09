//! Shared drive of the captured namespace producer over the direct harness:
//! paired model mutations, real open and lookup holds, one capture and
//! attempt, a recording, failing, interleaving and row-perturbing provider and
//! counting or rejecting consumers. Everything goes through the public API.
#![allow(dead_code)]
use crate::common::{name, Store};
use crate::harness::Bench;
use crate::oracle::{self, Model, Stamp, Walked};
use layerfs_content::object::inode_leaf::InodeKind as CanonicalKind;
use layerfs_content::{
    filesystem::{FilesystemRootId, FilesystemUpdateCounters, SymlinkTarget},
    ConstructionPolicy, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole,
};
use layerfs_overlay::{
    Capture, CapturedReader, CapturedRunCursor, CapturedRunReply, DirectoryEntry,
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope, Inode,
    InodeKind, LookupOwner, MaintenanceCursor, OpenFile, OperationOwner, Overlay, PAGE_ROWS,
    WRITE_WINDOW,
};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{
    CapturedNamespace, CapturedNamespaceAttempt, CapturedNamespaceCustody, CapturedNamespaceWork,
    Operation, OperationRecordApply, OperationRecordReply, Outcome, OverlayCapturedNamespace,
    OverlayCapturedRuns, OverlayOperationRecords, Position, Time, ViewStat, WorkspaceError,
    WorkspaceResult,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

/// The producer's record kinds in the operation's file scope 0.
pub const CONTEXT: u32 = 0x434E_0000;
pub const HEADER: u32 = 0x434E_0001;
pub const VALUE: u32 = 0x434E_0002;
pub const FRESH: u32 = 0x434E_0003;
pub const PRODUCER_KINDS: std::ops::RangeInclusive<u32> = 0x434E_0000..=0x434E_000F;
/// The three documented terminal labels of an attempt.
pub const ORIGINAL: &str = "captured namespace original refusal";
pub const FILE: &str = "captured namespace file refusal";
pub const BACKING: &str = "indexed edit backing original refusal";

/// Counted evidence goes to the process output directly, so the receipt of
/// the one bounded run holds it without a second, uncaptured run.
pub fn evidence(line: fmt::Arguments<'_>) {
    use std::io::Write;
    writeln!(std::io::stdout().lock(), "{line}").unwrap();
}
fn request() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// One sealed capture with its independently retained reader and operation.
#[derive(Clone, Copy, Debug)]
pub struct Taken {
    pub capture: Capture,
    pub reader: CapturedReader,
    pub operation: OperationOwner,
    pub reader_request: u64,
    pub operation_request: u64,
}
pub fn take(b: &Bench) -> Taken {
    let capture = b.overlay.capture(b.route()).unwrap();
    let reader_request = request();
    let reader = b
        .overlay
        .acquire_captured_reader(capture, reader_request)
        .unwrap();
    let operation_request = request();
    let operation = b
        .overlay
        .acquire_operation(b.route(), operation_request)
        .unwrap();
    Taken {
        capture,
        reader,
        operation,
        reader_request,
        operation_request,
    }
}
/// The one attempt, with the frozen default policy and its own capacities.
pub fn construct<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    b: &Bench,
    provider: &P,
    taken: &Taken,
    consumer: &mut dyn FinalizedConsumer,
) -> CapturedNamespaceAttempt {
    let policy = ConstructionPolicy::frozen_default();
    let producer = CapturedNamespace::new(&b.workspace, provider, taken.reader, taken.operation);
    Timing::disabled("captured-namespace", |timing| {
        Ok::<CapturedNamespaceAttempt, ContentError>(producer.construct(
            policy,
            &policy.capacities(),
            &b.fixture.store,
            consumer,
            timing.child("construct"),
        ))
    })
    .0
    .unwrap()
}
/// Both engine owners of this attempt are still exactly retained.
pub fn retained(b: &Bench, taken: &Taken) {
    assert_eq!(
        b.overlay
            .retained_captured_reader(b.route(), taken.reader_request)
            .unwrap(),
        Some(taken.reader),
        "the captured reader was released"
    );
    assert_eq!(
        b.overlay
            .retained_operation(b.route(), taken.operation_request)
            .unwrap(),
        Some(taken.operation),
        "the operation owner was released"
    );
}
/// One record of the namespace's own scope, read through the port.
pub fn record(b: &Bench, taken: &Taken, kind: u32, serial: u64) -> Option<Vec<u8>> {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&serial.to_be_bytes());
    OverlayOperationRecords::operation_record_get(
        &b.overlay,
        IndexedOperationRecordScope {
            owner: taken.operation,
            file_scope: 0,
        },
        IndexedOperationRecordKey { kind, key },
    )
    .unwrap()
    .value
}
/// The caller's real fences after every consumer of the custody has ended.
pub fn release(b: &Bench, taken: Taken, custody: CapturedNamespaceCustody) {
    assert_eq!(
        (custody.reader, custody.operation),
        (taken.reader, taken.operation)
    );
    retained(b, &taken);
    drop(custody);
    b.overlay.release_captured_reader(taken.reader).unwrap();
    b.overlay.release_operation(taken.operation).unwrap();
}
/// The current fact of one inode: its local row, or its base record restated
/// the way the Workspace restates it before a first local row exists.
pub fn fact(b: &Bench, serial: u64) -> Inode {
    if let Some(local) = b.overlay.inode(b.route(), serial).unwrap() {
        return local;
    }
    let stat = b.stat(serial).unwrap();
    let (kind, entries) = match stat.kind {
        CanonicalKind::RegularFile => (InodeKind::File, 0),
        CanonicalKind::Symlink => (InodeKind::Symlink, 0),
        CanonicalKind::Directory => (InodeKind::Directory, b.list(serial).0.len() as u64),
    };
    Inode {
        serial,
        kind,
        mode: u16::try_from(stat.metadata.mode).unwrap(),
        mtime_seconds: stat.metadata.mtime_seconds,
        mtime_nanoseconds: stat.metadata.mtime_nanoseconds,
        nlink: stat.namespace_refs,
        size: stat.logical_len,
        inherited_cutoff: stat.logical_len,
        born: 0,
        entries,
        subdirs: 0,
    }
}
/// A real writable descriptor of a file: the engine's own open custody.
pub fn hold(b: &Bench, serial: u64) -> OpenFile {
    let fact = fact(b, serial);
    b.window(|view| {
        b.overlay
            .open_file(view.source(), request(), &fact, true)
            .unwrap()
    })
}
/// A real kernel-lookup hold of any inode: no descriptor, only the lookup.
pub fn look(b: &Bench, serial: u64) -> LookupOwner {
    let fact = fact(b, serial);
    b.window(|view| {
        b.overlay
            .acquire_lookup(view.source(), request(), &fact, 1)
            .unwrap()
    })
}
/// Bounded maintenance until idle.
pub fn drain(b: &Bench) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..20_000 {
        let Some(step) = b.overlay.maintain(cursor).unwrap() else {
            return;
        };
        cursor = step.cursor;
    }
    panic!("maintenance stalled");
}

/// A successful attempt compared completely against the model.
pub struct Built {
    pub taken: Taken,
    pub root: FilesystemRootId,
    pub work: CapturedNamespaceWork,
    /// Content's own counted work of the canonical update.
    pub content: FilesystemUpdateCounters,
    pub custody: CapturedNamespaceCustody,
    pub walked: Walked,
}
impl Built {
    pub fn release(self, b: &Bench) {
        release(b, self.taken, self.custody);
    }
    /// The known local install of exactly this root, as a Commit would do
    /// after publication. The length authority learns the new file roots.
    pub fn install(self, b: &Bench) -> FilesystemRootId {
        {
            let mut lengths = b.fixture.store.lengths.lock().unwrap();
            for (root, length) in &self.walked.files {
                lengths.insert(*root, *length);
            }
        }
        let prepared = b
            .workspace
            .prepare_base_install(self.taken.capture, self.root)
            .unwrap();
        b.workspace
            .install_prepared_base(&b.overlay, prepared)
            .map_err(|(error, _)| error)
            .unwrap();
        assert_eq!(b.workspace.base().unwrap().identity(), self.root);
        release(b, self.taken, self.custody);
        self.root
    }
}
/// The attempt's root or a panic carrying its whole custody.
pub fn root_of(attempt: &CapturedNamespaceAttempt, what: &str) -> FilesystemRootId {
    match &attempt.result {
        Ok(result) => result.root,
        Err(error) => panic!("{what}: {error:?}; custody {:#?}", attempt.custody),
    }
}
pub fn clean(custody: &CapturedNamespaceCustody, what: &str) {
    assert!(
        custody.failure.is_none() && custody.file.is_none() && custody.records.failure.is_none(),
        "{what}: a successful attempt holds a failure: {custody:#?}"
    );
}
/// Capture, construct over `provider` and compare with `model`.
pub fn build_over<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    b: &Bench,
    provider: &P,
    model: &Model,
    what: &str,
) -> Built {
    let taken = take(b);
    build_taken(b, provider, taken, model, what)
}
pub fn build_taken<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    b: &Bench,
    provider: &P,
    taken: Taken,
    model: &Model,
    what: &str,
) -> Built {
    let attempt = construct(b, provider, &taken, &mut b.fixture.store.clone());
    let root = root_of(&attempt, what);
    let content = attempt
        .result
        .as_ref()
        .map(|result| result.counters)
        .unwrap();
    clean(&attempt.custody, what);
    assert_eq!(
        (attempt.custody.reader, attempt.custody.operation),
        (taken.reader, taken.operation)
    );
    retained(b, &taken);
    let walked = oracle::assert_tree(&b.fixture.store, root, model, what);
    Built {
        taken,
        root,
        work: attempt.custody.work,
        content,
        custody: attempt.custody,
        walked,
    }
}
pub fn build(b: &Bench, model: &Model, what: &str) -> Built {
    build_over(b, &b.overlay, model, what)
}

/// Every mutation applied to the live Workspace and to the model with one
/// clock. Paths are resolved through the live view, never through a capture.
pub struct Drive<'b> {
    pub b: &'b Bench,
    pub model: Model,
    clock: i64,
}
impl<'b> Drive<'b> {
    pub fn new(b: &'b Bench) -> Self {
        Self::with_model(b, Model::fixture(&b.fixture.bytes))
    }
    /// The same drive over another content-built root and its restated model.
    pub fn with_model(b: &'b Bench, model: Model) -> Self {
        Self {
            b,
            model,
            clock: 1_800_000_000,
        }
    }
    fn tick(&mut self) -> (Time, Stamp) {
        self.clock += 1;
        let stamp = (self.clock, 123_456_789);
        let now = Time {
            seconds: stamp.0,
            nanoseconds: stamp.1,
        };
        (now, stamp)
    }
    /// The time the next step will use.
    pub fn next_stamp(&self) -> Stamp {
        (self.clock + 1, 123_456_789)
    }
    pub fn serial(&self, path: &str) -> u64 {
        path.split('/')
            .filter(|step| !step.is_empty())
            .fold(1, |parent, child| {
                self.b
                    .lookup(parent, child)
                    .unwrap_or_else(|| panic!("live view: missing {path:?}"))
                    .serial
            })
    }
    fn at<'p>(&self, path: &'p str) -> (u64, &'p str) {
        match path.rsplit_once('/') {
            Some((parent, last)) => (self.serial(parent), last),
            None => (1, path),
        }
    }
    fn run(&self, operation: Operation, now: Time, changed: bool, what: &str) -> Option<ViewStat> {
        match self.b.run(operation, now) {
            Ok(Outcome::Applied { stat, .. }) => {
                assert!(changed, "{what}: published, but the model is unchanged");
                stat
            }
            Ok(Outcome::Unchanged { stat }) => {
                assert!(!changed, "{what}: unchanged, but the model changed");
                stat
            }
            Err(error) => panic!("{what}: {error:?}"),
        }
    }
    pub fn create(&mut self, path: &str, mode: u32) -> u64 {
        let (parent, last) = self.at(path);
        let (now, stamp) = self.tick();
        let changed = self.model.create(path, mode, stamp);
        let operation = Operation::Create {
            parent,
            name: name(last),
            mode,
        };
        self.run(operation, now, changed, path).unwrap().serial
    }
    pub fn mkdir(&mut self, path: &str, mode: u32) -> u64 {
        let (parent, last) = self.at(path);
        let (now, stamp) = self.tick();
        let changed = self.model.mkdir(path, mode, stamp);
        let operation = Operation::Mkdir {
            parent,
            name: name(last),
            mode,
        };
        self.run(operation, now, changed, path).unwrap().serial
    }
    pub fn symlink(&mut self, path: &str, target: &[u8]) -> u64 {
        let (parent, last) = self.at(path);
        let (now, stamp) = self.tick();
        let changed = self.model.symlink(path, target, stamp);
        let operation = Operation::Symlink {
            parent,
            name: name(last),
            target: SymlinkTarget::new(target.to_vec()).unwrap(),
        };
        self.run(operation, now, changed, path).unwrap().serial
    }
    pub fn link(&mut self, existing: &str, path: &str) {
        let serial = self.serial(existing);
        let (parent, last) = self.at(path);
        let (now, stamp) = self.tick();
        let changed = self.model.link(existing, path, stamp);
        let operation = Operation::Link {
            serial,
            parent,
            name: name(last),
        };
        self.run(operation, now, changed, path);
    }
    /// Unlink, or removal of an empty directory.
    pub fn remove(&mut self, path: &str) {
        let (parent, last) = self.at(path);
        let directory = self.model.kind(path) == oracle::Kind::Directory;
        let (now, stamp) = self.tick();
        let changed = self.model.remove(path, stamp);
        let operation = if directory {
            Operation::Rmdir {
                parent,
                name: name(last),
            }
        } else {
            Operation::Unlink {
                parent,
                name: name(last),
            }
        };
        self.run(operation, now, changed, path);
    }
    /// Replacing rename. Ancestry evidence accompanies a directory that moves
    /// to another parent, as the operation requires.
    pub fn rename(&mut self, from: &str, to: &str) {
        let (parent, last) = self.at(from);
        let (new_parent, new_last) = self.at(to);
        let evidence = (self.model.kind(from) == oracle::Kind::Directory && parent != new_parent)
            .then(|| {
                let steps = to.rsplit_once('/').map_or("", |(steps, _)| steps);
                steps
                    .split('/')
                    .filter(|step| !step.is_empty())
                    .map(name)
                    .collect()
            });
        let (now, stamp) = self.tick();
        let changed = self.model.rename(from, to, stamp);
        let operation = Operation::Rename {
            parent,
            name: name(last),
            new_parent,
            new_name: name(new_last),
            replace: true,
            destination_path: evidence,
        };
        self.run(operation, now, changed, from);
    }
    pub fn write(&mut self, path: &str, offset: u64, data: &[u8]) {
        assert!(data.len() <= WRITE_WINDOW, "one write window per step");
        let serial = self.serial(path);
        let (now, stamp) = self.tick();
        let changed = self.model.write(path, offset, data, stamp);
        let operation = Operation::Write {
            serial,
            position: Position::At(offset),
            data: data.into(),
        };
        self.run(operation, now, changed, path);
    }
    pub fn append(&mut self, path: &str, data: &[u8]) {
        assert!(data.len() <= WRITE_WINDOW, "one write window per step");
        let serial = self.serial(path);
        let (now, stamp) = self.tick();
        let changed = self.model.append(path, data, stamp);
        let operation = Operation::Write {
            serial,
            position: Position::End,
            data: data.into(),
        };
        self.run(operation, now, changed, path);
    }
    fn attributes(
        &mut self,
        path: &str,
        mode: Option<u32>,
        mtime: Option<Stamp>,
        size: Option<u64>,
    ) {
        let serial = self.serial(path);
        let (now, stamp) = self.tick();
        let mut changed = false;
        // The same order the operation applies its fields in.
        if let Some(mode) = mode {
            changed |= self.model.chmod(path, mode);
        }
        if let Some(size) = size {
            changed |= self.model.resize(path, size, stamp);
        }
        if let Some(mtime) = mtime {
            changed |= self.model.utimens(path, mtime);
        }
        let operation = Operation::SetAttributes {
            serial,
            mode,
            mtime: mtime.map(|(seconds, nanoseconds)| Time {
                seconds,
                nanoseconds,
            }),
            size,
        };
        self.run(operation, now, changed, path);
    }
    /// A descriptor write to a file that still has the name `path`.
    pub fn write_open(&mut self, file: OpenFile, path: &str, offset: u64, data: &[u8]) {
        assert!(data.len() <= WRITE_WINDOW, "one write window per step");
        assert_eq!(self.serial(path), file.serial());
        let (now, stamp) = self.tick();
        let changed = self.model.write(path, offset, data, stamp);
        let operation = Operation::WriteOpen {
            file,
            position: Position::At(offset),
            data: data.into(),
        };
        self.run(operation, now, changed, path);
    }
    /// A mapped store through a descriptor of the file named `path`.
    pub fn store_open(&mut self, file: OpenFile, path: &str, offset: u64, data: &[u8]) {
        assert!(data.len() <= WRITE_WINDOW, "one write window per step");
        assert_eq!(self.serial(path), file.serial());
        let (now, stamp) = self.tick();
        let changed = self.model.store(path, offset, data, stamp);
        let operation = Operation::StoreOpen {
            file,
            offset,
            data: data.into(),
        };
        self.run(operation, now, changed, path);
    }
    /// ftruncate through a descriptor of the file named `path`.
    pub fn resize_open(&mut self, file: OpenFile, path: &str, size: u64) {
        assert_eq!(self.serial(path), file.serial());
        let (now, stamp) = self.tick();
        let changed = self.model.resize(path, size, stamp);
        let operation = Operation::SetOpenAttributes {
            file,
            mode: None,
            mtime: None,
            size: Some(size),
        };
        self.run(operation, now, changed, path);
    }
    /// A descriptor write to a file no name reaches any more. It publishes,
    /// and the model, which holds only what a name reaches, does not change.
    pub fn orphan_write(&mut self, file: OpenFile, offset: u64, data: &[u8]) {
        let (now, _) = self.tick();
        let operation = Operation::WriteOpen {
            file,
            position: Position::At(offset),
            data: data.into(),
        };
        assert!(
            matches!(self.b.run(operation, now), Ok(Outcome::Applied { .. })),
            "a descriptor write to a held orphan publishes"
        );
    }
    pub fn chmod(&mut self, path: &str, mode: u32) {
        self.attributes(path, Some(mode), None, None);
    }
    pub fn utimens(&mut self, path: &str, mtime: Stamp) {
        self.attributes(path, None, Some(mtime), None);
    }
    pub fn resize(&mut self, path: &str, size: u64) {
        self.attributes(path, None, None, Some(size));
    }
    pub fn build(&self, what: &str) -> Built {
        build(self.b, &self.model, what)
    }
}

/// The provider calls a proof can count or fail.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Call {
    InodePage,
    InodePoint,
    NamePage,
    ParentPage,
    NamePoint,
    Symlink,
    RunStep,
    NamespaceApply,
    NamespaceGet,
    NamespaceContains,
    NamespaceKeys,
    FileApply,
    FileRead,
}
pub const CALLS: [Call; 13] = [
    Call::InodePage,
    Call::InodePoint,
    Call::NamePage,
    Call::ParentPage,
    Call::NamePoint,
    Call::Symlink,
    Call::RunStep,
    Call::NamespaceApply,
    Call::NamespaceGet,
    Call::NamespaceContains,
    Call::NamespaceKeys,
    Call::FileApply,
    Call::FileRead,
];
/// The one failure a proof injects, recognizable by downcast in custody.
#[derive(Debug)]
pub struct Injected(pub Call);
impl fmt::Display for Injected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Injected {}
pub fn injected(slot: &Option<WorkspaceError>) -> Option<Call> {
    match slot {
        Some(WorkspaceError::Service(original)) => {
            original.downcast_ref::<Injected>().map(|found| found.0)
        }
        _ => None,
    }
}
/// The custody slots that hold a failure. Exactly one after a failed attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Slot {
    Failure,
    Records,
    File,
}
pub fn slots(custody: &CapturedNamespaceCustody) -> Vec<Slot> {
    let mut held = Vec::new();
    if custody.failure.is_some() {
        held.push(Slot::Failure);
    }
    if custody.records.failure.is_some() {
        held.push(Slot::Records);
    }
    if custody.file.is_some() {
        held.push(Slot::File);
    }
    held
}

/// One page of the whole-capture name sequence.
pub struct NamePage {
    pub after: Option<(u64, Vec<u8>)>,
    pub rows: Vec<DirectoryEntry>,
}
/// One page of one parent's name sequence.
pub struct ParentPage {
    pub parent: u64,
    pub after: Option<Vec<u8>>,
    pub rows: Vec<DirectoryEntry>,
}
/// One name point and its answer.
pub struct Point {
    pub parent: u64,
    pub name: Vec<u8>,
    pub answer: Option<DirectoryEntry>,
}
/// One key window of the namespace's own record scope.
pub struct KeyWindow {
    pub kind: u32,
    pub after: Option<[u8; 32]>,
    pub keys: Vec<[u8; 32]>,
}
/// One guarded record job: its file scope and ordered keys, with its place
/// in the one global order of provider calls.
pub struct Applied {
    pub seq: u64,
    pub file_scope: u64,
    pub keys: Vec<IndexedOperationRecordKey>,
}
/// Everything the producer asked of its provider, in order.
#[derive(Default)]
pub struct Log {
    pub counts: BTreeMap<Call, u64>,
    /// The global sequence number of every call, in call order, per kind.
    pub order: BTreeMap<Call, Vec<u64>>,
    pub inode_pages: Vec<(u64, Vec<Inode>)>,
    pub inode_points: Vec<(u64, Option<Inode>)>,
    pub name_pages: Vec<NamePage>,
    pub parent_pages: Vec<ParentPage>,
    pub points: Vec<Point>,
    pub symlinks: Vec<u64>,
    pub applies: Vec<Applied>,
    pub key_windows: Vec<KeyWindow>,
    pub gets: Vec<IndexedOperationRecordKey>,
    pub largest_page: usize,
    pub failed: Option<Call>,
    pub after_failure: u64,
}
impl Log {
    pub fn count(&self, call: Call) -> u64 {
        self.counts.get(&call).copied().unwrap_or(0)
    }
    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }
    /// The global sequence numbers of the calls of one kind, in call order.
    pub fn sequence(&self, call: Call) -> &[u64] {
        self.order.get(&call).map_or(&[], Vec::as_slice)
    }
    /// The captured inode row of one serial, if the capture returned one.
    pub fn inode(&self, serial: u64) -> Option<Inode> {
        self.inode_pages
            .iter()
            .flat_map(|(_, rows)| rows)
            .find(|row| row.serial == serial)
            .cloned()
    }
    /// The whole-capture answer for one name: not a row, a whiteout, a serial.
    pub fn bound(&self, parent: u64, name: &str) -> Option<Option<u64>> {
        self.name_pages
            .iter()
            .flat_map(|page| &page.rows)
            .find(|row| row.parent == parent && row.name == name.as_bytes())
            .map(|row| row.serial)
    }
    /// Every parent with at least one row in the whole-capture name sequence.
    pub fn parents(&self) -> Vec<u64> {
        let mut parents: Vec<u64> = self
            .name_pages
            .iter()
            .flat_map(|page| &page.rows)
            .map(|row| row.parent)
            .collect();
        parents.dedup();
        parents
    }
    /// Calls only a file's own construction makes.
    pub fn file_calls(&self) -> u64 {
        self.count(Call::RunStep) + self.count(Call::FileApply) + self.count(Call::FileRead)
    }
    /// Guarded record jobs under one file's own scope.
    pub fn scoped(&self, serial: u64) -> usize {
        self.applies
            .iter()
            .filter(|job| job.file_scope == serial)
            .count()
    }
    /// The sequence number of the job that sealed the namespace's context:
    /// the second job of its own scope that writes the context key.
    pub fn sealed_at(&self) -> Option<u64> {
        self.applies
            .iter()
            .filter(|job| job.file_scope == 0 && job.keys.iter().any(|key| key.kind == CONTEXT))
            .nth(1)
            .map(|job| job.seq)
    }
    /// Where each header, value and fresh rank of the producer was applied,
    /// by kind and serial. Content's own update records share the scope under
    /// other kinds and are not the producer's rows. A producer key applied
    /// twice is a failure of the proof.
    pub fn applied(&self) -> BTreeMap<(u32, u64), u64> {
        let mut all = BTreeMap::new();
        for job in self.applies.iter().filter(|job| job.file_scope == 0) {
            let own = |key: &&IndexedOperationRecordKey| {
                PRODUCER_KINDS.contains(&key.kind) && key.kind != CONTEXT
            };
            for key in job.keys.iter().filter(own) {
                let serial = u64::from_be_bytes(key.key[24..].try_into().unwrap());
                assert!(
                    all.insert((key.kind, serial), job.seq).is_none(),
                    "kind {:#x} serial {serial} was applied twice",
                    key.kind
                );
            }
        }
        all
    }
}
/// One real mutation run immediately before the selected provider call.
type Hook<'a> = (Call, u64, Box<dyn FnOnce() + 'a>);
/// Wraps the direct Overlay: records every row and answer with one global
/// sequence number per call, fails exactly the selected call once, and runs a
/// selected real mutation before a selected call. It never repeats a call.
pub struct Recording<'a> {
    overlay: &'a Overlay,
    pub log: RefCell<Log>,
    fault: Cell<Option<(Call, u64)>>,
    /// Set by a rejecting consumer: later calls count as after the failure.
    halted: Rc<Cell<bool>>,
    /// The number of provider calls made so far, shared with a consumer.
    clock: Rc<Cell<u64>>,
    hooks: RefCell<Vec<Hook<'a>>>,
}
impl<'a> Recording<'a> {
    pub fn new(overlay: &'a Overlay) -> Self {
        Self {
            overlay,
            log: RefCell::new(Log::default()),
            fault: Cell::new(None),
            halted: Rc::new(Cell::new(false)),
            clock: Rc::new(Cell::new(0)),
            hooks: RefCell::new(Vec::new()),
        }
    }
    /// Runs `mutation` once, immediately before the call of this kind with
    /// zero-based index `nth` reaches the engine.
    pub fn before(&self, call: Call, nth: u64, mutation: impl FnOnce() + 'a) {
        self.hooks
            .borrow_mut()
            .push((call, nth, Box::new(mutation)));
    }
    /// Hooks that never ran: the selected call was never made.
    pub fn pending_hooks(&self) -> usize {
        self.hooks.borrow().len()
    }
    /// Fails the call of this kind with zero-based index `nth`.
    pub fn failing(overlay: &'a Overlay, call: Call, nth: u64) -> Self {
        let recording = Self::new(overlay);
        recording.fault.set(Some((call, nth)));
        recording
    }
    /// Counts and orders one call; returns its global sequence number.
    fn enter(&self, call: Call) -> WorkspaceResult<u64> {
        let index = self.log.borrow().count(call);
        let hook = {
            let mut hooks = self.hooks.borrow_mut();
            hooks
                .iter()
                .position(|hook| hook.0 == call && hook.1 == index)
                .map(|found| hooks.remove(found).2)
        };
        if let Some(mutation) = hook {
            mutation();
        }
        let seq = self.clock.get() + 1;
        self.clock.set(seq);
        let mut log = self.log.borrow_mut();
        if log.failed.is_some() || self.halted.get() {
            log.after_failure += 1;
        }
        *log.counts.entry(call).or_default() += 1;
        log.order.entry(call).or_default().push(seq);
        match self.fault.get() {
            Some((wanted, 0)) if wanted == call => {
                self.fault.set(None);
                log.failed = Some(call);
                Err(WorkspaceError::Service(Box::new(Injected(call))))
            }
            Some((wanted, left)) if wanted == call => {
                self.fault.set(Some((wanted, left - 1)));
                Ok(seq)
            }
            _ => Ok(seq),
        }
    }
    fn page(&self, rows: usize) {
        let mut log = self.log.borrow_mut();
        log.largest_page = log.largest_page.max(rows);
    }
}
impl OverlayCapturedRuns for Recording<'_> {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<Inode>> {
        self.enter(Call::InodePoint)?;
        let answer = OverlayCapturedRuns::captured_inode(self.overlay, reader, serial)?;
        self.log
            .borrow_mut()
            .inode_points
            .push((serial, answer.clone()));
        Ok(answer)
    }
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply> {
        self.enter(Call::RunStep)?;
        OverlayCapturedRuns::captured_run_step(self.overlay, cursor)
    }
}
impl OverlayCapturedNamespace for Recording<'_> {
    fn captured_inode_page(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> WorkspaceResult<Vec<Inode>> {
        self.enter(Call::InodePage)?;
        let rows = OverlayCapturedNamespace::captured_inode_page(self.overlay, reader, after)?;
        self.page(rows.len());
        self.log
            .borrow_mut()
            .inode_pages
            .push((after, rows.clone()));
        Ok(rows)
    }
    fn captured_directory_entry_page(
        &self,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.enter(Call::NamePage)?;
        let rows = OverlayCapturedNamespace::captured_directory_entry_page(
            self.overlay,
            reader,
            after.clone(),
        )?;
        self.page(rows.len());
        self.log.borrow_mut().name_pages.push(NamePage {
            after,
            rows: rows.clone(),
        });
        Ok(rows)
    }
    fn captured_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.enter(Call::ParentPage)?;
        let rows = OverlayCapturedNamespace::captured_directory_entries(
            self.overlay,
            reader,
            parent,
            after.clone(),
        )?;
        self.page(rows.len());
        self.log.borrow_mut().parent_pages.push(ParentPage {
            parent,
            after,
            rows: rows.clone(),
        });
        Ok(rows)
    }
    fn captured_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        self.enter(Call::NamePoint)?;
        let answer =
            OverlayCapturedNamespace::captured_directory_entry(self.overlay, reader, parent, name)?;
        self.log.borrow_mut().points.push(Point {
            parent,
            name: name.to_vec(),
            answer: answer.clone(),
        });
        Ok(answer)
    }
    fn captured_symlink(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<Vec<u8>> {
        self.enter(Call::Symlink)?;
        self.log.borrow_mut().symlinks.push(serial);
        OverlayCapturedNamespace::captured_symlink(self.overlay, reader, serial)
    }
}
fn own(scope: IndexedOperationRecordScope) -> bool {
    scope.file_scope == 0
}
impl OverlayOperationRecords for Recording<'_> {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>> {
        self.enter(if own(scope) {
            Call::NamespaceContains
        } else {
            Call::FileRead
        })?;
        OverlayOperationRecords::operation_record_contains(self.overlay, scope, key)
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>> {
        self.enter(if own(scope) {
            Call::NamespaceGet
        } else {
            Call::FileRead
        })?;
        if own(scope) {
            self.log.borrow_mut().gets.push(key);
        }
        OverlayOperationRecords::operation_record_get(self.overlay, scope, key)
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>> {
        let seq = self.enter(if own(scope) {
            Call::NamespaceApply
        } else {
            Call::FileApply
        })?;
        self.log.borrow_mut().applies.push(Applied {
            seq,
            file_scope: scope.file_scope,
            keys: changes.iter().map(|change| change.key).collect(),
        });
        OverlayOperationRecords::operation_record_apply(self.overlay, scope, changes)
    }
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.enter(if own(scope) {
            Call::NamespaceKeys
        } else {
            Call::FileRead
        })?;
        let reply =
            OverlayOperationRecords::operation_record_keys(self.overlay, scope, kind, excluded)?;
        self.page(reply.value.len());
        Ok(reply)
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.enter(if own(scope) {
            Call::NamespaceKeys
        } else {
            Call::FileRead
        })?;
        let reply =
            OverlayOperationRecords::operation_record_keys_after(self.overlay, scope, kind, after)?;
        self.page(reply.value.len());
        if own(scope) {
            self.log.borrow_mut().key_windows.push(KeyWindow {
                kind,
                after,
                keys: reply.value.clone(),
            });
        }
        Ok(reply)
    }
}

/// Changes rows, not calls: name rows a Workspace never writes are added to
/// what the capture really holds, consistently in the whole-capture sequence,
/// in the parent's own sequence and in the name point. An injected row
/// replaces the real row of the same parent and name. Every other call goes
/// to the recording provider unchanged, and the log holds what was answered.
pub struct Perturbed<'a> {
    pub inner: Recording<'a>,
    injected: Vec<DirectoryEntry>,
}
impl<'a> Perturbed<'a> {
    pub fn new(overlay: &'a Overlay, injected: Vec<DirectoryEntry>) -> Self {
        Self {
            inner: Recording::new(overlay),
            injected,
        }
    }
    /// The capture's real name rows with the injected rows, in key order.
    /// The real sequence is read to its end with a cursor that must advance.
    fn merged(&self, reader: CapturedReader) -> WorkspaceResult<Vec<DirectoryEntry>> {
        let mut rows = BTreeMap::<(u64, Vec<u8>), DirectoryEntry>::new();
        let mut after: Option<(u64, Vec<u8>)> = None;
        loop {
            let page = OverlayCapturedNamespace::captured_directory_entry_page(
                self.inner.overlay,
                reader,
                after.clone(),
            )?;
            let short = page.len() < PAGE_ROWS;
            for row in page {
                let key = (row.parent, row.name.clone());
                assert!(
                    after.as_ref().is_none_or(|last| *last < key),
                    "the real name sequence did not advance"
                );
                after = Some(key.clone());
                rows.insert(key, row);
            }
            if short {
                break;
            }
        }
        for row in &self.injected {
            rows.insert((row.parent, row.name.clone()), row.clone());
        }
        Ok(rows.into_values().collect())
    }
}
impl OverlayCapturedRuns for Perturbed<'_> {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<Inode>> {
        self.inner.captured_inode(reader, serial)
    }
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply> {
        self.inner.captured_run_step(cursor)
    }
}
impl OverlayCapturedNamespace for Perturbed<'_> {
    fn captured_inode_page(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> WorkspaceResult<Vec<Inode>> {
        self.inner.captured_inode_page(reader, after)
    }
    fn captured_directory_entry_page(
        &self,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.inner.enter(Call::NamePage)?;
        let rows: Vec<DirectoryEntry> = self
            .merged(reader)?
            .into_iter()
            .filter(|row| {
                after
                    .as_ref()
                    .is_none_or(|last| (last.0, last.1.as_slice()) < (row.parent, &row.name[..]))
            })
            .take(PAGE_ROWS)
            .collect();
        self.inner.page(rows.len());
        self.inner.log.borrow_mut().name_pages.push(NamePage {
            after,
            rows: rows.clone(),
        });
        Ok(rows)
    }
    fn captured_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.inner.enter(Call::ParentPage)?;
        let rows: Vec<DirectoryEntry> = self
            .merged(reader)?
            .into_iter()
            .filter(|row| row.parent == parent)
            .filter(|row| after.as_ref().is_none_or(|last| *last < row.name))
            .take(PAGE_ROWS)
            .collect();
        self.inner.page(rows.len());
        self.inner.log.borrow_mut().parent_pages.push(ParentPage {
            parent,
            after,
            rows: rows.clone(),
        });
        Ok(rows)
    }
    fn captured_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        self.inner.enter(Call::NamePoint)?;
        let answer = self
            .merged(reader)?
            .into_iter()
            .find(|row| row.parent == parent && row.name == name);
        self.inner.log.borrow_mut().points.push(Point {
            parent,
            name: name.to_vec(),
            answer: answer.clone(),
        });
        Ok(answer)
    }
    fn captured_symlink(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<Vec<u8>> {
        self.inner.captured_symlink(reader, serial)
    }
}
impl OverlayOperationRecords for Perturbed<'_> {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>> {
        self.inner.operation_record_contains(scope, key)
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>> {
        self.inner.operation_record_get(scope, key)
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>> {
        self.inner.operation_record_apply(scope, changes)
    }
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.inner.operation_record_keys(scope, kind, excluded)
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.inner.operation_record_keys_after(scope, kind, after)
    }
}

/// Accepts everything and records each offered object's role with the number
/// of provider calls made before it, so an offer can be placed in the order.
pub struct Counting {
    pub store: Store,
    pub offered: Vec<(ObjectRole, u64)>,
    clock: Rc<Cell<u64>>,
}
impl Counting {
    pub fn new(store: &Store, provider: &Recording<'_>) -> Self {
        Self {
            store: store.clone(),
            offered: Vec::new(),
            clock: provider.clock.clone(),
        }
    }
    pub fn of(&self, role: ObjectRole) -> usize {
        self.offered.iter().filter(|row| row.0 == role).count()
    }
    /// Objects offered after the provider call with this sequence number.
    pub fn after(&self, seq: u64) -> usize {
        self.offered.iter().filter(|row| row.1 >= seq).count()
    }
}
impl FinalizedConsumer for Counting {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.offered.push((object.role(), self.clock.get()));
        self.store.accept(object)
    }
}

/// What a rejecting consumer refuses: every object of one role, or exactly
/// one object by identity.
#[derive(Clone, Copy, Debug)]
pub enum Refused {
    Role(ObjectRole),
    Object(ObjectId),
}
/// Accepts everything else, and counts what follows a rejection.
pub struct Reject {
    pub store: Store,
    pub refused: Refused,
    pub accepted: u64,
    pub rejected: u64,
    pub after_rejection: u64,
    halted: Rc<Cell<bool>>,
}
impl Reject {
    /// The provider learns of the rejection, so every provider call that
    /// follows it is counted in its log as after the failure.
    pub fn new(store: &Store, refused: Refused, provider: &Recording<'_>) -> Self {
        Self {
            store: store.clone(),
            refused,
            accepted: 0,
            rejected: 0,
            after_rejection: 0,
            halted: provider.halted.clone(),
        }
    }
}
impl FinalizedConsumer for Reject {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if self.rejected != 0 {
            self.after_rejection += 1;
        }
        let refused = match self.refused {
            Refused::Role(role) => object.role() == role,
            Refused::Object(id) => object.id() == id,
        };
        if refused {
            self.halted.set(true);
            self.rejected += 1;
            return Err(ContentError::OutputRejected);
        }
        self.accepted += 1;
        self.store.accept(object)
    }
}
