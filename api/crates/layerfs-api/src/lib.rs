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

pub mod dispatch;
// the `home` family's folder; `home` itself names the re-exported home module below
pub(crate) mod families;
#[path = "families/home/mod.rs"]
mod home_family;
pub mod worker;

use dispatch as machinery;
// the six doctors' shared answer (`calls/doctor/`), which each
// family's doctor entry names
use dispatch::answer;
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

pub use layerfs_home::names;
pub use {
    dispatch::{
        answer::{keys, refusal, shape},
        asks_detail, close, guide, home, size, Arg, Asked, Call, CallRef, Calls, Client, Context,
        Families, Family, Form, Kind, Paging, Part, Place, Policy, Profile, Redact, Run, Scope,
        Section, DETAIL, WITH,
    },
    layerfs_home::json,
};
/// The home (`layerfs-home`) and the Store leases, for the front ends and
/// their tests.
pub mod home {
    pub use layerfs_home::home::*;
    pub use layerfs_home::process;
    pub use layerfs_store::lease;
}
/// The worker of machine `name`, started if it does not answer; `None`
/// when its calls run in this process.
#[cfg(feature = "machine")]
#[doc(hidden)]
pub fn running(
    home: &std::path::Path,
    name: &str,
) -> Result<Option<layerfs_machine::worker::Endpoint>, layerfs_protocol::sdk::SdkRefusal> {
    layerfs_machine::with_users::running(home, name, &layerfs_sandbox::users::MACHINE)
}
/// The process-wide cancel request a front end sets on a signal (#804 §6).
pub use layerfs_home::cancel;
pub use layerfs_store::admin::with_users::close_idle;
/// The Store formats this build writes, per engine, as `layerfs --version
/// --json` shows them (#804).
pub use layerfs_store::upgrade::FORMATS;

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
            list: families::project::calls,
        },
        Family {
            name: "branch",
            list: families::branch::calls,
        },
        Family {
            name: "commit",
            list: families::commit::calls,
        },
        Family {
            name: "file",
            list: families::file::calls,
        },
        Family {
            name: "sandbox",
            list: families::sandbox::calls,
        },
        Family {
            name: "workspace",
            list: families::workspace::calls,
        },
        Family {
            name: "machine",
            list: families::machine::calls,
        },
        Family {
            name: "store",
            list: families::store::calls,
        },
        Family {
            name: "backup",
            list: families::backup::calls,
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

/// Runs the CLI executable's checked call on its selected Store route.
#[doc(hidden)]
pub fn call_cli(
    scope: Option<&Scope>,
    name: &str,
    args: SdkValue,
    input: Option<CliInput>,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    machinery::call_cli(families(), scope, name, args, input, events)
}

/// A write input the CLI has not read yet.
#[doc(hidden)]
pub use layerfs_store::host_write::{Input as CliInput, InputKind as CliInputKind};

/// Whether the CLI defers the input of write `name` to `project`.
#[doc(hidden)]
pub fn cli_defers(name: &str, project: &str) -> bool {
    machinery::cli_defers(name, project)
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

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::dispatch as calls; // #828 compat
