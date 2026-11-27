//! The Stores this process holds open (the home's lease folder
//! `store/<name>/open/`): the open-Store cache, its idle checkpoint, the
//! reaper and closes, the host worker's entries, the `StoreService`
//! registry, the leases and what a call opens its Store with.
pub mod args;
#[doc(hidden)]
pub mod cache;
#[doc(hidden)]
pub mod checkpoint;
#[doc(hidden)]
pub mod close;
#[doc(hidden)]
pub mod host;
#[doc(hidden)]
pub mod lease;
#[doc(hidden)]
pub mod service;
