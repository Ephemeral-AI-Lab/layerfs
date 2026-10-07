//! Explicitly paid whole-root topology qualification of one immutable root.
//!
//! Decoding a root and demand-reading its root inode says nothing about the
//! rest of the graph. [`qualify_root`] reads every inode row and every binding
//! reachable from the root once, under bounded windows and caller-owned indexed
//! records, and returns a [`QualifiedRoot`] only when the namespace is a tree of
//! directories whose every inode is bound exactly as often as it declares.
//!
//! Entry module: declarations and re-exports only.

mod proof;
mod records;
mod walk;

pub use proof::{QualificationWork, QualifiedRoot, RootContext};
pub use walk::qualify_root;
