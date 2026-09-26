//! Relocated: the ordered page builder now lives in
//! `backing::binary_plus_tree::keyed::build`.
//!
//! The builder is an inherent method of the root owner, so this path has no
//! item to re-export; it stays as the module's old name until nothing declares
//! it. The relocation changed no page format, algorithm or public path.
