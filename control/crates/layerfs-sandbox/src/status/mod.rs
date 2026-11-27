//! What exists in the home and how it relates: the inventory reads by name
//! and the relationships no record stores (`inventory`).
#[doc(hidden)]
pub mod capacity;
pub(crate) mod get;
#[doc(hidden)]
pub mod inventory;
#[doc(hidden)]
pub mod list;
pub mod overlay;
#[doc(hidden)]
pub mod run_folder;
/// A Sandbox's state and a Workspace's, from records and daemon words.
pub mod state;
pub mod stats;
