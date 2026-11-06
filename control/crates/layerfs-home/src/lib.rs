//! The LayerFS home and its records (#536 §7.8; cut from `layerfs-control`
//! by layerfs-project#103): the home's layout and record files, its locks,
//! the one in-flight intent (`op`), typed names, call outcomes and event
//! streams (`events`), secrets, the call trace and the process identity records name. It knows no
//! entity: machines, Stores, Sandboxes and Workspaces build on it.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod events;
pub mod helpers;
pub mod home;
pub mod json;
pub mod names;
pub mod op;
#[doc(hidden)]
pub mod outcome;
pub mod process;
pub mod secrets;
#[doc(hidden)]
pub mod trace;
