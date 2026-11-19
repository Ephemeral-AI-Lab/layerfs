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
//! open the home; they share one answer (`calls/doctor/`), which is no
//! family.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod backup;
mod branch;
pub mod calls;
mod commit;
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
// the six doctors' shared answer (`calls/doctor/`), which each
// family's doctor entry names
use calls::doctor;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use std::path::Path;

pub use calls::{
    asks_detail, close, guide, home, json, keys, refusal, shape, size, Arg, Asked, Call, CallRef,
    Calls, Client, Context, Families, Family, Form, Kind, Paging, Part, Place, Policy, Profile,
    Redact, Run, Scope, Section, DETAIL, WITH,
};
pub use layerfs_home::names;
/// The home (`layerfs-home`) and the Store leases, for the front ends and
/// their tests.
pub mod home {
    pub use layerfs_home::home::*;
    pub use layerfs_home::process;
    pub use layerfs_store::lease;
}
#[cfg(feature = "machine")]
#[doc(hidden)]
pub use layerfs_control::machine::running;
/// The worker protocol (`layerfs-machine`) and its server (`layerfs-control`),
/// for the front ends and their tests.
pub mod worker {
    pub use layerfs_control::worker_server::serve;
    pub use layerfs_machine::worker::*;
}
pub use layerfs_control::store::close_idle;

/// The ten families, in the order help lists them, the same from the
/// `layerfs` binary, Node and Python.
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
