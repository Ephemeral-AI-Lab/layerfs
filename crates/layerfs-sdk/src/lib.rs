//! Thin public facade over WorkingStore, OperationWorkspace, presentations, and explicit Sync.

#![forbid(unsafe_code)]

mod product;

pub use product::*;

pub const COMPONENT: &str = "layerfs-sdk";
