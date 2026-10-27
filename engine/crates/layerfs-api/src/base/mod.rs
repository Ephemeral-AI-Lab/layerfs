//! Shared authenticated control, original failure custody, and the public
//! Store method table with its argument readers.
pub(crate) mod args;
pub(crate) mod control;
pub(crate) mod dispatch;
pub(crate) mod failure;
#[doc(hidden)]
pub mod internal;
mod recovery;
pub(crate) mod sdk;
