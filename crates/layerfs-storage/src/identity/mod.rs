//! Frozen logical and physical LayerFS V1 identities.
//!
//! Every identity domain has a distinct Rust type with a private raw
//! constructor.  The only runtime digest path in this module is unkeyed
//! BLAKE3-256, and all multi-field preimages are fed incrementally.
//!
//! Raw digest bytes cannot construct an identity:
//!
//! ```compile_fail
//! use layerfs_storage::identity::{
//!     ExplicitDirectoryNodeV1,
//!     ImplicitRootDirectoryV1,
//!     LogicalChunkIdV1,
//! };
//! let _ = LogicalChunkIdV1::from_digest([0; 32]);
//! let _ = ExplicitDirectoryNodeV1::from_digest([0; 32]);
//! let _ = ImplicitRootDirectoryV1::from_digest([0; 32]);
//! ```
//!
//! Logical and physical domains cannot be crossed:
//!
//! ```compile_fail
//! use layerfs_storage::identity::{LogicalChunkIdV1, PhysicalChunkIdV1};
//! fn needs_logical(_: LogicalChunkIdV1) {}
//! fn wrong(id: PhysicalChunkIdV1) { needs_logical(id); }
//! ```

mod framing;
mod logical;
mod physical;

pub(crate) use framing::{derive_framed_bytes, FramedHasherV1};
pub use logical::*;
pub use physical::*;

pub const DIGEST_BYTES: usize = 32;
pub const COMPARISON_WINDOW_BYTES: usize = 65_536;
/// Exact resident size of one pinned BLAKE3 1.8.5 streaming state on this
/// target. Operation memory plans charge one unit for each concurrently live
/// identity/checksum state.
pub const IDENTITY_HASHER_BYTES_V1: u64 = core::mem::size_of::<blake3::Hasher>() as u64;

const SCHEMA_V1_LE: [u8; 2] = 1_u16.to_le_bytes();
const ELSHASH1: [u8; 8] = *b"ELSHASH1";

#[allow(dead_code)]
pub(crate) const TAG_CHUNKER_SPEC: u8 = 0x01;
#[allow(dead_code)]
pub(crate) const TAG_DIGEST_SPEC: u8 = 0x02;
#[allow(dead_code)]
pub(crate) const TAG_PROFILE_SPEC: u8 = 0x03;
pub(crate) const TAG_PHYSICAL_CHUNK: u8 = 0x10;
pub(crate) const TAG_PHYSICAL_VERSION_RECORD: u8 = 0x11;
pub(crate) const TAG_PHYSICAL_TREE: u8 = 0x12;
pub(crate) const TAG_PHYSICAL_FILE: u8 = 0x13;
pub(crate) const TAG_PHYSICAL_SYMLINK: u8 = 0x14;
#[allow(dead_code)]
pub(crate) const TAG_PACK: u8 = 0x20;
#[allow(dead_code)]
pub(crate) const TAG_OBJECT_CHECKSUM: u8 = 0x21;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; DIGEST_BYTES]);

        impl $name {
            pub const fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
                &self.0
            }

            #[allow(dead_code)]
            pub(crate) const fn from_digest(bytes: [u8; DIGEST_BYTES]) -> Self {
                Self(bytes)
            }
        }
    };
}

typed_id!(LogicalChunkIdV1);
typed_id!(LogicalFileIdV1);
typed_id!(FileNodeIdV1);
typed_id!(SymlinkNodeIdV1);
typed_id!(DirectoryNodeIdV1);
typed_id!(VersionIdV1);

typed_id!(PhysicalChunkIdV1);
typed_id!(PhysicalFileIdV1);
typed_id!(PhysicalTreeIdV1);
typed_id!(PhysicalSymlinkIdV1);
typed_id!(PhysicalVersionRecordIdV1);
typed_id!(PackIdV1);
typed_id!(ObjectChecksumV1);

typed_id!(DigestSpecId);
typed_id!(ChunkerSpecId);
typed_id!(ProfileId);
