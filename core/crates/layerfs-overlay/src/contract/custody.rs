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
}

/// One nonrecycled operation owner, independent of command/process lifetime.
/// Its backed scratch remains in cleanup custody after exact release.
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
