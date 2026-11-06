//! The host Workspace operations, here or in a machine's worker
//! (`sandbox.host`): mount, Commit, unmount, exec and the status read; every
//! other operation is a Sandbox operation (`sandbox::dispatch`). Moved from
//! `sandbox/dispatch.rs` so Sandbox code never imports Workspace code
//! (layerfs-project#106).
use crate::exec;
use crate::workspace::serve as workspace;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{refused, Outcome};

/// Runs one host operation once: a Workspace operation here, any other in
/// `sandbox::dispatch`, which checks a Commit `message` first.
pub fn call(op: &str, args: SdkValue) -> Outcome {
    match op {
        "workspace.mount" | "workspace.commit" | "workspace.unmount" | "workspace.exec" => {
            crate::dispatch::checked(&args)?;
        }
        "backend.daemon.workspace.status" => return backend(op.to_owned(), args),
        _ => return crate::dispatch::call(op, args),
    }
    match op {
        "workspace.mount" => workspace::mount(&args),
        "workspace.commit" => workspace::commit(&workspace::by_name(args)?),
        "workspace.unmount" => workspace::unmount(&workspace::by_name(args)?),
        _ => exec::exec(&workspace::by_name(args)?),
    }
}

/// Runs the Workspace status read once.
pub fn backend(op: String, args: SdkValue) -> Outcome {
    match op.as_str() {
        "backend.daemon.workspace.status" => workspace::status(&workspace::by_name(args)?),
        _ => Err(refused(
            "UnknownMethod",
            format!("no backend operation {op}"),
        )),
    }
}
