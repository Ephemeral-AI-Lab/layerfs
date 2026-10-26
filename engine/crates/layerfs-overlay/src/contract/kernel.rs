//! Kernel connection identity and one atomic semantic observation's custody.
use crate::{FileRead, Inode, KernelDirectory, OpenFile, OverlayResult, Publication};

/// One engine-minted kernel connection. Copying the token acquires no owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelMount {
    pub(crate) engine: u64,
    pub(crate) owner: u64,
    pub(crate) root: u64,
}
impl KernelMount {
    pub const fn engine(self) -> u64 {
        self.engine
    }
    pub const fn owner_id(self) -> u64 {
        self.owner
    }
    pub const fn root_serial(self) -> u64 {
        self.root
    }
}

/// Result of the caller's bounded semantic decision over one SourceRows view.
/// None requires no acquisition (for example missing facts or a negative name).
pub enum KernelDecision<T> {
    Needs(T),
    Finished { inode: Option<Inode>, value: T },
}
/// The deciding value is retained independently of transaction completion.
/// On an uncertain completion it is evidence, never authority to send success.
#[derive(Debug)]
pub struct KernelObservation<T> {
    pub decision: Option<T>,
    pub result: OverlayResult<Option<FileRead>>,
    /// Open candidate minted inside the transaction, including failed/unknown
    /// completion. Only result Ok proves acquisition; never release/adopt this
    /// candidate on a guess when the original result failed or is uncertain.
    /// A successful observation is required before this can be used as a handle.
    pub open_candidate: Option<OpenFile>,
    /// Original directory-open candidate; usable only after successful result.
    pub directory_candidate: Option<KernelDirectory>,
}

/// Whether the kernel connection's logical admission has been revoked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelMountState {
    Live,
    Revoked,
    Gone,
}

/// Kernel custody one successful kernel mutation's reply hands over. It is
/// acquired in the publishing transaction, before any reply attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelEffect {
    /// A reply without an entry: write, attributes, unlink, rmdir, rename.
    None,
    /// An entry reply: one lookup reference on `serial` bound under `parent`.
    Entry {
        serial: u64,
        parent: u64,
        directory: bool,
    },
    /// A created regular file replied with its entry and an open descriptor.
    Open {
        serial: u64,
        parent: u64,
        writable: bool,
    },
}
/// One published kernel mutation and the descriptor it opened, if any.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelApplied {
    pub publication: Publication,
    pub file: Option<OpenFile>,
}
