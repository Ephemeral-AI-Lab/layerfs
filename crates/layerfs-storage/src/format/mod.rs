//! Canonical M6.1 format primitives reused by LayerFS L0.

pub mod codec;
pub mod path;

pub use codec::{
    checked_encoded_len, checked_u32, checked_usize, require_at_most, require_nonzero_at_most,
    usize_from_u32, validate_chunk_object_count, validate_chunk_reference_len,
    validate_chunk_refs_per_file, validate_chunk_refs_per_version, validate_count_at_most,
    validate_directory_mode, validate_directory_page_depth, validate_domain, validate_entry_count,
    validate_extents_per_file, validate_extents_per_version, validate_file_mode,
    validate_flags_zero, validate_index_page_depth, validate_leaf_page_depth,
    validate_logical_chunk_payload_len, validate_logical_length, validate_nonzero_count_at_most,
    validate_physical_chunk_payload_len, validate_physical_object_len, validate_reserved_zero,
    validate_schema_v1, validate_total_object_count, validate_tree_index_fanout,
    validate_tree_leaf_fanout, validate_tree_object_count, ByteSink, DirectoryModeContext,
    ExtentTagV1, LogicalChildKindV1, PhysicalObjectKindV1, PhysicalTreeChildKindV1, PresenceV1,
    SliceCursor, SliceSink, TreeSubtypeV1, MAX_CHUNK_BYTES, MAX_CHUNK_OBJECTS,
    MAX_CHUNK_REFS_PER_FILE, MAX_CHUNK_REFS_PER_VERSION, MAX_ENTRIES, MAX_EXTENTS_PER_FILE,
    MAX_EXTENTS_PER_VERSION, MAX_LOGICAL_BYTES, MAX_PHYSICAL_OBJECT_BYTES, MAX_TOTAL_OBJECTS,
    MAX_TREE_INDEX_FANOUT, MAX_TREE_LEAF_FANOUT, MAX_TREE_OBJECTS, MAX_TREE_PAGE_DEPTH,
    PORTABLE_MODE_MAX, ROOT_DIRECTORY_MODE_SENTINEL_V1,
};

pub use path::{
    compare_paths_unsigned, compare_unsigned, require_strictly_increasing,
    require_strictly_increasing_paths, validate_component, validate_path, validate_symlink_target,
    PathValidator, ValidatedComponent, ValidatedPath, ValidatedSymlinkTarget, MAX_COMPONENT_BYTES,
    MAX_PATH_BYTES, MAX_PATH_DEPTH, MAX_SYMLINK_TARGET_BYTES,
};
