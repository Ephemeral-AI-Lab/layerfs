//! Exact captured namespace normalization and its sealed streamed row source.
mod cursor;
mod inodes;
mod normalize;
mod records;
pub(super) use cursor::Cursor;
pub(super) use inodes::Builder;
pub(super) use normalize::{begin, names, seal, values};
pub(super) use records::{Backing, Facts, Shared, State};
