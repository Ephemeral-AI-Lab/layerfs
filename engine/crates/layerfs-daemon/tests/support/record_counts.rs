//! A counting pass-through of the constructor-thread ports of one Commit.
//! Every call is forwarded unchanged to the real `OwnerClient`, so each
//! counted call is one original owner job; nothing is cached or answered here.
use layerfs_daemon::OwnerClient;
use layerfs_overlay::{
    CapturedReader, CapturedRunCursor, CapturedRunReply, DirectoryEntry,
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope, Inode,
};
use layerfs_workspace::{
    OperationRecordApply, OperationRecordReply, OverlayCapturedNamespace, OverlayCapturedRuns,
    OverlayOperationRecords, WorkspaceResult,
};
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

/// The six calls of the record port.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Call {
    Contains,
    Get,
    GetMany,
    Apply,
    FirstKeys,
    KeysAfter,
}
impl Call {
    pub const ALL: [Call; 6] = [
        Call::Contains,
        Call::Get,
        Call::GetMany,
        Call::Apply,
        Call::FirstKeys,
        Call::KeysAfter,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Call::Contains => "contains",
            Call::Get => "get",
            Call::GetMany => "get_many",
            Call::Apply => "apply",
            Call::FirstKeys => "first_keys",
            Call::KeysAfter => "keys_after",
        }
    }
}

/// Who owns a record kind. Scope 0 holds the producer's and Content's
/// filesystem kinds; every other scope is one changed file's.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Owner {
    /// Workspace `CapturedNamespace`: context, headers, values, fresh ranks.
    Producer,
    /// Content's streamed filesystem update over the namespace scope.
    Namespace,
    /// Workspace `CapturedFileEdits`: a file's context and normalized edits.
    FilePlan,
    /// Content's backed file edit engine over a stored file.
    EditEngine,
}
pub fn owner(file_scope: u64, kind: u32) -> Owner {
    match (file_scope, kind >> 16) {
        (0, 0x434E) => Owner::Producer,
        (0, _) => Owner::Namespace,
        (_, 0x4346) => Owner::FilePlan,
        _ => Owner::EditEngine,
    }
}
/// The source name of a record kind, for the printed table only.
pub fn kind_name(file_scope: u64, kind: u32) -> String {
    let name = match (owner(file_scope, kind), kind & 0xFFFF) {
        (Owner::Producer, 0) => "context",
        (Owner::Producer, 1) => "header",
        (Owner::Producer, 2) => "value",
        (Owner::Producer, 3) => "fresh",
        (Owner::Namespace, 0) => "context",
        (Owner::Namespace, 1) => "parent",
        (Owner::Namespace, 2) => "root",
        (Owner::Namespace, 3) => "count",
        (Owner::Namespace, 4) => "fresh",
        (Owner::Namespace, 5) => "row",
        (Owner::Namespace, 6) => "touch",
        (Owner::Namespace, 7) => "work",
        (Owner::Namespace, 8) => "frame",
        (Owner::Namespace, 9) => "node",
        (Owner::Namespace, 0x10) => "qualify_context",
        (Owner::Namespace, 0x11) => "qualify_inode",
        (Owner::Namespace, 0x12) => "qualify_queue",
        (Owner::Namespace, 0x20) => "topology_placed",
        (Owner::Namespace, 0x21) => "topology_territory",
        (Owner::Namespace, 0x22) => "topology_rooted",
        (Owner::Namespace, 0x23) => "topology_queue",
        (Owner::Namespace, 0x24) => "topology_scanned",
        (Owner::FilePlan, 0) => "context",
        (Owner::FilePlan, 1) => "edits",
        (Owner::EditEngine, 1) => "node_draft",
        (Owner::EditEngine, 2) => "page_draft",
        (Owner::EditEngine, 3) => "node_refs",
        (Owner::EditEngine, 4) => "page_refs",
        (Owner::EditEngine, 5) => "node_detached",
        (Owner::EditEngine, 6) => "page_detached",
        (Owner::EditEngine, 7) => "node_resolved",
        (Owner::EditEngine, 8) => "page_resolved",
        (Owner::EditEngine, 9) => "emitted",
        (Owner::EditEngine, 10) => "scope",
        _ => return format!("{kind:#x}"),
    };
    name.to_string()
}

/// What one Commit's constructor asked of the owner, as it asked.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    /// Jobs by file scope, call and record kind. An apply is filed under the
    /// lowest kind it changes; `mixed_applies` counts those with several.
    pub records: BTreeMap<(u64, Call, u32), u64>,
    /// Changes carried by the applies of one file scope and lowest kind.
    pub changes: BTreeMap<(u64, u32), u64>,
    /// Applies by the number of changes they carried.
    pub apply_sizes: BTreeMap<usize, u64>,
    pub mixed_applies: u64,
    /// Key windows by the number of keys returned.
    pub window_sizes: BTreeMap<usize, u64>,
    /// Point reads that found no record.
    pub absent_gets: u64,
    /// Captured reader jobs by port method.
    pub reads: BTreeMap<&'static str, u64>,
    /// Wall of the constructor thread inside record port calls and inside
    /// captured reader port calls: submit, the owner's service and the wake
    /// back. Calls are counted here whether or not the split above is kept.
    pub record_calls: u64,
    pub record_ns: u64,
    pub read_calls: u64,
    pub read_ns: u64,
}
impl Seen {
    pub fn record_jobs(&self) -> u64 {
        self.records.values().sum()
    }
    pub fn read_jobs(&self) -> u64 {
        self.reads.values().sum()
    }
    pub fn by_owner(&self) -> BTreeMap<Owner, u64> {
        let mut total = BTreeMap::new();
        for ((scope, _, kind), jobs) in &self.records {
            *total.entry(owner(*scope, *kind)).or_default() += jobs;
        }
        total
    }
    pub fn by_call(&self) -> BTreeMap<Call, u64> {
        let mut total = BTreeMap::new();
        for ((_, call, _), jobs) in &self.records {
            *total.entry(*call).or_default() += jobs;
        }
        total
    }
    /// Jobs of one owner by call and kind name, file scopes summed.
    pub fn of(&self, wanted: Owner) -> BTreeMap<(String, Call), u64> {
        let mut total = BTreeMap::new();
        for ((scope, call, kind), jobs) in &self.records {
            if owner(*scope, *kind) == wanted {
                *total.entry((kind_name(*scope, *kind), *call)).or_default() += jobs;
            }
        }
        total
    }
    /// Each changed file's scope: (plan jobs, edit-engine jobs).
    pub fn files(&self) -> BTreeMap<u64, (u64, u64)> {
        let mut files = BTreeMap::<u64, (u64, u64)>::new();
        for ((scope, _, kind), jobs) in &self.records {
            if *scope == 0 {
                continue;
            }
            let file = files.entry(*scope).or_default();
            match owner(*scope, *kind) {
                Owner::EditEngine => file.1 += jobs,
                _ => file.0 += jobs,
            }
        }
        files
    }
    /// One file scope's jobs by call and kind name.
    pub fn file(&self, wanted: u64) -> BTreeMap<(String, Call), u64> {
        let mut total = BTreeMap::new();
        for ((scope, call, kind), jobs) in &self.records {
            if *scope == wanted {
                let name = format!("{:?}.{}", owner(*scope, *kind), kind_name(*scope, *kind));
                *total.entry((name, *call)).or_default() += jobs;
            }
        }
        total
    }
}

pub struct Counting<'a> {
    inner: &'a OwnerClient,
    seen: RefCell<Seen>,
    /// Whether the split by scope, kind and call is kept, or only the clocks.
    split: bool,
}
impl<'a> Counting<'a> {
    pub fn new(inner: &'a OwnerClient) -> Self {
        Self {
            inner,
            seen: RefCell::new(Seen::default()),
            split: true,
        }
    }
    /// Two clocks and two call counts only, for a loop that is timed.
    pub fn clocks(inner: &'a OwnerClient) -> Self {
        Self {
            split: false,
            ..Self::new(inner)
        }
    }
    pub fn into_seen(self) -> Seen {
        self.seen.into_inner()
    }
    fn record(&self, scope: IndexedOperationRecordScope, call: Call, kind: u32) {
        if self.split {
            *self
                .seen
                .borrow_mut()
                .records
                .entry((scope.file_scope, call, kind))
                .or_default() += 1;
        }
    }
    fn read(&self, name: &'static str) {
        if self.split {
            *self.seen.borrow_mut().reads.entry(name).or_default() += 1;
        }
    }
    fn record_call<T>(&self, call: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let value = call();
        let elapsed = start.elapsed().as_nanos() as u64;
        let mut seen = self.seen.borrow_mut();
        seen.record_calls += 1;
        seen.record_ns += elapsed;
        value
    }
    fn read_call<T>(&self, call: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let value = call();
        let elapsed = start.elapsed().as_nanos() as u64;
        let mut seen = self.seen.borrow_mut();
        seen.read_calls += 1;
        seen.read_ns += elapsed;
        value
    }
    fn window(&self, keys: &WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>>) {
        if !self.split {
            return;
        }
        if let Ok(reply) = keys {
            *self
                .seen
                .borrow_mut()
                .window_sizes
                .entry(reply.value.len())
                .or_default() += 1;
        }
    }
}
impl OverlayOperationRecords for Counting<'_> {
    fn operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<bool>> {
        self.record(scope, Call::Contains, key.kind);
        self.record_call(|| self.inner.operation_record_contains(scope, key))
    }
    fn operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> WorkspaceResult<OperationRecordReply<Option<Vec<u8>>>> {
        self.record(scope, Call::Get, key.kind);
        let reply = self.record_call(|| self.inner.operation_record_get(scope, key));
        if self.split && matches!(&reply, Ok(reply) if reply.value.is_none()) {
            self.seen.borrow_mut().absent_gets += 1;
        }
        reply
    }
    /// One job, recorded under the kind of its first key.
    fn operation_record_get_many(
        &self,
        scope: IndexedOperationRecordScope,
        keys: &[IndexedOperationRecordKey],
    ) -> WorkspaceResult<OperationRecordReply<Vec<Option<Vec<u8>>>>> {
        self.record(scope, Call::GetMany, keys.first().map_or(0, |key| key.kind));
        self.record_call(|| self.inner.operation_record_get_many(scope, keys))
    }
    fn operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: Vec<IndexedOperationRecordChange>,
    ) -> WorkspaceResult<OperationRecordReply<OperationRecordApply>> {
        let lowest = changes.iter().map(|change| change.key.kind).min();
        let highest = changes.iter().map(|change| change.key.kind).max();
        let kind = lowest.unwrap_or(0);
        self.record(scope, Call::Apply, kind);
        if self.split {
            let mut seen = self.seen.borrow_mut();
            *seen.changes.entry((scope.file_scope, kind)).or_default() += changes.len() as u64;
            *seen.apply_sizes.entry(changes.len()).or_default() += 1;
            seen.mixed_applies += u64::from(lowest != highest);
        }
        self.record_call(|| self.inner.operation_record_apply(scope, changes))
    }
    fn operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.record(scope, Call::FirstKeys, kind);
        let keys = self.record_call(|| self.inner.operation_record_keys(scope, kind, excluded));
        self.window(&keys);
        keys
    }
    fn operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> WorkspaceResult<OperationRecordReply<Vec<[u8; 32]>>> {
        self.record(scope, Call::KeysAfter, kind);
        let keys = self.record_call(|| self.inner.operation_record_keys_after(scope, kind, after));
        self.window(&keys);
        keys
    }
}
impl OverlayCapturedRuns for Counting<'_> {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<Inode>> {
        self.read("captured_inode");
        self.read_call(|| self.inner.captured_inode(reader, serial))
    }
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply> {
        self.read("captured_run_step");
        self.read_call(|| self.inner.captured_run_step(cursor))
    }
}
impl OverlayCapturedNamespace for Counting<'_> {
    fn captured_inode_page(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> WorkspaceResult<Vec<Inode>> {
        self.read("captured_inode_page");
        self.read_call(|| self.inner.captured_inode_page(reader, after))
    }
    fn captured_directory_entry_page(
        &self,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.read("captured_directory_entry_page");
        self.read_call(|| self.inner.captured_directory_entry_page(reader, after))
    }
    fn captured_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.read("captured_directory_entries");
        self.read_call(|| {
            OverlayCapturedNamespace::captured_directory_entries(self.inner, reader, parent, after)
        })
    }
    fn captured_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        self.read("captured_directory_entry");
        self.read_call(|| self.inner.captured_directory_entry(reader, parent, name))
    }
    fn captured_symlink(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<Vec<u8>> {
        self.read("captured_symlink");
        self.read_call(|| self.inner.captured_symlink(reader, serial))
    }
    fn keep_live_row(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<bool> {
        self.inner.keep_live_row(reader, serial)
    }
}
