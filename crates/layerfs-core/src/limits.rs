pub const MAX_PATH_BYTES: usize = 4 * 1024;
pub const MAX_COMPONENT_BYTES: usize = 255;
pub const MAX_PATH_COMPONENTS: usize = 256;
pub const MAX_OBJECT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_OBJECT_FIELD_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CHILD_REFERENCES: usize = 100_000;
pub const MAX_ENCODED_STRING_BYTES: usize = MAX_PATH_BYTES;
pub const MAX_DECODE_NESTING_DEPTH: usize = 8;
pub const MAX_DURABLE_LIVE_ALLOCATION: u64 = 1_073_741_824;
pub const MAX_MAPPING_DEPTH: usize = 256;
pub const MAX_DELTA_PAGE_BYTES: usize = 8 * 1024 * 1024;

// The compatibility-bearing Phase-4 mapping profile. WP4-P deliberately
// exposes constants, not a runtime format selector.
pub const FILE_LEAF_CAPACITY: usize = 64;
pub const FILE_BRANCH_CAPACITY: usize = 64;
pub const DIRECTORY_PAGE_CEILING: usize = 256 * 1024;
pub const MAPPING_PROFILE_FIELD_BYTES: usize = MAX_OBJECT_FIELD_BYTES;
