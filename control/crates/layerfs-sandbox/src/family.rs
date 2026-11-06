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
//! home reaches that Sandbox's daemon (`ssh`, `lima` and `docker`, #535 G4,
//! `direct.rs`) it keeps its keys and Workspace records too, and its
//! Workspace calls but exec run here. A Sandbox on `docker` is a
//! container whose image comes from the docker machine's record and travels
//! with the call (H §7.6, §7.7); a sqlite Store's is started by the docker
//! machine's worker on every host (G8). Where a call runs is
//! `place::plan`'s answer; the deploy settings are chosen here and resolved
//! where it runs (`deploy`). A PostgreSQL Store's Sandboxes run on `native`
//! or `docker` from this process, which resolves the password (O3), until
//! the worker takes the resolved URL (#535 phase 5a). Each call is in its
//! own file (`create`, `list`, `get`, `close`), the route of a host
//! operation in `route.rs`, and the `workspace` calls in `workspace/calls.rs`.
use super::close::close;
use super::create::create;
use super::get::get;
use super::list::list;
use super::record::SandboxRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::context::Context;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The Sandbox named `name`, or `NoSuchSandbox`.
pub fn sandbox_of(home: &Path, name: &str) -> Result<SandboxRecord> {
    layerfs_home::names::SandboxName::parse(name)?;
    super::record::sandbox(home, name)?.ok_or_else(|| {
        refused(
            "NoSuchSandbox",
            format!("no Sandbox named {name}; `sandbox.create(project, name, machine)` starts one"),
        )
    })
}

/// Runs one `sandbox` call.
pub fn sandbox(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    match call {
        "create" => create(context, args),
        "list" => list(&context.home, args),
        "get" => get(context, args),
        "close" => close(context, args),
        _ => Err(refused("InvalidInput", format!("no call sandbox.{call}"))),
    }
}

/// One page of `items` (`page_size`, default 50, 1 to 1000; `page_pos`)
/// and the next page's position, or null after the last.
pub fn page(items: Vec<SdkValue>, args: &SdkValue) -> (Vec<SdkValue>, SdkValue) {
    let size = args.get_int("page_size").unwrap_or(50).clamp(1, 1000) as usize;
    let pos = args.get_int("page_pos").unwrap_or(0).max(0) as usize;
    let total = items.len();
    let page: Vec<SdkValue> = items.into_iter().skip(pos).take(size).collect();
    let next = match pos + page.len() < total {
        true => SdkValue::Int((pos + page.len()) as i64),
        false => SdkValue::Null,
    };
    (page, next)
}
