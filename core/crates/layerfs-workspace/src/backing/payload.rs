use super::{
    budget::{Budget, Charge},
    directory::{identity, Directory},
    reader::PayloadReader,
    segments::{self, Header, Window, ALIGN, DATA_BYTES, WINDOW_BYTES},
};
use crate::*;
use layerfs_bridge::contract::Source;
use std::{
    collections::{BTreeMap, VecDeque},
    fs, io,
    mem::size_of,
    ops::Range,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

/// One registry entry: the `u64` payload id, the `Arc` value and this entry's
/// share of a `BTreeMap` node. The map allocates its nodes itself, so the
/// retained memory is charged at this fixed worst-case entry size instead of
/// the exact node layout. A candidate queue entry is one further id, covered by
/// the 64 bytes of slack the per-record charge already holds.
const REGISTRY_ENTRY_BYTES: usize = 32;
pub struct PayloadHost {
    pub budget: Arc<Budget>,
    pub quota: u64,
    pub common: Arc<Directory>,
    pub directories: Arc<AtomicUsize>,
    pub state: Mutex<State>,
    /// Payload ids whose last external owner just went away or whose custody
    /// page was just released. This is the whole routine-reclamation work list:
    /// `maintain` drains it instead of walking the registry, so accepting one
    /// more write never costs work proportional to earlier acquisitions.
    candidates: Mutex<VecDeque<u64>>,
    windows: Mutex<[Option<Box<Window>>; 4]>,
    _windows_charge: Charge,
    _charge: Charge,
}
pub struct State {
    /// Live ownership records, indexed by monotonically increasing payload id.
    /// Iteration is insertion order, which is what the deliberate scoped pass
    /// and the status aggregates rely on. A by-id lookup is `O(log N)`.
    pub records: BTreeMap<u64, Arc<Record>>,
    /// Ownership records visited by consumer-wide routine reclamation. This is
    /// ordinary product telemetry: it is the quantity that must not grow with
    /// the number of earlier acquisitions when one more write is accepted.
    pub routine_scans: u64,
    /// Ownership records visited by an indexed or deliberate scoped lookup.
    pub lookup_scans: u64,
    capacity_charge: Charge,
    next: u64,
    pub allocated: u64,
    pub reserved: u64,
    pub stopped: bool,
    pub complete: bool,
    pub metadata_allocated: u64,
    pub metadata_reserved: u64,
    pub metadata_stopped: bool,
    pub metadata_complete: bool,
}
impl State {
    pub(crate) fn note_routine(&mut self, count: u64) {
        self.routine_scans = self.routine_scans.saturating_add(count);
    }
    pub(crate) fn note_lookup(&mut self, count: u64) {
        self.lookup_scans = self.lookup_scans.saturating_add(count);
    }
    /// Charges the indexed registry for the entries it now holds.
    pub(crate) fn charge_registry(&mut self, entries: usize) -> Result<(), WorkspaceError> {
        self.capacity_charge
            .resize(REGISTRY_ENTRY_BYTES.saturating_mul(entries))
    }
}
pub struct Record {
    pub id: u64,
    pub length: u64,
    pub directory: Arc<Directory>,
    pub state: Mutex<RecordState>,
    _charge: Charge,
}
pub struct RecordState {
    pub ready: bool,
    pub created: u32,
    pub completed: u64,
    pub allocated: u64,
    pub reserved: u64,
    pub next_cleanup: u32,
    pub partial: Option<Partial>,
    pub failure: Option<BackingFailure>,
    pub complete: bool,
    pub admission_blocked: bool,
    pub custody: Option<(u64, super::metadata_pages::PageRef)>,
}
#[derive(Clone, Copy)]
pub struct Partial {
    pub index: u32,
    pub identity: Option<(u64, u64)>,
    pub allocated: Option<u64>,
    pub reserved: u64,
    pub collider: bool,
}
#[derive(Clone)]
pub struct OwnedPayload {
    pub(crate) host: Arc<PayloadHost>,
    pub(crate) record: Arc<Record>,
}
impl Drop for OwnedPayload {
    fn drop(&mut self) {
        // The registry holds one reference to a live record, so exactly two
        // means this drop leaves the registry as the only owner: the moment
        // routine reclamation may consider it. A retained reader, commit
        // source or clone holds its own reference and enqueues the record when
        // its own last one goes away.
        if Arc::strong_count(&self.record) == 2 {
            self.host.note_released(self.record.id);
        }
    }
}
pub struct WindowLease {
    host: Arc<PayloadHost>,
    index: usize,
    pub window: Option<Box<Window>>,
}
impl Drop for WindowLease {
    fn drop(&mut self) {
        if let Ok(mut slots) = self.host.windows.lock() {
            slots[self.index] = self.window.take();
        }
    }
}
pub(crate) fn clock(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(io::ErrorKind::TimedOut.into())
    } else {
        Ok(())
    }
}
pub(crate) fn filename(payload: u64, index: u32) -> String {
    format!("p-{payload:016x}-{index:08x}")
}
pub(crate) fn header(record: &Record, index: u32) -> Header {
    Header {
        incarnation: record.directory.incarnation,
        payload: record.id,
        length: record.length,
        index,
        logical: (record.length - u64::from(index) * DATA_BYTES as u64).min(DATA_BYTES as u64)
            as u32,
    }
}
pub(crate) fn segment_bytes(record: &Record, index: u32) -> u64 {
    ALIGN as u64 + u64::from(header(record, index).logical).div_ceil(ALIGN as u64) * ALIGN as u64
}
pub(crate) fn bare_failure(phase: BackingPhase, kind: io::ErrorKind) -> WorkspaceError {
    WorkspaceError::Backing(BackingFailure {
        phase,
        payload: 0,
        declared_bytes: 0,
        completed_bytes: 0,
        created_segments: 0,
        allocated_bytes: 0,
        reserved_bytes: 0,
        cleanup_failed: false,
        accounting_complete: true,
        kind,
    })
}

impl PayloadHost {
    pub fn new(
        root: PathBuf,
        quota: u64,
        budget: &Arc<Budget>,
    ) -> Result<Arc<Self>, WorkspaceError> {
        if !cfg!(target_os = "linux") {
            return Err(WorkspaceError::Unsupported);
        }
        if quota == 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        let charge = budget.reserve(8192)?;
        let mut windows_charge = budget.reserve(5 * WINDOW_BYTES)?;
        let mut windows = [None, None, None, None];
        for window in &mut windows {
            *window = Some(Box::new(Window([0; WINDOW_BYTES])));
        }
        windows_charge.resize(4 * WINDOW_BYTES)?;
        let common = Directory::reserve(root, [0; 32], budget, None)?;
        Ok(Arc::new(Self {
            budget: budget.clone(),
            quota,
            common,
            directories: Arc::new(AtomicUsize::new(0)),
            candidates: Mutex::new(VecDeque::new()),
            state: Mutex::new(State {
                records: BTreeMap::new(),
                routine_scans: 0,
                lookup_scans: 0,
                capacity_charge: budget.reserve(0)?,
                next: 1,
                allocated: 0,
                reserved: 0,
                stopped: false,
                complete: true,
                metadata_allocated: 0,
                metadata_reserved: 0,
                metadata_stopped: false,
                metadata_complete: true,
            }),
            windows: Mutex::new(windows),
            _windows_charge: windows_charge,
            _charge: charge,
        }))
    }
    /// The record registered under `id`, if it is still registered.
    pub(crate) fn registered(&self, id: u64) -> Result<Option<Arc<Record>>, WorkspaceError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .records
            .get(&id)
            .cloned())
    }
    /// The number of ownership records the registry currently holds.
    pub(crate) fn registry_len(&self) -> Result<usize, WorkspaceError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .records
            .len())
    }
    /// Drops one released record from the registry. The caller has already
    /// released its segments and bytes, and re-derives the aggregate admission
    /// flags once its pass finishes rather than once per released owner.
    pub(crate) fn unregister(&self, id: u64) -> Result<(), WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.records.remove(&id).is_none() {
            return Ok(());
        }
        let entries = state.records.len();
        state.charge_registry(entries)
    }
    /// Names one record a trigger has just released for routine reclamation.
    ///
    /// A candidate that is not eligible when it is drained is simply dropped
    /// from the queue: both transitions into routine eligibility - the last
    /// external owner going away and the custody page being released - are
    /// themselves triggers, so the record is re-enqueued at that point rather
    /// than found by a walk.
    pub(crate) fn note_released(&self, id: u64) {
        if let Ok(mut queue) = self.candidates.lock() {
            queue.push_back(id);
        }
    }
    /// Takes the current release list, leaving the queue empty.
    pub(crate) fn take_released(&self) -> Vec<u64> {
        match self.candidates.lock() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }
    pub fn directory(
        &self,
        path: PathBuf,
        incarnation: [u8; 32],
    ) -> Result<Arc<Directory>, WorkspaceError> {
        Directory::reserve(path, incarnation, &self.budget, Some(&self.directories))
    }
    pub fn initialize(&self, directory: &Directory) -> io::Result<()> {
        self.common.initialize(false)?;
        directory.initialize(true)
    }
    pub fn window(
        self: &Arc<Self>,
        first: usize,
        end: usize,
    ) -> Result<WindowLease, WorkspaceError> {
        let mut slots = self.windows.lock().map_err(|_| WorkspaceError::Io)?;
        let index = (first..end)
            .find(|index| slots[*index].is_some())
            .ok_or(WorkspaceError::Busy)?;
        Ok(WindowLease {
            host: self.clone(),
            index,
            window: slots[index].take(),
        })
    }
    pub fn status(&self) -> Result<BackingStatus, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let mut failed = 0;
        let mut retained = 0;
        for record in state.records.values() {
            let owner = record.state.lock().map_err(|_| WorkspaceError::Io)?;
            failed += usize::from(owner.failure.is_some());
            retained += usize::from(Arc::strong_count(record) > 1 || owner.custody.is_some());
        }
        let slots = self.windows.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(BackingStatus {
            quota_bytes: self.quota,
            allocated_bytes: state.allocated,
            reserved_bytes: state.reserved,
            payloads: state.records.len(),
            retained_payloads: retained,
            failed_payloads: failed,
            routine_scans: state.routine_scans,
            lookup_scans: state.lookup_scans,
            readers: slots[1..3].iter().filter(|slot| slot.is_none()).count(),
            acquiring: slots[0].is_none(),
            cleaning: slots[3].is_none(),
            admission_stopped: state.stopped,
            accounting_complete: state.complete,
        })
    }
    pub fn stop(&self, record: &Record) {
        if let Ok(mut state) = self.state.lock() {
            state.stopped = true;
            state.complete = false;
        }
        if let Ok(mut state) = record.state.lock() {
            state.complete = false;
            state.admission_blocked = true;
        }
    }
    fn stop_profile(&self, record: &Record) {
        if let Ok(mut state) = self.state.lock() {
            state.stopped = true;
        }
        if let Ok(mut state) = record.state.lock() {
            state.admission_blocked = true;
        }
    }
    pub fn refresh(&self, state: &mut State) -> Result<(), WorkspaceError> {
        let mut complete = state.metadata_complete;
        let mut stopped = state.metadata_stopped
            || state
                .allocated
                .checked_add(state.reserved)
                .is_none_or(|used| used > self.quota);
        for record in state.records.values() {
            let owner = record.state.lock().map_err(|_| WorkspaceError::Io)?;
            complete &= owner.complete;
            stopped |= owner.admission_blocked;
        }
        state.complete = complete;
        state.stopped = stopped;
        Ok(())
    }
    pub fn failure(
        &self,
        record: &Record,
        phase: BackingPhase,
        error: &io::Error,
    ) -> WorkspaceError {
        let Ok(mut global) = self.state.lock() else {
            return WorkspaceError::Io;
        };
        let Ok(mut state) = record.state.lock() else {
            return WorkspaceError::Io;
        };
        if !state.ready {
            let keep = state.partial.map_or(0, |partial| partial.reserved);
            let released = state.reserved - keep;
            state.reserved = keep;
            global.reserved -= released;
        }
        let failure = BackingFailure {
            phase,
            payload: record.id,
            declared_bytes: record.length,
            completed_bytes: state.completed,
            created_segments: state.created,
            allocated_bytes: state.allocated,
            reserved_bytes: state.reserved,
            cleanup_failed: phase == BackingPhase::Cleanup,
            accounting_complete: state.complete,
            kind: error.kind(),
        };
        state.failure = Some(failure.clone());
        WorkspaceError::Backing(failure)
    }
    fn reserve(
        self: &Arc<Self>,
        directory: Arc<Directory>,
        length: u64,
        bytes: u64,
    ) -> Result<Arc<Record>, WorkspaceError> {
        let charge = self.budget.reserve(size_of::<Record>() + 64)?;
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped {
            return Err(WorkspaceError::Busy);
        }
        if state
            .allocated
            .checked_add(state.reserved)
            .and_then(|used| used.checked_add(bytes))
            .is_none_or(|used| used > self.quota)
        {
            return Err(bare_failure(
                BackingPhase::Acquire,
                io::ErrorKind::StorageFull,
            ));
        }
        let entries = state.records.len() + 1;
        state.charge_registry(entries)?;
        let id = state.next;
        state.next = id.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let record = Arc::new(Record {
            id,
            length,
            directory,
            state: Mutex::new(RecordState {
                ready: false,
                created: 0,
                completed: 0,
                allocated: 0,
                reserved: bytes,
                next_cleanup: 0,
                partial: None,
                failure: None,
                complete: true,
                admission_blocked: false,
                custody: None,
            }),
            _charge: charge,
        });
        state.reserved += bytes;
        state.records.insert(id, record.clone());
        Ok(record)
    }
    fn observe(&self, record: &Record, amount: u64) -> io::Result<()> {
        let mut global = self
            .state
            .lock()
            .map_err(|_| io::Error::other("payload accounting poisoned"))?;
        let mut state = record
            .state
            .lock()
            .map_err(|_| io::Error::other("payload ownership poisoned"))?;
        let reservation = state
            .partial
            .ok_or_else(|| io::Error::other("missing allocation owner"))?
            .reserved;
        global.allocated = global
            .allocated
            .checked_add(amount)
            .ok_or_else(|| io::Error::other("allocation overflow"))?;
        state.allocated = state
            .allocated
            .checked_add(amount)
            .ok_or_else(|| io::Error::other("allocation overflow"))?;
        global.reserved -= reservation;
        state.reserved -= reservation;
        let partial = state
            .partial
            .as_mut()
            .ok_or_else(|| io::Error::other("missing allocation owner"))?;
        partial.allocated = Some(amount);
        partial.reserved = 0;
        Ok(())
    }
    pub fn acquire(
        self: &Arc<Self>,
        directory: Arc<Directory>,
        length: u64,
        source: &mut dyn Source,
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> Result<OwnedPayload, WorkspaceError> {
        let bytes = segments::planned(length)
            .map_err(|error| bare_failure(BackingPhase::Acquire, error.kind()))?;
        clock(deadline).map_err(|error| bare_failure(BackingPhase::Acquire, error.kind()))?;
        let mut lease = self.window(0, 1)?;
        let record = self.reserve(directory, length, bytes)?;
        let mut phase = BackingPhase::Create;
        let result = (|| {
            let directory = record.directory.file()?;
            let window = lease
                .window
                .as_mut()
                .ok_or_else(|| io::Error::other("missing acquisition window"))?;
            let check = || -> io::Result<()> {
                clock(deadline)?;
                if cancel.load(Ordering::Acquire) {
                    return Err(io::ErrorKind::Interrupted.into());
                }
                Ok(())
            };
            for index in 0..segments::segment_count(length) {
                check()?;
                phase = BackingPhase::Create;
                let name = filename(record.id, index);
                let planned = segment_bytes(&record, index);
                {
                    let mut state = record
                        .state
                        .lock()
                        .map_err(|_| io::Error::other("payload ownership poisoned"))?;
                    state.partial = Some(Partial {
                        index,
                        identity: None,
                        allocated: None,
                        reserved: planned,
                        collider: false,
                    });
                }
                match fs::symlink_metadata(record.directory.path.join(&name)) {
                    Ok(_) => {
                        record
                            .state
                            .lock()
                            .map_err(|_| io::Error::other("payload ownership poisoned"))?
                            .partial
                            .as_mut()
                            .unwrap()
                            .collider = true;
                        self.stop(&record);
                        return Err(io::ErrorKind::AlreadyExists.into());
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        self.stop(&record);
                        return Err(error);
                    }
                }
                let file = match segments::create(&directory, &name) {
                    Ok(file) => file,
                    Err(error) => {
                        if error.kind() == io::ErrorKind::AlreadyExists {
                            record
                                .state
                                .lock()
                                .map_err(|_| io::Error::other("payload ownership poisoned"))?
                                .partial
                                .as_mut()
                                .unwrap()
                                .collider = true;
                        }
                        self.stop(&record);
                        return Err(error);
                    }
                };
                {
                    let mut state = record
                        .state
                        .lock()
                        .map_err(|_| io::Error::other("payload ownership poisoned"))?;
                    state.created = index + 1;
                }
                let metadata = match file.metadata() {
                    Ok(value) => value,
                    Err(error) => {
                        self.stop(&record);
                        return Err(error);
                    }
                };
                record
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("payload ownership poisoned"))?
                    .partial
                    .as_mut()
                    .unwrap()
                    .identity = Some(identity(&metadata));
                phase = BackingPhase::Allocate;
                let allocation = segments::allocate(&file, planned);
                let observed = match allocation.as_ref() {
                    Ok(bytes) => Ok(*bytes),
                    Err(_) => segments::allocated(&file),
                };
                match observed {
                    Ok(amount) => {
                        self.observe(&record, amount)?;
                        if allocation.is_ok() && amount != planned {
                            self.stop_profile(&record);
                            return Err(io::Error::new(
                                io::ErrorKind::Unsupported,
                                "segment allocation differs from reserved profile",
                            ));
                        } else if amount > planned {
                            self.stop_profile(&record);
                        }
                    }
                    Err(error) => {
                        self.stop(&record);
                        return Err(error);
                    }
                }
                allocation?;
                check()?;
                phase = BackingPhase::Write;
                let header = header(&record, index);
                header.fill(window);
                segments::write(&file, window, 0, ALIGN)?;
                check()?;
                let mut logical = 0usize;
                while logical < header.logical as usize {
                    check()?;
                    phase = BackingPhase::Input;
                    let count = (header.logical as usize - logical).min(WINDOW_BYTES);
                    let mut received = 0;
                    while received < count {
                        check()?;
                        let read = source.read(&mut window.0[received..count], deadline, cancel)?;
                        check()?;
                        if read == 0 {
                            return Err(io::ErrorKind::UnexpectedEof.into());
                        }
                        if read > count - received {
                            return Err(io::ErrorKind::InvalidData.into());
                        }
                        received += read;
                    }
                    check()?;
                    let physical = count.div_ceil(ALIGN) * ALIGN;
                    window.0[count..physical].fill(0);
                    phase = BackingPhase::Write;
                    segments::write(&file, window, ALIGN as u64 + logical as u64, physical)?;
                    logical += count;
                    record
                        .state
                        .lock()
                        .map_err(|_| io::Error::other("payload ownership poisoned"))?
                        .completed += count as u64;
                    check()?;
                }
                drop(file);
                record
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("payload ownership poisoned"))?
                    .partial = None;
            }
            phase = BackingPhase::Input;
            check()?;
            if source.read(&mut window.0[..1], deadline, cancel)? != 0 {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            check()?;
            record
                .state
                .lock()
                .map_err(|_| io::Error::other("payload ownership poisoned"))?
                .ready = true;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(OwnedPayload {
                host: self.clone(),
                record,
            }),
            Err(error) => Err(self.failure(&record, phase, &error)),
        }
    }
}
impl OwnedPayload {
    pub fn len(&self) -> u64 {
        self.record.length
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn reader(&self, range: Range<u64>) -> Result<PayloadReader, WorkspaceError> {
        PayloadReader::new(self.clone(), range)
    }
}
impl Workspace {
    pub fn own_payload(
        &self,
        length: u64,
        source: &mut dyn Source,
        deadline: Instant,
    ) -> Result<OwnedPayload, WorkspaceError> {
        let _operation = self.begin(false, deadline)?;
        let host = self
            .host
            .payloads
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let directory = self
            .inner
            .directory
            .clone()
            .ok_or(WorkspaceError::Unsupported)?;
        segments::planned(length)
            .map_err(|error| bare_failure(BackingPhase::Acquire, error.kind()))?;
        self.maintain_backing(deadline)?;
        host.acquire(directory, length, source, deadline, &self.inner.stopping)
    }
    pub fn backing_status(&self) -> Result<BackingStatus, WorkspaceError> {
        self.host
            .payloads
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?
            .status()
    }
}
