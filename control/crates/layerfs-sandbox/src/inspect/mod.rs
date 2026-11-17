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
//! (layerfs-project#164), and so is where its data disk goes (`space`: one
//! bounded walk of LayerFS's own folders).
//!
//! What every kind keeps to (#179 B): no lock, lease, daemon call or Store
//! open, nothing written and nothing started; never `registry`, `lease`,
//! `open::hold`, `daemon::pool` or `route`'s host operations. Admission and
//! deadlines are `owner`'s; the caller's wait for a worker is
//! `place::READ`.
mod files;
mod host;
mod live;
mod log;
mod machine;
mod overlay;
mod owner;
pub mod process;
mod root;
mod roots;
mod route;
mod space;
mod stats;

pub use crate::deploy::Side;
pub use files::{
    sandbox_files, sandbox_page, FilesRequest, DEPTH, ENTRIES, MAX_DEPTH, MAX_ENTRIES, MAX_WINDOW,
    WINDOW,
};
pub use live::{sandboxes_live, Live, MACHINE_STOPPED};
pub use log::{machine_log, sandbox_log, Window};
pub use machine::{machine_files, machine_page};
pub use overlay::overlay_sizes;
pub use owner::{admit, on_owner, running, Budget, HARD, PERMITS, SOFT};
pub use route::{ask, in_worker, serve, OP};
pub use space::{machine_space, ENTRIES as SPACE_ENTRIES};
pub use stats::machine_stats;

use route::run_folder;
