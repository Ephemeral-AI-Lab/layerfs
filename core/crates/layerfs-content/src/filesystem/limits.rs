//! Named limits of the filesystem tree profile.
//!
//! Every bound here is enforced by a specific check in this module tree. The
//! figures are the pinned reference profile's own: 255 bytes per name, 4096
//! bytes per path, 256 path components, 8-KiB tree pages, depth 31, 50-100
//! recorded rows in a non-root inode leaf, 64-127 children in a non-root inode
//! branch, and the 2/5 fill rule for non-root directory and attribute pages.

/// Largest canonical bytes of one tree page, including the object envelope.
pub const MAXIMUM_PAGE_BYTES: usize = 8_192;
/// Largest bytes of one name component.
pub const MAXIMUM_NAME_BYTES: usize = 255;
/// Largest bytes of one canonical path.
pub const MAXIMUM_PATH_BYTES: usize = 4_096;
/// Largest number of components in one canonical path.
pub const MAXIMUM_PATH_COMPONENTS: usize = 256;
/// Largest supported tree height.
pub const MAXIMUM_TREE_LEVEL: u8 = 31;
/// Smallest recorded rows of a non-root inode leaf.
pub const MINIMUM_INODE_LEAF_ROWS: u64 = 50;
/// Largest recorded rows of one inode leaf.
pub const MAXIMUM_INODE_LEAF_ROWS: u64 = 100;
/// Smallest children of a non-root inode branch.
pub const MINIMUM_INODE_BRANCH_CHILDREN: u64 = 64;
/// Largest children of one inode branch.
pub const MAXIMUM_INODE_BRANCH_CHILDREN: u64 = 127;
/// Smallest canonical bytes of a non-root directory or attribute page.
pub const MINIMUM_FILLED_PAGE_BYTES: usize = 3_277;
/// Default bytes one filesystem operation may hold for its own unfinished pages.
pub const DEFAULT_OPERATION_SCRATCH_BYTES: usize = 4 * 1024 * 1024 - 1;
/// Largest bytes of one symbolic-link target, matching the reference grammar.
pub const MAXIMUM_SYMLINK_TARGET_BYTES: usize = 4_096;
/// Largest bytes of one generic attribute domain.
pub const MAXIMUM_ATTRIBUTE_DOMAIN_BYTES: usize = 64;
/// Largest bytes of one generic attribute key.
pub const MAXIMUM_ATTRIBUTE_KEY_BYTES: usize = 255;
/// The only domain whose keys carry typed meaning in this profile.
pub const PORTABLE_ATTRIBUTE_DOMAIN: &str = "portable";
/// Largest bytes of one attribute value.
pub const MAXIMUM_ATTRIBUTE_VALUE_BYTES: usize = 1024 * 1024;
/// Largest keys one attribute-key listing may return.
///
/// A declared operation bound, not a format bound: the attribute grammar bounds a
/// page, not a tree, so without this a single `attribute_keys` call would own an
/// unbounded name set built from the whole tree. The figure matches the read-wave
/// ceiling, so one listing cannot outgrow one read wave.
pub const MAXIMUM_ATTRIBUTE_KEYS: usize = 4_096;
/// Rows one directory leaf may hold at the page ceiling.
///
/// A page is a fixed header plus one row per entry, and a row is never shorter
/// than a 2-byte name length, a one-byte name and an 8-byte serial. This quotient
/// is therefore the largest row count any page can have, and the check that uses
/// it is a count ceiling, not a size proof: whether a particular set of rows fits
/// depends on how long its names are, and the encoder decides that from the exact
/// encoded size before it assembles the page. The reference's split threshold
/// added one to this quotient, which is one row more than a page can ever hold.
pub const MAXIMUM_DIRECTORY_LEAF_ROWS: usize =
    (MAXIMUM_PAGE_BYTES - EMPTY_PAGE_BYTES) / (2 + 1 + 8);

/// Bytes a tree page carries before its first row.
pub const EMPTY_PAGE_BYTES: usize = 44;

/// True when a page of `bytes` canonical length satisfies the 2/5 fill rule.
pub const fn filled_page(bytes: usize) -> bool {
    bytes * 5 >= MAXIMUM_PAGE_BYTES * 2
}
