//! The SDK's public surface: the call table, one file per call under its
//! family (`<family>/<call>.rs`: the call's entry and the function that runs
//! it, one call of a control crate), each family's calls listed in its
//! `mod.rs`, and the call machinery in `calls/` (argument checks, folder
//! scope, the one dispatcher; layerfs-project#107). Only this crate speaks
//! the SDK; the control crates' public paths the front ends use are
//! re-exported here. The `branch`, `commit` and `file`
//! calls reach the Store engine's SDK facade (layerfs-store-service
//! `methods/`) in the caller's process, the only route for Store calls
//! (#352 F-020).
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod backup;
mod branch;
pub mod calls;
mod commit;
mod file;
mod machine;
mod project;
mod sandbox;
mod store;
mod workspace;

use calls as machinery;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use std::path::Path;

pub use calls::{
    home, json, keys, size, Arg, Call, CallRef, Calls, Context, Families, Family, Kind, Run, Scope,
};
pub use layerfs_home::names;
/// The home (`layerfs-home`), with opening and converting it
/// (`layerfs-control` `migrate`) and the Store leases, as the front ends
/// used them before the control split (layerfs-project#103).
pub mod home {
    pub use layerfs_control::migrate::{import_env, note_failure, open, open_from};
    pub use layerfs_home::home::*;
    pub use layerfs_home::process;
    pub use layerfs_store::lease;
}
/// `layerfs doctor` (#536 H7): report-only, outside the call table.
pub use layerfs_control::doctor::{doctor, doctor_json};
#[cfg(feature = "machine")]
#[doc(hidden)]
pub use layerfs_machine::running;
/// The worker protocol (`layerfs-machine`) and its server (`layerfs-control`),
/// as the front ends used them before the control split (layerfs-project#104).
pub mod worker {
    pub use layerfs_control::worker_server::{serve, Served, REQUEST_WAIT};
    pub use layerfs_machine::worker::*;
}
pub use layerfs_control::store::close_idle;
pub use layerfs_control::{
    projects, AddFailure, AddRequest, AddedProject, CreateRequest, InitCounters, InitCounts,
    InitError, InitFailure, InitRequest, InitStep, InstallError, InstallFailure, InstallWork,
    Installed, ProjectApi, ProjectError, SealedProject, SourceRefusal,
};
pub use layerfs_sandbox::{daemon, OperationCause, OperationFailure, SdkError, SdkResult};
pub use layerfs_sandbox::{
    BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi,
};

/// The nine families, in the order help lists them: 64 calls, the same
/// from the `layerfs` binary, Node and Python.
pub fn families() -> Families {
    &[
        Family {
            name: "project",
            list: project::calls,
        },
        Family {
            name: "branch",
            list: branch::calls,
        },
        Family {
            name: "commit",
            list: commit::calls,
        },
        Family {
            name: "file",
            list: file::calls,
        },
        Family {
            name: "sandbox",
            list: sandbox::calls,
        },
        Family {
            name: "workspace",
            list: workspace::calls,
        },
        Family {
            name: "machine",
            list: machine::calls,
        },
        Family {
            name: "store",
            list: store::calls,
        },
        Family {
            name: "backup",
            list: backup::calls,
        },
    ]
}

/// Every call, in table order.
pub fn calls() -> Vec<CallRef> {
    machinery::table::calls(families()).collect()
}

/// The call named `family.call`.
pub fn find(name: &str) -> Option<CallRef> {
    machinery::find(families(), name)
}

/// The whole table as JSON text, for front ends that build their functions
/// from it.
pub fn table_json() -> String {
    machinery::table_json(families())
}

/// Runs the call `name` (`family.call`) once with named `args`; see
/// [`calls::call`].
pub fn call(
    home: Option<&Path>,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    machinery::call(families(), home, name, args, events)
}

/// [`call`] for a client with a folder scope (O32).
pub fn call_scoped(
    home: Option<&Path>,
    scope: Option<&Scope>,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    machinery::call_scoped(families(), home, scope, name, args, events)
}

/// [`call`] with JSON text on both sides, for the Node and Python bindings.
pub fn call_json(
    home: Option<&Path>,
    scope: Option<&str>,
    name: &str,
    args: &str,
    events: &mut dyn FnMut(String),
) -> String {
    machinery::call_json(families(), home, scope, name, args, events)
}
