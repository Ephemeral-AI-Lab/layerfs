//! Atomic file changes: one private Workspace per call, changed, committed
//! once and closed.
pub mod edit;
pub mod files;
pub mod mutation;
pub mod recursive;
