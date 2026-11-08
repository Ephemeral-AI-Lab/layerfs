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
//! (`create`, `list`, `get`, `close`), the route of a host operation in
//! `route.rs`, and the `workspace` calls in `workspace/calls.rs`.
pub use super::close::{close, CloseRequest, Closed};
pub use super::create::{create, CreateRequest, Created};
pub use super::get::{get, GetRequest, Got, ListedWorkspace};
pub use super::list::{list, ListRequest, Listed};
use super::record::SandboxRecord;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The Sandbox named `name`, or `NoSuchSandbox`; one recorded under the
/// retired docker backend is refused `Unsupported` before any call is
/// routed (D-8, layerfs-project#111): only `sandbox.close` takes it, and
/// forgets it.
pub fn sandbox_of(home: &Path, name: &str) -> Result<SandboxRecord> {
    let record = named(home, name)?;
    match record.retired_container(home)? {
        Some(container) => Err(super::reopen::retired(name, &container)),
        None => Ok(record),
    }
}

/// The Sandbox named `name`, or `NoSuchSandbox`, whatever its backend.
pub fn named(home: &Path, name: &str) -> Result<SandboxRecord> {
    layerfs_home::names::SandboxName::parse(name)?;
    super::record::sandbox(home, name)?.ok_or_else(|| {
        refused(
            "NoSuchSandbox",
            format!("no Sandbox named {name}; `sandbox.create(project, name, machine)` starts one"),
        )
    })
}

/// Which page of a list a call asks for (`page_size`, default 50, 1 to
/// 1000; `page_pos`, default 0), as sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Page {
    /// `page_size`, if sent.
    pub size: Option<i64>,
    /// `page_pos`, if sent.
    pub pos: Option<i64>,
}

/// One page of `items` and the next page's position, or `None` after the
/// last.
pub fn page<T>(items: Vec<T>, page: Page) -> (Vec<T>, Option<usize>) {
    let size = page.size.unwrap_or(50).clamp(1, 1000) as usize;
    let pos = page.pos.unwrap_or(0).max(0) as usize;
    let total = items.len();
    let page: Vec<T> = items.into_iter().skip(pos).take(size).collect();
    let next = (pos + page.len() < total).then_some(pos + page.len());
    (page, next)
}
