//! What spans the control entities (#536; split by layerfs-project#101):
//! the worker's server, `inventory` (the relationships no record stores),
//! `doctor`, and the machine, Store,
//! project and backup calls with the reads they need from other entities
//! (`Related`). The call machinery is `layerfs-api`'s (layerfs-project#107).
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod doctor;
pub(crate) mod files;
pub mod home;
pub(crate) mod machine;
pub(crate) mod sandbox;
pub(crate) mod store;
pub use layerfs_project::store::{
    projects, AddFailure, AddRequest, AddedProject, CreateRequest, InitError, InitFailure,
    InitRequest, InstallError, InstallFailure, InstallWork, Installed, ProjectApi, SealedProject,
};
pub use layerfs_project::{InitCounters, InitCounts, InitStep, ProjectError, SourceRefusal};
pub use layerfs_store_service::methods::call;

// The six doctors (call surface 2.0): `home`, `machine`, `store`,
// `project`, `sandbox` and `backup doctor` answer one report ([`report`])
// made by one runner ([`frame`]) over each kind's checks ([`kinds`]). A
// doctor changes nothing and starts no machine. It waits for no lock: the
// only locks it takes are the host worker's startup lock and `worker.lock`,
// each tried once without waiting by the host worker probe and released at
// once, and only for a socket that has no listener or lacks its private
// mode (`machine get --with live` shares that probe). A home that changes meanwhile may be
// reported as it was part way.
//
// No doctor's report holds what interrupted operations left: LayerFS
// removes it itself where nothing can own it (spec v20,
// `layerfs_sandbox::sweep` and the sweeps of image copies and backups).

pub use doctor::kinds::kinds;
pub use doctor::report::{Kind, Node, Report, Thing};
pub use doctor::runner::{run, run_with, CheckId, KindDoctor, Located, Options, Probe, Registry};

pub(crate) use home::scan::shell;
pub use machine::network::{inspect_in_worker as inspect_network_in_worker, OP as NETWORK_OP};
pub use sandbox::doctor::worker_diagnose as diagnose_worker_sandbox;
#[path = "home/bucket.rs"]
mod backup_target;
pub use store::doctor::in_worker as diagnose_store_in_worker;

// Inspection (layerfs-project#178, #179 A): reads of a machine and its
// Sandboxes that clients poll, on one route for every machine kind.
//
// A call asks `route::ask(home, machine, kind, args)`. `owner::on_owner`
// places it with no lease, Store, write or intent check. A machine whose
// calls run in this process (native macOS and Linux) admits the request
// here and serves it on `Side::Home(home)`. Every other machine (docker,
// Lima, forwarded too, Apple Container) is sent the worker operation
// `inspect` `{ kind, .. }` once, and its worker admits and serves it on
// `Side::Worker(root)`.
//
// A request is `{ kind, .. }` and [`serve`] is the one place a kind is
// matched: a new kind is one arm there, and its call is one [`ask`]. Each
// datum is read on the side that holds it: a Sandbox's run folder (its
// `daemon.log`, its overlay files) is found from the Sandbox's daemon
// record on that side (`route::run_folder`), and so is its daemon process
// (`process`: whether it runs, since when, its cgroup's usage). A
// machine's own numbers (`stats`: processors, memory, the data disk,
// Store sizes, live Sandboxes) are read under that side's root
// (layerfs-project#164), and so is where its data disk goes (`space`: one
// bounded walk of LayerFS's own folders).
//
// What every kind keeps to (#179 B): no lock, lease, daemon call or Store
// open, nothing written and nothing started; never `registry`, `lease`,
// `open::hold`, `daemon::pool` or `route`'s host operations. Admission and
// deadlines are `owner`'s; the caller's wait for a worker is
// `place::READ`.

pub use files::log::{machine_log, sandbox_log, Window};
pub use files::walk::{
    sandbox_files, sandbox_page, FilesRequest, DEPTH, ENTRIES, MAX_DEPTH, MAX_ENTRIES, MAX_WINDOW,
    WINDOW,
};
pub use layerfs_sandbox::deploy::Side;
pub use layerfs_sandbox::inspect::overlay::overlay_sizes;
pub use layerfs_sandbox::inspect::owner::{admit, on_owner, running, Budget, HARD, PERMITS, SOFT};
pub use layerfs_sandbox::inspect::route::OP;
pub use machine::files::{machine_files, machine_page};
pub use machine::space::{machine_space, ENTRIES as SPACE_ENTRIES};
pub use machine::stats::machine_stats;

mod route;
pub use route::{ask, in_worker, serve};
