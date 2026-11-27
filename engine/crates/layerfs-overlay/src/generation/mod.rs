//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod failed;
pub(crate) mod capture;
pub(crate) mod tickets;
pub(crate) mod captured;


pub(crate) use super::kernel::directory::INLINE_PAGES;
