//! The generation cycle: live rows sealed for Commit, then installed.
pub(crate) mod capture;
pub(crate) mod captured;
pub(crate) mod failed;
pub(crate) mod install;
pub(crate) mod tickets;
