//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod access;
pub(crate) mod captured_runs;
pub(crate) mod captured_types;
pub(crate) mod cells;
pub(crate) mod digest;
pub(crate) mod layers;
pub(crate) mod pieces;
pub(crate) mod runs;
pub(crate) mod stream;
pub(crate) use access::check;
