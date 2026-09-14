//! Host overlay admission. Disk quota reservations are distinct from physical
//! occupancy/RSS. The Payload lifetime owns the permit, including detached ranges.
use crate::overlay_index::{Index, Limits as IndexLimits};
use crate::overlay_payload::{Limits as PayloadLimits, Payload};
use crate::overlay_ranges::{Limits as RangeLimits, Ranges};
use layerfs_fuse::live_runtime::{LiveReservation, LiveRuntime, Scheduler};
use layerfs_workspace_core::ResourcePolicy;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

const PAGE: u64 = 4096;
const CATALOG_PER_PAGE: u64 = 256;
const MOVE: u64 = 64 * 1024;
const BACKING_FILES: usize = 6;
const RECORD_BYTES: usize = 8192; // Existing symlinks are <=4096; inode/range records are fixed.
const MIB: u64 = 1024 * 1024;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}
fn capacity(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::WouldBlock, message)
}
fn add(a: u64, b: u64) -> io::Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| invalid("overlay budget overflow"))
}
fn mul(a: u64, b: u64) -> io::Result<u64> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("overlay budget overflow"))
}
fn align(bytes: u64) -> io::Result<u64> {
    Ok(add(bytes, PAGE - 1)? / PAGE * PAGE)
}
fn as_usize(bytes: u64) -> io::Result<usize> {
    usize::try_from(bytes).map_err(|_| invalid("overlay address-size limit"))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HostLimits {
    pub memory_bytes: u64,
    pub disk_bytes: u64,
    pub files: usize,
}
impl Default for HostLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 128 * MIB,
            disk_bytes: 128 * 1024 * MIB,
            files: 512,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Usage {
    pub memory_bytes: u64,
    pub reserved_disk_bytes: u64,
    pub reserved_files: usize,
    pub workspaces: usize,
}
impl Usage {
    fn plus(self, memory: u64, disk: u64, files: usize, workspaces: usize) -> io::Result<Self> {
        Ok(Self {
            memory_bytes: add(self.memory_bytes, memory)?,
            reserved_disk_bytes: add(self.reserved_disk_bytes, disk)?,
            reserved_files: self
                .reserved_files
                .checked_add(files)
                .ok_or_else(|| invalid("overlay file reservation overflow"))?,
            workspaces: self
                .workspaces
                .checked_add(workspaces)
                .ok_or_else(|| invalid("overlay workspace reservation overflow"))?,
        })
    }
}
pub(crate) struct HostAdmission {
    scheduler: Scheduler,
    limits: HostLimits,
    used: Mutex<Usage>,
}
impl HostAdmission {
    pub(crate) fn new(scheduler: Scheduler, limits: HostLimits) -> io::Result<Arc<Self>> {
        if limits.memory_bytes == 0 || limits.disk_bytes == 0 || limits.files == 0 {
            return Err(invalid("host overlay budget"));
        }
        Ok(Arc::new(Self {
            scheduler,
            limits,
            used: Mutex::new(Usage::default()),
        }))
    }
    pub(crate) fn shared() -> io::Result<Arc<Self>> {
        static HOST: OnceLock<Arc<HostAdmission>> = OnceLock::new();
        if let Some(host) = HOST.get() {
            return Ok(host.clone());
        }
        let host = Self::new(LiveRuntime::shared()?.scheduler(), HostLimits::default())?;
        Ok(HOST.get_or_init(|| host).clone())
    }
    fn check(&self, usage: Usage) -> io::Result<()> {
        if usage.memory_bytes > self.limits.memory_bytes
            || usage.reserved_disk_bytes > self.limits.disk_bytes
            || usage.reserved_files > self.limits.files
        {
            return Err(capacity("host overlay aggregate admission"));
        }
        Ok(())
    }
    pub(crate) fn usage(&self) -> Usage {
        *self.used.lock().unwrap()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Components {
    pub index: IndexLimits,
    pub payload: PayloadLimits,
    pub ranges: RangeLimits,
    pub fixed_memory_bytes: u64,
    pub reserved_disk_bytes: u64,
}
fn index_limits(bytes: u64, roots: usize) -> io::Result<IndexLimits> {
    // Exact monotonic inversion includes both double-header catalogs and their
    // allocation-block rounding. max_pages alone would undercharge disk by6.25%.
    let mut low = 0;
    let mut high = bytes / PAGE;
    while low < high {
        let middle = low + (high - low + 1) / 2;
        if index_bytes(middle).is_ok_and(|cost| cost <= bytes) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    if low == 0 {
        return Err(invalid("overlay index physical budget too small"));
    }
    Ok(IndexLimits {
        max_pages: low,
        max_roots: roots,
        max_key_bytes: 384,
        max_value_bytes: RECORD_BYTES,
    })
}
fn index_bytes(pages: u64) -> io::Result<u64> {
    add(mul(pages, PAGE)?, align(mul(pages, CATALOG_PER_PAGE)?)?)
}
fn components(policy: ResourcePolicy) -> io::Result<Components> {
    let p = policy.overlay;
    p.validate()
        .map_err(|_| invalid("workspace overlay policy"))?;
    let index = index_limits(p.max_index_bytes, p.max_roots)?;
    let payload_index = index_limits(p.max_payload_index_bytes, p.max_roots)?;
    let payload_bytes = add(
        align(add(policy.max_spool_bytes, p.max_retained_payload_bytes)?)?,
        MOVE,
    )?
    .max(2 * MOVE);
    let disk = add(
        add(
            add(p.max_index_bytes, p.max_payload_index_bytes)?,
            payload_bytes,
        )?,
        p.max_scratch_bytes,
    )?;
    // Root Arc allocations + admitted retired slots are <=80B/root. Each index
    // serializes page I/O: reserve two decoded paths, not every disk page. A
    // deliberately conservative16KiB/level includes allocation/object overhead.
    let index_memory = add(
        mul(p.max_roots as u64, 80)?,
        add(64 * 16 * 1024, RECORD_BYTES as u64)?,
    )?;
    let owner_memory = mul(p.max_payload_owners as u64, 80)?;
    let operation_slots = add(
        add(p.max_readers as u64, p.max_writers as u64)?,
        add(p.max_requests as u64, p.max_prepared as u64)?,
    )?;
    let slot_memory = add(
        mul(operation_slots, 128)?,
        mul(p.max_replay_entries as u64, 96)?,
    )?;
    let fixed = add(
        add(
            add(mul(index_memory, 2)?, owner_memory)?,
            add(slot_memory, p.max_replay_bytes)?,
        )?,
        2 * MOVE,
    )?;
    p.check_memory(fixed)
        .map_err(|_| invalid("overlay fixed resident capacity exceeds policy"))?;
    as_usize(fixed)?;
    Ok(Components {
        index,
        payload: PayloadLimits {
            physical_bytes: payload_bytes,
            owners: p.max_payload_owners,
            readers: p.max_readers,
            writes: p.max_writers,
            index: payload_index,
        },
        // Preserve the core's existing1TiB result and8193-piece input limits.
        ranges: RangeLimits::default(),
        fixed_memory_bytes: fixed,
        reserved_disk_bytes: disk,
    })
}

pub(crate) struct Resources {
    pub budget: Arc<Budget>,
    pub payload: Payload,
    pub index: Index,
    pub ranges: Ranges,
}
pub(crate) struct Budget {
    policy: ResourcePolicy,
    components: Components,
    host: Arc<HostAdmission>,
    active: Mutex<Active>,
    _fixed: LiveReservation,
}
#[derive(Default)]
struct Active {
    slots: [usize; 5],
    memory: u64,
    transport: u64,
    scratch: u64,
    files: usize,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Operation {
    Read,
    Write,
    Prepare,
    Request,
    Scratch,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Charge {
    /// Additional owned buffers; borrowed caller input/output is not charged again.
    pub memory_bytes: u64,
    pub transport_bytes: u64,
    pub scratch_bytes: u64,
    pub files: usize,
}
/// Hold with the actual buffer/journal/request through its last use. The budget
/// Arc also keeps aggregate admission alive if the Workspace itself is gone.
pub(crate) struct Permit {
    budget: Arc<Budget>,
    operation: Operation,
    charge: Charge,
    _live: LiveReservation,
    _transfer: LiveReservation,
}
/// Canonical construction has its own bounded domain, outside the live-overlay
/// per-Workspace envelope, while consuming the same aggregate host admission.
/// Keep this owner until private output and admission buffers are both gone.
pub(crate) struct ConstructionReservation {
    host: Arc<HostAdmission>,
    charge: Usage,
    _live: LiveReservation,
}
impl Drop for ConstructionReservation {
    fn drop(&mut self) {
        let mut used = self.host.used.lock().unwrap();
        used.memory_bytes -= self.charge.memory_bytes;
        used.reserved_disk_bytes -= self.charge.reserved_disk_bytes;
        used.reserved_files -= self.charge.reserved_files;
    }
}
impl Budget {
    fn reserve(policy: ResourcePolicy, host: Arc<HostAdmission>) -> io::Result<Arc<Self>> {
        let components = components(policy)?;
        let fixed = host
            .scheduler
            .reserve_live(as_usize(components.fixed_memory_bytes)?)?;
        {
            let mut used = host.used.lock().unwrap();
            let next = used.plus(
                components.fixed_memory_bytes,
                components.reserved_disk_bytes,
                policy.overlay.max_files,
                1,
            )?;
            host.check(next)?;
            *used = next;
        }
        Ok(Arc::new(Self {
            policy,
            components,
            host,
            active: Mutex::new(Active::default()),
            _fixed: fixed,
        }))
    }
    pub(crate) fn open(
        directory: &Path,
        policy: ResourcePolicy,
        admission: Arc<HostAdmission>,
    ) -> io::Result<Resources> {
        let budget = Self::reserve(policy, admission)?;
        let payload =
            Payload::temporary_with_lifetime(directory, budget.components.payload, budget.clone())?;
        let index = Index::temporary_with_owner(
            directory,
            budget.components.index,
            Arc::new(payload.clone()),
        )?;
        let ranges = Ranges::new(index.clone(), payload.clone(), budget.components.ranges)?;
        Ok(Resources {
            budget,
            payload,
            index,
            ranges,
        })
    }
    pub(crate) fn reserve_construction(
        &self,
        memory_bytes: u64,
        disk_bytes: u64,
        files: usize,
    ) -> io::Result<ConstructionReservation> {
        let live = self.host.scheduler.reserve_live(as_usize(memory_bytes)?)?;
        let charge = Usage {
            memory_bytes,
            reserved_disk_bytes: disk_bytes,
            reserved_files: files,
            workspaces: 0,
        };
        let mut used = self.host.used.lock().unwrap();
        let next = used.plus(memory_bytes, disk_bytes, files, 0)?;
        self.host.check(next)?;
        *used = next;
        Ok(ConstructionReservation {
            host: self.host.clone(),
            charge,
            _live: live,
        })
    }
    pub(crate) fn policy(&self) -> ResourcePolicy {
        self.policy
    }
    pub(crate) fn limits(&self) -> Components {
        self.components
    }
    /// Bounded non-index range/cursor/prepare working sets. Index page buffers and
    /// owner tables have a separate fixed reservation; these are not duplicates.
    pub(crate) fn operation_charge(operation: Operation) -> u64 {
        match operation {
            Operation::Read => 128 * 1024,
            Operation::Write => 256 * 1024,
            Operation::Prepare => 512 * 1024,
            Operation::Request | Operation::Scratch => 0,
        }
    }
    fn slot_limit(&self, operation: Operation) -> usize {
        let p = self.policy.overlay;
        match operation {
            Operation::Read => p.max_readers,
            Operation::Write => p.max_writers,
            Operation::Prepare | Operation::Scratch => p.max_prepared,
            Operation::Request => p.max_requests,
        }
    }
    pub(crate) fn enter(
        self: &Arc<Self>,
        operation: Operation,
        mut charge: Charge,
    ) -> io::Result<Permit> {
        charge.memory_bytes = add(charge.memory_bytes, Self::operation_charge(operation))?;
        let memory = add(charge.memory_bytes, charge.transport_bytes)?;
        let live = self
            .host
            .scheduler
            .reserve_live(as_usize(charge.memory_bytes)?)?;
        let transfer = self
            .host
            .scheduler
            .reserve_transfer(as_usize(charge.transport_bytes)?)?;
        {
            let mut active = self.active.lock().unwrap();
            let slot = operation as usize;
            if active.slots[slot] >= self.slot_limit(operation) {
                return Err(capacity("overlay operation admission"));
            }
            let next_memory = add(active.memory, memory)?;
            let next_transport = add(active.transport, charge.transport_bytes)?;
            let next_scratch = add(active.scratch, charge.scratch_bytes)?;
            let next_files = active
                .files
                .checked_add(charge.files)
                .ok_or_else(|| invalid("overlay file admission overflow"))?;
            let p = self.policy.overlay;
            if add(self.components.fixed_memory_bytes, next_memory)? > p.max_memory_bytes
                || next_transport > p.max_transport_bytes
                || next_scratch > p.max_scratch_bytes
                || next_files > p.max_files - BACKING_FILES
            {
                return Err(capacity("workspace overlay resource admission"));
            }
            let mut global = self.host.used.lock().unwrap();
            let next_global = global.plus(memory, 0, 0, 0)?;
            self.host.check(next_global)?;
            active.slots[slot] += 1;
            active.memory = next_memory;
            active.transport = next_transport;
            active.scratch = next_scratch;
            active.files = next_files;
            *global = next_global;
        }
        Ok(Permit {
            budget: self.clone(),
            operation,
            charge,
            _live: live,
            _transfer: transfer,
        })
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut active = self.budget.active.lock().unwrap();
        let mut global = self.budget.host.used.lock().unwrap();
        let memory = self.charge.memory_bytes + self.charge.transport_bytes;
        active.slots[self.operation as usize] -= 1;
        active.memory -= memory;
        active.transport -= self.charge.transport_bytes;
        active.scratch -= self.charge.scratch_bytes;
        active.files -= self.charge.files;
        global.memory_bytes -= memory;
    }
}
impl Drop for Budget {
    fn drop(&mut self) {
        let mut used = self.host.used.lock().unwrap();
        used.memory_bytes -= self.components.fixed_memory_bytes;
        used.reserved_disk_bytes -= self.components.reserved_disk_bytes;
        used.reserved_files -= self.policy.overlay.max_files;
        used.workspaces -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> ResourcePolicy {
        let mut p = ResourcePolicy {
            max_spool_bytes: 64 * 1024,
            ..ResourcePolicy::default()
        };
        p.overlay.max_index_bytes = 8 * MIB;
        p.overlay.max_payload_index_bytes = 8 * MIB;
        p.overlay.max_retained_payload_bytes = 64 * 1024;
        p.overlay.max_scratch_bytes = 64 * 1024;
        p.overlay.max_memory_bytes = 8 * MIB;
        p.overlay.max_transport_bytes = 64 * 1024;
        p.overlay.max_replay_bytes = 256;
        p.overlay.max_roots = 32;
        p.overlay.max_payload_owners = 64;
        p.overlay.max_readers = 2;
        p.overlay.max_writers = 2;
        p.overlay.max_prepared = 2;
        p.overlay.max_requests = 4;
        p.overlay.max_replay_entries = 4;
        p
    }
    fn admission(p: ResourcePolicy, count: usize) -> Arc<HostAdmission> {
        HostAdmission::new(
            LiveRuntime::shared().unwrap().scheduler(),
            HostLimits {
                memory_bytes: p.overlay.max_memory_bytes * count as u64,
                disk_bytes: components(p).unwrap().reserved_disk_bytes * count as u64,
                files: p.overlay.max_files * count,
            },
        )
        .unwrap()
    }
    #[test]
    fn full_index_capacity_and_operation_limits_accept_exact_reject_plus_one() {
        for pages in [1, 15, 16, 17, 1024] {
            let bytes = index_bytes(pages).unwrap();
            assert_eq!(index_limits(bytes, 32).unwrap().max_pages, pages);
            assert!(index_bytes(pages + 1).unwrap() > bytes);
        }
        let p = policy();
        let host = admission(p, 1);
        let budget = Budget::reserve(p, host.clone()).unwrap();
        let baseline = host.usage();
        let held = budget
            .enter(
                Operation::Scratch,
                Charge {
                    memory_bytes: p.overlay.max_memory_bytes - baseline.memory_bytes,
                    scratch_bytes: p.overlay.max_scratch_bytes,
                    files: 2,
                    ..Charge::default()
                },
            )
            .unwrap();
        assert!(budget
            .enter(
                Operation::Scratch,
                Charge {
                    memory_bytes: 1,
                    ..Charge::default()
                }
            )
            .is_err());
        assert!(budget
            .enter(
                Operation::Scratch,
                Charge {
                    scratch_bytes: 1,
                    ..Charge::default()
                }
            )
            .is_err());
        assert!(budget
            .enter(
                Operation::Scratch,
                Charge {
                    files: 1,
                    ..Charge::default()
                }
            )
            .is_err());
        drop(held);
        assert_eq!(host.usage(), baseline);
        let transport = budget
            .enter(
                Operation::Request,
                Charge {
                    transport_bytes: p.overlay.max_transport_bytes,
                    ..Charge::default()
                },
            )
            .unwrap();
        assert!(budget
            .enter(
                Operation::Request,
                Charge {
                    transport_bytes: 1,
                    ..Charge::default()
                }
            )
            .is_err());
        drop(transport);
        let first = budget.enter(Operation::Read, Charge::default()).unwrap();
        let second = budget.enter(Operation::Read, Charge::default()).unwrap();
        assert!(budget.enter(Operation::Read, Charge::default()).is_err());
        drop((first, second, budget));
        assert_eq!(host.usage(), Usage::default());
    }
    #[test]
    fn two_workspaces_share_aggregate_reservation_and_failed_op_does_not_leak() {
        let p = policy();
        let host = admission(p, 2);
        let first = Budget::reserve(p, host.clone()).unwrap();
        let second = Budget::reserve(p, host.clone()).unwrap();
        assert_eq!(host.usage().workspaces, 2);
        let before = host.usage();
        assert!(Budget::reserve(p, host.clone()).is_err());
        assert_eq!(host.usage(), before);
        assert!(first
            .enter(
                Operation::Prepare,
                Charge {
                    memory_bytes: u64::MAX,
                    ..Charge::default()
                }
            )
            .is_err());
        assert_eq!(host.usage(), before);
        drop(first);
        assert_eq!(host.usage().workspaces, 1);
        drop(second);
        assert_eq!(host.usage(), Usage::default());
    }
    #[test]
    fn detached_snapshot_and_payload_range_retain_aggregate_admission() {
        let p = policy();
        let host = admission(p, 1);
        let resources = Budget::open(&std::env::temp_dir(), p, host.clone()).unwrap();
        let snapshot = resources.index.empty_root().unwrap();
        let payload = resources.payload.write_from(&mut &b"held"[..], 4).unwrap();
        drop(resources);
        assert_eq!(host.usage().workspaces, 1);
        assert!(Budget::reserve(p, host.clone()).is_err());
        drop(snapshot);
        assert_eq!(host.usage().workspaces, 1);
        let mut bytes = [0; 4];
        payload.read_exact_at(&mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"held");
        drop(payload);
        assert_eq!(host.usage(), Usage::default());
    }
    #[test]
    fn aggregate_counter_and_policy_overflow_are_rejected_without_wraparound() {
        let full = Usage {
            memory_bytes: u64::MAX,
            reserved_disk_bytes: u64::MAX,
            reserved_files: usize::MAX,
            workspaces: usize::MAX,
        };
        assert!(full.plus(1, 0, 0, 0).is_err());
        assert!(full.plus(0, 1, 0, 0).is_err());
        assert!(full.plus(0, 0, 1, 0).is_err());
        assert!(full.plus(0, 0, 0, 1).is_err());
        let mut p = policy();
        p.max_spool_bytes = u64::MAX;
        assert!(components(p).is_err());
        assert!(index_bytes(u64::MAX).is_err());
    }
}
