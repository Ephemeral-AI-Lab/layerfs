//! Engine-minted, independently releasable filesystem processing custody.
use crate::{BaseSource, Capture, Route};

/// One actual open descriptor. Cloning a token does not acquire another owner;
/// use and release validate its exact nonrecycled SQL owner identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenFile {
    pub(crate) route: Route,
    pub(crate) owner: u64,
    pub(crate) serial: u64,
    pub(crate) writable: bool,
}
impl OpenFile {
    /// Nonrecycled engine identity suitable for an encoded native handle.
    /// The number alone grants no access; validate mount, route and serial.
    pub const fn owner_id(self) -> u64 {
        self.owner
    }
    pub const fn route(self) -> Route {
        self.route
    }
    pub const fn serial(self) -> u64 {
        self.serial
    }
    pub const fn writable(self) -> bool {
        self.writable
    }
}
/// One read-processing window, independent of descriptor and Exec lifetimes.
/// It owns its own exact base source and file reader reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileRead {
    pub(crate) source: BaseSource,
    pub(crate) serial: u64,
}
impl FileRead {
    pub const fn source(self) -> BaseSource {
        self.source
    }
    pub const fn serial(self) -> u64 {
        self.serial
    }
}
/// Fixed captured input retained independently of install/failure/descriptor
/// custody. Its original root/floor remain valid until exact release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapturedReader {
    pub(crate) capture: Capture,
    pub(crate) owner: u64,
    pub(crate) installed: i64,
}
impl CapturedReader {
    pub const fn capture(self) -> Capture {
        self.capture
    }
    pub const fn root(self) -> [u8; 32] {
        self.capture.base_root
    }
    /// Exact engine-minted reader identity for an owning local input context.
    /// Copying this number acquires no lease and authorizes no independent job.
    pub const fn owner_id(self) -> u64 {
        self.owner
    }
    /// Immutable installed-generation floor retained by this reader. Both
    /// source and retained-reader SQL rows enforce nonnegative generations.
    pub const fn installed_floor(self) -> u64 {
        self.installed as u64
    }
    /// Newness relative to this reader's retained installed floor. The caller
    /// supplies the `born` field from this exact reader's inode point/page.
    pub const fn created_above(self, born: u64) -> bool {
        born > self.installed as u64
    }
}

/// One nonrecycled operation owner, independent of command/process lifetime.
/// Its backed operation_record remains in cleanup custody after exact release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationOwner {
    pub(crate) route: Route,
    pub(crate) owner: u64,
}
impl OperationOwner {
    pub const fn route(self) -> Route {
        self.route
    }
}

/// One actual lookup reference, independently retained until exact FORGET/release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LookupOwner {
    pub(crate) route: Route,
    pub(crate) owner: u64,
    pub(crate) serial: u64,
}
impl LookupOwner {
    pub const fn route(self) -> Route {
        self.route
    }
    pub const fn serial(self) -> u64 {
        self.serial
    }
}
