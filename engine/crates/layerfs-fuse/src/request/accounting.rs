//! Per-mount opcode and disposal counts; one frame can carry several units.
use crate::phase::{Phases, PHASES};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Every kernel operation the pinned adapter can receive on Linux.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Opcode {
    Lookup,
    Forget,
    BatchForget,
    Getattr,
    Setattr,
    Readlink,
    Mknod,
    Mkdir,
    Unlink,
    Rmdir,
    Symlink,
    Rename,
    Link,
    Open,
    Read,
    Write,
    Flush,
    Release,
    Fsync,
    Opendir,
    Readdir,
    Readdirplus,
    Releasedir,
    Fsyncdir,
    Statfs,
    Setxattr,
    Getxattr,
    Listxattr,
    Removexattr,
    Access,
    Create,
    Getlk,
    Setlk,
    Bmap,
    Ioctl,
    Poll,
    Fallocate,
    Lseek,
    CopyFileRange,
}
pub const OPCODES: usize = Opcode::CopyFileRange as usize + 1;
impl Opcode {
    /// Snake-case names, indexed by `Opcode as usize`, as telemetry reports them.
    pub const NAMES: [&'static str; OPCODES] = [
        "lookup",
        "forget",
        "batch_forget",
        "getattr",
        "setattr",
        "readlink",
        "mknod",
        "mkdir",
        "unlink",
        "rmdir",
        "symlink",
        "rename",
        "link",
        "open",
        "read",
        "write",
        "flush",
        "release",
        "fsync",
        "opendir",
        "readdir",
        "readdirplus",
        "releasedir",
        "fsyncdir",
        "statfs",
        "setxattr",
        "getxattr",
        "listxattr",
        "removexattr",
        "access",
        "create",
        "getlk",
        "setlk",
        "bmap",
        "ioctl",
        "poll",
        "fallocate",
        "lseek",
        "copy_file_range",
    ];
}

/// The single terminal disposition of one received unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Disposal {
    /// The receiving worker ran the original request and its reply.
    Served,
    /// Replied on the receive loop with no engine job,
    /// durability or SQL work.
    Inline,
    /// A declared refusal or invalid identity, replied on the receive loop.
    Refused,
    /// Reply-bearing unit met terminal/failed admission: one error attempt.
    Terminal,
}
const DISPOSALS: usize = Disposal::Terminal as usize + 1;

pub struct Accounting {
    opcodes: [AtomicU64; OPCODES],
    disposals: [AtomicU64; DISPOSALS],
    forget_units: AtomicU64,
    store_units: AtomicU64,
    largest_read: AtomicU64,
    largest_write: AtomicU64,
    lookup_missing: AtomicU64,
    served_ns: AtomicU64,
    nanos: [AtomicU64; OPCODES],
    phases: Arc<Phases>,
}
/// Monotonic counters copied without a lock; fields are individually exact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpcodeWork {
    pub opcodes: [u64; OPCODES],
    pub served: u64,
    pub inline: u64,
    pub refused: u64,
    pub terminal: u64,
    /// Independently bounded ownership units, distinct from FORGET frames.
    pub forget_units: u64,
    /// WRITE units that carried the per-request page-cache flag: stores from
    /// a shared mapping, counted among the WRITE opcode's frames.
    pub store_units: u64,
    /// Largest size any READ frame asked for, and largest data length any
    /// WRITE frame carried, as the kernel sent them: the observed request
    /// maxima beside the negotiated ones. Zero until the first such frame.
    pub largest_read: u64,
    pub largest_write: u64,
    /// LOOKUP requests whose decided reply was ENOENT, included in the lookup
    /// opcode count above.
    pub lookup_missing: u64,
    /// Wall nanoseconds request workers spent inside `RequestOwner::serve`,
    /// summed over every served unit. A pending future parks that worker, so
    /// waits for service jobs are included; the request still runs to
    /// completion on the worker that received it.
    pub served_ns: u64,
    /// Wall nanoseconds the receive workers spent inside each opcode's
    /// callback, from entry to return (the reply is sent inside it). FORGET
    /// charges every unit, so its time divides by `forget_units`.
    pub nanos: [u64; OPCODES],
    /// Count and nanoseconds of each [`Phase`] inside the callbacks.
    pub phase_counts: [u64; PHASES],
    pub phase_nanos: [u64; PHASES],
}
impl OpcodeWork {
    pub fn count(&self, opcode: Opcode) -> u64 {
        self.opcodes[opcode as usize]
    }
    pub fn frames(&self) -> u64 {
        self.opcodes.iter().sum()
    }
}
impl Default for Accounting {
    fn default() -> Self {
        Self {
            opcodes: std::array::from_fn(|_| AtomicU64::new(0)),
            disposals: std::array::from_fn(|_| AtomicU64::new(0)),
            forget_units: AtomicU64::new(0),
            store_units: AtomicU64::new(0),
            largest_read: AtomicU64::new(0),
            largest_write: AtomicU64::new(0),
            lookup_missing: AtomicU64::new(0),
            served_ns: AtomicU64::new(0),
            nanos: std::array::from_fn(|_| AtomicU64::new(0)),
            phases: Arc::new(Phases::default()),
        }
    }
}

/// Charges the wall time from its creation to its drop to one opcode, and
/// makes its accounting the current one for [`charge`] meanwhile.
pub struct Timed {
    accounting: Arc<Accounting>,
    opcode: Opcode,
    started: Instant,
    previous: Option<Arc<Phases>>,
}
impl Drop for Timed {
    fn drop(&mut self) {
        let elapsed = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.accounting.nanos[self.opcode as usize].fetch_add(elapsed, Ordering::Relaxed);
        crate::phase::restore(self.previous.take());
    }
}
impl Accounting {
    pub(super) fn opcode(&self, opcode: Opcode) {
        self.opcodes[opcode as usize].fetch_add(1, Ordering::Relaxed);
    }
    /// Times one callback of `opcode` until the returned guard drops.
    pub fn timed(self: &Arc<Self>, opcode: Opcode) -> Timed {
        let previous = crate::phase::enter(self.phases.clone());
        Timed {
            accounting: self.clone(),
            opcode,
            started: Instant::now(),
            previous,
        }
    }
    pub(super) fn disposed(&self, disposal: Disposal) {
        self.disposals[disposal as usize].fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn forget_unit(&self) {
        self.forget_units.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn store_unit(&self) {
        self.store_units.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn read_size(&self, size: u32) {
        self.largest_read
            .fetch_max(u64::from(size), Ordering::Relaxed);
    }
    pub(super) fn write_size(&self, length: usize) {
        self.largest_write
            .fetch_max(length as u64, Ordering::Relaxed);
    }
    pub(super) fn lookup_missing(&self) {
        self.lookup_missing.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn served_ns(&self, elapsed: std::time::Duration) {
        self.served_ns.fetch_add(
            u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }
    pub fn observe(&self) -> OpcodeWork {
        let disposal = |value: Disposal| self.disposals[value as usize].load(Ordering::Relaxed);
        OpcodeWork {
            opcodes: std::array::from_fn(|index| self.opcodes[index].load(Ordering::Relaxed)),
            served: disposal(Disposal::Served),
            inline: disposal(Disposal::Inline),
            refused: disposal(Disposal::Refused),
            terminal: disposal(Disposal::Terminal),
            forget_units: self.forget_units.load(Ordering::Relaxed),
            store_units: self.store_units.load(Ordering::Relaxed),
            largest_read: self.largest_read.load(Ordering::Relaxed),
            largest_write: self.largest_write.load(Ordering::Relaxed),
            lookup_missing: self.lookup_missing.load(Ordering::Relaxed),
            served_ns: self.served_ns.load(Ordering::Relaxed),
            nanos: std::array::from_fn(|index| self.nanos[index].load(Ordering::Relaxed)),
            phase_counts: self.phases.counts(),
            phase_nanos: self.phases.nanos(),
        }
    }
}
