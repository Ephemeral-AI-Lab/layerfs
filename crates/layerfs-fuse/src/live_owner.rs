//! Execution-side live state. The mutable namespace, file pieces and
//! replacement payload bytes are owned here, in the sandbox: payload lives in
//! packed local segments (no durability flush; the workspace is disposable).
//! Canonical construction and the Store stay on the host; Commit captures a
//! frozen generation locally and the host pulls its records and bytes over
//! the snapshot lane.
use crate::live_runtime::{LiveRuntime, OperationGate, Scheduler};
use crate::live_transport::BackingConnection;
use crate::live_wire::EditMetric;
use crate::live_wire::{self as wire, Input};
use crate::local_spool::LocalSpool;
use crate::port::{DirectoryPage, KernelEntry, KernelReferences};
use crate::{Attr, FilesystemPort, Kind, NodeId, PortError, PortResult, ROOT};
use layerfs_workspace_core::backing::BackingId;
use layerfs_workspace_core::file_edit::{Piece, SpoolSlice};
use layerfs_workspace_core::namespace::{AcquiredInode, NameLookup, ResolvedName};
use layerfs_workspace_core::{Data, LiveWorkspace, ReadPlan, ReadSource, ResourcePolicy};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone)]
pub struct LiveOwner(Arc<Owner>);

struct Owner {
    scheduler: Scheduler,
    read_scope: u64,
    state: Mutex<LiveWorkspace>,
    kernel_refs: Mutex<KernelRefState>,
    active_prefill: Mutex<Option<(NodeId, Arc<PrefillCompletionState>)>>,
    head: Mutex<Option<[u8; 33]>>,
    /// Immutable-base service (SEED/LOOKUP/DIRECTORY_PAGE/READ_BASE) on the
    /// host; ordinary mutations never wait on it.
    backing: BackingConnection,
    /// Sandbox-owned payload backing; no durability flush is ever issued.
    spool: Arc<LocalSpool>,
    /// Mount incarnation: late or misaddressed snapshot tokens can never act
    /// on a successor owner reusing the same endpoint.
    incarnation: u64,
    attempt: AtomicU64,
    /// The one active Commit's captured generation (one per workspace).
    snapshot: Mutex<Option<SnapshotSlot>>,
    failed: AtomicBool,
    closing: AtomicBool,
    cached: Mutex<std::collections::BTreeSet<NodeId>>,
    ordering: Mutex<HashMap<NodeId, Arc<tokio::sync::Mutex<()>>>>,
    namespace: tokio::sync::Mutex<()>,
    directories: Mutex<HashMap<NodeId, DirectoryCookies>>,
    edit: Mutex<Option<PendingSplices>>,
    kernel_edit: Mutex<Option<Arc<KernelEdit>>>,
    gate: OperationGate,
    writes: crate::write_metrics::AtomicFuseWriteMetrics,
    reads: crate::write_metrics::AtomicFuseReadMetrics,
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    notifier: std::sync::OnceLock<fuser::Notifier>,
    #[cfg(target_os = "linux")]
    kernel_root: Mutex<Option<Arc<std::fs::File>>>,
    cut: Mutex<Option<crate::live_runtime::OperationCut>>,
}

/// The active capture: identity for every frozen-input frame and completion.
#[derive(Clone, Copy)]
struct SnapshotToken {
    incarnation: u64,
    attempt: u64,
    generation: u64,
}

struct SnapshotSlot {
    token: SnapshotToken,
    complete: Option<PendingComplete>,
}

/// One publication's completion records, applied under the slot's token.
struct PendingComplete {
    root: layerfs_content::ObjectId,
    head: Option<[u8; 33]>,
    received: u64,
    applied: u64,
    skipped: u64,
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.scheduler
            .immutable_reads()
            .remove_scope(self.read_scope);
    }
}

struct PendingSplices {
    node: NodeId,
    count: usize,
    received: usize,
    prepared: Option<layerfs_workspace_core::file_edit::PreparedFileEdit>,
    cache_ranges: Vec<std::ops::Range<u64>>,
    diagnostic: Option<EditDiagnostic>,
    cut: crate::live_runtime::OperationCut,
    _charge: crate::live_runtime::LiveReservation,
}

struct EditDiagnostic {
    nonce: Vec<u8>,
    values: [u64; wire::EDIT_DIAGNOSTIC_FIELDS.len()],
    backing_start: (u64, u64),
}

fn note_edit(diagnostic: &mut Option<EditDiagnostic>, field: EditMetric, started: Option<Instant>) {
    if let (Some(diagnostic), Some(started)) = (diagnostic, started) {
        diagnostic.values[field as usize] += ns(started);
    }
}

struct KernelEdit {
    node: NodeId,
    file: layerfs_workspace_core::FileData,
    ranges: Vec<std::ops::Range<u64>>,
    _charge: crate::live_runtime::LiveReservation,
}

struct KernelEditGuard<'a>(&'a Mutex<Option<Arc<KernelEdit>>>);
impl KernelEditGuard<'_> {
    async fn finish(self, flush: crate::live_runtime::CacheFlush) {
        // Invalidation can return while an admitted callback waits for inode
        // ordering. Retire protection only after those callbacks finish.
        let _cut = flush.finish().await;
        drop(self);
    }
}
impl Drop for KernelEditGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut edit) = self.0.lock() {
            edit.take();
        }
    }
}

struct DirectoryCookies {
    generation: (layerfs_content::ObjectId, u64),
    next: u64,
    names: std::collections::BTreeMap<Vec<u8>, (u64, NodeId)>,
    ordered: std::collections::BTreeMap<u64, (NodeId, Vec<u8>)>,
}

fn core(error: layerfs_workspace_core::Error) -> PortError {
    use layerfs_workspace_core::Error;
    if std::env::var_os("LAYERFS_EDIT_FAILURE_DIAGNOSTIC").is_some() {
        eprintln!(
            "{{\"kind\":\"edit-core-failure\",\"error\":{:?}}}",
            format!("{error:?}")
        );
    }
    match error {
        Error::NotFound(_) => PortError::NotFound,
        Error::InvalidInput("name exists") => PortError::Exists,
        Error::InvalidInput("directory not empty") => PortError::NotEmpty,
        Error::InvalidInput("workspace spool limit" | "workspace live allocation") => {
            PortError::NoSpace
        }
        Error::InvalidInput(_) | Error::Core(_) => PortError::Invalid,
        Error::Integrity(_) => PortError::Io,
    }
}
fn io(_: std::io::Error) -> PortError {
    PortError::Io
}

/// Decode one completion record's attribute header. Mirrors `Attr` exactly:
/// size, mode, links, mtime and kind; the node id rides the record itself.
fn decode_complete_attr(input: &mut Input<'_>, node: NodeId) -> PortResult<Attr> {
    let size = input.u64().map_err(io)?;
    let mode = input.u32().map_err(io)?;
    let links = input.u32().map_err(io)?;
    let mtime_seconds = input.u64().map_err(io)? as i64;
    let mtime_nanoseconds = input.u32().map_err(io)?;
    let kind = match input.byte().map_err(io)? {
        1 => Kind::File,
        2 => Kind::Directory,
        3 => Kind::Symlink,
        _ => return Err(PortError::Invalid),
    };
    Ok(Attr {
        node,
        size,
        kind,
        mode,
        links,
        // mtime_seconds travels as u64 and was restored to its signed value.
        mtime_seconds,
        mtime_nanoseconds,
    })
}

/// A fresh mount identity per LiveOwner: snapshot and completion tokens carry
/// it so a late frame can never act on a successor owner.
fn next_incarnation() -> u64 {
    use std::sync::atomic::AtomicU64 as Counter;
    static NEXT: Counter = Counter::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos() as u64);
    nanos ^ NEXT.fetch_add(1, Ordering::Relaxed).rotate_left(32) ^ (std::process::id() as u64) << 32
}

// BTreeMap releases allocation on FORGET; HashMap spare capacity would outlive
// per-entry reservations. This covers a sparsely occupied map node and permit.
const KERNEL_REFERENCE_BYTES: usize = 256;

#[derive(Default)]
struct KernelRefState {
    detached: bool,
    entries: std::collections::BTreeMap<NodeId, KernelRefCount>,
}
struct KernelRefCount {
    lookups: u64,
    writable_seen: bool,
    #[cfg(any(
        test,
        all(target_os = "linux", any(feature = "host", feature = "proxy"))
    ))]
    prefilled_root: Option<layerfs_content::ObjectId>,
    _charge: crate::live_runtime::LiveReservation,
}
struct PreparedKernelRef {
    lookups: u64,
    charge: Option<crate::live_runtime::LiveReservation>,
}

#[derive(Default)]
struct PrefillCompletionState {
    done: AtomicBool,
    changed: tokio::sync::Notify,
}
impl PrefillCompletionState {
    async fn wait(&self) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.done.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}

#[cfg(any(
    test,
    all(target_os = "linux", any(feature = "host", feature = "proxy"))
))]
struct PrefillCompletion {
    owner: LiveOwner,
    node: NodeId,
    state: Arc<PrefillCompletionState>,
}
#[cfg(any(
    test,
    all(target_os = "linux", any(feature = "host", feature = "proxy"))
))]
impl Drop for PrefillCompletion {
    fn drop(&mut self) {
        let mut active = self
            .owner
            .0
            .active_prefill
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if active
            .as_ref()
            .is_some_and(|(node, state)| *node == self.node && Arc::ptr_eq(state, &self.state))
        {
            active.take();
        }
        drop(active);
        self.state.done.store(true, Ordering::Release);
        self.state.changed.notify_waiters();
    }
}

#[cfg(any(
    test,
    all(target_os = "linux", any(feature = "host", feature = "proxy"))
))]
struct KernelPrefill {
    completion: PrefillCompletion,
    root: layerfs_content::ObjectId,
    bytes: Vec<u8>,
    _ordinary: tokio::sync::OwnedRwLockReadGuard<()>,
}
#[cfg(any(
    test,
    all(target_os = "linux", any(feature = "host", feature = "proxy"))
))]
impl KernelPrefill {
    fn record_success(&self) {
        let Ok(state) = self.completion.owner.state() else {
            return;
        };
        let Some(node) = state.nodes.get(&self.completion.node) else {
            return;
        };
        if !matches!(&node.data, Data::File(layerfs_workspace_core::FileData::Base { root, len })
            if root.0 == self.root && *len == self.bytes.len() as u64)
        {
            return;
        }
        let Ok(mut refs) = self.completion.owner.0.kernel_refs.lock() else {
            return;
        };
        if let Some(row) = refs.entries.get_mut(&self.completion.node) {
            if !row.writable_seen {
                row.prefilled_root = Some(self.root);
            }
        }
    }
}

impl LiveOwner {
    // Call only after obtaining this inode's existing mutation ordering guard.
    // The marker belongs to Owner, so FORGET cannot discard an in-flight store.
    async fn exclude_prefill(&self, node: NodeId) -> PortResult<()> {
        let active = {
            let state = self.state()?;
            state.attr(node).map_err(core)?;
            let mut refs = self.0.kernel_refs.lock().map_err(|_| PortError::Io)?;
            if let Some(row) = refs.entries.get_mut(&node) {
                row.writable_seen = true;
            }
            self.0
                .active_prefill
                .lock()
                .map_err(|_| PortError::Io)?
                .as_ref()
                .filter(|(active_node, _)| *active_node == node)
                .map(|(_, done)| done.clone())
        };
        if let Some(active) = active {
            active.wait().await;
        }
        Ok(())
    }

    #[cfg(any(
        test,
        all(target_os = "linux", any(feature = "host", feature = "proxy"))
    ))]
    fn begin_kernel_prefill(&self, node: NodeId) -> Option<KernelPrefill> {
        // Optional prefill neither queues ahead of SDK invalidation nor allocates
        // per-read ordering state. Existing mutation orders are only try-locked.
        let ordinary = self.0.gate.try_ordinary()?;
        let state = self.state().ok()?;
        let refs = self.0.kernel_refs.lock().ok()?;
        if refs.detached
            || self.0.failed.load(Ordering::Acquire)
            || self.0.closing.load(Ordering::Acquire)
        {
            return None;
        }
        let row = refs.entries.get(&node)?;
        let Data::File(layerfs_workspace_core::FileData::Base { root, len }) =
            &state.nodes.get(&node)?.data
        else {
            return None;
        };
        if *len == 0
            || *len > wire::IMMUTABLE_PREFETCH_FILE_BYTES as u64
            || row.writable_seen
            || row.prefilled_root == Some(root.0)
        {
            return None;
        }
        let ordering = self.0.ordering.lock().ok()?;
        let _order = match ordering.get(&node) {
            Some(order) => Some(order.clone().try_lock_owned().ok()?),
            None => None,
        };
        let bytes =
            self.0
                .scheduler
                .immutable_reads()
                .get(self.0.read_scope, root.0, 0, *len as usize)?;
        if bytes.len() as u64 != *len {
            return None;
        }
        let mut active = self.0.active_prefill.lock().ok()?;
        if active.is_some() {
            return None;
        }
        let done = Arc::new(PrefillCompletionState::default());
        *active = Some((node, done.clone()));
        Some(KernelPrefill {
            completion: PrefillCompletion {
                owner: self.clone(),
                node,
                state: done,
            },
            root: root.0,
            bytes,
            _ordinary: ordinary,
        })
    }
}

impl LiveOwner {
    // state -> kernel_refs is the only lock order. Never await backing under either.
    // Reserve before a namespace mutation; node=None means a not-yet-created node.
    fn prepare_kernel_ref(
        &self,
        state: &LiveWorkspace,
        refs: &mut KernelRefState,
        node: Option<NodeId>,
        count: u64,
    ) -> PortResult<PreparedKernelRef> {
        if refs.detached || count == 0 {
            return Err(PortError::Io);
        }
        if let Some(node) = node {
            let live = state.nodes.get(&node).ok_or(PortError::NotFound)?;
            if let Some(existing) = refs.entries.get(&node) {
                return Ok(PreparedKernelRef {
                    lookups: existing.lookups.checked_add(count).ok_or(PortError::Io)?,
                    charge: None,
                });
            }
            live.pins.checked_add(1).ok_or(PortError::Io)?;
        }
        Ok(PreparedKernelRef {
            lookups: count,
            charge: Some(
                self.0
                    .scheduler
                    .reserve_live(KERNEL_REFERENCE_BYTES)
                    .map_err(|_| PortError::NoSpace)?,
            ),
        })
    }

    // All checks/allocation happened before mutation. Caller still owns both locks.
    fn install_kernel_ref(
        state: &mut LiveWorkspace,
        refs: &mut KernelRefState,
        node: NodeId,
        prepared: PreparedKernelRef,
    ) {
        match prepared.charge {
            Some(charge) => {
                state
                    .nodes
                    .get_mut(&node)
                    .expect("prepared live inode")
                    .pins += 1;
                let previous = refs.entries.insert(
                    node,
                    KernelRefCount {
                        lookups: prepared.lookups,
                        writable_seen: false,
                        #[cfg(any(
                            test,
                            all(target_os = "linux", any(feature = "host", feature = "proxy"))
                        ))]
                        prefilled_root: None,
                        _charge: charge,
                    },
                );
                debug_assert!(previous.is_none());
            }
            None => {
                refs.entries
                    .get_mut(&node)
                    .expect("prepared kernel reference")
                    .lookups = prepared.lookups
            }
        }
    }

    // Called while the existing directory-page namespace + state locks are held.
    // No fallible work may follow successful installation until those locks drop.
    fn retain_kernel_page(
        &self,
        state: &mut LiveWorkspace,
        nodes: Vec<NodeId>,
    ) -> PortResult<KernelReferences> {
        let mut counts = std::collections::BTreeMap::<NodeId, u64>::new();
        for &node in &nodes {
            *counts.entry(node).or_default() += 1;
        }
        let mut refs = self.0.kernel_refs.lock().map_err(|_| PortError::Io)?;
        if refs.detached {
            return Err(PortError::Io);
        }
        let prepared = counts
            .into_iter()
            .map(|(node, count)| {
                self.prepare_kernel_ref(state, &mut refs, Some(node), count)
                    .map(|prepared| (node, prepared))
            })
            .collect::<PortResult<Vec<_>>>()?;
        for (node, prepared) in prepared {
            Self::install_kernel_ref(state, &mut refs, node, prepared);
        }
        for &node in &nodes {
            if state.nodes[&node].attr(node).kind == Kind::File {
                self.note_cached_open(node);
            }
        }
        Ok(KernelReferences {
            owner: Some(self.clone()),
            nodes,
        })
    }

    fn forget_kernel_ref(&self, node: NodeId, count: u64) -> PortResult<()> {
        if count == 0 {
            return Ok(());
        }
        let reclaimed = {
            let mut state = self.state()?;
            let mut refs = self.0.kernel_refs.lock().map_err(|_| PortError::Io)?;
            if refs.detached {
                return Ok(());
            }
            // The kernel seeds its root inode with one implicit lookup; no
            // positive entry reply (and therefore no extra owner pin) created it.
            if node == ROOT && count == 1 && !refs.entries.contains_key(&node) {
                return Ok(());
            }
            let existing = refs.entries.get_mut(&node).ok_or(PortError::Io)?;
            let remaining = existing.lookups.checked_sub(count).ok_or(PortError::Io)?;
            if remaining != 0 {
                existing.lookups = remaining;
                return Ok(());
            }
            // Validate before consuming ownership; do not retry a consumed decrement.
            if state.nodes.get(&node).is_none_or(|node| node.pins == 0) {
                return Err(PortError::Io);
            }
            state.unpin(node).map_err(core)?;
            refs.entries.remove(&node);
            !state.nodes.contains_key(&node)
        };
        if reclaimed {
            self.0
                .ordering
                .lock()
                .map_err(|_| PortError::Io)?
                .remove(&node);
            self.0
                .directories
                .lock()
                .map_err(|_| PortError::Io)?
                .remove(&node);
            self.0
                .cached
                .lock()
                .map_err(|_| PortError::Io)?
                .remove(&node);
        }
        Ok(())
    }
}

impl LiveOwner {
    pub async fn connect(
        endpoint: String,
        capability: [u8; 32],
        scheduler: Scheduler,
        backing_dir: std::path::PathBuf,
    ) -> PortResult<Self> {
        let backing = BackingConnection::connect(endpoint, capability, &scheduler)
            .await
            .map_err(io)?;
        Self::from_backing(backing, scheduler, backing_dir).await
    }

    pub async fn local(
        handler: Arc<crate::live_transport::BackingHandler>,
        scheduler: Scheduler,
        backing_dir: std::path::PathBuf,
    ) -> PortResult<Self> {
        Self::from_backing(
            BackingConnection::local(handler, scheduler.clone()),
            scheduler,
            backing_dir,
        )
        .await
    }

    async fn from_backing(
        backing: BackingConnection,
        scheduler: Scheduler,
        backing_dir: std::path::PathBuf,
    ) -> PortResult<Self> {
        let seed = backing.call(&[wire::SEED]).await?;
        let mut input = Input(&seed);
        let root = input.object().map_err(io)?;
        let head = input.head().map_err(io)?;
        let policy = ResourcePolicy {
            max_spool_bytes: input.u64().map_err(io)?,
            max_final_delta_memory_bytes: input.u64().map_err(io)?,
        };
        let (id, node) = wire::node_in(input.0, |_, _, _| Err(wire::invalid())).map_err(io)?;
        if id != ROOT {
            return Err(PortError::Io);
        }
        let spool = Arc::new(LocalSpool::new(backing_dir)?);
        Ok(Self(Arc::new(Owner {
            read_scope: scheduler.immutable_reads().new_scope(),
            scheduler,
            writes: Default::default(),
            reads: Default::default(),
            #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
            notifier: Default::default(),
            #[cfg(target_os = "linux")]
            kernel_root: Default::default(),
            state: Mutex::new(LiveWorkspace::new(node, policy, root)),
            kernel_refs: Default::default(),
            active_prefill: Default::default(),
            head: Mutex::new(head),
            backing,
            spool,
            incarnation: next_incarnation(),
            attempt: AtomicU64::new(0),
            snapshot: Default::default(),
            failed: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            cached: Default::default(),
            ordering: Default::default(),
            namespace: Default::default(),
            directories: Default::default(),
            edit: Default::default(),
            kernel_edit: Default::default(),
            gate: Default::default(),
            cut: Default::default(),
        })))
    }

    pub fn spool(&self) -> Arc<LocalSpool> {
        self.0.spool.clone()
    }

    pub fn incarnation(&self) -> u64 {
        self.0.incarnation
    }

    fn state(&self) -> PortResult<std::sync::MutexGuard<'_, LiveWorkspace>> {
        self.0.state.lock().map_err(|_| PortError::Io)
    }

    async fn ordered(&self, node: NodeId) -> PortResult<tokio::sync::OwnedMutexGuard<()>> {
        self.state()?.attr(node).map_err(core)?;
        let order = {
            let mut ordering = self.0.ordering.lock().map_err(|_| PortError::Io)?;
            ordering.entry(node).or_default().clone()
        };
        Ok(order.lock_owned().await)
    }

    async fn directory_page_retained(
        &self,
        node: NodeId,
        after: u64,
        kernel: bool,
    ) -> PortResult<(DirectoryPage, KernelReferences)> {
        let _namespace = self.0.namespace.lock().await;
        let generation = {
            let state = self.state()?;
            (
                state.base_root,
                state.nodes.get(&node).ok_or(PortError::NotFound)?.revision,
            )
        };
        let refresh = self
            .0
            .directories
            .lock()
            .map_err(|_| PortError::Io)?
            .get(&node)
            .is_none_or(|cursor| cursor.generation != generation);
        if refresh {
            let entries = self.entries(node).await?;
            let mut directories = self.0.directories.lock().map_err(|_| PortError::Io)?;
            let cursor = directories.entry(node).or_insert_with(|| DirectoryCookies {
                generation,
                next: 3,
                names: Default::default(),
                ordered: Default::default(),
            });
            cursor.names.retain(|name, (cookie, id)| {
                let keep = entries.get(name) == Some(id);
                if !keep {
                    cursor.ordered.remove(cookie);
                }
                keep
            });
            for (name, id) in entries {
                if !cursor.names.contains_key(&name) {
                    let cookie = cursor.next;
                    cursor.next = cookie.checked_add(1).ok_or(PortError::NoSpace)?;
                    cursor.names.insert(name.clone(), (cookie, id));
                    cursor.ordered.insert(cookie, (id, name));
                }
            }
            cursor.generation = generation;
        }
        let mut state = self.state()?;
        let directories = self.0.directories.lock().map_err(|_| PortError::Io)?;
        let cursor = directories.get(&node).ok_or(PortError::Io)?;
        let mut page = Vec::new();
        if after == 0 {
            page.push((1, state.attr(node).map_err(core)?, b".".to_vec()));
        }
        if after < 2 {
            page.push((
                2,
                state
                    .attr(state.parent_of(node).map_err(core)?)
                    .map_err(core)?,
                b"..".to_vec(),
            ));
        }
        for (cookie, (id, name)) in cursor
            .ordered
            .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
            .take(128 - page.len())
        {
            page.push((*cookie, state.attr(*id).map_err(core)?, name.clone()));
        }
        drop(directories);
        let references = if kernel {
            let nodes = page
                .iter()
                .filter(|(_, _, name)| name != b"." && name != b"..")
                .map(|(_, attr, _)| attr.node)
                .collect();
            self.retain_kernel_page(&mut state, nodes)?
        } else {
            KernelReferences::default()
        };
        drop(state);
        Ok((page, references))
    }

    async fn name(&self, parent: NodeId, name: &[u8]) -> PortResult<ResolvedName> {
        self.name_with_prefetch(parent, name, true).await
    }

    async fn name_with_prefetch(
        &self,
        parent: NodeId,
        name: &[u8],
        prefetch: bool,
    ) -> PortResult<ResolvedName> {
        let (lookup, root) = {
            let state = self.state()?;
            (
                state.prepare_name(parent, name).map_err(core)?,
                state.base_root,
            )
        };
        match lookup {
            NameLookup::Ready(name) => Ok(name),
            NameLookup::Acquire(input) => {
                let cache = self.0.scheduler.immutable_reads();
                if let Some(encoded) = cache.get_name(
                    self.0.read_scope,
                    root,
                    input.directory.0,
                    input.name.as_bytes(),
                ) {
                    let acquired = Self::immutable_node(&encoded)?;
                    return self
                        .state()?
                        .complete_name(input, Some(acquired))
                        .map_err(core);
                }
                if let Some(page) = cache.get_page(self.0.read_scope, root, input.directory.0, &[])
                {
                    let mut page = Input(&page);
                    let more = page.byte().map_err(io)? != 0;
                    let count = page.u32().map_err(io)?;
                    let mut last = Vec::new();
                    let mut found = None;
                    for _ in 0..count {
                        let name = page.bytes().map_err(io)?;
                        let encoded = page.bytes().map_err(io)?;
                        page.bytes().map_err(io)?; // cached pages contain no payloads
                        if name == input.name.as_bytes() {
                            found = Some(Self::immutable_node(encoded)?);
                        }
                        last = name.to_vec();
                    }
                    page.done().map_err(io)?;
                    if found.is_some() || !more || input.name.as_bytes() <= last.as_slice() {
                        return self.state()?.complete_name(input, found).map_err(core);
                    }
                }
                let directory = input.directory;
                let mut request = vec![if prefetch {
                    wire::LOOKUP
                } else {
                    wire::LOOKUP_METADATA
                }];
                request.extend_from_slice(root.as_bytes());
                request.extend_from_slice(directory.0.as_bytes());
                wire::bytes_out(&mut request, input.name.as_bytes()).map_err(io)?;
                let response = self.0.backing.call(&request).await?;
                let mut received = Input(&response);
                let mut prefetched = Vec::new();
                let mut complete_entries = std::collections::BTreeMap::new();
                let acquired = match received.byte().map_err(io)? {
                    0 => None,
                    1 => {
                        let (encoded, acquired) =
                            self.immutable_reply(&mut received, &mut prefetched)?;
                        complete_entries.insert(input.name.as_bytes().to_vec(), encoded);
                        Some(acquired)
                    }
                    _ => return Err(PortError::Io),
                };
                let count = received.u32().map_err(io)?;
                if count > 127 {
                    return Err(PortError::Io);
                }
                let mut previous = Vec::new();
                let mut siblings = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    let name = received.bytes().map_err(io)?.to_vec();
                    layerfs_content::CanonicalName::from_bytes(&name).map_err(|_| PortError::Io)?;
                    if name <= previous || name == input.name.as_bytes() {
                        return Err(PortError::Io);
                    }
                    previous = name.clone();
                    let (encoded, _) = self.immutable_reply(&mut received, &mut prefetched)?;
                    complete_entries.insert(name.clone(), encoded.clone());
                    siblings.push((name, encoded));
                }
                let complete = match received.byte().map_err(io)? {
                    0 => false,
                    1 => true,
                    _ => return Err(PortError::Io),
                };
                received.done().map_err(io)?;
                // complete_name revalidates root and parent revision before changing live state.
                let resolved = self.state()?.complete_name(input, acquired).map_err(core)?;
                self.cache_prefetched(prefetched);
                if complete {
                    let mut page = vec![0];
                    page.extend_from_slice(&(complete_entries.len() as u32).to_be_bytes());
                    for (name, encoded) in complete_entries {
                        wire::bytes_out(&mut page, &name).map_err(io)?;
                        wire::bytes_out(&mut page, &encoded).map_err(io)?;
                        wire::bytes_out(&mut page, &[]).map_err(io)?;
                    }
                    cache.insert_page(self.0.read_scope, root, directory.0, &[], page);
                }
                for (name, encoded) in siblings {
                    cache.insert_name(self.0.read_scope, root, directory.0, &name, encoded);
                }
                Ok(resolved)
            }
        }
    }

    fn immutable_node(encoded: &[u8]) -> PortResult<AcquiredInode> {
        let (_, node) = wire::node_in(encoded, |_, _, _| Err(wire::invalid())).map_err(io)?;
        if !node.paths.is_empty() || node.pins != 0 || node.revision != 0 {
            return Err(PortError::Io);
        }
        match &node.data {
            Data::File(layerfs_workspace_core::FileData::Edited { .. }) => {
                return Err(PortError::Io);
            }
            Data::Directory(directory)
                if directory.base.is_none() || !directory.changes.is_empty() =>
            {
                return Err(PortError::Io);
            }
            _ => {}
        }
        Ok(AcquiredInode {
            inode: node.canonical.ok_or(PortError::Io)?,
            mode: node.mode,
            links: node.links,
            mtime_seconds: node.mtime_seconds,
            mtime_nanoseconds: node.mtime_nanoseconds,
            data: node.data,
        })
    }

    fn immutable_reply(
        &self,
        input: &mut Input<'_>,
        prefetched: &mut Vec<(layerfs_content::ObjectId, Vec<u8>)>,
    ) -> PortResult<(Vec<u8>, AcquiredInode)> {
        let encoded = input.bytes().map_err(io)?.to_vec();
        let acquired = Self::immutable_node(&encoded)?;
        let bytes = input.bytes().map_err(io)?;
        if !bytes.is_empty() {
            let Data::File(layerfs_workspace_core::FileData::Base { root, len }) = &acquired.data
            else {
                return Err(PortError::Io);
            };
            if bytes.len() > wire::IMMUTABLE_PREFETCH_FILE_BYTES || bytes.len() as u64 != *len {
                return Err(PortError::Io);
            }
            prefetched.push((root.0, bytes.to_vec()));
        }
        Ok((encoded, acquired))
    }

    fn cache_prefetched(&self, prefetched: Vec<(layerfs_content::ObjectId, Vec<u8>)>) {
        for (root, bytes) in prefetched {
            self.0.reads.note_read_ahead_miss(0, bytes.len() as u64, 0);
            self.0
                .scheduler
                .immutable_reads()
                .insert(self.0.read_scope, root, 0, bytes);
        }
    }

    pub async fn write_owned(&self, node: NodeId, offset: u64, bytes: &[u8]) -> PortResult<usize> {
        if self.0.failed.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        if bytes.len() > 1024 * 1024 {
            return Err(PortError::Invalid);
        }
        let _order = self.ordered(node).await?;
        self.exclude_prefill(node).await?;
        let acknowledged = bytes.len();
        let edit = self
            .0
            .kernel_edit
            .lock()
            .map_err(|_| PortError::Io)?
            .clone();
        let reconciled;
        let bytes = if let Some(edit) = edit.filter(|edit| edit.node == node) {
            // A queued dirty folio can contain pre-SDK bytes even though the
            // live view is installed. Preserve the SDK ranges while laundering
            // that folio; unrelated mapped writes remain in the incoming bytes.
            let len = match &edit.file {
                layerfs_workspace_core::FileData::Base { len, .. } => *len,
                layerfs_workspace_core::FileData::Edited { pieces, .. } => pieces.len(),
            };
            let count = bytes.len().min(len.saturating_sub(offset) as usize);
            let mut patched = bytes[..count].to_vec();
            for range in &edit.ranges {
                let start = offset.max(range.start);
                let end = (offset + count as u64).min(range.end);
                if start < end {
                    let replacement = self
                        .read_file(&edit.file, start, (end - start) as usize)
                        .await?;
                    if replacement.len() != (end - start) as usize {
                        return Err(PortError::Io);
                    }
                    patched[(start - offset) as usize..(end - offset) as usize]
                        .copy_from_slice(&replacement);
                }
            }
            reconciled = patched;
            &reconciled[..]
        } else {
            bytes
        };
        if bytes.is_empty() {
            return Ok(acknowledged);
        }
        // Sandbox-owned payload: reserve local segment space, prepare the
        // piece referencing it, then install the bytes before applying. The
        // piece only becomes visible after its bytes are physically readable
        // in owned workspace state.
        let (segment, start) = self.0.spool.reserve(bytes.len() as u64)?;
        let preparing = Instant::now();
        let prepared = {
            let result = self.state()?.prepare_write(
                node,
                offset,
                bytes.len(),
                Some(SpoolSlice {
                    segment: segment.clone(),
                    offset: start,
                    len: bytes.len() as u64,
                }),
            );
            match result {
                Ok(prepared) => prepared,
                Err(error) => {
                    // The failed reservation's segment is dropped when nothing
                    // else retains it; the core charged nothing.
                    let spool = self.0.spool.clone();
                    let abandoned = segment.clone();
                    let _ = self
                        .0
                        .scheduler
                        .physical(move || spool.abandon(abandoned).map_err(|_| wire::invalid()))
                        .await;
                    return Err(core(error));
                }
            }
        };
        self.0
            .writes
            .live_edit_ns
            .fetch_add(ns(preparing), Ordering::Relaxed);
        let encoding = Instant::now();
        {
            let spool = self.0.spool.clone();
            let payload_segment = segment.clone();
            let payload = bytes.to_vec();
            self.0
                .scheduler
                .physical(move || {
                    spool
                        .write(&payload_segment, start, &payload)
                        .map_err(|_| wire::invalid())
                })
                .await
                .map_err(io)?;
        }
        self.0
            .writes
            .note_client_frame(0, bytes.len() as u64, ns(encoding), 0);
        let applying = Instant::now();
        let result = self.state()?.apply_edit(prepared).map_err(core);
        self.0
            .writes
            .live_edit_ns
            .fetch_add(ns(applying), Ordering::Relaxed);
        result.map(|_| acknowledged)
    }

    pub async fn read_owned(&self, node: NodeId, offset: u64, size: usize) -> PortResult<Vec<u8>> {
        if size > 1024 * 1024 {
            return Err(PortError::Invalid);
        }
        let file = {
            let state = self.state()?;
            let Data::File(file) = &state.nodes.get(&node).ok_or(PortError::NotFound)?.data else {
                return Err(PortError::Invalid);
            };
            file.clone()
        };
        self.read_file(&file, offset, size).await
    }

    async fn read_file(
        &self,
        file: &layerfs_workspace_core::FileData,
        offset: u64,
        size: usize,
    ) -> PortResult<Vec<u8>> {
        let plan = ReadPlan::for_file(file, offset, size).map_err(core)?;
        let pieces = match plan.source {
            ReadSource::Base(root, start, end) => vec![Piece::Base {
                root,
                offset: start,
                len: end - start,
            }],
            ReadSource::Edited(pieces) => pieces,
        };
        let mut out = Vec::with_capacity(plan.requested as usize);
        for piece in pieces {
            let mut request;
            let length;
            match piece {
                Piece::Zero { len } => {
                    out.resize(out.len() + len as usize, 0);
                    continue;
                }
                Piece::Inline { bytes, offset, len } => {
                    out.extend_from_slice(&bytes[offset as usize..(offset + len) as usize]);
                    continue;
                }
                Piece::Base { root, offset, len } => {
                    if let Some(bytes) = self.0.scheduler.immutable_reads().get(
                        self.0.read_scope,
                        root.0,
                        offset,
                        len as usize,
                    ) {
                        self.0.reads.note_read_ahead_hit(bytes.len() as u64);
                        out.extend(bytes);
                        continue;
                    }
                    request = vec![wire::READ_BASE];
                    request.extend_from_slice(root.0.as_bytes());
                    wire::u64_out(&mut request, offset);
                    length = len;
                }
                Piece::Spool {
                    ref segment,
                    offset,
                    len,
                } => {
                    // Sandbox-owned bytes: read the local segment through the
                    // bounded physical worker. Every applied piece range was
                    // written before its piece became visible.
                    if len == 0 {
                        continue;
                    }
                    let spool = self.0.spool.clone();
                    let reader = segment.clone();
                    let bytes = self
                        .0
                        .scheduler
                        .physical(move || {
                            spool
                                .read(&reader, offset, len)
                                .map_err(|_| wire::invalid())
                        })
                        .await
                        .map_err(io)?;
                    out.extend(bytes);
                    continue;
                }
            }
            if length == 0 {
                continue;
            }
            request.extend_from_slice(&(length as u32).to_be_bytes());
            let bytes = self.0.backing.call(&request).await?;
            if bytes.len() as u64 != length {
                return Err(PortError::Io);
            }
            if let Piece::Base { root, offset, .. } = piece {
                self.0
                    .reads
                    .note_read_ahead_miss(length, bytes.len() as u64, length);
                self.0.scheduler.immutable_reads().insert(
                    self.0.read_scope,
                    root.0,
                    offset,
                    bytes.clone(),
                );
            }
            out.extend_from_slice(&bytes);
        }
        Ok(out)
    }

    async fn entries(
        &self,
        node: NodeId,
    ) -> PortResult<std::collections::BTreeMap<Vec<u8>, NodeId>> {
        let (root, base, revision, changes) = {
            let state = self.state()?;
            let directory = state.directory(node).map_err(core)?;
            (
                state.base_root,
                directory.base,
                state.nodes[&node].revision,
                directory.changes.clone(),
            )
        };
        let mut entries = std::collections::BTreeMap::new();
        if let Some(base) = base {
            let mut after = Vec::new();
            loop {
                let cursor = after.clone();
                let cache = self.0.scheduler.immutable_reads();
                let response =
                    if let Some(page) = cache.get_page(self.0.read_scope, root, base.0, &cursor) {
                        page
                    } else {
                        let mut request = vec![wire::DIRECTORY_PAGE];
                        request.extend_from_slice(root.as_bytes());
                        request.extend_from_slice(base.0.as_bytes());
                        wire::bytes_out(&mut request, &cursor).map_err(io)?;
                        self.0.backing.call(&request).await?
                    };
                let mut input = Input(&response);
                let more = match input.byte().map_err(io)? {
                    0 => false,
                    1 => true,
                    _ => return Err(PortError::Io),
                };
                let count = input.u32().map_err(io)? as usize;
                if count > 128 || (more && count == 0) {
                    return Err(PortError::Io);
                }
                // Cache immutable metadata/completeness only; payloads have their
                // own entries in the same budget, avoiding duplicate content charge.
                let mut encoded_page = vec![u8::from(more)];
                encoded_page.extend_from_slice(&(count as u32).to_be_bytes());
                let mut prefetched = Vec::new();
                let mut page = Vec::with_capacity(count);
                for _ in 0..count {
                    let name = input.bytes().map_err(io)?.to_vec();
                    if name <= after {
                        return Err(PortError::Io);
                    }
                    after = name.clone();
                    let (encoded, acquired) = self.immutable_reply(&mut input, &mut prefetched)?;
                    wire::bytes_out(&mut encoded_page, &name).map_err(io)?;
                    wire::bytes_out(&mut encoded_page, &encoded).map_err(io)?;
                    wire::bytes_out(&mut encoded_page, &[]).map_err(io)?;
                    page.push((name, acquired));
                }
                input.done().map_err(io)?;
                let mut state = self.state()?;
                if state.base_root != root
                    || state.nodes.get(&node).map(|node| node.revision) != Some(revision)
                {
                    return Err(PortError::Io);
                }
                for (name, acquired) in page {
                    if changes.contains_key(&name) {
                        continue;
                    }
                    let path = state.child_path(node, &name).map_err(core)?;
                    let child = state.install_immutable_node(acquired, path).map_err(core)?;
                    state.remember_directory_parent(child, node).map_err(core)?;
                    state
                        .remember_name(node, &name, Some(child))
                        .map_err(core)?;
                    entries.insert(name, child);
                }
                drop(state);
                self.cache_prefetched(prefetched);
                cache.insert_page(self.0.read_scope, root, base.0, &cursor, encoded_page);
                if entries.len() > 16384 {
                    return Err(PortError::NoSpace);
                }
                if !more {
                    break;
                }
            }
        }
        for (name, child) in changes {
            if let Some(child) = child {
                entries.insert(name, child);
            }
        }
        if entries.len() > 16384 {
            return Err(PortError::NoSpace);
        }
        Ok(entries)
    }

    async fn empty_directory(&self, node: NodeId) -> PortResult<bool> {
        {
            let state = self.state()?;
            let directory = state.directory(node).map_err(core)?;
            if directory.changes.values().any(Option::is_some) {
                return Ok(false);
            }
            if directory.base.is_none() {
                return Ok(true);
            }
        }
        Ok(self.entries(node).await?.is_empty())
    }

    fn run<T>(&self, future: impl std::future::Future<Output = PortResult<T>>) -> PortResult<T> {
        LiveRuntime::shared().map_err(io)?.block_on(future)
    }
}

impl FilesystemPort for LiveOwner {
    fn supports_kernel_lifetime(&self) -> bool {
        true
    }
    fn validate_kernel_open(&self, node: NodeId) -> PortResult<()> {
        if self.0.failed.load(Ordering::Acquire) || self.0.closing.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        match self.state()?.attr(node).map_err(core)?.kind {
            Kind::File => Ok(()),
            _ => Err(PortError::Invalid),
        }
    }
    fn prepare_kernel_open(&self, node: NodeId, writable: bool) -> crate::PortFuture<'_, ()> {
        Box::pin(async move {
            if writable {
                let _order = self.ordered(node).await?;
                self.exclude_prefill(node).await?;
                return self.validate_kernel_open(node);
            }
            self.validate_kernel_open(node)?;
            #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
            if let Some(notifier) = self.0.notifier.get().cloned() {
                if let Some(prefill) = self.begin_kernel_prefill(node) {
                    // The dedicated worker owns the gate, bytes and completion.
                    // Canceling this waiter cannot permit mutation during STORE.
                    let (sent, done) = tokio::sync::oneshot::channel();
                    if self.0.scheduler.try_prefill(Box::new(move || {
                        if notifier
                            .store(fuser::INodeNo(node.0), 0, &prefill.bytes)
                            .is_ok()
                        {
                            prefill
                                .completion
                                .owner
                                .0
                                .reads
                                .note_kernel_prefill(prefill.bytes.len());
                            prefill.record_success();
                        }
                        let _ = sent.send(());
                    })) {
                        let _ = done.await;
                    }
                }
            }
            self.validate_kernel_open(node)
        })
    }
    fn kernel_entry_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        operation: KernelEntry,
    ) -> crate::PortFuture<'a, (Attr, KernelReferences)> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let name = self.name(parent, name).await?;
            let mut state = self.state()?;
            let mut refs = self.0.kernel_refs.lock().map_err(|_| PortError::Io)?;
            let existing = match &operation {
                KernelEntry::Lookup => Some(name.existing().ok_or(PortError::NotFound)?),
                KernelEntry::Link { node } => Some(*node),
                _ => None,
            };
            let prepared = self.prepare_kernel_ref(&state, &mut refs, existing, 1)?;
            let writable = matches!(&operation, KernelEntry::Create { .. });
            let attr = match operation {
                KernelEntry::Lookup => state.attr(existing.unwrap()),
                KernelEntry::Create { mode } => state.create_file(name, mode, None),
                KernelEntry::Mkdir { mode } => state.mkdir(name, mode, None),
                KernelEntry::Symlink { target } => state.symlink(name, target),
                KernelEntry::Link { node } => state.link(node, name),
            }
            .map_err(core)?;
            Self::install_kernel_ref(&mut state, &mut refs, attr.node, prepared);
            if writable {
                refs.entries
                    .get_mut(&attr.node)
                    .expect("retained create")
                    .writable_seen = true;
            }
            if attr.kind == Kind::File {
                self.note_cached_open(attr.node);
            }
            drop(refs);
            drop(state);
            Ok((
                attr,
                KernelReferences {
                    owner: Some(self.clone()),
                    nodes: vec![attr.node],
                },
            ))
        })
    }
    fn kernel_forget(&self, node: NodeId, nlookup: u64) -> PortResult<()> {
        let result = self.forget_kernel_ref(node, nlookup);
        if result.is_err() {
            self.0.failed.store(true, Ordering::Release);
        }
        result
    }
    fn kernel_detach(&self) -> PortResult<()> {
        let result = self.run(async {
            self.prepare_shutdown().map_err(io)?;
            // Admission is closed and old cuts were released. This local cut drains
            // callbacks; never park it in Owner.cut, which shutdown also takes.
            let _cut = self.0.gate.cache_flush().await.finish().await;
            let mut state = self.state()?;
            let mut refs = self.0.kernel_refs.lock().map_err(|_| PortError::Io)?;
            if refs.detached {
                return Ok(());
            }
            for node in refs.entries.keys() {
                if state.nodes.get(node).is_none_or(|node| node.pins == 0) {
                    return Err(PortError::Io);
                }
            }
            refs.detached = true;
            for (node, _) in std::mem::take(&mut refs.entries) {
                state.unpin(node).map_err(core)?;
                if !state.nodes.contains_key(&node) {
                    self.0
                        .ordering
                        .lock()
                        .map_err(|_| PortError::Io)?
                        .remove(&node);
                    self.0
                        .directories
                        .lock()
                        .map_err(|_| PortError::Io)?
                        .remove(&node);
                    self.0
                        .cached
                        .lock()
                        .map_err(|_| PortError::Io)?
                        .remove(&node);
                }
            }
            Ok(())
        });
        if result.is_err() {
            self.0.failed.store(true, Ordering::Release);
        }
        result
    }

    fn note_cached_open(&self, node: NodeId) {
        if let Ok(mut cached) = self.0.cached.lock() {
            cached.insert(node);
        }
    }

    fn note_kernel_operation(&self, operation: crate::KernelOperation) {
        self.0.reads.note_kernel_operation(operation);
    }
    fn note_fuse_max_write(&self, bytes: u32) {
        self.0.writes.note_max_write(bytes as u64);
    }
    fn note_fuse_read_config(&self, bytes: u32, flags: u64) {
        self.0.reads.note_config(bytes as u64, flags);
    }
    fn note_readdir_page(&self, offset: u64, entries: u64) {
        self.0.reads.note_readdir_page(offset, entries);
    }

    fn admit_callback(
        &self,
        _operation: crate::KernelOperation,
        bytes: usize,
        writeback: bool,
    ) -> PortResult<crate::CallbackGuard> {
        if matches!(
            _operation,
            crate::KernelOperation::Release | crate::KernelOperation::Releasedir
        ) {
            return Ok(crate::CallbackGuard::default());
        }
        if self.0.closing.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        match _operation {
            crate::KernelOperation::Write => self.0.writes.note_kernel_write(bytes as u64),
            crate::KernelOperation::Read => self.0.reads.note_kernel_read(bytes as u64),
            _ => {}
        }
        let control = writeback
            || matches!(
                _operation,
                crate::KernelOperation::Read
                    | crate::KernelOperation::Fsync
                    | crate::KernelOperation::Fsyncdir
                    | crate::KernelOperation::Flush
            );
        let admission = self
            .0
            .scheduler
            .try_admit_from_receiver(
                bytes
                    .checked_mul(3)
                    .and_then(|n| n.checked_add(65536))
                    .ok_or(PortError::NoSpace)?,
                control,
            )
            .map_err(|_| PortError::NoSpace)?;
        Ok(crate::CallbackGuard {
            _gate: None,
            _admission: Some(admission),
        })
    }
    fn callback_gate(
        &self,
        operation: crate::KernelOperation,
        writeback: bool,
    ) -> crate::PortFuture<'_, Option<tokio::sync::OwnedRwLockReadGuard<()>>> {
        Box::pin(async move {
            if matches!(
                operation,
                crate::KernelOperation::Release | crate::KernelOperation::Releasedir
            ) {
                return Ok(None);
            }
            if self.0.failed.load(Ordering::Acquire)
                && !matches!(
                    operation,
                    crate::KernelOperation::Read
                        | crate::KernelOperation::Getattr
                        | crate::KernelOperation::Lookup
                        | crate::KernelOperation::Readlink
                        | crate::KernelOperation::Readdir
                        | crate::KernelOperation::Readdirplus
                        | crate::KernelOperation::Access
                        | crate::KernelOperation::Statfs
                )
            {
                return Err(PortError::Io);
            }
            // A page-fault READ can own a folio required by invalidation.
            // Drain it with laundering callbacks, not behind the ordinary cut.
            let writeback = writeback
                || matches!(
                    operation,
                    crate::KernelOperation::Read
                        | crate::KernelOperation::Fsync
                        | crate::KernelOperation::Fsyncdir
                        | crate::KernelOperation::Flush
                );
            let gate = self.0.gate.enter(writeback).await;
            if self.0.closing.load(Ordering::Acquire) {
                return Err(PortError::Io);
            }
            Ok(Some(gate))
        })
    }

    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    fn submit_write(
        &self,
        node: NodeId,
        offset: u64,
        bytes: &[u8],
        writeback: bool,
        mut reply: crate::WriteReply,
    ) {
        let owner = self.clone();
        // Cached write-through callbacks can hold a folio needed by invalidation.
        let kernel_cache = writeback
            || self
                .0
                .cached
                .lock()
                .is_ok_and(|cached| cached.contains(&node));
        let bytes = bytes.to_vec();
        self.0.writes.note_client_copy(bytes.len() as u64);
        let queued = Instant::now();
        self.0.scheduler.submit(async move {
            owner
                .0
                .writes
                .live_write_dispatch_ns
                .fetch_add(ns(queued), Ordering::Relaxed);
            reply._guard._gate = match owner
                .callback_gate(crate::KernelOperation::Write, kernel_cache)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.complete(Err(error));
                    return;
                }
            };
            let result = owner.write_owned(node, offset, &bytes).await;
            if writeback && result.is_err() {
                owner.0.failed.store(true, Ordering::Release);
            }
            reply.complete(result);
        });
    }
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    fn submit_read(&self, node: NodeId, offset: u64, size: usize, mut reply: crate::ReadReply) {
        let owner = self.clone();
        self.0.scheduler.submit(async move {
            reply._guard._gate = match owner
                .callback_gate(crate::KernelOperation::Read, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.complete(Err(error));
                    return;
                }
            };
            reply.complete(owner.read_owned(node, offset, size).await);
        });
    }
    fn lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        self.run(self.lookup_async(parent, name))
    }
    fn lookup_async<'a>(&'a self, parent: NodeId, name: &'a [u8]) -> crate::PortFuture<'a, Attr> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let name = self.name(parent, name).await?;
            self.state()?
                .attr(name.existing().ok_or(PortError::NotFound)?)
                .map_err(core)
        })
    }

    fn attr(&self, node: NodeId) -> PortResult<Attr> {
        self.state()?.attr(node).map_err(core)
    }
    fn readlink(&self, node: NodeId) -> PortResult<Vec<u8>> {
        match &self
            .state()?
            .nodes
            .get(&node)
            .ok_or(PortError::NotFound)?
            .data
        {
            Data::Symlink(target) => Ok(target.clone()),
            _ => Err(PortError::Invalid),
        }
    }
    fn create_file(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.run(self.create_file_async(parent, name, mode))
    }
    fn create_file_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> crate::PortFuture<'a, Attr> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let name = self.name(parent, name).await?;
            self.state()?.create_file(name, mode, None).map_err(core)
        })
    }
    fn mkdir(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.run(self.mkdir_async(parent, name, mode))
    }
    fn mkdir_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> crate::PortFuture<'a, Attr> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let name = self.name(parent, name).await?;
            self.state()?.mkdir(name, mode, None).map_err(core)
        })
    }
    fn symlink(&self, parent: NodeId, name: &[u8], target: Vec<u8>) -> PortResult<Attr> {
        self.run(self.symlink_async(parent, name, target))
    }
    fn symlink_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        target: Vec<u8>,
    ) -> crate::PortFuture<'a, Attr> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let name = self.name(parent, name).await?;
            self.state()?.symlink(name, target).map_err(core)
        })
    }
    fn read(&self, node: NodeId, offset: u64, size: usize) -> PortResult<Vec<u8>> {
        self.run(self.read_owned(node, offset, size))
    }
    fn write(&self, node: NodeId, offset: u64, bytes: &[u8]) -> PortResult<usize> {
        self.run(self.write_owned(node, offset, bytes))
    }
    fn pin(&self, node: NodeId, truncate: bool, writable: bool) -> PortResult<()> {
        self.run(self.pin_async(node, truncate, writable))
    }
    fn pin_async<'a>(&'a self, node: NodeId, truncate: bool, _: bool) -> crate::PortFuture<'a, ()> {
        Box::pin(async move {
            self.state()?.pin(node).map_err(core)?;
            if truncate {
                if let Err(error) = self.truncate_async(node, 0).await {
                    self.state()?.unpin(node).map_err(core)?;
                    return Err(error);
                }
            }
            Ok(())
        })
    }
    fn create_file_open(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        self.run(self.create_file_open_async(parent, name, mode))
    }
    fn create_file_open_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> crate::PortFuture<'a, Attr> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let name = self.name(parent, name).await?;
            let mut state = self.state()?;
            let attr = state.create_file(name, mode, None).map_err(core)?;
            state.pin(attr.node).map_err(core)?;
            Ok(attr)
        })
    }
    fn pin_directory(&self, node: NodeId) -> PortResult<()> {
        self.state()?.pin_directory(node).map_err(core)
    }
    fn unpin_directory(&self, node: NodeId) -> PortResult<()> {
        self.unpin(node, false)
    }
    fn unpin(&self, node: NodeId, _: bool) -> PortResult<()> {
        let reclaimed = {
            let mut state = self.state()?;
            state.unpin(node).map_err(core)?;
            !state.nodes.contains_key(&node)
        };
        if reclaimed {
            self.0
                .ordering
                .lock()
                .map_err(|_| PortError::Io)?
                .remove(&node);
            self.0
                .directories
                .lock()
                .map_err(|_| PortError::Io)?
                .remove(&node);
            self.0
                .cached
                .lock()
                .map_err(|_| PortError::Io)?
                .remove(&node);
        }
        Ok(())
    }
    fn truncate(&self, node: NodeId, size: u64) -> PortResult<()> {
        self.run(self.truncate_async(node, size))
    }
    fn truncate_async<'a>(&'a self, node: NodeId, size: u64) -> crate::PortFuture<'a, ()> {
        Box::pin(async move {
            let _order = self.ordered(node).await?;
            self.exclude_prefill(node).await?;
            let prepared = self.state()?.prepare_truncate(node, size).map_err(core)?;
            if let Some(prepared) = prepared {
                let mut check = vec![wire::CHECK, 0];
                for range in prepared.backing_ranges() {
                    wire::u64_out(&mut check, range.segment.id().0);
                }
                self.0.backing.call(&check).await?;
                self.state()?.apply_edit(prepared).map_err(core)?;
            }
            Ok(())
        })
    }
    fn chmod(&self, node: NodeId, mode: u32) -> PortResult<()> {
        self.run(self.chmod_async(node, mode))
    }
    fn chmod_async<'a>(&'a self, node: NodeId, mode: u32) -> crate::PortFuture<'a, ()> {
        Box::pin(async move {
            let directory = self.state()?.attr(node).map_err(core)?.kind == Kind::Directory;
            let _namespace = if directory {
                Some(self.0.namespace.lock().await)
            } else {
                None
            };
            let _order = self.ordered(node).await?;
            self.state()?.chmod(node, mode).map_err(core)
        })
    }
    fn set_mtime(&self, node: NodeId, seconds: i64, nanos: u32) -> PortResult<()> {
        self.run(self.set_mtime_async(node, seconds, nanos))
    }
    fn set_mtime_async<'a>(
        &'a self,
        node: NodeId,
        seconds: i64,
        nanos: u32,
    ) -> crate::PortFuture<'a, ()> {
        Box::pin(async move {
            let directory = self.state()?.attr(node).map_err(core)?.kind == Kind::Directory;
            let _namespace = if directory {
                Some(self.0.namespace.lock().await)
            } else {
                None
            };
            let _order = self.ordered(node).await?;
            self.state()?.set_mtime(node, seconds, nanos).map_err(core)
        })
    }
    fn fsync(&self, node: Option<NodeId>) -> PortResult<()> {
        self.run(self.fsync_async(node))
    }
    fn fsync_async<'a>(&'a self, _node: Option<NodeId>) -> crate::PortFuture<'a, ()> {
        Box::pin(async move {
            // Volatile sync contract: the workspace backing is disposable and
            // never receives a durability flush. Every acknowledged write is
            // already installed in owned, readable workspace state (bytes are
            // written before their piece is applied), so this validates state
            // and surfaces known write/allocation errors, then retires idle
            // allocations. It never exports facts, canonicalizes content or
            // waits for a host acknowledgment.
            if self.0.failed.load(Ordering::Acquire) {
                return Err(PortError::Io);
            }
            let known_errors = {
                let state = self.state()?;
                state.policy.check(state.spool_bytes).is_ok()
            };
            if !known_errors {
                // Surface the recorded allocation overrun; the failing write
                // already reported its own error synchronously.
                return Err(PortError::NoSpace);
            }
            let spool = self.0.spool.clone();
            self.0
                .scheduler
                .physical(move || spool.retire_idle().map_err(|_| wire::invalid()))
                .await
                .map_err(io)?;
            Ok(())
        })
    }
    fn readdir(&self, node: NodeId) -> PortResult<Vec<(NodeId, Kind, Vec<u8>)>> {
        self.run(async {
            let _namespace = self.0.namespace.lock().await;
            let entries = self.entries(node).await?;
            let state = self.state()?;
            let mut result = vec![
                (node, Kind::Directory, b".".to_vec()),
                (
                    state.parent_of(node).map_err(core)?,
                    Kind::Directory,
                    b"..".to_vec(),
                ),
            ];
            for (name, id) in entries {
                result.push((id, state.attr(id).map_err(core)?.kind, name));
            }
            Ok(result)
        })
    }
    fn link(&self, node: NodeId, parent: NodeId, name: &[u8]) -> PortResult<Attr> {
        self.run(self.link_async(node, parent, name))
    }
    fn link_async<'a>(
        &'a self,
        node: NodeId,
        parent: NodeId,
        name: &'a [u8],
    ) -> crate::PortFuture<'a, Attr> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let name = self.name(parent, name).await?;
            self.state()?.link(node, name).map_err(core)
        })
    }
    fn unlink(&self, parent: NodeId, name: &[u8], directory: bool) -> PortResult<()> {
        self.run(self.unlink_async(parent, name, directory))
    }
    fn unlink_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        directory: bool,
    ) -> crate::PortFuture<'a, ()> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let name = self.name(parent, name).await?;
            let node = name.existing().ok_or(PortError::NotFound)?;
            let empty = if directory {
                self.empty_directory(node).await?
            } else {
                true
            };
            self.state()?.unlink(name, directory, empty).map_err(core)?;
            if !self.state()?.nodes.contains_key(&node) {
                self.0
                    .ordering
                    .lock()
                    .map_err(|_| PortError::Io)?
                    .remove(&node);
                self.0
                    .directories
                    .lock()
                    .map_err(|_| PortError::Io)?
                    .remove(&node);
            }
            Ok(())
        })
    }
    fn rename(
        &self,
        parent: NodeId,
        name: &[u8],
        target_parent: NodeId,
        target: &[u8],
        no_replace: bool,
    ) -> PortResult<()> {
        self.run(self.rename_async(parent, name, target_parent, target, no_replace))
    }
    fn rename_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        target_parent: NodeId,
        target: &'a [u8],
        no_replace: bool,
    ) -> crate::PortFuture<'a, ()> {
        Box::pin(async move {
            let _namespace = self.0.namespace.lock().await;
            let source = self.name(parent, name).await?;
            let target = self.name(target_parent, target).await?;
            let empty = if let Some(node) = target.existing() {
                if self.state()?.attr(node).map_err(core)?.kind == Kind::Directory {
                    self.empty_directory(node).await?
                } else {
                    true
                }
            } else {
                true
            };
            self.state()?
                .rename(source, target, no_replace, empty)
                .map_err(core)
        })
    }
    fn readdir_page_async<'a>(
        &'a self,
        node: NodeId,
        after: usize,
    ) -> crate::PortFuture<'a, Vec<(NodeId, Kind, Vec<u8>)>> {
        Box::pin(async move {
            self.directory_page_async(node, after as u64)
                .await
                .map(|entries| {
                    entries
                        .into_iter()
                        .map(|(_, attr, name)| (attr.node, attr.kind, name))
                        .collect()
                })
        })
    }
    fn readdirplus_page_async<'a>(
        &'a self,
        node: NodeId,
        after: usize,
    ) -> crate::PortFuture<'a, Vec<(Attr, Vec<u8>)>> {
        Box::pin(async move {
            self.directory_page_async(node, after as u64)
                .await
                .map(|entries| {
                    entries
                        .into_iter()
                        .map(|(_, attr, name)| (attr, name))
                        .collect()
                })
        })
    }
    fn directory_page_async<'a>(
        &'a self,
        node: NodeId,
        after: u64,
    ) -> crate::PortFuture<'a, Vec<(u64, Attr, Vec<u8>)>> {
        Box::pin(async move {
            self.directory_page_retained(node, after, false)
                .await
                .map(|(page, _)| page)
        })
    }
    fn kernel_directory_page_async<'a>(
        &'a self,
        node: NodeId,
        after: u64,
    ) -> crate::PortFuture<'a, (DirectoryPage, KernelReferences)> {
        Box::pin(self.directory_page_retained(node, after, true))
    }
}

impl LiveOwner {
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    pub fn set_notifier(&self, notifier: fuser::Notifier) -> std::io::Result<()> {
        self.0.notifier.set(notifier).map_err(|_| wire::invalid())
    }

    #[cfg(target_os = "linux")]
    pub fn set_kernel_root(&self, root: std::fs::File) -> std::io::Result<()> {
        let mut slot = self.0.kernel_root.lock().map_err(|_| wire::invalid())?;
        if slot.is_some() {
            return Err(wire::invalid());
        }
        *slot = Some(Arc::new(root));
        Ok(())
    }

    pub fn prepare_shutdown(&self) -> std::io::Result<()> {
        self.0.closing.store(true, Ordering::Release);
        self.0.cut.lock().map_err(|_| wire::invalid())?.take();
        self.0.edit.lock().map_err(|_| wire::invalid())?.take();
        self.0.snapshot.lock().map_err(|_| wire::invalid())?.take();
        #[cfg(target_os = "linux")]
        self.0
            .kernel_root
            .lock()
            .map_err(|_| wire::invalid())?
            .take();
        Ok(())
    }

    async fn flush_kernel_cache(
        &self,
        mut diagnostic: Option<&mut EditDiagnostic>,
    ) -> PortResult<()> {
        #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
        {
            use std::os::fd::AsRawFd;
            let cached = self.0.cached.lock().map_err(|_| PortError::Io)?.clone();
            if let Some(diagnostic) = diagnostic.as_mut() {
                diagnostic.values[EditMetric::CachedNodes as usize] += cached.len() as u64;
            }
            let nodes: Vec<_> = {
                let state = self.state()?;
                cached
                    .into_iter()
                    .filter(|id| state.nodes.contains_key(id))
                    .collect()
            };
            if nodes.is_empty() {
                return Ok(());
            }
            if let Some(diagnostic) = diagnostic.as_mut() {
                diagnostic.values[EditMetric::KernelFlushes as usize] += 1;
            }
            let notifier = self.0.notifier.get().ok_or(PortError::Io)?.clone();
            let root = self
                .0
                .kernel_root
                .lock()
                .map_err(|_| PortError::Io)?
                .clone()
                .ok_or(PortError::Io)?;
            self.0
                .scheduler
                .kernel(move || {
                    for node in nodes {
                        notifier.inval_inode(fuser::INodeNo(node.0), 0, 0)?;
                    }
                    // Linux invalidation does not propagate every laundering error.
                    // This descriptor predates writes, so syncfs observes the mount's
                    // writeback error sequence, including kernel-side allocation errors.
                    nix::unistd::syncfs(root.as_raw_fd()).map_err(std::io::Error::from)
                })
                .await
                .map_err(io)?;
        }
        let _ = &mut diagnostic;
        if self.0.failed.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        Ok(())
    }

    fn invalidate(&self, node: NodeId) -> PortResult<()> {
        #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
        {
            self.0
                .notifier
                .get()
                .ok_or(PortError::Io)?
                .inval_inode(fuser::INodeNo(node.0), 0, 0)
                .map_err(io)
        }
        #[cfg(not(all(target_os = "linux", any(feature = "host", feature = "proxy"))))]
        {
            let _ = node;
            Err(PortError::Invalid)
        }
    }

    /// Take an operation cut for an SDK edit transaction: exclude ordinary
    /// and writeback callbacks while queued kernel folios are reconciled.
    /// Commit capture never calls this; snapshots are stable by frontier
    /// ownership, not by pausing the workspace.
    async fn take_edit_cut(
        &self,
        diagnostic: &mut Option<EditDiagnostic>,
    ) -> PortResult<crate::live_runtime::OperationCut> {
        if self.0.failed.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        if self.0.cut.lock().map_err(|_| PortError::Io)?.is_some() {
            return Err(PortError::Busy);
        }
        let started = diagnostic.as_ref().map(|_| Instant::now());
        let flush = self.0.gate.cache_flush().await;
        note_edit(diagnostic, EditMetric::Gate, started);
        let started = diagnostic.as_ref().map(|_| Instant::now());
        self.flush_kernel_cache(diagnostic.as_mut()).await?;
        note_edit(diagnostic, EditMetric::Kernel, started);
        let started = diagnostic.as_ref().map(|_| Instant::now());
        let cut = flush.finish().await;
        *self.0.cut.lock().map_err(|_| PortError::Io)? = Some(cut);
        note_edit(diagnostic, EditMetric::Gate, started);
        self.0
            .cut
            .lock()
            .map_err(|_| PortError::Io)?
            .take()
            .ok_or(PortError::Io)
    }

    pub async fn local_control(&self, bytes: &[u8]) -> PortResult<Vec<u8>> {
        if bytes == [wire::SHUTDOWN] {
            self.prepare_shutdown().map_err(io)?;
            return Ok(Vec::new());
        }
        let result = self.control_request(bytes).await;
        if result.is_err()
            && matches!(
                bytes.first(),
                Some(&wire::EDIT_BEGIN) | Some(&wire::EDIT_PART) | Some(&wire::EDIT_END)
            )
        {
            self.0.edit.lock().map_err(|_| PortError::Io)?.take();
        }
        result
    }

    async fn control_request(&self, bytes: &[u8]) -> PortResult<Vec<u8>> {
        let mut input = Input(bytes);
        let mut out = Vec::new();
        let opcode = input.byte().map_err(io)?;
        let diagnostic_started = match opcode {
            wire::EDIT_BEGIN => {
                let mut preview = Input(input.0);
                preview.bytes().map_err(io)?;
                preview.u64().map_err(io)?;
                (!preview.0.is_empty()).then(Instant::now)
            }
            wire::EDIT_PART | wire::EDIT_END => self
                .0
                .edit
                .lock()
                .map_err(|_| PortError::Io)?
                .as_ref()
                .and_then(|pending| pending.diagnostic.as_ref())
                .map(|_| Instant::now()),
            _ => None,
        };
        let mut completed_diagnostic = None;
        match opcode {
            wire::EDIT_BEGIN => {
                let path = std::str::from_utf8(input.bytes().map_err(io)?)
                    .map_err(|_| PortError::Invalid)?
                    .to_owned();
                layerfs_content::CanonicalPath::new(&path).map_err(|_| PortError::Invalid)?;
                let count = input.u64().map_err(io)? as usize;
                let mut diagnostic = if input.0.is_empty() {
                    None
                } else {
                    let nonce = input.bytes().map_err(io)?;
                    if !wire::valid_edit_diagnostic_nonce(nonce) {
                        return Err(PortError::Invalid);
                    }
                    Some(EditDiagnostic {
                        nonce: nonce.to_vec(),
                        values: [0; wire::EDIT_DIAGNOSTIC_FIELDS.len()],
                        backing_start: (
                            self.0
                                .backing
                                .metrics
                                .live_backing_wait_ns
                                .load(Ordering::Relaxed),
                            self.0
                                .backing
                                .metrics
                                .live_backing_calls
                                .load(Ordering::Relaxed),
                        ),
                    })
                };
                input.done().map_err(io)?;
                if count == 0 {
                    return Err(PortError::Invalid);
                }
                if self.0.edit.lock().map_err(|_| PortError::Io)?.is_some()
                    || self.0.cut.lock().map_err(|_| PortError::Io)?.is_some()
                {
                    return Err(PortError::Busy);
                }
                let charge = self
                    .0
                    .scheduler
                    .reserve_live(9 * 1024 * 1024)
                    .map_err(|_| PortError::NoSpace)?;
                // The edit reconciles the kernel cache around its ranges; the
                // cut is local and unrelated to Commit capture.
                let cut = self.take_edit_cut(&mut diagnostic).await?;
                let mut node = ROOT;
                let started = diagnostic.as_ref().map(|_| Instant::now());
                for name in path.split('/').filter(|name| !name.is_empty()) {
                    let _namespace = self.0.namespace.lock().await;
                    let name = self
                        .name_with_prefetch(node, name.as_bytes(), false)
                        .await?;
                    node = self
                        .state()?
                        .attr(name.existing().ok_or(PortError::NotFound)?)
                        .map_err(core)?
                        .node;
                }
                note_edit(&mut diagnostic, EditMetric::Lookup, started);
                *self.0.edit.lock().map_err(|_| PortError::Io)? = Some(PendingSplices {
                    node,
                    count,
                    received: 0,
                    prepared: None,
                    cache_ranges: Vec::new(),
                    diagnostic,
                    cut,
                    _charge: charge,
                });
            }
            wire::EDIT_PART => {
                let start = input.u64().map_err(io)?;
                let delete = input.u64().map_err(io)?;
                let mut held = self.0.edit.lock().map_err(|_| PortError::Io)?;
                let pending = held.as_mut().ok_or(PortError::Invalid)?;
                let preparing = pending.diagnostic.as_ref().map(|_| Instant::now());
                if pending.received == pending.count {
                    return Err(PortError::Invalid);
                }
                let piece = match input.byte().map_err(io)? {
                    0 => {
                        let bytes = input.bytes().map_err(io)?;
                        if bytes.len() > layerfs_workspace_core::file_edit::MAX_INLINE_PER_EDIT {
                            return Err(PortError::Invalid);
                        }
                        (!bytes.is_empty()).then(|| Piece::Inline {
                            bytes: Arc::from(bytes),
                            offset: 0,
                            len: bytes.len() as u64,
                        })
                    }
                    1 => {
                        let len = input.u64().map_err(io)?;
                        (len != 0).then_some(Piece::Zero { len })
                    }
                    _ => return Err(PortError::Invalid),
                };
                input.done().map_err(io)?;
                let replacement_len = piece.as_ref().map_or(0, Piece::len);
                let cache_end = if replacement_len == delete {
                    start
                        .checked_add(replacement_len)
                        .ok_or(PortError::Invalid)?
                } else {
                    // An insertion/deletion changes every following byte's position.
                    u64::MAX
                };
                pending.cache_ranges.push(start..cache_end);
                let state = self.state()?;
                if let Some(prepared) = pending.prepared.as_mut() {
                    state
                        .extend_splices(prepared, start, delete, piece)
                        .map_err(core)?;
                } else {
                    pending.prepared = Some(
                        state
                            .prepare_splices(pending.node, vec![(start, delete, piece)])
                            .map_err(core)?,
                    );
                }
                pending.received += 1;
                note_edit(&mut pending.diagnostic, EditMetric::Prepare, preparing);
            }
            wire::EDIT_END => {
                input.done().map_err(io)?;
                let mut pending = self
                    .0
                    .edit
                    .lock()
                    .map_err(|_| PortError::Io)?
                    .take()
                    .ok_or(PortError::Invalid)?;
                let applying = pending.diagnostic.as_ref().map(|_| Instant::now());
                if pending.received != pending.count {
                    return Err(PortError::Invalid);
                }
                let node = pending.node;
                #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
                let cache_data = self
                    .0
                    .cached
                    .lock()
                    .map_err(|_| PortError::Io)?
                    .contains(&node);
                #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
                let _cache_charge = self
                    .0
                    .notifier
                    .get()
                    .filter(|_| cache_data)
                    .map(|_| {
                        self.0
                            .scheduler
                            .reserve_live(2 * 1024 * 1024)
                            .map_err(|_| PortError::NoSpace)
                    })
                    .transpose()?;
                self.state()?
                    .apply_edit(pending.prepared.ok_or(PortError::Invalid)?)
                    .map_err(core)?;
                let file = match &self
                    .state()?
                    .nodes
                    .get(&node)
                    .ok_or(PortError::NotFound)?
                    .data
                {
                    Data::File(file) => file.clone(),
                    _ => return Err(PortError::Invalid),
                };
                // Protect the installed ranges until invalidation has drained
                // old dirty folios. Reopen folio callbacks, but keep namespace
                // and size changes excluded until reconciliation completes.
                *self.0.kernel_edit.lock().map_err(|_| PortError::Io)? =
                    Some(Arc::new(KernelEdit {
                        node,
                        file: file.clone(),
                        ranges: pending.cache_ranges.clone(),
                        _charge: pending._charge,
                    }));
                note_edit(&mut pending.diagnostic, EditMetric::Apply, applying);
                let reconciling = pending.diagnostic.as_ref().map(|_| Instant::now());
                let flush = pending.cut.reopen_writeback();
                let kernel_edit = KernelEditGuard(&self.0.kernel_edit);
                #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
                if let Some(notifier) = self.0.notifier.get().cloned() {
                    if let Some(diagnostic) = pending.diagnostic.as_mut() {
                        diagnostic.values[EditMetric::ReconcileNotifier as usize] += 1;
                        diagnostic.values[EditMetric::ReconcileCached as usize] +=
                            u64::from(cache_data);
                    }
                    let updated = async {
                        let mut ranges = if cache_data {
                            pending.cache_ranges
                        } else {
                            Vec::new()
                        };
                        ranges.sort_unstable_by_key(|range| range.start);
                        let mut end = 0;
                        for range in ranges {
                            let mut offset = range.start.max(end);
                            while offset < range.end {
                                let size = (range.end - offset).min(1024 * 1024) as usize;
                                let bytes = self.read_file(&file, offset, size).await?;
                                if bytes.is_empty() {
                                    break;
                                }
                                let length = bytes.len() as u64;
                                let notifier = notifier.clone();
                                // Invalidation can launder a refaulted dirty page. Update
                                // the edited bytes first so that writeback preserves them.
                                self.0
                                    .scheduler
                                    .kernel(move || {
                                        notifier.store(fuser::INodeNo(node.0), offset, &bytes)
                                    })
                                    .await
                                    .map_err(io)?;
                                offset += length;
                            }
                            end = end.max(offset);
                        }
                        let owner = self.clone();
                        self.0
                            .scheduler
                            .kernel(move || owner.invalidate(node).map_err(|_| wire::invalid()))
                            .await
                            .map_err(io)
                    }
                    .await;
                    if let Err(error) = updated {
                        self.0.failed.store(true, Ordering::Release);
                        return Err(error);
                    }
                }
                #[cfg(not(all(target_os = "linux", any(feature = "host", feature = "proxy"))))]
                let _ = (node, file);
                kernel_edit.finish(flush).await;
                note_edit(&mut pending.diagnostic, EditMetric::Reconcile, reconciling);
                if let Some(diagnostic) = pending.diagnostic.as_mut() {
                    diagnostic.values[EditMetric::BackingWait as usize] = self
                        .0
                        .backing
                        .metrics
                        .live_backing_wait_ns
                        .load(Ordering::Relaxed)
                        .checked_sub(diagnostic.backing_start.0)
                        .ok_or(PortError::Io)?;
                    diagnostic.values[EditMetric::BackingCalls as usize] = self
                        .0
                        .backing
                        .metrics
                        .live_backing_calls
                        .load(Ordering::Relaxed)
                        .checked_sub(diagnostic.backing_start.1)
                        .ok_or(PortError::Io)?;
                }
                completed_diagnostic = pending.diagnostic;
            }
            wire::WRITE_METRICS => {
                input.done().map_err(io)?;
                let mut metrics = self.0.writes.take();
                metrics.merge(self.0.backing.metrics.take());
                metrics.write_to(&mut out).map_err(io)?;
            }
            wire::READ_METRICS => {
                input.done().map_err(io)?;
                self.0.reads.take().write_to(&mut out).map_err(io)?;
            }
            wire::INVALIDATE => {
                let node = NodeId(input.u64().map_err(io)?);
                input.done().map_err(io)?;
                self.invalidate(node)?;
            }
            wire::CAPTURE => {
                input.done().map_err(io)?;
                if self.0.failed.load(Ordering::Acquire) || self.0.closing.load(Ordering::Acquire) {
                    return Err(PortError::Io);
                }
                let mut slot = self.0.snapshot.lock().map_err(|_| PortError::Io)?;
                if let Some(slot) = slot.as_ref() {
                    // One active Commit: the retained attempt must be resolved
                    // (completed or cancelled) before the next capture.
                    if slot.complete.is_some() {
                        return Err(PortError::Busy);
                    }
                    return Err(PortError::Busy);
                }
                let (generation, frontier_len, base_root) = {
                    let mut state = self.state()?;
                    state.capture_frontier().map_err(core)?;
                    (
                        state.mutation_generation,
                        state.frontier_len() as u64,
                        state.base_root,
                    )
                };
                let attempt = self.0.attempt.fetch_add(1, Ordering::AcqRel) + 1;
                *slot = Some(SnapshotSlot {
                    token: SnapshotToken {
                        incarnation: self.0.incarnation,
                        attempt,
                        generation,
                    },
                    complete: None,
                });
                drop(slot);
                wire::u64_out(&mut out, self.0.incarnation);
                wire::u64_out(&mut out, attempt);
                wire::u64_out(&mut out, generation);
                wire::u64_out(&mut out, frontier_len);
                out.extend_from_slice(base_root.as_bytes());
                let head = self.0.head.lock().map_err(|_| PortError::Io)?;
                wire::bytes_out(&mut out, head.as_ref().map_or(&[][..], |bytes| &bytes[..]))
                    .map_err(io)?;
            }
            wire::SNAP_CANCEL => {
                let incarnation = input.u64().map_err(io)?;
                let attempt = input.u64().map_err(io)?;
                input.done().map_err(io)?;
                let mut slots = self.0.snapshot.lock().map_err(|_| PortError::Io)?;
                match slots.as_ref() {
                    Some(slot)
                        if slot.token.incarnation == incarnation
                            && slot.token.attempt == attempt
                            && slot.complete.is_none() =>
                    {
                        // Abandon the owned attempt before publication: the
                        // frontier is released and nothing was published.
                        slots.take();
                        let mut state = self.state()?;
                        state.release_frontier();
                    }
                    _ => return Err(PortError::Invalid),
                }
            }
            wire::COMPLETE_BEGIN => {
                let incarnation = input.u64().map_err(io)?;
                let attempt = input.u64().map_err(io)?;
                let root = input.object().map_err(io)?;
                let head = input.head().map_err(io)?;
                input.done().map_err(io)?;
                let mut slot = self.0.snapshot.lock().map_err(|_| PortError::Io)?;
                let slot = match slot.as_mut() {
                    Some(slot)
                        if slot.token.incarnation == incarnation
                            && slot.token.attempt == attempt =>
                    {
                        slot
                    }
                    _ => return Err(PortError::Invalid),
                };
                // A retry after a failed END restarts the record stream.
                slot.complete = Some(PendingComplete {
                    root,
                    head,
                    received: 0,
                    applied: 0,
                    skipped: 0,
                });
            }
            wire::COMPLETE_NODE => {
                let incarnation = input.u64().map_err(io)?;
                let attempt = input.u64().map_err(io)?;
                let mut slot = self.0.snapshot.lock().map_err(|_| PortError::Io)?;
                let slot = match slot.as_mut() {
                    Some(slot)
                        if slot.token.incarnation == incarnation
                            && slot.token.attempt == attempt
                            && slot.complete.is_some() =>
                    {
                        slot
                    }
                    _ => return Err(PortError::Invalid),
                };
                let pending = slot.complete.as_mut().ok_or(PortError::Invalid)?;
                let mut records = 0;
                while !input.0.is_empty() {
                    if records == wire::COMPLETE_PAGE_RECORDS {
                        return Err(PortError::Invalid);
                    }
                    let node = NodeId(input.u64().map_err(io)?);
                    let revision = input.u64().map_err(io)?;
                    let inode = layerfs_content::tree::inode::InodeId::from_slice(
                        input.raw(32).map_err(io)?,
                    )
                    .map_err(|_| PortError::Invalid)?;
                    let content = input.object().map_err(io)?;
                    let attr = decode_complete_attr(&mut input, node)?;
                    let mut state = self.state()?;
                    let applied = state
                        .install_covered_record(node, revision, inode, content, attr)
                        .map_err(core)?;
                    pending.received += 1;
                    if applied {
                        pending.applied += 1;
                    } else {
                        pending.skipped += 1;
                    }
                    records += 1;
                }
            }
            wire::COMPLETE_END => {
                let incarnation = input.u64().map_err(io)?;
                let attempt = input.u64().map_err(io)?;
                let generation = input.u64().map_err(io)?;
                let count = input.u64().map_err(io)?;
                input.done().map_err(io)?;
                let (applied, skipped) = {
                    let mut slots = self.0.snapshot.lock().map_err(|_| PortError::Io)?;
                    let (root, head, received, applied, skipped) = match slots.as_mut() {
                        Some(slot)
                            if slot.token.incarnation == incarnation
                                && slot.token.attempt == attempt
                                && slot.token.generation == generation
                                && slot.complete.is_some() =>
                        {
                            let pending = slot.complete.take().ok_or(PortError::Invalid)?;
                            (
                                pending.root,
                                pending.head,
                                pending.received,
                                pending.applied,
                                pending.skipped,
                            )
                        }
                        _ => return Err(PortError::Invalid),
                    };
                    if received != count {
                        // An incomplete record stream never finishes the
                        // generation; the host retries the exact transaction.
                        slots.take();
                        let mut state = self.state()?;
                        state.release_frontier();
                        return Err(PortError::Invalid);
                    }
                    {
                        let mut state = self.state()?;
                        state.finish_covered_generation(root, generation);
                    }
                    *self.0.head.lock().map_err(|_| PortError::Io)? = head;
                    slots.take();
                    (applied, skipped)
                };
                wire::u64_out(&mut out, applied);
                wire::u64_out(&mut out, skipped);
                let spool = self.0.spool.clone();
                self.0
                    .scheduler
                    .physical(move || spool.retire_idle().map_err(|_| wire::invalid()))
                    .await
                    .map_err(io)?;
            }
            wire::OBSERVE => {
                input.done().map_err(io)?;
                let state = self.state()?;
                wire::u64_out(&mut out, state.covered_generation);
                wire::u64_out(&mut out, state.dirty.len() as u64);
                wire::u64_out(&mut out, state.spool_bytes);
                wire::u64_out(&mut out, self.0.spool.physical_bytes());
                wire::u64_out(&mut out, self.0.spool.physical_peak_bytes());
                let head = self.0.head.lock().map_err(|_| PortError::Io)?;
                wire::bytes_out(&mut out, head.as_ref().map_or(&[][..], |bytes| &bytes[..]))
                    .map_err(io)?;
            }
            _ => return Err(PortError::Invalid),
        }
        if let Some(started) = diagnostic_started {
            if let Some(diagnostic) = completed_diagnostic.as_mut() {
                diagnostic.values[EditMetric::Control as usize] += ns(started);
                wire::u64_out(&mut out, wire::EDIT_DIAGNOSTIC_VERSION);
                wire::bytes_out(&mut out, &diagnostic.nonce).map_err(io)?;
                for value in diagnostic.values {
                    wire::u64_out(&mut out, value);
                }
            } else if let Some(pending) = self.0.edit.lock().map_err(|_| PortError::Io)?.as_mut() {
                note_edit(&mut pending.diagnostic, EditMetric::Control, Some(started));
            }
        }
        Ok(out)
    }

    /// Handle one host-initiated snapshot-lane frame. The lane is separate
    /// from control so bulk frozen-input transfer can never starve SDK
    /// edits, status or immutable-base reads.
    pub(crate) async fn snapshot_request(&self, bytes: &[u8]) -> PortResult<Vec<u8>> {
        let mut input = Input(bytes);
        let opcode = input.byte().map_err(io)?;
        let mut out = Vec::new();
        let incarnation = input.u64().map_err(io)?;
        let attempt = input.u64().map_err(io)?;
        {
            let slots = self.0.snapshot.lock().map_err(|_| PortError::Io)?;
            match slots.as_ref() {
                Some(slot)
                    if slot.token.incarnation == incarnation && slot.token.attempt == attempt => {}
                _ => return Err(PortError::Invalid),
            }
        }
        match opcode {
            wire::SNAP_RECORDS => {
                let after = NodeId(input.u64().map_err(io)?);
                input.done().map_err(io)?;
                let mut page = Vec::with_capacity(wire::FACT_PAGE_BYTES);
                let mut count = 0u64;
                // Set when the scan stopped at a page boundary with frontier
                // ids still unread: a short page is not an exhausted frontier,
                // because a byte-bounded page can hold far fewer than
                // `FACT_PAGE_NODES` records.
                let mut more = false;
                let generation;
                {
                    let state = self.state()?;
                    generation = {
                        let slots = self.0.snapshot.lock().map_err(|_| PortError::Io)?;
                        slots
                            .as_ref()
                            .map(|slot| slot.token.generation)
                            .ok_or(PortError::Invalid)?
                    };
                    for id in state.frontier_ids() {
                        if id <= after {
                            continue;
                        }
                        let Some(node) = state.frozen_node(id) else {
                            // A frontier id whose node vanished and was never
                            // retained cannot happen: protect runs before
                            // every removal.
                            return Err(PortError::Io);
                        };
                        let encoded = wire::node_out(id, &node).map_err(io)?;
                        if encoded.len() + 4 > wire::MAX_FRAME {
                            // Bounded pages cannot carry an oversize node;
                            // fail loudly instead of truncating input.
                            return Err(PortError::NoSpace);
                        }
                        if count != 0
                            && (page.len() + encoded.len() + 4 > wire::FACT_PAGE_BYTES
                                || count == wire::FACT_PAGE_NODES as u64)
                        {
                            // This id and every id after it stay for the next
                            // page; the frontier is not exhausted.
                            more = true;
                            break;
                        }
                        wire::bytes_out(&mut page, &encoded).map_err(io)?;
                        count += 1;
                    }
                }
                // The generation in the token is the capture generation; a
                // mismatch means the frontier was replaced under us.
                let slots = self.0.snapshot.lock().map_err(|_| PortError::Io)?;
                match slots.as_ref() {
                    Some(slot)
                        if slot.token.incarnation == incarnation
                            && slot.token.attempt == attempt => {}
                    _ => return Err(PortError::Invalid),
                }
                let last = if count == 0 {
                    after
                } else {
                    // Decode the last record's id from the page tail.
                    let mut tail = Input(&page);
                    let mut last = after;
                    for _ in 0..count {
                        let record = tail.bytes().map_err(io)?;
                        let mut record_input = Input(record);
                        last = NodeId(record_input.u64().map_err(io)?);
                    }
                    last
                };
                let done = u8::from(!more);
                out.push(done);
                wire::u64_out(&mut out, generation);
                wire::u64_out(&mut out, count);
                wire::u64_out(&mut out, last.0);
                out.extend_from_slice(&page);
            }
            wire::SNAP_READ => {
                let segment = BackingId(input.u64().map_err(io)?);
                let offset = input.u64().map_err(io)?;
                let len = input.u32().map_err(io)? as u64;
                input.done().map_err(io)?;
                if len > 1024 * 1024 {
                    return Err(PortError::Invalid);
                }
                // Validate the segment exists and the range is within the
                // reserved high-water before any physical read.
                let high_water = self.0.spool.segment_len(segment)?;
                if offset.checked_add(len).is_none_or(|end| end > high_water) {
                    return Err(PortError::Invalid);
                }
                let reference = self.0.spool.segment_reference(segment)?;
                let spool = self.0.spool.clone();
                let bytes = self
                    .0
                    .scheduler
                    .physical(move || {
                        let bytes = spool
                            .read(&reference, offset, len)
                            .map_err(|_| wire::invalid())?;
                        // The host owns these bytes now, so the sandbox releases
                        // its resident copy: a bulk transfer must not re-inflate
                        // the sandbox's memory charge with the payload it hands
                        // over, and a re-read is served from storage instead.
                        spool.evict_served(&reference, offset, len);
                        Ok(bytes)
                    })
                    .await
                    .map_err(io)?;
                if bytes.len() as u64 != len {
                    return Err(PortError::Io);
                }
                out.extend_from_slice(&bytes);
            }
            _ => return Err(PortError::Invalid),
        }
        Ok(out)
    }

    pub fn serve_control(
        &self,
        endpoint: String,
        capability: [u8; 32],
    ) -> std::io::Result<LiveControl> {
        use std::net::ToSocketAddrs;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let address = endpoint
            .to_socket_addrs()?
            .next()
            .ok_or_else(wire::invalid)?;
        let observer_owner = self.clone();
        let observer: tokio::task::JoinHandle<std::io::Result<()>> =
            self.0.scheduler.handle.spawn(async move {
                let mut stream = tokio::net::TcpStream::connect(address).await?;
                stream.set_nodelay(true)?;
                stream.write_all(&capability).await?;
                stream.write_u8(b'o').await?;
                if stream.read_u8().await? != 1 {
                    return Err(wire::invalid());
                }
                loop {
                    if stream.read_u32().await? != 1 || stream.read_u8().await? != wire::OBSERVE {
                        return Err(wire::invalid());
                    }
                    let result = observer_owner
                        .control_request(&[wire::OBSERVE])
                        .await
                        .map_err(|_| wire::invalid())?;
                    crate::live_transport::write_frame(&mut stream, Some(0), &result).await?;
                }
            });
        let snapshot_owner = self.clone();
        let snapshot: tokio::task::JoinHandle<std::io::Result<()>> =
            self.0.scheduler.handle.spawn(async move {
                use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
                let mut stream = tokio::net::TcpStream::connect(address).await?;
                stream.set_nodelay(true)?;
                stream.write_all(&capability).await?;
                stream.write_u8(b's').await?;
                if stream.read_u8().await? != 1 {
                    return Err(wire::invalid());
                }
                loop {
                    let len = stream.read_u32().await? as usize;
                    if len == 0 || len > wire::MAX_FRAME {
                        return Err(wire::invalid());
                    }
                    let _admitted = snapshot_owner
                        .0
                        .scheduler
                        .admit_snapshot(len + wire::MAX_FRAME)
                        .await?;
                    let mut bytes = vec![0; len];
                    stream.read_exact(&mut bytes).await?;
                    let result = snapshot_owner.snapshot_request(&bytes).await;
                    match result {
                        Ok(bytes) => {
                            crate::live_transport::write_frame(&mut stream, Some(0), &bytes)
                                .await?;
                        }
                        Err(error) => {
                            crate::live_transport::write_frame(
                                &mut stream,
                                Some(1),
                                &[crate::protocol::error_code(error)],
                            )
                            .await?;
                        }
                    }
                }
            });
        let (shutdown_send, shutdown) = std::sync::mpsc::sync_channel(1);
        let wake = shutdown_send.clone();
        let requested = Arc::new(AtomicBool::new(false));
        let requested_control = requested.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_control = stopped.clone();
        let stopped_wake = wake.clone();
        let (finished, mut finish) = tokio::sync::oneshot::channel();
        let owner = self.clone();
        let thread = self.0.scheduler.handle.spawn(async move {
            let result = async {
                let mut stream = tokio::net::TcpStream::connect(address).await?;
                stream.set_nodelay(true)?;
                stream.write_all(&capability).await?;
                stream.write_u8(b'c').await?;
                if stream.read_u8().await? != 1 {
                    return Err(wire::invalid());
                }
                loop {
                    let len = stream.read_u32().await? as usize;
                    if len == 0 || len > wire::MAX_FRAME {
                        return Err(wire::invalid());
                    }
                    let _admitted = owner
                        .0
                        .scheduler
                        .admit_lifecycle(len + wire::MAX_FRAME)
                        .await?;
                    let mut bytes = vec![0; len];
                    stream.read_exact(&mut bytes).await?;
                    let shutdown_requested = bytes == [wire::SHUTDOWN];
                    let result = if shutdown_requested {
                        requested_control.store(true, Ordering::Release);
                        match shutdown_send.try_send(()) {
                            Ok(()) | Err(std::sync::mpsc::TrySendError::Full(())) => {}
                            Err(_) => return Err(wire::invalid()),
                        }
                        if (&mut finish).await.map_err(|_| wire::invalid())? {
                            Ok(Vec::new())
                        } else {
                            Err(PortError::Io)
                        }
                    } else {
                        owner.control_request(&bytes).await
                    };
                    if result.is_err()
                        && matches!(
                            bytes.first(),
                            Some(&wire::EDIT_BEGIN)
                                | Some(&wire::EDIT_PART)
                                | Some(&wire::EDIT_END)
                        )
                    {
                        owner.0.edit.lock().map_err(|_| wire::invalid())?.take();
                    }
                    match result {
                        Ok(bytes) => {
                            crate::live_transport::write_frame(&mut stream, Some(0), &bytes)
                                .await?;
                        }
                        Err(error) => {
                            crate::live_transport::write_frame(
                                &mut stream,
                                Some(1),
                                &[crate::protocol::error_code(error)],
                            )
                            .await?;
                        }
                    }
                    if shutdown_requested {
                        return Ok(());
                    }
                }
            }
            .await;
            stopped_control.store(true, Ordering::Release);
            let _ = stopped_wake.try_send(());
            result
        });
        Ok(LiveControl {
            shutdown,
            wake,
            requested,
            stopped,
            finished: Some(finished),
            thread: Some(thread),
            observer: Some(observer),
            snapshot: Some(snapshot),
        })
    }
}

pub struct LiveControl {
    shutdown: std::sync::mpsc::Receiver<()>,
    wake: std::sync::mpsc::SyncSender<()>,
    requested: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    finished: Option<tokio::sync::oneshot::Sender<bool>>,
    thread: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    observer: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    snapshot: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
}
impl LiveControl {
    pub fn wait_for_shutdown(&self) -> std::io::Result<()> {
        self.shutdown.recv().map_err(|_| wire::invalid())?;
        if self.shutdown_requested() {
            Ok(())
        } else {
            Err(wire::invalid())
        }
    }
    pub fn shutdown_waker(&self) -> std::sync::mpsc::SyncSender<()> {
        self.wake.clone()
    }
    pub fn shutdown_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
    pub fn poll_shutdown(&self, timeout: std::time::Duration) -> std::io::Result<bool> {
        match self.shutdown.recv_timeout(timeout) {
            Ok(()) if self.stopped.load(Ordering::Acquire) && !self.shutdown_requested() => {
                Err(wire::invalid())
            }
            Ok(()) => Ok(true),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(false),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(wire::invalid()),
        }
    }
    pub fn cancel(mut self) -> std::io::Result<()> {
        let runtime = LiveRuntime::shared()?;
        for task in [
            self.thread.take(),
            self.observer.take(),
            self.snapshot.take(),
        ]
        .into_iter()
        .flatten()
        {
            task.abort();
            let _ = runtime.block_on(task);
        }
        Ok(())
    }

    pub fn finish_shutdown(mut self, success: bool) -> std::io::Result<()> {
        self.finished
            .take()
            .ok_or_else(wire::invalid)?
            .send(success)
            .map_err(|_| wire::invalid())?;
        let result = LiveRuntime::shared()?
            .block_on(self.thread.take().ok_or_else(wire::invalid)?)
            .map_err(|_| wire::invalid())?;
        for task in [self.observer.take(), self.snapshot.take()]
            .into_iter()
            .flatten()
        {
            task.abort();
            let _ = LiveRuntime::shared()?.block_on(task);
        }
        result
    }
}

impl Drop for LiveControl {
    fn drop(&mut self) {
        if let Some(thread) = &self.thread {
            thread.abort();
        }
        for task in [self.observer.as_ref(), self.snapshot.as_ref()]
            .into_iter()
            .flatten()
        {
            task.abort();
        }
    }
}

fn ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod immutable_acquisition_tests {
    use super::*;
    use layerfs_content::{
        file::content::FileContentRoot, tree::directory::DirectoryStateRoot, tree::inode::InodeId,
        ObjectId,
    };
    use layerfs_workspace_core::{DirectoryData, FileData, Node};
    use std::time::Duration;

    fn identity(label: &[u8]) -> ObjectId {
        ObjectId::for_bytes(label)
    }

    fn node(data: Data, directory: bool) -> Node {
        Node {
            revision: 0,
            canonical: Some(InodeId([if directory { 1 } else { 2 }; 32])),
            paths: Default::default(),
            mode: if directory { 0o755 } else { 0o644 },
            links: if directory { 2 } else { 1 },
            pins: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            data,
        }
    }

    fn seed() -> Vec<u8> {
        let mut root = node(
            Data::Directory(DirectoryData {
                base: Some(DirectoryStateRoot(identity(b"directory"))),
                changes: Default::default(),
            }),
            true,
        );
        root.paths.insert(String::new());
        let policy = ResourcePolicy::default();
        let mut out = identity(b"old namespace").as_bytes().to_vec();
        wire::bytes_out(&mut out, &[]).unwrap();
        wire::u64_out(&mut out, policy.max_spool_bytes);
        wire::u64_out(&mut out, policy.max_final_delta_memory_bytes);
        out.extend(wire::node_out(ROOT, &root).unwrap());
        out
    }

    #[derive(Default)]
    struct BarrierBacking {
        base: Vec<u8>,
        seen: Vec<u8>,
        fail: Option<u8>,
    }
    impl BarrierBacking {
        fn request(&mut self, bytes: &[u8]) -> PortResult<Vec<u8>> {
            let mut input = Input(bytes);
            let opcode = input.byte().unwrap();
            self.seen.push(opcode);
            if self.fail == Some(opcode) {
                self.fail = None;
                return Err(PortError::NoSpace);
            }
            let mut out = Vec::new();
            match opcode {
                wire::SEED => return Ok(seed()),
                wire::READ_BASE => {
                    let root = input.object().unwrap();
                    assert_eq!(root, identity(b"file root"));
                    let start = input.u64().unwrap() as usize;
                    let len = input.u32().unwrap() as usize;
                    out.extend_from_slice(&self.base[start..start + len]);
                }
                _ => panic!("unexpected backing opcode {opcode}"),
            }
            Ok(out)
        }
    }

    fn fact(content: &[u8], declared_len: u64) -> Vec<u8> {
        let file = node(
            Data::File(FileData::Base {
                root: FileContentRoot(identity(b"file root")),
                len: declared_len,
            }),
            false,
        );
        let mut out = Vec::new();
        wire::bytes_out(&mut out, &wire::node_out(NodeId(2), &file).unwrap()).unwrap();
        wire::bytes_out(&mut out, content).unwrap();
        out
    }

    fn lookup_reply(content: &[u8], declared_len: u64) -> Vec<u8> {
        let mut out = vec![1];
        out.extend(fact(content, declared_len));
        out.extend_from_slice(&1u32.to_be_bytes());
        wire::bytes_out(&mut out, b"sibling").unwrap();
        out.extend(fact(b"old", 3));
        out.push(0);
        out
    }

    fn backing_dir(label: &str) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "layerfs-owner-test-{label}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn barrier_owner(runtime: &LiveRuntime, backing: Arc<Mutex<BarrierBacking>>) -> LiveOwner {
        let handler = Arc::new(move |bytes: &[u8]| backing.lock().unwrap().request(bytes));
        let owner = runtime
            .block_on(LiveOwner::from_backing(
                BackingConnection::local(handler, runtime.scheduler()),
                runtime.scheduler(),
                backing_dir("barrier"),
            ))
            .unwrap();
        if let Data::Directory(directory) =
            &mut owner.state().unwrap().nodes.get_mut(&ROOT).unwrap().data
        {
            directory.base = None;
        }
        owner
    }

    #[test]
    fn local_writes_are_sandbox_owned_and_readable_without_backing_traffic() {
        let runtime = LiveRuntime::new().unwrap();
        let backing = Arc::new(Mutex::new(BarrierBacking::default()));
        let owner = barrier_owner(&runtime, backing.clone());
        let file = owner.create_file(ROOT, b"file", 0o644).unwrap().node;
        runtime
            .block_on(owner.write_owned(file, 0, b"abcdef"))
            .unwrap();
        runtime
            .block_on(owner.write_owned(file, 6, b"ghijkl"))
            .unwrap();
        assert_eq!(
            runtime.block_on(owner.read_owned(file, 0, 12)).unwrap(),
            b"abcdefghijkl"
        );
        // No host payload traffic exists on this route: only SEED was seen.
        assert_eq!(backing.lock().unwrap().seen, vec![wire::SEED]);
        // Volatile fsync: no durability flush, no host acknowledgment, but
        // errors surface and bytes remain readable.
        owner.fsync(Some(file)).unwrap();
        assert_eq!(
            runtime.block_on(owner.read_owned(file, 3, 4)).unwrap(),
            b"defg"
        );
        assert_eq!(
            backing.lock().unwrap().seen,
            vec![wire::SEED],
            "fsync performs no wire traffic"
        );
        owner.spool().destroy().unwrap();
    }

    #[test]
    fn failed_prepare_leaves_no_stranded_local_allocation() {
        let runtime = LiveRuntime::new().unwrap();
        let backing = Arc::new(Mutex::new(BarrierBacking::default()));
        let owner = barrier_owner(&runtime, backing.clone());
        let file = owner.create_file(ROOT, b"file", 0o644).unwrap().node;
        runtime
            .block_on(owner.write_owned(file, 0, b"abc"))
            .unwrap();
        // A write beyond every bound fails without poisoning the owner or
        // leaking a reservation.
        assert!(runtime
            .block_on(owner.write_owned(file, u64::MAX, b"bad"))
            .is_err());
        assert!(!owner.0.failed.load(Ordering::Acquire));
        owner.fsync(Some(file)).unwrap();
        assert_eq!(
            runtime.block_on(owner.read_owned(file, 0, 3)).unwrap(),
            b"abc"
        );
        runtime
            .block_on(owner.write_owned(file, 3, b"def"))
            .unwrap();
        assert_eq!(
            runtime.block_on(owner.read_owned(file, 0, 6)).unwrap(),
            b"abcdef"
        );
        owner.spool().destroy().unwrap();
    }

    #[test]
    fn capture_serves_stable_frozen_records_and_payload_while_live_writes_continue() {
        let runtime = LiveRuntime::new().unwrap();
        let backing = Arc::new(Mutex::new(BarrierBacking::default()));
        let owner = barrier_owner(&runtime, backing.clone());
        let file = owner.create_file(ROOT, b"a", 0o644).unwrap().node;
        runtime
            .block_on(owner.write_owned(file, 0, b"AAAA"))
            .unwrap();

        // CAPTURE.
        let reply = runtime
            .block_on(owner.local_control(&[wire::CAPTURE]))
            .unwrap();
        let mut input = Input(&reply);
        let incarnation = input.u64().unwrap();
        let attempt = input.u64().unwrap();
        let generation = input.u64().unwrap();
        let frontier_len = input.u64().unwrap();
        assert!(frontier_len >= 2, "root + file");
        let _base_root = input.object().unwrap();
        let _head = input.head().unwrap();
        assert_eq!(incarnation, owner.incarnation());

        // Live successor writes continue without any pause.
        runtime
            .block_on(owner.write_owned(file, 0, b"aaaa"))
            .unwrap();

        // Pull the frozen records: the file's captured inline/spool state must
        // show the capture-time bytes, not the successor's.
        let mut records = Vec::new();
        let mut after = 0u64;
        loop {
            let mut request = vec![wire::SNAP_RECORDS];
            wire::u64_out(&mut request, incarnation);
            wire::u64_out(&mut request, attempt);
            wire::u64_out(&mut request, after);
            let page = runtime.block_on(owner.snapshot_request(&request)).unwrap();
            let mut page_input = Input(&page);
            let done = page_input.byte().unwrap();
            let _page_generation = page_input.u64().unwrap();
            let count = page_input.u64().unwrap();
            after = page_input.u64().unwrap();
            for _ in 0..count {
                let record = page_input.bytes().unwrap().to_vec();
                records.push(record);
            }
            if done == 1 || count == 0 {
                break;
            }
        }
        assert!(!records.is_empty());
        // The live view moved on.
        assert_eq!(
            runtime.block_on(owner.read_owned(file, 0, 4)).unwrap(),
            b"aaaa"
        );

        // COMPLETE with the captured revision: the re-mutated node keeps its
        // live state (record skipped), and the generation still finishes.
        let attr = {
            let state = owner.state().unwrap();
            state.nodes[&file].attr(file)
        };
        let mut begin = vec![wire::COMPLETE_BEGIN];
        wire::u64_out(&mut begin, incarnation);
        wire::u64_out(&mut begin, attempt);
        begin.extend_from_slice(identity(b"published-root").as_bytes());
        wire::bytes_out(&mut begin, &[]).unwrap();
        runtime.block_on(owner.local_control(&begin)).unwrap();

        let captured_revision = {
            // The revision at capture is one less than the live revision
            // after the post-capture write.
            let state = owner.state().unwrap();
            state.nodes[&file].revision - 1
        };
        let mut node_frame = vec![wire::COMPLETE_NODE];
        wire::u64_out(&mut node_frame, incarnation);
        wire::u64_out(&mut node_frame, attempt);
        wire::u64_out(&mut node_frame, file.0);
        wire::u64_out(&mut node_frame, captured_revision);
        node_frame.extend_from_slice(&[8; 32]);
        node_frame.extend_from_slice(identity(b"content").as_bytes());
        wire::u64_out(&mut node_frame, attr.size);
        node_frame.extend_from_slice(&attr.mode.to_be_bytes());
        node_frame.extend_from_slice(&attr.links.to_be_bytes());
        wire::u64_out(&mut node_frame, attr.mtime_seconds as u64);
        node_frame.extend_from_slice(&attr.mtime_nanoseconds.to_be_bytes());
        node_frame.push(1);
        runtime.block_on(owner.local_control(&node_frame)).unwrap();

        let mut end = vec![wire::COMPLETE_END];
        wire::u64_out(&mut end, incarnation);
        wire::u64_out(&mut end, attempt);
        wire::u64_out(&mut end, generation);
        wire::u64_out(&mut end, 1);
        let summary = runtime.block_on(owner.local_control(&end)).unwrap();
        let mut summary_input = Input(&summary);
        assert_eq!(summary_input.u64().unwrap(), 0, "skipped: re-mutated");
        assert_eq!(summary_input.u64().unwrap(), 1);

        // The G+1 change survives completion as live dirty state.
        assert!(owner.state().unwrap().dirty.contains(&file));
        assert_eq!(
            runtime.block_on(owner.read_owned(file, 0, 4)).unwrap(),
            b"aaaa"
        );
        // A late duplicate completion cannot act: the slot is released.
        let mut stale = vec![wire::COMPLETE_BEGIN];
        wire::u64_out(&mut stale, incarnation);
        wire::u64_out(&mut stale, attempt);
        stale.extend_from_slice(identity(b"other").as_bytes());
        wire::bytes_out(&mut stale, &[]).unwrap();
        assert_eq!(
            runtime.block_on(owner.local_control(&stale)),
            Err(PortError::Invalid)
        );
        owner.spool().destroy().unwrap();
    }

    #[test]
    fn stale_snapshot_tokens_and_cancel_release_the_frontier() {
        let runtime = LiveRuntime::new().unwrap();
        let backing = Arc::new(Mutex::new(BarrierBacking::default()));
        let owner = barrier_owner(&runtime, backing.clone());
        let file = owner.create_file(ROOT, b"a", 0o644).unwrap().node;
        runtime
            .block_on(owner.write_owned(file, 0, b"AAAA"))
            .unwrap();
        let reply = runtime
            .block_on(owner.local_control(&[wire::CAPTURE]))
            .unwrap();
        let mut input = Input(&reply);
        let incarnation = input.u64().unwrap();
        let attempt = input.u64().unwrap();
        let _generation = input.u64().unwrap();
        let _frontier_len = input.u64().unwrap();

        // Wrong incarnation and wrong attempt are rejected on both lanes.
        for (bad_incarnation, bad_attempt) in
            [(incarnation + 1, attempt), (incarnation, attempt + 1)]
        {
            let mut request = vec![wire::SNAP_RECORDS];
            wire::u64_out(&mut request, bad_incarnation);
            wire::u64_out(&mut request, bad_attempt);
            wire::u64_out(&mut request, 0);
            assert_eq!(
                runtime.block_on(owner.snapshot_request(&request)),
                Err(PortError::Invalid)
            );
            let mut cancel = vec![wire::SNAP_CANCEL];
            wire::u64_out(&mut cancel, bad_incarnation);
            wire::u64_out(&mut cancel, bad_attempt);
            assert_eq!(
                runtime.block_on(owner.local_control(&cancel)),
                Err(PortError::Invalid)
            );
        }

        // A second capture while the attempt is unresolved is Busy.
        assert_eq!(
            runtime.block_on(owner.local_control(&[wire::CAPTURE])),
            Err(PortError::Busy)
        );

        // CANCEL releases the frontier and the next capture succeeds.
        let mut cancel = vec![wire::SNAP_CANCEL];
        wire::u64_out(&mut cancel, incarnation);
        wire::u64_out(&mut cancel, attempt);
        runtime.block_on(owner.local_control(&cancel)).unwrap();
        assert!(!owner.state().unwrap().frontier_active());
        runtime
            .block_on(owner.local_control(&[wire::CAPTURE]))
            .unwrap();
        owner.spool().destroy().unwrap();
    }

    #[test]
    fn delayed_lookup_cannot_install_after_root_or_parent_revision_changes() {
        let runtime = LiveRuntime::new().unwrap();
        for change_root in [true, false] {
            let (entered, pending) = std::sync::mpsc::channel();
            let (release, released) = std::sync::mpsc::channel();
            let released = Mutex::new(released);
            let handler = Arc::new(move |request: &[u8]| match request.first() {
                Some(&wire::SEED) => Ok(seed()),
                Some(&wire::LOOKUP) => {
                    entered.send(()).unwrap();
                    released
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    Ok(lookup_reply(b"old", 3))
                }
                _ => Err(PortError::Io),
            });
            let owner = runtime
                .block_on(LiveOwner::local(
                    handler,
                    runtime.scheduler(),
                    backing_dir("lookup"),
                ))
                .unwrap();
            let acquiring = owner.clone();
            let task = runtime
                .scheduler()
                .handle
                .spawn(async move { acquiring.lookup_async(ROOT, b"file").await });
            pending.recv_timeout(Duration::from_secs(5)).unwrap();
            // Exercise the install race directly: a completed checkpoint changes
            // base_root; an SDK namespace/metadata edit changes the parent revision.
            let nodes_after_change = {
                let mut state = owner.state().unwrap();
                if change_root {
                    state.base_root = identity(b"new namespace");
                } else {
                    state.chmod(ROOT, 0o700).unwrap();
                }
                state.nodes.clone()
            };
            release.send(()).unwrap();
            assert_eq!(runtime.block_on(task).unwrap(), Err(PortError::Io));
            assert_eq!(owner.state().unwrap().nodes, nodes_after_change);
            assert!(
                owner
                    .0
                    .scheduler
                    .immutable_reads()
                    .get_name(
                        owner.0.read_scope,
                        identity(b"old namespace"),
                        identity(b"directory"),
                        b"sibling",
                    )
                    .is_none(),
                "stale acquisition must not install sibling metadata"
            );
        }
    }

    #[test]
    fn malformed_grouped_replies_do_not_install_live_nodes() {
        let runtime = LiveRuntime::new().unwrap();
        let mut truncated = lookup_reply(b"old", 3);
        truncated.pop();
        let oversized = vec![b'x'; wire::IMMUTABLE_PREFETCH_FILE_BYTES + 1];
        let mut invalid_count = vec![0];
        invalid_count.extend_from_slice(&128u32.to_be_bytes());
        for reply in [
            lookup_reply(b"no", 3),
            lookup_reply(&oversized, oversized.len() as u64),
            truncated,
            invalid_count,
        ] {
            let handler = Arc::new(move |request: &[u8]| match request.first() {
                Some(&wire::SEED) => Ok(seed()),
                Some(&wire::LOOKUP) => Ok(reply.clone()),
                _ => Err(PortError::Io),
            });
            let owner = runtime
                .block_on(LiveOwner::local(
                    handler,
                    runtime.scheduler(),
                    backing_dir("lookup"),
                ))
                .unwrap();
            let before = owner.state().unwrap().nodes.clone();
            assert_eq!(
                runtime.block_on(owner.lookup_async(ROOT, b"file")),
                Err(PortError::Io)
            );
            assert_eq!(owner.state().unwrap().nodes, before);
            assert!(owner
                .0
                .scheduler
                .immutable_reads()
                .get_name(
                    owner.0.read_scope,
                    identity(b"old namespace"),
                    identity(b"directory"),
                    b"sibling",
                )
                .is_none());
        }
    }

    fn kernel_owner(runtime: &LiveRuntime) -> LiveOwner {
        let handler = Arc::new(|request: &[u8]| match request {
            [wire::SEED] => Ok(seed()),
            _ => Err(PortError::Io),
        });
        let owner = runtime
            .block_on(LiveOwner::local(
                handler,
                runtime.scheduler(),
                backing_dir("lookup"),
            ))
            .unwrap();
        if let Data::Directory(directory) =
            &mut owner.state().unwrap().nodes.get_mut(&ROOT).unwrap().data
        {
            directory.base = None;
        }
        owner
    }

    #[test]
    fn sdk_edit_metadata_lookup_keeps_regular_grouped_prefetch() {
        let runtime = LiveRuntime::new().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let handler = Arc::new(move |request: &[u8]| match request.first() {
            Some(&wire::SEED) => Ok(seed()),
            Some(&wire::LOOKUP_METADATA) => {
                observed.lock().unwrap().push(wire::LOOKUP_METADATA);
                let mut reply = vec![1];
                reply.extend(fact(&[], 3));
                reply.extend_from_slice(&0u32.to_be_bytes());
                reply.push(0);
                Ok(reply)
            }
            Some(&wire::LOOKUP) => {
                observed.lock().unwrap().push(wire::LOOKUP);
                Ok(lookup_reply(b"old", 3))
            }
            _ => Err(PortError::Io),
        });
        let owner = runtime
            .block_on(LiveOwner::local(
                handler,
                runtime.scheduler(),
                backing_dir("lookup"),
            ))
            .unwrap();
        runtime.block_on(async {
            let mut begin = vec![wire::EDIT_BEGIN];
            wire::bytes_out(&mut begin, b"file").unwrap();
            wire::u64_out(&mut begin, 1);
            owner.local_control(&begin).await.unwrap();
            let mut part = vec![wire::EDIT_PART];
            wire::u64_out(&mut part, 0);
            wire::u64_out(&mut part, 3);
            part.push(0);
            wire::bytes_out(&mut part, b"new").unwrap();
            owner.local_control(&part).await.unwrap();
            owner.local_control(&[wire::EDIT_END]).await.unwrap();
            assert_eq!(*requests.lock().unwrap(), [wire::LOOKUP_METADATA]);
            let edited = owner.lookup_async(ROOT, b"file").await.unwrap();
            assert_eq!(owner.read_owned(edited.node, 0, 3).await.unwrap(), b"new");
            owner.lookup_async(ROOT, b"other").await.unwrap();
            owner.lookup_async(ROOT, b"sibling").await.unwrap();
            assert_eq!(
                *requests.lock().unwrap(),
                [wire::LOOKUP_METADATA, wire::LOOKUP]
            );
            assert_eq!(owner.read_owned(edited.node, 0, 3).await.unwrap(), b"new");
        });
    }

    #[test]
    fn edit_diagnostic_preserves_plain_mutation_and_failed_batch_retry() {
        let runtime = LiveRuntime::new().unwrap();
        let owner = kernel_owner(&runtime);
        let node = owner.create_file(ROOT, b"edited", 0o600).unwrap().node;
        let nonce = b"0123456789abcdef";
        let begin = |diagnostic: bool| {
            let mut bytes = vec![wire::EDIT_BEGIN];
            wire::bytes_out(&mut bytes, b"edited").unwrap();
            wire::u64_out(&mut bytes, 1);
            if diagnostic {
                wire::bytes_out(&mut bytes, nonce).unwrap();
            }
            bytes
        };
        let part = |start, delete, replacement: &[u8]| {
            let mut bytes = vec![wire::EDIT_PART];
            wire::u64_out(&mut bytes, start);
            wire::u64_out(&mut bytes, delete);
            bytes.push(0);
            wire::bytes_out(&mut bytes, replacement).unwrap();
            bytes
        };
        runtime.block_on(async {
            owner.local_control(&begin(true)).await.unwrap();
            owner.local_control(&part(0, 0, b"abc")).await.unwrap();
            let reply = owner.local_control(&[wire::EDIT_END]).await.unwrap();
            let values = wire::read_edit_diagnostic(&reply, nonce).unwrap();
            assert!(values[EditMetric::Control as usize] > 0);
            assert!(values[EditMetric::Prepare as usize] > 0);
            assert_eq!(values[EditMetric::FactNodes as usize], 0);
            assert_eq!(values[EditMetric::FactBytes as usize], 0);
            assert_eq!(owner.read_owned(node, 0, 10).await.unwrap(), b"abc");
            // A capture retains the edited frontier without any pause; cancel
            // releases it. This replaces the old FREEZE/RESUME facts export.
            let capture = owner.local_control(&[wire::CAPTURE]).await.unwrap();
            let mut input = Input(&capture);
            let incarnation = input.u64().unwrap();
            let attempt = input.u64().unwrap();
            let generation = input.u64().unwrap();
            let frontier_len = input.u64().unwrap();
            assert!(frontier_len >= 2);
            let state = owner.state().unwrap();
            assert_eq!(generation, state.mutation_generation);
            drop(state);
            let mut cancel = vec![wire::SNAP_CANCEL];
            wire::u64_out(&mut cancel, incarnation);
            wire::u64_out(&mut cancel, attempt);
            owner.local_control(&cancel).await.unwrap();
            owner.local_control(&begin(true)).await.unwrap();
            assert!(owner.local_control(&part(99, 1, b"bad")).await.is_err());
            assert!(owner.0.edit.lock().unwrap().is_none());
            assert_eq!(owner.read_owned(node, 0, 10).await.unwrap(), b"abc");
            assert!(owner.local_control(&begin(false)).await.unwrap().is_empty());
            assert!(owner
                .local_control(&part(0, 3, b"xyz"))
                .await
                .unwrap()
                .is_empty());
            assert!(owner
                .local_control(&[wire::EDIT_END])
                .await
                .unwrap()
                .is_empty());
            assert_eq!(owner.read_owned(node, 0, 10).await.unwrap(), b"xyz");
            // The second edit bumped the mutation generation; a fresh capture
            // observes the newer state.
            let capture = owner.local_control(&[wire::CAPTURE]).await.unwrap();
            let mut input = Input(&capture);
            let incarnation = input.u64().unwrap();
            let attempt = input.u64().unwrap();
            let newer = input.u64().unwrap();
            assert!(newer > generation);
            let mut cancel = vec![wire::SNAP_CANCEL];
            wire::u64_out(&mut cancel, incarnation);
            wire::u64_out(&mut cancel, attempt);
            owner.local_control(&cancel).await.unwrap();
        });
    }

    #[test]
    fn stale_folio_preserves_sdk_ranges_and_cannot_regrow_truncated_tail() {
        let runtime = LiveRuntime::new().unwrap();
        let handler = Arc::new(|request: &[u8]| match request {
            [wire::SEED] => Ok(seed()),
            [wire::RESERVE, ..] => {
                let mut reply = Vec::new();
                for value in [7, 0, 1024 * 1024] {
                    wire::u64_out(&mut reply, value);
                }
                Ok(reply)
            }
            _ => Err(PortError::Io),
        });
        let owner = runtime
            .block_on(LiveOwner::local(
                handler,
                runtime.scheduler(),
                backing_dir("lookup"),
            ))
            .unwrap();
        if let Data::Directory(directory) =
            &mut owner.state().unwrap().nodes.get_mut(&ROOT).unwrap().data
        {
            directory.base = None;
        }
        let node = owner.create_file(ROOT, b"edited", 0o600).unwrap().node;
        let file = {
            let mut state = owner.state().unwrap();
            let prepared = state
                .prepare_splices(
                    node,
                    vec![(
                        0,
                        0,
                        Some(Piece::Inline {
                            bytes: Arc::from(&b"ASB"[..]),
                            offset: 0,
                            len: 3,
                        }),
                    )],
                )
                .unwrap();
            state.apply_edit(prepared).unwrap();
            let Data::File(file) = &state.nodes[&node].data else {
                panic!("file")
            };
            file.clone()
        };
        *owner.0.kernel_edit.lock().unwrap() = Some(Arc::new(KernelEdit {
            node,
            file,
            ranges: vec![1..2],
            _charge: owner.0.scheduler.reserve_live(1024).unwrap(),
        }));
        let guard = KernelEditGuard(&owner.0.kernel_edit);
        assert_eq!(
            runtime
                .block_on(owner.write_owned(node, 0, b"Q0Z"))
                .unwrap(),
            3
        );
        assert_eq!(
            runtime.block_on(owner.read_owned(node, 0, 3)).unwrap(),
            b"QSZ"
        );
        // Model a callback admitted during reconciliation but still waiting
        // for inode ordering when the kernel notification returns.
        let cut = runtime.block_on(async { owner.0.gate.cache_flush().await.finish().await });
        let flush = cut.reopen_writeback();
        let order = runtime.block_on(owner.ordered(node)).unwrap();
        let callback = runtime.block_on(owner.0.gate.enter(true));
        let mut queued = Box::pin(async {
            let _callback = callback;
            owner.write_owned(node, 0, b"Q0Z").await
        });
        runtime.block_on(std::future::poll_fn(|context| {
            assert!(std::future::Future::poll(queued.as_mut(), context).is_pending());
            std::task::Poll::Ready(())
        }));
        let mut finished = Box::pin(guard.finish(flush));
        runtime.block_on(std::future::poll_fn(|context| {
            assert!(std::future::Future::poll(finished.as_mut(), context).is_pending());
            std::task::Poll::Ready(())
        }));
        assert!(owner.0.kernel_edit.lock().unwrap().is_some());
        assert!(owner.0.gate.try_ordinary().is_none());
        drop(order);
        assert_eq!(runtime.block_on(queued).unwrap(), 3);
        runtime.block_on(finished);
        assert!(owner.0.kernel_edit.lock().unwrap().is_none());
        assert!(owner.0.gate.try_ordinary().is_some());
        assert_eq!(
            runtime.block_on(owner.read_owned(node, 0, 3)).unwrap(),
            b"QSZ"
        );
        runtime.block_on(owner.write_owned(node, 1, b"N")).unwrap();
        assert_eq!(
            runtime.block_on(owner.read_owned(node, 0, 3)).unwrap(),
            b"QNZ"
        );
        let file = {
            let mut state = owner.state().unwrap();
            let prepared = state.prepare_splices(node, vec![(2, 1, None)]).unwrap();
            state.apply_edit(prepared).unwrap();
            let Data::File(file) = &state.nodes[&node].data else {
                panic!("file")
            };
            file.clone()
        };
        *owner.0.kernel_edit.lock().unwrap() = Some(Arc::new(KernelEdit {
            node,
            file,
            ranges: vec![2..u64::MAX],
            _charge: owner.0.scheduler.reserve_live(1024).unwrap(),
        }));
        let guard = KernelEditGuard(&owner.0.kernel_edit);
        assert_eq!(
            runtime
                .block_on(owner.write_owned(node, 0, b"abcde"))
                .unwrap(),
            5
        );
        assert_eq!(
            runtime.block_on(owner.read_owned(node, 0, 9)).unwrap(),
            b"ab"
        );
        drop(guard);
        runtime.block_on(owner.write_owned(node, 2, b"M")).unwrap();
        assert_eq!(
            runtime.block_on(owner.read_owned(node, 0, 9)).unwrap(),
            b"abM"
        );
    }

    #[test]
    fn kernel_references_preserve_unlinked_contents_until_last_forget() {
        let runtime = LiveRuntime::new().unwrap();
        let owner = kernel_owner(&runtime);
        let (attr, refs) = runtime
            .block_on(owner.kernel_entry_async(ROOT, b"held", KernelEntry::Create { mode: 0o600 }))
            .unwrap();
        refs.submitted();
        {
            let mut state = owner.state().unwrap();
            let edit = state
                .prepare_splices(
                    attr.node,
                    vec![(
                        0,
                        0,
                        Some(Piece::Inline {
                            bytes: Arc::from(&b"held"[..]),
                            offset: 0,
                            len: 4,
                        }),
                    )],
                )
                .unwrap();
            state.apply_edit(edit).unwrap();
        }
        let (again, refs) = runtime
            .block_on(owner.kernel_lookup_async(ROOT, b"held"))
            .unwrap();
        assert_eq!(again.node, attr.node);
        refs.submitted();
        assert_eq!(owner.state().unwrap().nodes[&attr.node].pins, 1);
        assert_eq!(
            owner.0.kernel_refs.lock().unwrap().entries[&attr.node].lookups,
            2
        );
        owner.unlink(ROOT, b"held", false).unwrap();
        assert_eq!(owner.read(attr.node, 0, 4).unwrap(), b"held");
        owner.kernel_forget(attr.node, 1).unwrap();
        assert_eq!(owner.read(attr.node, 0, 4).unwrap(), b"held");
        owner.kernel_forget(attr.node, 1).unwrap();
        assert_eq!(owner.attr(attr.node), Err(PortError::NotFound));
        assert!(owner.0.kernel_refs.lock().unwrap().entries.is_empty());
        owner.kernel_forget(ROOT, 1).unwrap();
    }

    #[test]
    fn kernel_reference_keeps_old_base_contents_after_unlink_and_root_change() {
        let runtime = LiveRuntime::new().unwrap();
        let handler = Arc::new(|request: &[u8]| match request.first() {
            Some(&wire::SEED) => Ok(seed()),
            Some(&wire::LOOKUP) => Ok(lookup_reply(b"old", 3)),
            _ => Err(PortError::Io),
        });
        let owner = runtime
            .block_on(LiveOwner::local(
                handler,
                runtime.scheduler(),
                backing_dir("lookup"),
            ))
            .unwrap();
        let (attr, refs) = runtime
            .block_on(owner.kernel_lookup_async(ROOT, b"file"))
            .unwrap();
        refs.submitted();
        owner.unlink(ROOT, b"file", false).unwrap();
        owner.state().unwrap().base_root = identity(b"new namespace");
        assert_eq!(owner.read(attr.node, 0, 3).unwrap(), b"old");
        owner.kernel_forget(attr.node, 1).unwrap();
        assert_eq!(owner.attr(attr.node), Err(PortError::NotFound));
    }

    #[test]
    fn kernel_page_rollback_overflow_and_detach_balance_pins() {
        let runtime = LiveRuntime::new().unwrap();
        let owner = kernel_owner(&runtime);
        let file = owner.create_file(ROOT, b"a", 0o600).unwrap().node;
        owner.link(file, ROOT, b"b").unwrap();
        let (page, mut refs) = runtime
            .block_on(owner.kernel_directory_page_async(ROOT, 0))
            .unwrap();
        assert_eq!(page.len(), 4);
        assert_eq!(
            owner.0.kernel_refs.lock().unwrap().entries[&file].lookups,
            2
        );
        assert_eq!(owner.state().unwrap().nodes[&file].pins, 1);
        refs.release_unemitted(1).unwrap();
        assert_eq!(
            owner.0.kernel_refs.lock().unwrap().entries[&file].lookups,
            1
        );
        drop(refs); // An attr conversion error or canceled reply rolls back the emitted prefix too.
        assert!(owner.0.kernel_refs.lock().unwrap().entries.is_empty());
        assert_eq!(owner.state().unwrap().nodes[&file].pins, 0);
        let (_, refs) = runtime
            .block_on(owner.kernel_directory_page_async(ROOT, 0))
            .unwrap();
        refs.submitted();
        owner
            .0
            .kernel_refs
            .lock()
            .unwrap()
            .entries
            .get_mut(&file)
            .unwrap()
            .lookups = u64::MAX;
        let before = owner.state().unwrap().nodes.clone();
        let generation = owner.state().unwrap().mutation_generation;
        assert_eq!(
            runtime
                .block_on(owner.kernel_entry_async(ROOT, b"c", KernelEntry::Link { node: file }))
                .err(),
            Some(PortError::Io)
        );
        assert_eq!(owner.state().unwrap().nodes, before);
        assert_eq!(owner.state().unwrap().mutation_generation, generation);
        owner
            .0
            .kernel_refs
            .lock()
            .unwrap()
            .entries
            .get_mut(&file)
            .unwrap()
            .lookups = 2;
        // Shutdown must drop an older Commit cut before draining callbacks itself.
        *owner.0.cut.lock().unwrap() =
            Some(runtime.block_on(async { owner.0.gate.cache_flush().await.finish().await }));
        owner.kernel_detach().unwrap();
        owner.kernel_detach().unwrap();
        assert!(owner.0.kernel_refs.lock().unwrap().entries.is_empty());
        assert_eq!(owner.state().unwrap().nodes[&file].pins, 0);
        assert_eq!(owner.lookup(ROOT, b"a").unwrap().node, file);
        assert_eq!(
            runtime
                .block_on(owner.kernel_lookup_async(ROOT, b"a"))
                .err(),
            Some(PortError::Io)
        );
        owner.kernel_forget(file, 2).unwrap(); // Late notification after drained detach is harmless.
    }

    fn prefill_owner(runtime: &LiveRuntime) -> (LiveOwner, NodeId, ObjectId) {
        let owner = kernel_owner(runtime);
        let file = owner.create_file(ROOT, b"prefill", 0o600).unwrap().node;
        let root = identity(b"prefill file");
        owner.state().unwrap().nodes.get_mut(&file).unwrap().data = Data::File(FileData::Base {
            root: FileContentRoot(root),
            len: 3,
        });
        let (_, references) = runtime
            .block_on(owner.kernel_lookup_async(ROOT, b"prefill"))
            .unwrap();
        references.submitted();
        owner
            .0
            .scheduler
            .immutable_reads()
            .insert(owner.0.read_scope, root, 0, b"old".to_vec());
        (owner, file, root)
    }

    #[test]
    fn writable_open_write_and_truncate_wait_for_prefill_even_after_forget() {
        let runtime = LiveRuntime::new().unwrap();
        for operation in 0..3 {
            let (owner, file, _) = prefill_owner(&runtime);
            assert!(owner.0.ordering.lock().unwrap().is_empty());
            let prefill = owner.begin_kernel_prefill(file).unwrap();
            assert!(owner.0.ordering.lock().unwrap().is_empty());
            owner.kernel_forget(file, 1).unwrap();
            let (_, references) = runtime
                .block_on(owner.kernel_lookup_async(ROOT, b"prefill"))
                .unwrap();
            references.submitted();
            let waiting = owner.clone();
            let task = runtime.scheduler().handle.spawn(async move {
                match operation {
                    0 => waiting.prepare_kernel_open(file, true).await,
                    1 => waiting.truncate_async(file, 3).await,
                    _ => waiting.write_owned(file, 0, b"x").await.map(|_| ()),
                }
            });
            runtime.block_on(async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !owner.0.kernel_refs.lock().unwrap().entries[&file].writable_seen {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
            });
            assert!(!task.is_finished());
            drop(prefill);
            let result = runtime.block_on(task).unwrap();
            result.unwrap();
            assert!(owner.0.active_prefill.lock().unwrap().is_none());
            assert!(owner.begin_kernel_prefill(file).is_none());
            if operation == 2 {
                // The sandbox-local write completes after the prefill is
                // released; its bytes are locally readable.
                assert_eq!(
                    runtime.block_on(owner.read_owned(file, 0, 4)).unwrap(),
                    b"xld"
                );
            }
        }
    }

    #[test]
    fn canceled_prefill_waiter_keeps_blocking_job_guards_and_revalidates_root() {
        let runtime = LiveRuntime::new().unwrap();
        let (owner, file, root) = prefill_owner(&runtime);
        let (started, entered) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = Arc::new(Mutex::new(released));
        let (done, finished) = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let prefill = owner.begin_kernel_prefill(file).unwrap();
                    let done = prefill.completion.state.clone();
                    let started = started.clone();
                    let released = released.clone();
                    let (finished, waiting) = tokio::sync::oneshot::channel::<()>();
                    if owner.0.scheduler.try_prefill(Box::new(move || {
                        let _prefill = prefill;
                        let _finished = finished;
                        started.send(()).unwrap();
                        released
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap();
                        panic!("exercise completion unwinding");
                    })) {
                        break (done, waiting);
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap()
        });
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        let waiter = runtime.scheduler().handle.spawn(async move {
            let _ = finished.await;
        });
        waiter.abort();
        let _ = runtime.block_on(waiter);
        // Required work in other workspaces uses neither the optional worker
        // nor its rendezvous channel and must progress while it is blocked.
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), async {
                owner.0.scheduler.kernel(|| Ok(())).await.unwrap();
                owner.0.scheduler.physical(|| Ok(())).await.unwrap();
            })
            .await
            .unwrap();
        });
        assert!(owner.0.gate.try_ordinary().is_some()); // Multiple reads remain allowed.
        assert!(runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_millis(10), owner.0.gate.cache_flush()).await
            })
            .is_err());
        assert!(!done.done.load(Ordering::Acquire));
        release.send(()).unwrap();
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), done.wait())
                .await
                .unwrap();
        });
        assert!(owner.0.active_prefill.lock().unwrap().is_none());
        let prefill = owner.begin_kernel_prefill(file).unwrap();
        owner.state().unwrap().nodes.get_mut(&file).unwrap().data = Data::File(FileData::Base {
            root: FileContentRoot(identity(b"replacement root")),
            len: 3,
        });
        prefill.record_success();
        assert_eq!(
            owner.0.kernel_refs.lock().unwrap().entries[&file].prefilled_root,
            None
        );
        drop(prefill);
        owner.state().unwrap().nodes.get_mut(&file).unwrap().data = Data::File(FileData::Base {
            root: FileContentRoot(root),
            len: 3,
        });
        let prefill = owner.begin_kernel_prefill(file).unwrap();
        prefill.record_success();
        drop(prefill);
        assert_eq!(
            owner.0.kernel_refs.lock().unwrap().entries[&file].prefilled_root,
            Some(root)
        );
        assert!(owner.begin_kernel_prefill(file).is_none());
    }
}
