//! Attribute trees: generic keys, portable typed fields and opaque values.

pub mod build;
pub mod codec;
pub mod keys;
pub mod patch;
pub mod portable;
pub mod read;
pub mod value;

pub use build::{build_attribute_tree, AttributeBuildWork, AttributeTreeBuilder};
pub use codec::{decode_attribute_page, encode_attribute_page, AttributeEntry, AttributePage};
pub use keys::AttributeKey;
pub use patch::{apply_patches, AttributePatch, AttributePatchWork};
pub use portable::PortableMetadata;
pub use read::{
    lookup, lookup_many, read_opaque, read_portable, read_value_bounded, AttributeReadWork,
};
pub use value::{emit_value, read_value};
