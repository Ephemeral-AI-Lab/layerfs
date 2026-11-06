//! Readers opened at mount, with no shared admission queue or request quota.
use super::{open::Counts, read_handle::ReadSession, PortError, StoreReader};
use layerfs_history::WorkspaceId;
use layerfs_storage::{StorageError, StoragePolicy, StorageResult};
use std::{
    collections::BTreeMap,
    fmt,
    sync::{atomic::Ordering, Arc, Mutex},
};

pub type ReaderFactory = Arc<dyn Fn(usize) -> StorageResult<StoreReader> + Send + Sync>;
/// Readers of closed SDK private Workspaces kept for the next one (#352
/// F-025 fix 4): an SDK file call binds a private Workspace per call, and
/// opening its reader (a read-only SQLite connection and Storage) was a fixed
/// cost. On SQLite a mounted Workspace's reader still opens at mount and
/// closes at unmount; a Store opened with shared readers (PostgreSQL, #64)
/// keeps every Workspace's healthy reader here, so a mount after an unmount
/// opens no connection, and one idle reader opens after the daemon reported
/// ready ([`ReadSet::prefill`]), so its first mount opens none either. At
/// most this many stay idle: with the writer, its lane connection, the
/// control reader and one per mounted Workspace, they bound a daemon's
/// PostgreSQL connections.
pub const IDLE_READERS: usize = 4;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadAdmissionError {
    Capacity,
    Stopped,
    NoReaders,
    Poisoned,
}
impl fmt::Display for ReadAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store read admission: {self:?}")
    }
}
impl std::error::Error for ReadAdmissionError {}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadServiceWork {
    pub stopping: bool,
    pub poisoned: bool,
    pub readers: usize,
    pub quarantined: usize,
    pub opening_failures: usize,
    pub requests: u64,
    pub lock_wait_ns: u64,
}
struct State {
    sessions: BTreeMap<Option<WorkspaceId>, Option<Arc<ReadSession>>>,
    next: usize,
    stopping: bool,
    retired_requests: u64,
    retired_wait_ns: u64,
    opening_failures: BTreeMap<WorkspaceId, (usize, Arc<PortError>)>,
    /// Readers that served a closed Workspace without failure, with the
    /// identity each was opened under; reads take a fresh transaction each
    /// call, so a kept reader carries no Workspace state.
    idle: Vec<(usize, StoreReader)>,
    /// Workspaces whose reader goes back to `idle` at close.
    pooled: std::collections::BTreeSet<WorkspaceId>,
}
pub(super) struct ReadSet {
    state: Mutex<State>,
    factory: ReaderFactory,
    counts: Arc<Counts>,
    capacity: usize,
    policy: StoragePolicy,
    /// Every Workspace takes and returns idle readers, not only pooled ones.
    shared: bool,
    /// Serializes the one open of a control reader not opened at start.
    opening_control: Mutex<()>,
    /// The one idle reader opened after readiness was claimed (#64).
    prefill_claimed: std::sync::atomic::AtomicBool,
}
impl ReadSet {
    /// `control`, when given, is the control reader opened at start; without
    /// one, it opens at its first use ([`Self::control`]). `shared` makes
    /// every Workspace's reader come from and return to the idle readers.
    pub fn new(
        control: Option<StoreReader>,
        factory: ReaderFactory,
        counts: Arc<Counts>,
        (capacity, policy, shared): (usize, StoragePolicy, bool),
    ) -> Arc<Self> {
        let control = control.map(|control| Arc::new(ReadSession::new(0, control, counts.clone())));
        Arc::new(Self {
            state: Mutex::new(State {
                sessions: BTreeMap::from_iter(control.map(|control| (None, Some(control)))),
                next: 1,
                stopping: false,
                retired_requests: 0,
                retired_wait_ns: 0,
                opening_failures: BTreeMap::new(),
                idle: Vec::new(),
                pooled: Default::default(),
            }),
            factory,
            counts,
            capacity,
            policy,
            shared,
            opening_control: Mutex::new(()),
            prefill_claimed: std::sync::atomic::AtomicBool::new(false),
        })
    }
    /// Opens one idle reader ahead of the first mount (#64, owner decision
    /// 2026-11-06: one eager reader, opened after the daemon reported
    /// ready, never before). Only shared readers (PostgreSQL) are kept
    /// idle; `None` without them, on any call after the first, and when an
    /// idle reader is already there. It is one attempt: a failure is
    /// counted and returned to the caller to record, nothing is kept and
    /// nothing opens it again; the first mount then opens its own reader.
    /// The reader joins the idle readers under their cap
    /// ([`IDLE_READERS`]); a mount that comes first opens its own.
    pub fn prefill(&self) -> Option<StorageResult<()>> {
        if !self.shared || self.prefill_claimed.swap(true, Ordering::SeqCst) {
            return None;
        }
        let index = {
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(_) => return Some(Err(StorageError::Integrity("reader set poisoned"))),
            };
            if state.stopping || !state.idle.is_empty() {
                return None;
            }
            let index = state.next;
            match index.checked_add(1) {
                Some(next) => state.next = next,
                None => return Some(Err(StorageError::Integrity("reader identity exhausted"))),
            }
            index
        };
        let opened =
            (self.factory)(index).and_then(|reader| match reader.storage.policy() == self.policy {
                true => Ok(reader),
                false => Err(StorageError::Integrity("idle reader policy")),
            });
        let reader = match opened {
            Ok(reader) => reader,
            Err(error) => {
                self.counts.prefill_failures.fetch_add(1, Ordering::Relaxed);
                return Some(Err(error));
            }
        };
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return Some(Err(StorageError::Integrity("reader set poisoned"))),
        };
        // a reader the set cannot keep closes with its drop
        if !state.stopping && state.idle.len() < IDLE_READERS {
            state.idle.push((index, reader));
            self.counts
                .readers_prefilled
                .fetch_add(1, Ordering::Relaxed);
        }
        Some(Ok(()))
    }
    /// Opens `workspace`'s reader; a `pooled` (SDK private) Workspace takes
    /// an idle one first and gives it back at close.
    pub fn mount(&self, workspace: WorkspaceId, pooled: bool) -> Result<(), Arc<PortError>> {
        let candidate = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| Arc::new(PortError::Poisoned))?;
            if state.stopping || state.sessions.contains_key(&Some(workspace)) {
                return Err(Arc::new(PortError::ReadAdmission(
                    ReadAdmissionError::Stopped,
                )));
            }
            if state.sessions.keys().flatten().count() >= self.capacity {
                return Err(Arc::new(PortError::ReadAdmission(
                    ReadAdmissionError::Capacity,
                )));
            }
            state.sessions.insert(Some(workspace), None);
            match pooled || self.shared {
                true => {
                    state.pooled.insert(workspace);
                    state.idle.pop()
                }
                false => None,
            }
        };
        // An idle reader is checked outside the lock (a PostgreSQL one with
        // one round trip, #64); one that cannot serve is dropped, never
        // handed to this Workspace, and a new reader opens instead.
        let (index, result) = match candidate.filter(|(_, reader)| reader.usable()) {
            Some((index, reader)) => {
                self.counts.readers_reused.fetch_add(1, Ordering::Relaxed);
                let session = ReadSession::new(index, reader, self.counts.clone());
                (index, Ok(Arc::new(session)))
            }
            None => {
                let index = {
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| Arc::new(PortError::Poisoned))?;
                    let index = state.next;
                    match index.checked_add(1) {
                        Some(next) => state.next = next,
                        None => {
                            state.sessions.remove(&Some(workspace));
                            return Err(Arc::new(PortError::Storage(StorageError::Integrity(
                                "reader identity exhausted",
                            ))));
                        }
                    }
                    index
                };
                self.counts.readers_opened.fetch_add(1, Ordering::Relaxed);
                let result = (self.factory)(index).and_then(|reader| {
                    if reader.storage.policy() != self.policy {
                        return Err(StorageError::Integrity("Workspace reader policy"));
                    }
                    Ok(Arc::new(ReadSession::new(
                        index,
                        reader,
                        self.counts.clone(),
                    )))
                });
                (index, result)
            }
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| Arc::new(PortError::Poisoned))?;
        match result {
            Ok(reader) => {
                if state.stopping {
                    reader.close()?;
                    state.sessions.remove(&Some(workspace));
                    return Err(Arc::new(PortError::ReadAdmission(
                        ReadAdmissionError::Stopped,
                    )));
                }
                state.sessions.insert(Some(workspace), Some(reader));
                Ok(())
            }
            Err(error) => {
                // Unknown opening never authorizes a new open for this identity.
                let unknown = error.is_unknown_outcome();
                let error = Arc::new(PortError::Storage(error));
                if unknown {
                    state
                        .opening_failures
                        .insert(workspace, (index, error.clone()));
                } else {
                    state.sessions.remove(&Some(workspace));
                }
                Err(error)
            }
        }
    }
    pub fn admit(&self) -> Result<(), ReadAdmissionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ReadAdmissionError::Poisoned)?;
        if state.stopping {
            Err(ReadAdmissionError::Stopped)
        } else {
            Ok(())
        }
    }
    /// The control reader (session 0), opened here at its first use when
    /// the Store did not open it at start (#64). That open is the calling
    /// operation's one attempt: its failure is that operation's, nothing is
    /// kept, and a later operation opens it again. A read-only open changes
    /// nothing on the server, so an unknown outcome leaves no custody.
    pub fn control(&self) -> Result<Arc<ReadSession>, Arc<PortError>> {
        let admission = |error| Arc::new(PortError::ReadAdmission(error));
        match self.session(None) {
            Err(ReadAdmissionError::NoReaders) => {}
            other => return other.map_err(admission),
        }
        let _opening = self
            .opening_control
            .lock()
            .map_err(|_| Arc::new(PortError::Poisoned))?;
        match self.session(None) {
            Err(ReadAdmissionError::NoReaders) => {}
            other => return other.map_err(admission),
        }
        let reader = (self.factory)(0).map_err(|error| Arc::new(PortError::Storage(error)))?;
        if reader.storage.policy() != self.policy {
            return Err(Arc::new(PortError::Storage(StorageError::Integrity(
                "control reader policy",
            ))));
        }
        let session = Arc::new(ReadSession::new(0, reader, self.counts.clone()));
        let mut state = self
            .state
            .lock()
            .map_err(|_| Arc::new(PortError::Poisoned))?;
        if state.stopping {
            drop(state);
            session.close()?;
            return Err(admission(ReadAdmissionError::Stopped));
        }
        state.sessions.insert(None, Some(session.clone()));
        Ok(session)
    }
    pub fn session(
        &self,
        workspace: Option<WorkspaceId>,
    ) -> Result<Arc<ReadSession>, ReadAdmissionError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ReadAdmissionError::Poisoned)?;
        if state.stopping {
            return Err(ReadAdmissionError::Stopped);
        }
        state
            .sessions
            .get(&workspace)
            .and_then(Clone::clone)
            .ok_or(ReadAdmissionError::NoReaders)
    }
    pub fn close(&self, workspace: WorkspaceId) -> Result<(), Arc<PortError>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Arc::new(PortError::Poisoned))?;
        let pooled = state.pooled.remove(&workspace);
        if let Some(session) = state.sessions.get(&Some(workspace)).and_then(Clone::clone) {
            let reader = session.release()?;
            state.retired_requests = state
                .retired_requests
                .saturating_add(session.requests.load(Ordering::Relaxed));
            state.retired_wait_ns = state
                .retired_wait_ns
                .saturating_add(session.wait_ns.load(Ordering::Relaxed));
            if let Some(reader) = reader.filter(|_| pooled) {
                if !state.stopping && state.idle.len() < IDLE_READERS {
                    state.idle.push((session.index, reader));
                }
            }
        }
        if let Some((_, cause)) = state.opening_failures.get(&workspace) {
            return Err(cause.clone());
        }
        state.sessions.remove(&Some(workspace));
        Ok(())
    }
    fn sessions(&self) -> Vec<Arc<ReadSession>> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .sessions
            .values()
            .flatten()
            .cloned()
            .collect()
    }
    pub fn work(&self) -> ReadServiceWork {
        let (mut work, sessions) = {
            let (state, poisoned) = match self.state.lock() {
                Ok(state) => (state, false),
                Err(poison) => (poison.into_inner(), true),
            };
            (
                ReadServiceWork {
                    stopping: state.stopping || poisoned,
                    poisoned,
                    opening_failures: state.opening_failures.len(),
                    requests: state.retired_requests,
                    lock_wait_ns: state.retired_wait_ns,
                    ..ReadServiceWork::default()
                },
                state
                    .sessions
                    .values()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        };
        for session in sessions {
            work.readers += 1;
            work.quarantined += usize::from(session.quarantined());
            work.requests = work
                .requests
                .saturating_add(session.requests.load(Ordering::Relaxed));
            work.lock_wait_ns = work
                .lock_wait_ns
                .saturating_add(session.wait_ns.load(Ordering::Relaxed));
        }
        work
    }
    pub fn storage_diagnostics(&self, index: usize) -> Option<layerfs_storage::Diagnostics> {
        self.sessions()
            .iter()
            .find(|session| session.index == index)?
            .diagnostics()
    }
    pub fn failures(&self) -> Vec<(usize, Arc<PortError>)> {
        let mut failures = self
            .sessions()
            .iter()
            .filter_map(|session| session.failure().map(|error| (session.index, error)))
            .collect::<Vec<_>>();
        failures.extend(
            self.state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .opening_failures
                .values()
                .cloned(),
        );
        failures
    }
    pub fn stop(&self) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if state.stopping {
                return;
            }
            state.stopping = true;
            state.idle.clear();
        }
        let sessions = self.sessions();
        for session in &sessions {
            let _ = session.close();
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for session in sessions {
            if session.failure().is_none() {
                state.retired_requests = state
                    .retired_requests
                    .saturating_add(session.requests.load(Ordering::Relaxed));
                state.retired_wait_ns = state
                    .retired_wait_ns
                    .saturating_add(session.wait_ns.load(Ordering::Relaxed));
            }
        }
        state.sessions.retain(|_, session| {
            session
                .as_ref()
                .is_none_or(|session| session.failure().is_some())
        });
    }
}
