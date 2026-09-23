//! The published genesis result and honest future Workspace surface.
use layerfs_bridge::contract::{Failure, LAYER_BYTES, STACK_BYTES};

/// One project is one existing LayerStack, with its published genesis root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub id: [u8; STACK_BYTES],
    pub genesis_layer: [u8; LAYER_BYTES],
    pub root: [u8; 32],
    pub root_serial: u64,
}

/// Preserves definite, unknown and cleanup failures from the real Service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Backend(Failure),
    Unsupported,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend(error) => write!(f, "{error}"),
            Self::Unsupported => f.write_str("operation is not implemented"),
        }
    }
}
impl std::error::Error for Error {}
impl From<Failure> for Error {
    fn from(error: Failure) -> Self {
        Self::Backend(error)
    }
}

impl Project {
    /// Workspace provisioning has no Core implementation yet.
    pub fn mount_workspace(&self) -> Result<Workspace, Error> {
        Err(Error::Unsupported)
    }
}

/// Future writable mount; no value is returned until provisioning exists.
#[derive(Debug)]
pub struct Workspace;
impl Workspace {
    pub fn commit(&self) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
    pub fn unmount(&self) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
    pub fn close_clean(&self) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
    pub fn exec(&self, _shell_command: &str) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
}
