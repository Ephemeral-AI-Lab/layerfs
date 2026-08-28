//! Read-only compatibility decoders for retained Phase-4 v1/v2 mappings.
//! No type in this module can construct or publish a filesystem root.

mod decoder;

pub use decoder::{
    decode_mapping, read_mapping, DirectoryPageRef, FileChild, FileReferenceV1, FileReferenceV2,
    FileRoot, LegacyMapping, LegacyTransition, MappingVersion, TransitionOperation,
};
