//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod layers;
pub(crate) mod pieces;
pub(crate) mod stream;
pub(crate) mod cells;
pub(crate) mod work;
pub(crate) use cells::access::check;
