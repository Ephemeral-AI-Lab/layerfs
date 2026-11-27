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

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::reply::cursor; // #828 compat
#[doc(hidden)]
pub use crate::reply::events; // #828 compat
#[doc(hidden)]
pub mod helpers {
    // #828 compat
    pub use crate::lock::mutex::lock; // #828 compat
    pub use crate::record::token::{ordered_token, random}; // #828 compat
    pub use crate::reply::values::*; // #828 compat
}
#[doc(hidden)]
pub mod home {
    // #828 compat
    #[allow(unused_imports)]
    pub use crate::disk::*; // #828 compat
    #[doc(hidden)]
    pub use crate::lock::entity as entity_lock; // #828 compat
    #[doc(hidden)]
    pub use crate::lock::flock; // #828 compat
    #[doc(hidden)]
    pub use crate::lock::home as lock; // #828 compat
    #[doc(hidden)]
    pub use crate::record::fields; // #828 compat
    #[doc(hidden)]
    pub mod records {
        // #828 compat
        pub use crate::log::lines::{log_line, log_swept, SWEPT, UNCERTAIN}; // #828 compat
        pub use crate::record::file::*; // #828 compat
    }
    #[doc(hidden)]
    pub use crate::record::generation; // #828 compat
    #[doc(hidden)]
    pub use crate::record::observed; // #828 compat
    #[doc(hidden)]
    pub use crate::secret::saved as secrets; // #828 compat
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[doc(hidden)]
    pub use crate::socket as host_socket; // #828 compat
}
#[doc(hidden)]
pub use crate::log::calls as logs; // #828 compat
#[doc(hidden)]
pub use crate::log::tail; // #828 compat
#[doc(hidden)]
pub use crate::log::trace; // #828 compat
#[doc(hidden)]
pub use crate::record::intent as op; // #828 compat
#[doc(hidden)]
pub use crate::record::process; // #828 compat
#[doc(hidden)]
pub use crate::reply::outcome; // #828 compat
#[doc(hidden)]
pub use crate::reply::state; // #828 compat
#[doc(hidden)]
pub use crate::reply::time; // #828 compat
#[doc(hidden)]
pub use crate::secret::keys as secrets; // #828 compat
#[doc(hidden)]
pub use crate::secret::wipe; // #828 compat
