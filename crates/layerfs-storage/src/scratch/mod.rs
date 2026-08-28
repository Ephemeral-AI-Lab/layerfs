//! Owned temporary SQLite tables used by bounded verification workflows.

mod lifecycle;
mod metrics;
mod namespace;
mod schema;
mod table;

pub(crate) use lifecycle::recover_owned_near;
pub use metrics::ScratchObservation;
pub use namespace::DiskNamespace;
pub use table::DiskTable;

#[cfg(test)]
mod tests;
