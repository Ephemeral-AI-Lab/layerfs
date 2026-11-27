//! The refusals before any effect: a call's arguments (`arguments`), its
//! folder scope (`scope`), the permanent platform restrictions
//! (`platform_policy`) and the calls only this computer runs (`host_only`).
pub(crate) mod arguments;
pub(crate) mod host_only;
pub(crate) mod platform_policy;
pub mod scope;
