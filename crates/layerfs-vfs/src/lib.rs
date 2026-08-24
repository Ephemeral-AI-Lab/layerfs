//! LayerFS filesystem-facing projection.

#![forbid(unsafe_code)]

pub mod capture;
pub mod driver;
mod managed_edit;
pub mod materialize;
pub mod workspace;

pub use workspace::{ExternalWorkspace, LayerVfs, ManagedWorkspace, VfsError};
pub type RootId = layerfs_core::ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRoute {
    CaptureStream,
    MaterializeStream,
    ExactNoop,
    ClonePatch,
    InPlacePatch,
    InPlaceShift,
    Rename,
    ProtectedExactNoop,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeOperationCounters {
    pub route: Option<NativeRoute>,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub patch_bytes: u64,
    pub suffix_bytes_shifted: u64,
    pub clone_attempts: u64,
    pub clone_successes: u64,
    pub clone_fallbacks: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationCounters {
    pub rope: layerfs_core::content::rope::RopeCounters,
    pub namespace: layerfs_core::namespace::NamespaceCounters,
    pub inode_table: layerfs_core::inode::InodeTableCounters,
    pub native: NativeOperationCounters,
}

impl OperationCounters {
    pub fn merge(mut self, source: Self) -> Result<Self, layerfs_core::CoreError> {
        self.add_rope(source.rope)?;
        self.add_namespace(source.namespace)?;
        self.add_inode_table(source.inode_table)?;
        self.add_native(source.native)?;
        Ok(self)
    }

    pub(crate) fn add_rope(
        &mut self,
        source: layerfs_core::content::rope::RopeCounters,
    ) -> Result<(), layerfs_core::CoreError> {
        self.rope.payload_bytes_read =
            add(self.rope.payload_bytes_read, source.payload_bytes_read)?;
        self.rope.payload_bytes_written = add(
            self.rope.payload_bytes_written,
            source.payload_bytes_written,
        )?;
        self.rope.cdc_bytes_scanned = add(self.rope.cdc_bytes_scanned, source.cdc_bytes_scanned)?;
        self.rope.chunks_created = add(self.rope.chunks_created, source.chunks_created)?;
        self.rope.nodes_read = add(self.rope.nodes_read, source.nodes_read)?;
        self.rope.nodes_created = add(self.rope.nodes_created, source.nodes_created)?;
        Ok(())
    }

    pub(crate) fn add_namespace(
        &mut self,
        source: layerfs_core::namespace::NamespaceCounters,
    ) -> Result<(), layerfs_core::CoreError> {
        self.namespace.nodes_read = add(self.namespace.nodes_read, source.nodes_read)?;
        self.namespace.nodes_created = add(self.namespace.nodes_created, source.nodes_created)?;
        Ok(())
    }

    pub(crate) fn add_inode_table(
        &mut self,
        source: layerfs_core::inode::InodeTableCounters,
    ) -> Result<(), layerfs_core::CoreError> {
        self.inode_table.nodes_read = add(self.inode_table.nodes_read, source.nodes_read)?;
        self.inode_table.nodes_created = add(self.inode_table.nodes_created, source.nodes_created)?;
        Ok(())
    }

    pub(crate) fn add_native(
        &mut self,
        source: NativeOperationCounters,
    ) -> Result<(), layerfs_core::CoreError> {
        self.native.route = source.route.or(self.native.route);
        self.native.bytes_read = add(self.native.bytes_read, source.bytes_read)?;
        self.native.bytes_written = add(self.native.bytes_written, source.bytes_written)?;
        self.native.patch_bytes = add(self.native.patch_bytes, source.patch_bytes)?;
        self.native.suffix_bytes_shifted = add(
            self.native.suffix_bytes_shifted,
            source.suffix_bytes_shifted,
        )?;
        self.native.clone_attempts = add(self.native.clone_attempts, source.clone_attempts)?;
        self.native.clone_successes = add(self.native.clone_successes, source.clone_successes)?;
        self.native.clone_fallbacks = add(self.native.clone_fallbacks, source.clone_fallbacks)?;
        Ok(())
    }
}

fn add(left: u64, right: u64) -> Result<u64, layerfs_core::CoreError> {
    left.checked_add(right)
        .ok_or(layerfs_core::CoreError::LengthOverflow)
}

/// Structural upper bound: one 1 MiB xattr set, one <=1.125 MiB serialized
/// metadata item, one 1 MiB stream window, and <0.875 MiB of bounded tree,
/// descriptor, and SQL parameter pages. SQLite caches and caller buffers are
/// reported separately.
pub const OPERATION_Q_BOUND_BYTES: u64 = 4 * 1024 * 1024;

pub const COMPONENT: &str = "layerfs-vfs";

#[cfg(test)]
mod operation_counter_tests {
    use super::*;

    #[test]
    fn operation_counters_preserve_structural_and_native_facts() {
        let mut counters = OperationCounters::default();
        counters
            .add_rope(layerfs_core::content::rope::RopeCounters {
                cdc_bytes_scanned: 4096,
                nodes_created: 2,
                ..Default::default()
            })
            .unwrap();
        counters
            .add_namespace(layerfs_core::namespace::NamespaceCounters {
                nodes_read: 3,
                nodes_created: 1,
            })
            .unwrap();
        counters
            .add_inode_table(layerfs_core::inode::InodeTableCounters {
                nodes_read: 4,
                nodes_created: 2,
            })
            .unwrap();
        counters
            .add_native(NativeOperationCounters {
                route: Some(NativeRoute::ClonePatch),
                bytes_written: 4096,
                patch_bytes: 4096,
                clone_attempts: 1,
                clone_successes: 1,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(counters.rope.cdc_bytes_scanned, 4096);
        assert_eq!(counters.rope.nodes_created, 2);
        assert_eq!(counters.namespace.nodes_read, 3);
        assert_eq!(counters.inode_table.nodes_created, 2);
        assert_eq!(counters.native.route, Some(NativeRoute::ClonePatch));
        assert_eq!(counters.native.patch_bytes, 4096);
    }
}
