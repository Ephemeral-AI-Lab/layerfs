//! The SDK's public surface: the call table, one file per call under its
//! family (`<family>/<call>.rs`: the call's entry and the function that runs
//! it, one call of a control crate), each family's calls listed in its
//! `mod.rs`, and the call machinery in `calls/` (argument checks, folder
//! scope, the one dispatcher, and the forms of an answer and a refusal;
//! layerfs-project#107, call surface 2.0). Only this crate speaks
//! the SDK; the control crates' public paths the front ends use are
//! re-exported here. The `branch`, `commit` and `file`
//! calls reach the Store engine's SDK facade (layerfs-store-service
//! `methods/`) in the caller's process, the only route for Store calls
//! (#352 F-020). The six doctors (`home`, `machine`, `store`, `project`,
//! `sandbox`, `backup doctor`) are table calls of their families that never
//! open the home; the `doctor` family keeps `doctor.fix`.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod backup;
mod branch;
pub mod calls;
mod commit;
mod doctor;
mod file;
// the `home` family's folder; `home` itself names the re-exported home module below
#[path = "home/mod.rs"]
mod home_family;
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
    asks_detail, home, json, keys, refusal, shape, size, Arg, Asked, Call, CallRef, Calls, Client,
    Context, Families, Family, Form, Kind, Paging, Part, Place, Policy, Profile, Redact, Run,
    Saves, Scope, Section, DETAIL, WITH,
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
#[cfg(feature = "machine")]
#[doc(hidden)]
pub use layerfs_control::machine::running;
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
/// Ends this process's idle Sandbox daemon sessions inside a bound
/// (layerfs-project#181): what a one-shot process calls when its call is
/// over, beside [`close_idle`].
pub use layerfs_sandbox::registry::{end_idle_sessions, SESSION_END};
pub use layerfs_sandbox::{daemon, OperationCause, OperationFailure, SdkError, SdkResult};
pub use layerfs_sandbox::{
    BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi,
};

/// The eleven families, in the order help lists them, the same
/// from the `layerfs` binary, Node and Python.
pub fn families() -> Families {
    &[
        Family {
            name: "home",
            list: home_family::calls,
        },
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
        Family {
            name: "doctor",
            list: doctor::calls,
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

/// Runs the call `name` (`family.call`) once with named `args`, logged as
/// a direct library call (`client` `rust`); see [`calls::call`].
pub fn call(
    home: Option<&Path>,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    machinery::call(families(), home, name, args, events)
}

/// [`call`] for a front end (`client`) with a folder scope (O32).
pub fn call_scoped(
    home: Option<&Path>,
    scope: Option<&Scope>,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
    client: Client,
) -> Outcome {
    machinery::call_scoped(families(), home, scope, name, args, events, client)
}

/// [`call`] with JSON text on both sides, for the Node and Python bindings.
pub fn call_json(
    home: Option<&Path>,
    scope: Option<&str>,
    name: &str,
    args: &str,
    events: &mut dyn FnMut(String),
    client: Client,
) -> String {
    machinery::call_json(families(), home, scope, name, args, events, client)
}
