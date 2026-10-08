//! Native connection identity and one atomic semantic observation's custody.
use crate::{FileRead, Inode, NativeDirectory, OpenFile, OverlayResult, Publication, Route};

/// One engine-minted native connection. Copying the token acquires no owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeMount {
    pub(crate) route: Route,
    pub(crate) owner: u64,
    pub(crate) root: u64,
}
impl NativeMount {
    pub const fn route(self) -> Route {
        self.route
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
pub enum NativeDecision<T> {
    Needs(T),
    Finished { inode: Option<Inode>, value: T },
}
/// The deciding value is retained independently of transaction completion.
/// On an uncertain completion it is evidence, never authority to send success.
#[derive(Debug)]
pub struct NativeObservation<T> {
    pub decision: Option<T>,
    pub result: OverlayResult<Option<FileRead>>,
    /// Candidate minted inside the transaction, including failed/unknown
    /// completion. Only result Ok proves acquisition; never release/adopt this
    /// candidate on a guess when the original result failed or is uncertain.
    pub candidate: Option<FileRead>,
    /// Original open candidate, subject to the same completion rule as read.
    /// A successful observation is required before this can be used as a handle.
    pub open_candidate: Option<OpenFile>,
    /// Original directory-open candidate; usable only after successful result.
    pub directory_candidate: Option<NativeDirectory>,
}

/// Whether the native connection's logical admission has been revoked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeMountState {
    Live,
    Revoked,
    Gone,
}

/// Kernel custody one successful native mutation's reply hands over. It is
/// acquired in the publishing transaction, before any reply attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeEffect {
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
/// One published native mutation and the descriptor it opened, if any.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeApplied {
    pub publication: Publication,
    pub file: Option<OpenFile>,
}
