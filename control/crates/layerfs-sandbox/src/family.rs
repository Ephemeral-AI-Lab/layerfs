//! The `sandbox` and `workspace` families (#533 step 12): Sandboxes named
//! across the home (unique under `layerfs.lock`), recorded in
//! `sandbox/<name>/record.json` (layout 3, #536 §5), each running one
//! project's daemon on the machine that holds its Store. A Workspace is
//! `<sandbox>:<path>`. A Sandbox on `native` runs in this process's host
//! Sandbox code: host processes over loopback NFS on macOS, namespaces on
//! Linux; what reopens it is in the same folder. On any other machine the
//! same call runs in that machine's worker, which keeps the Sandbox's thin
//! record, keys and Workspaces in `sandbox/<name>/` under its own root
//! (§6); this home keeps the name, the project and the machine. Where the
//! home reaches that Sandbox's daemon (`lima`, #535 G4,
//! `direct.rs`) it keeps its keys and Workspace records too, and its
//! Workspace calls but exec run here. A Sandbox on `docker` runs in
//! namespaces in the docker machine's worker container, reached through
//! that worker (D-8); its image travels with the call (H §7.6, §7.7). Where
//! a call runs is `place::plan`'s answer; the deploy settings are chosen
//! here and resolved where it runs (`deploy`). Each call is in its own file
//! (`create`, `list`, `get`, `remove`), the state of a Sandbox in
//! `state.rs`, the route of a host operation in `route.rs`, and the
//! `workspace` calls in `workspace/calls.rs`.
pub use super::lifecycle::create::{create, create_on, CreateRequest, Created};
pub use super::lifecycle::remove::{remove, RemoveRequest, Removed};
use super::record::SandboxRecord;
pub use super::status::get::{get, Down, GetRequest, Got, ListedWorkspace};
pub use super::status::list::{list, ListRequest, Listed, ListedSandbox};
use layerfs_home::reply::outcome::refused;
use layerfs_protocol::sdk::SdkRefusal;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The Sandbox named `name`, or `NoSuchSandbox`; one recorded under the
/// retired docker backend is refused `Unsupported` before any call is
/// routed (D-8, layerfs-project#111): only `sandbox.remove` takes it, and
/// forgets it.
pub fn sandbox_of(home: &Path, name: &str) -> Result<SandboxRecord> {
    let record = named(home, name)?;
    match record.retired_container() {
        Some(container) => Err(super::record::reopen::retired(name, container.as_deref())),
        None => Ok(record),
    }
}

/// The Sandbox named `name`, or `NoSuchSandbox`, whatever its backend.
pub fn named(home: &Path, name: &str) -> Result<SandboxRecord> {
    layerfs_home::names::SandboxName::parse(name)?;
    super::record::sandbox(home, name)?.ok_or_else(|| {
        refused(
            "NoSuchSandbox",
            format!(
                "no Sandbox named {name}; `layerfs sandbox create <project> {name}` starts one"
            ),
        )
    })
}
