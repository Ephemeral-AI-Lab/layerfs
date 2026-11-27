//! The SDK's public surface: the call table, one file per call under its
//! family (`families/<family>/<call>.rs`: the call's entry and the function
//! that runs it, one call of a control crate), each family's calls listed in
//! its `mod.rs`, and the call machinery in `dispatch/` (argument checks,
//! folder scope, the one dispatcher, and the forms of an answer and a
//! refusal; layerfs-project#107, call surface 2.0). Only this crate speaks
//! the SDK; the control crates' public paths the front ends use are
//! re-exported here. The `branch`, `commit` and `file`
//! calls reach the Store engine's SDK facade (layerfs-store-service
//! `methods/`) in the caller's process, the only route for Store calls
//! (#352 F-020). The six doctors (`home`, `machine`, `store`, `project`,
//! `sandbox`, `backup doctor`) are table calls of their families that never
//! open the home; they share one answer (`dispatch/answer/doctor.rs`), which
//! is no family.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod dispatch;
mod families;
pub mod worker;

use dispatch as machinery;
// the six doctors' shared answer (`dispatch/answer/doctor.rs`), which each
// family's doctor entry names
use dispatch::answer::doctor;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

pub use dispatch::answer::{keys, refusal, shape};
pub use dispatch::{
    asks_detail, close, guide, home, json, size, Arg, Asked, Call, CallRef, Calls, Client, Context,
    Families, Family, Form, Kind, Paging, Part, Place, Policy, Profile, Redact, Run, Scope,
    Section, DETAIL, WITH,
};
pub use families::families;
pub use layerfs_home::names;
/// The home (`layerfs-home`) and the Store leases, for the front ends and
/// their tests.
pub mod home {
    pub use layerfs_home::disk::*;
    pub use layerfs_home::lock::{entity as entity_lock, flock, home as lock};
    pub use layerfs_home::record::process;
    pub use layerfs_home::secret::saved as secrets;
    pub use layerfs_store::admin::open::lease;
}
/// The worker of machine `name`, started if it does not answer; `None`
/// when its calls run in this process.
#[cfg(feature = "machine")]
#[doc(hidden)]
pub fn running(
    home: &std::path::Path,
    name: &str,
) -> Result<Option<layerfs_machine::route::Endpoint>, layerfs_protocol::sdk::SdkRefusal> {
    layerfs_machine::with_users::running(home, name, &layerfs_sandbox::users::MACHINE)
}
/// The process-wide cancel request a front end sets on a signal (#804 §6).
pub use layerfs_home::cancel;
pub use layerfs_store::admin::with_users::close_idle;
/// The Store formats this build writes, per engine, as `layerfs --version
/// --json` shows them (#804).
pub use layerfs_store::upgrade::FORMATS;

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
/// a direct library call (`client` `rust`); see [`dispatch::call`].
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
pub use layerfs_store::admin::host_worker::write::{Input as CliInput, InputKind as CliInputKind};

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
