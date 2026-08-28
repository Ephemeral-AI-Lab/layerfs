//! Frozen legacy_full evaluator session composition.

mod error;
mod external;
mod layer_fs;
mod lease;
mod managed_edit;
mod managed_lifecycle;
mod managed_state;
mod operation_q;
#[cfg(test)]
mod tests;

pub(crate) use error::{VfsError, VfsResult};
pub(crate) use external::ExternalWorkspace;
pub(crate) use layer_fs::{topology_edge_key, LayerFs};
use managed_lifecycle::ManagedState;
pub(crate) use managed_lifecycle::ManagedWorkspace;
