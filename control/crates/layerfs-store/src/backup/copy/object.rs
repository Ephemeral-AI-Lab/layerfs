//! What a backup records beside a Store's file on the bucket (its SQLite
//! version and binding), and what the bucket must hold before an upload
//! goes ahead. `backup.create` (layerfs-store) uploads.

/// Metadata key of the SQLite version that wrote the pushed Store.
pub const SQLITE_VERSION: &str = "layerfs-sqlite";
/// Metadata key of the Store's binding key (hex), which a load opens with.
/// The binding is not a secret; the cursor key never leaves its holder.
pub const BINDING: &str = "layerfs-binding";

/// What the bucket must hold for an upload to go ahead: no copy, or the copy
/// with this sha256. Not atomic (no compare-and-swap, which not every
/// provider has), so an upload by another machine in between is not
/// detected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expected {
    /// The remote holds no copy of this Store.
    Absent,
    /// The remote copy has this sha256.
    Sha256([u8; 32]),
}
