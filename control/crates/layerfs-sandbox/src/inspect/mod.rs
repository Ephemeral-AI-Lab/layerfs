//! Inspection (layerfs-project#178, #179 A): reads of a machine and its
//! Sandboxes that clients poll, on one route for every machine kind.
//!
//! A call asks `route::ask(home, machine, kind, args)`. `owner::on_owner`
//! places it with no lease, Store, write or intent check. A machine whose
//! calls run in this process (native macOS and Linux) admits the request
//! here and serves it on `Side::Home(home)`. Every other machine (docker,
//! Lima, forwarded too, Apple Container) is sent the worker operation
//! `inspect` `{ kind, .. }` once, and its worker admits and serves it on
//! `Side::Worker(root)`.
//!
//! A request is `{ kind, .. }` and [`serve`] is the one place a kind is
//! matched: a new kind is one arm there, and its call is one [`ask`]. Each
//! datum is read on the side that holds it: a Sandbox's run folder (its
//! `daemon.log`, its overlay files) is found from the Sandbox's daemon
//! record on that side (`route::run_folder`), and so is its daemon process
//! (`process`: whether it runs, since when, its cgroup's usage). A
//! machine's own numbers (`stats`: processors, memory, the data disk,
//! Store sizes, live Sandboxes) are read under that side's root
//! (layerfs-project#164).
//!
//! What every kind keeps to (#179 B): no lock, lease, daemon call or Store
//! open, nothing written and nothing started; never `registry`, `lease`,
//! `open::hold`, `daemon::pool` or `route`'s host operations. Admission and
//! deadlines are `owner`'s; the caller's wait for a worker is
//! `place::READ`.
#[cfg(unix)]
#[cfg(unix)]
mod files;
mod host;
#[cfg(unix)]
#[cfg(unix)]
mod live;
mod log;
#[cfg(unix)]
#[cfg(unix)]
mod machine;
#[cfg(unix)]
mod overlay;
mod owner;
pub mod process;
#[cfg(unix)]
#[cfg(unix)]
mod root;
#[cfg(unix)]
mod roots;
#[cfg(unix)]
mod route;
#[cfg(unix)]
mod stats;

pub use crate::deploy::Side;
#[cfg(unix)]
#[cfg(unix)]
pub use files::{
    sandbox_files, sandbox_page, FilesRequest, DEPTH, ENTRIES, MAX_DEPTH, MAX_ENTRIES, MAX_WINDOW,
    WINDOW,
};

/// Windows: the live read face waits for the monitoring port (#179);
/// the type shape stays for the list response.
#[cfg(windows)]
mod live_win {
    /// One Sandbox run/live observation (#179).
    #[derive(Clone, Debug)]
    pub struct Live {
        /// Whether the daemon process runs.
        pub live: Option<bool>,
        /// Why `live` is `None`.
        pub reason: Option<String>,
    }
    /// The reason a machine that records no worker answers.
    pub const MACHINE_STOPPED: &str = "machine_stopped";
}
#[cfg(unix)]
pub use live::{sandboxes_live, Live, MACHINE_STOPPED};
#[cfg(windows)]
pub use live_win::{Live, MACHINE_STOPPED};
#[cfg(unix)]
pub use log::{machine_log, sandbox_log};
pub use log::Window;
#[cfg(unix)]
pub use machine::{machine_files, machine_page};
#[cfg(unix)]
pub use overlay::overlay_sizes;
pub use owner::{admit, on_owner, running, Budget, HARD, PERMITS, SOFT};
#[cfg(unix)]
pub use route::{ask, in_worker, serve, OP};
#[cfg(unix)]
pub use stats::machine_stats;

#[cfg(unix)]
use route::run_folder;
