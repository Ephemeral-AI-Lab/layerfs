//! LayerFS filesystem-facing projection.

#![forbid(unsafe_code)]

pub mod capture;
pub mod driver;
mod managed_edit;
pub mod materialize;
pub mod workspace;

pub use workspace::{ExternalWorkspace, LayerVfs, ManagedWorkspace, VfsError};
pub type RootId = layerfs_core::ObjectId;

/// Structural upper bound: one 1 MiB xattr set, one <=1.125 MiB serialized
/// metadata item, one 1 MiB stream window, and <0.875 MiB of bounded tree,
/// descriptor, and SQL parameter pages. SQLite caches and caller buffers are
/// reported separately.
pub const OPERATION_Q_BOUND_BYTES: u64 = 4 * 1024 * 1024;

pub const COMPONENT: &str = "layerfs-vfs";
