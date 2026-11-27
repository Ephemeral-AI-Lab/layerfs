//! Directory pages: bindings, sorted updates and bounded reads.

pub mod codec;
pub mod read;
pub(crate) mod read_ahead;
pub mod update;

pub use codec::{decode_directory_page, encode_directory_page, DirectoryPage};
pub use read::{
    list_after, lookup, lookup_many, root_shape, root_shapes, DirectoryReadWork, ListingPage,
};
pub use update::{apply_bindings, build_directory, empty_directory};
