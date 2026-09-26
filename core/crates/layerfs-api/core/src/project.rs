//! The published genesis result and Project errors.
use layerfs_bridge::contract::{Failure, BRANCH_BYTES, COMMIT_BYTES, LAYER_BYTES, STACK_BYTES};

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

/// One published Branch of a project, with the roots it resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub id: [u8; BRANCH_BYTES],
    pub name: String,
    pub base_layer: [u8; LAYER_BYTES],
    pub head_commit: Option<[u8; COMMIT_BYTES]>,
    /// Root of the head Commit, absent when the Branch has no Commit.
    pub head_root: Option<[u8; 32]>,
    pub base_root: [u8; 32],
    pub effective_root: [u8; 32],
    pub root_serial: Option<u64>,
    pub scope: [u8; 32],
}
