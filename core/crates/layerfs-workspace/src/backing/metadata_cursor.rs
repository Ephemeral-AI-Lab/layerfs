//! Relocated: the bounded ordered reader for one file's extents now lives in
//! `backing::binary_plus_tree::extent::cursor`.
//!
//! This path stays a reexport until nothing imports it. It is a relocation
//! only - no page format, algorithm or public path changed.
pub use super::binary_plus_tree::extent::cursor::*;
