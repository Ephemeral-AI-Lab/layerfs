//! LayerFS filesystem-facing projection.

#![forbid(unsafe_code)]

pub mod capture;
pub mod driver;
mod managed_edit;
pub mod materialize;
mod refresh;
mod resolver;
pub mod workspace;

pub use layerfs_core::CanonicalPath;
pub use layerfs_engine::integrity::IntegrityMode;
pub use layerfs_engine::refs::RefState;
pub use workspace::{ExternalWorkspace, LayerVfs, ManagedWorkspace, VfsError};
pub type RootId = layerfs_core::ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRoute {
    CaptureStream,
    MaterializeStream,
    NativeDurableOutput,
    ExactNoop,
    ClonePatch,
    CloneShift,
    InPlacePatch,
    InPlaceShift,
    Rename,
    ProtectedExactNoop,
    FullFallback,
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
    pub temp_calls: u64,
    pub sync_calls: u64,
    pub rename_calls: u64,
    pub replace_calls: u64,
    pub metadata_calls: u64,
    pub create_calls: u64,
    pub remove_calls: u64,
    pub hard_link_calls: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationCounters {
    pub rope: layerfs_core::content::rope::RopeCounters,
    pub metadata_rope: layerfs_core::content::rope::RopeCounters,
    pub namespace: layerfs_core::namespace::NamespaceCounters,
    pub inode_table: layerfs_core::inode::InodeTableCounters,
    pub native: NativeOperationCounters,
    pub projection: crate::driver::ProjectionFacts,
    pub materialize_inclusive_ns: u64,
    pub workspace_materializations: u64,
    pub workspace_reuses: u64,
    pub rematerializations: u64,
    pub descriptor_resets: u64,
    pub root_diff_nodes: u64,
    pub changed_paths: u64,
    pub full_fallback_files: u64,
    pub plan_rows: u64,
    pub plan_scratch_high_water_bytes: u64,
    pub current_digest_bytes: u64,
    pub uncached_prior_digest_bytes: u64,
    pub changed_current_cdc_bytes: u64,
    pub unchanged_file_roots_reused: u64,
    pub authority_full_scans: u64,
    pub scratch_tables: u64,
    pub scratch_statements: u64,
    pub scratch_rows: u64,
    pub scratch_high_water_bytes: u64,
    pub scratch_owner_setup_statements: u64,
    pub scratch_derived_setup_statements: u64,
    pub scratch_operation_statements: u64,
    pub scratch_store_reopens: u64,
    pub scratch_store_inspection_statements: u64,
    pub scratch_store_inspection_wall_ns: u64,
    pub scratch_setup_wall_ns: u64,
    pub scratch_operation_wall_ns: u64,
    pub operation_q_current_bytes: u64,
    pub operation_q_high_water_bytes: u64,
    pub operation_q_terminal_bytes: u64,
    pub owned_temp_current: u64,
    pub owned_temp_terminal: u64,
    pub descriptor_spool_bytes_current: u64,
    pub descriptor_spool_bytes_terminal: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ManagedReplayStep {
    pub tree_level_before: Option<u8>,
    pub counters: OperationCounters,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedSplice {
    pub(crate) before: RefState,
    pub(crate) after: RefState,
    pub(crate) path: CanonicalPath,
    pub(crate) start: u64,
    pub(crate) delete_len: u64,
    pub(crate) insert_len: u64,
    pub(crate) old_len: u64,
    pub(crate) new_len: u64,
}

impl AcceptedSplice {
    pub fn before(&self) -> &RefState {
        &self.before
    }
    pub fn after(&self) -> &RefState {
        &self.after
    }
    pub fn path(&self) -> &CanonicalPath {
        &self.path
    }
    pub fn start(&self) -> u64 {
        self.start
    }
    pub fn delete_len(&self) -> u64 {
        self.delete_len
    }
    pub fn insert_len(&self) -> u64 {
        self.insert_len
    }
}

impl OperationCounters {
    pub fn merge(mut self, source: Self) -> Result<Self, layerfs_core::CoreError> {
        add_rope_counters(&mut self.rope, source.rope)?;
        add_rope_counters(&mut self.metadata_rope, source.metadata_rope)?;
        self.add_namespace(source.namespace)?;
        self.add_inode_table(source.inode_table)?;
        self.add_native(source.native)?;
        self.projection = self
            .projection
            .checked_add(source.projection)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        self.materialize_inclusive_ns = add(
            self.materialize_inclusive_ns,
            source.materialize_inclusive_ns,
        )?;
        self.workspace_materializations = add(
            self.workspace_materializations,
            source.workspace_materializations,
        )?;
        self.workspace_reuses = add(self.workspace_reuses, source.workspace_reuses)?;
        self.rematerializations = add(self.rematerializations, source.rematerializations)?;
        self.descriptor_resets = add(self.descriptor_resets, source.descriptor_resets)?;
        self.root_diff_nodes = add(self.root_diff_nodes, source.root_diff_nodes)?;
        self.changed_paths = add(self.changed_paths, source.changed_paths)?;
        self.full_fallback_files = add(self.full_fallback_files, source.full_fallback_files)?;
        self.plan_rows = add(self.plan_rows, source.plan_rows)?;
        self.plan_scratch_high_water_bytes = self
            .plan_scratch_high_water_bytes
            .max(source.plan_scratch_high_water_bytes);
        self.current_digest_bytes = add(self.current_digest_bytes, source.current_digest_bytes)?;
        self.uncached_prior_digest_bytes = add(
            self.uncached_prior_digest_bytes,
            source.uncached_prior_digest_bytes,
        )?;
        self.changed_current_cdc_bytes = add(
            self.changed_current_cdc_bytes,
            source.changed_current_cdc_bytes,
        )?;
        self.unchanged_file_roots_reused = add(
            self.unchanged_file_roots_reused,
            source.unchanged_file_roots_reused,
        )?;
        self.authority_full_scans = add(self.authority_full_scans, source.authority_full_scans)?;
        self.scratch_tables = add(self.scratch_tables, source.scratch_tables)?;
        self.scratch_statements = add(self.scratch_statements, source.scratch_statements)?;
        self.scratch_rows = add(self.scratch_rows, source.scratch_rows)?;
        self.scratch_high_water_bytes = add(
            self.scratch_high_water_bytes,
            source.scratch_high_water_bytes,
        )?;
        self.scratch_owner_setup_statements = add(
            self.scratch_owner_setup_statements,
            source.scratch_owner_setup_statements,
        )?;
        self.scratch_derived_setup_statements = add(
            self.scratch_derived_setup_statements,
            source.scratch_derived_setup_statements,
        )?;
        self.scratch_operation_statements = add(
            self.scratch_operation_statements,
            source.scratch_operation_statements,
        )?;
        self.scratch_store_reopens = add(self.scratch_store_reopens, source.scratch_store_reopens)?;
        self.scratch_store_inspection_statements = add(
            self.scratch_store_inspection_statements,
            source.scratch_store_inspection_statements,
        )?;
        self.scratch_store_inspection_wall_ns = add(
            self.scratch_store_inspection_wall_ns,
            source.scratch_store_inspection_wall_ns,
        )?;
        self.scratch_setup_wall_ns = add(self.scratch_setup_wall_ns, source.scratch_setup_wall_ns)?;
        self.scratch_operation_wall_ns = add(
            self.scratch_operation_wall_ns,
            source.scratch_operation_wall_ns,
        )?;
        self.operation_q_current_bytes = self
            .operation_q_current_bytes
            .max(source.operation_q_current_bytes);
        self.operation_q_high_water_bytes = self
            .operation_q_high_water_bytes
            .max(source.operation_q_high_water_bytes);
        self.operation_q_terminal_bytes = source.operation_q_terminal_bytes;
        self.owned_temp_current = source.owned_temp_current;
        self.owned_temp_terminal = source.owned_temp_terminal;
        self.descriptor_spool_bytes_current = source.descriptor_spool_bytes_current;
        self.descriptor_spool_bytes_terminal = source.descriptor_spool_bytes_terminal;
        Ok(self)
    }

    /// Payload bytes touched by the content rope, excluding metadata value ropes.
    pub fn content_payload_bytes_read(&self) -> Option<u64> {
        self.rope
            .payload_bytes_read
            .checked_sub(self.metadata_rope.payload_bytes_read)
    }

    /// Payload bytes emitted by the content rope, excluding metadata value ropes.
    pub fn content_payload_bytes_written(&self) -> Option<u64> {
        self.rope
            .payload_bytes_written
            .checked_sub(self.metadata_rope.payload_bytes_written)
    }

    pub(crate) fn add_scratch(
        &mut self,
        source: layerfs_engine::scratch::ScratchObservation,
    ) -> Result<(), layerfs_core::CoreError> {
        self.scratch_tables = add(self.scratch_tables, source.tables)?;
        self.scratch_statements = add(self.scratch_statements, source.statements)?;
        self.scratch_rows = add(self.scratch_rows, source.rows)?;
        self.scratch_high_water_bytes =
            add(self.scratch_high_water_bytes, source.high_water_bytes)?;
        self.scratch_owner_setup_statements = add(
            self.scratch_owner_setup_statements,
            source.owner_setup_statements,
        )?;
        self.scratch_derived_setup_statements = add(
            self.scratch_derived_setup_statements,
            source.derived_setup_statements,
        )?;
        self.scratch_operation_statements = add(
            self.scratch_operation_statements,
            source.operation_statements,
        )?;
        self.scratch_store_reopens = add(self.scratch_store_reopens, source.store_reopens)?;
        self.scratch_store_inspection_statements = add(
            self.scratch_store_inspection_statements,
            source.store_inspection_statements,
        )?;
        self.scratch_store_inspection_wall_ns = add(
            self.scratch_store_inspection_wall_ns,
            source.store_inspection_wall_ns,
        )?;
        self.scratch_setup_wall_ns = add(self.scratch_setup_wall_ns, source.setup_wall_ns)?;
        self.scratch_operation_wall_ns =
            add(self.scratch_operation_wall_ns, source.operation_wall_ns)?;
        Ok(())
    }

    pub(crate) fn add_rope(
        &mut self,
        source: layerfs_core::content::rope::RopeCounters,
    ) -> Result<(), layerfs_core::CoreError> {
        add_rope_counters(&mut self.rope, source)
    }

    pub(crate) fn add_metadata_rope(
        &mut self,
        source: layerfs_core::content::rope::RopeCounters,
    ) -> Result<(), layerfs_core::CoreError> {
        add_rope_counters(&mut self.metadata_rope, source)?;
        add_rope_counters(&mut self.rope, source)
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
        self.native.temp_calls = add(self.native.temp_calls, source.temp_calls)?;
        self.native.sync_calls = add(self.native.sync_calls, source.sync_calls)?;
        self.native.rename_calls = add(self.native.rename_calls, source.rename_calls)?;
        self.native.replace_calls = add(self.native.replace_calls, source.replace_calls)?;
        self.native.metadata_calls = add(self.native.metadata_calls, source.metadata_calls)?;
        self.native.create_calls = add(self.native.create_calls, source.create_calls)?;
        self.native.remove_calls = add(self.native.remove_calls, source.remove_calls)?;
        self.native.hard_link_calls = add(self.native.hard_link_calls, source.hard_link_calls)?;
        Ok(())
    }
}

fn add_rope_counters(
    target: &mut layerfs_core::content::rope::RopeCounters,
    source: layerfs_core::content::rope::RopeCounters,
) -> Result<(), layerfs_core::CoreError> {
    target.payload_bytes_read = add(target.payload_bytes_read, source.payload_bytes_read)?;
    target.payload_bytes_written = add(target.payload_bytes_written, source.payload_bytes_written)?;
    target.cdc_bytes_scanned = add(target.cdc_bytes_scanned, source.cdc_bytes_scanned)?;
    target.chunks_created = add(target.chunks_created, source.chunks_created)?;
    target.nodes_read = add(target.nodes_read, source.nodes_read)?;
    target.nodes_created = add(target.nodes_created, source.nodes_created)?;
    target.tree_level_before = match (target.tree_level_before, source.tree_level_before) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(_), Some(_)) => None,
    };
    target.logical_len_before =
        merge_optional_equal(target.logical_len_before, source.logical_len_before);
    target.logical_len_after =
        merge_optional_equal(target.logical_len_after, source.logical_len_after);
    Ok(())
}

fn merge_optional_equal<T: Copy + Eq>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(_), Some(_)) => None,
    }
}

fn add(left: u64, right: u64) -> Result<u64, layerfs_core::CoreError> {
    left.checked_add(right)
        .ok_or(layerfs_core::CoreError::LengthOverflow)
}

/// Structural upper bound: <=3 MiB compact xattr framing, <=3 MiB chunked
/// Apple name/index admission, one <=1 MiB active value/stream window, and
/// bounded metadata-tree summaries. Every individual chunk/buffer is <=1 MiB;
/// managed serialization itself is disk-backed. SQLite caches and caller
/// buffers are reported separately.
pub const OPERATION_Q_BOUND_BYTES: u64 = 8 * 1024 * 1024;

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
            .add_metadata_rope(layerfs_core::content::rope::RopeCounters {
                cdc_bytes_scanned: 16,
                payload_bytes_written: 16,
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

        assert_eq!(counters.rope.cdc_bytes_scanned, 4112);
        assert_eq!(counters.metadata_rope.cdc_bytes_scanned, 16);
        assert_eq!(counters.metadata_rope.payload_bytes_written, 16);
        assert_eq!(counters.rope.nodes_created, 2);
        assert_eq!(counters.namespace.nodes_read, 3);
        assert_eq!(counters.inode_table.nodes_created, 2);
        assert_eq!(counters.native.route, Some(NativeRoute::ClonePatch));
        assert_eq!(counters.native.patch_bytes, 4096);
        assert_eq!(counters.content_payload_bytes_read(), Some(0));
        assert_eq!(counters.content_payload_bytes_written(), Some(0));
    }

    #[test]
    fn content_payload_facts_exclude_metadata_ropes_and_detect_bad_accounting() {
        let counters = OperationCounters {
            rope: layerfs_core::content::rope::RopeCounters {
                payload_bytes_read: 12,
                payload_bytes_written: 20,
                ..Default::default()
            },
            metadata_rope: layerfs_core::content::rope::RopeCounters {
                payload_bytes_read: 4,
                payload_bytes_written: 6,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(counters.content_payload_bytes_read(), Some(8));
        assert_eq!(counters.content_payload_bytes_written(), Some(14));

        let invalid = OperationCounters {
            rope: layerfs_core::content::rope::RopeCounters {
                payload_bytes_read: 3,
                ..Default::default()
            },
            metadata_rope: layerfs_core::content::rope::RopeCounters {
                payload_bytes_read: 4,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(invalid.content_payload_bytes_read(), None);
    }
}
