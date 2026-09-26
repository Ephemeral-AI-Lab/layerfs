//! Relocated: the length-indexed extent sequence now lives in
//! `backing::binary_plus_tree::extent::splice`, and its bounded cursor in
//! `backing::binary_plus_tree::extent::cursor`.
//!
//! This path stays a reexport until nothing imports it. It is a relocation
//! only - no page format, algorithm or public path changed.
pub use super::binary_plus_tree::extent::splice::*;
