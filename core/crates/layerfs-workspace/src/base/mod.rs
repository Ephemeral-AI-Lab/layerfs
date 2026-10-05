//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod cache;
pub(crate) mod client;
pub(crate) mod view;
pub use view::{BaseRead, BaseStat, BaseView};
