//! Per-mount opcode and disposal counts; one frame can carry several units.
use std::sync::atomic::{AtomicU64, Ordering};

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

/// The single terminal disposition of one received unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Disposal {
    /// An owned bounded unit entered the shared dispatcher with its reply.
    Handoff,
    /// Replied on the receive loop; no engine job, durability or SQL work.
    Inline,
    /// A declared refusal or invalid identity, replied on the receive loop.
    Refused,
    /// Reply-bearing unit met terminal/failed admission: one error attempt.
    Terminal,
    /// No-reply ownership unit met terminal admission. Its kernel reference is
    /// disposed only by drain-qualified revocation, never a guessed decrement.
    Unadmitted,
}
const DISPOSALS: usize = Disposal::Unadmitted as usize + 1;

pub struct Accounting {
    opcodes: [AtomicU64; OPCODES],
    disposals: [AtomicU64; DISPOSALS],
    forget_units: AtomicU64,
}
/// Monotonic counters copied without a lock; fields are individually exact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpcodeWork {
    pub opcodes: [u64; OPCODES],
    pub handoffs: u64,
    pub inline: u64,
    pub refused: u64,
    pub terminal: u64,
    pub unadmitted: u64,
    /// Independently bounded ownership units, distinct from FORGET frames.
    pub forget_units: u64,
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
        }
    }
}
impl Accounting {
    pub(super) fn opcode(&self, opcode: Opcode) {
        self.opcodes[opcode as usize].fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn disposed(&self, disposal: Disposal) {
        self.disposals[disposal as usize].fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn forget_unit(&self) {
        self.forget_units.fetch_add(1, Ordering::Relaxed);
    }
    pub fn observe(&self) -> OpcodeWork {
        let disposal = |value: Disposal| self.disposals[value as usize].load(Ordering::Relaxed);
        OpcodeWork {
            opcodes: std::array::from_fn(|index| self.opcodes[index].load(Ordering::Relaxed)),
            handoffs: disposal(Disposal::Handoff),
            inline: disposal(Disposal::Inline),
            refused: disposal(Disposal::Refused),
            terminal: disposal(Disposal::Terminal),
            unadmitted: disposal(Disposal::Unadmitted),
            forget_units: self.forget_units.load(Ordering::Relaxed),
        }
    }
}
