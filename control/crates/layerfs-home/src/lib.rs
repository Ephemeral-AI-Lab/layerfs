//! The LayerFS home and its records (#536 §7.8; cut from `layerfs-control`
//! by layerfs-project#103): the home's layout and record files, its locks,
//! the one in-flight intent (`op`), typed names, call outcomes and event
//! streams (`events`), secrets, the call trace, the call log (`logs`, #659), the one state type (`state`), the cursor reader of a log and its rotated file (`tail`) and the process identity records name. It knows no
//! entity: machines, Stores, Sandboxes and Workspaces build on it.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cancel;
pub mod disk;
pub mod json;
pub mod lock;
pub mod log;
pub mod names;
pub mod record;
pub mod reply;
pub mod secret;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod socket;
