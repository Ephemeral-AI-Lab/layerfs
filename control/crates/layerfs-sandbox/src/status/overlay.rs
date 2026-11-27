//! The inspect kind `overlay_sizes` (layerfs-project#178 `overlay_bytes`):
//! the size of each Workspace's overlay file, read on the side that holds
//! the Sandbox's run folder for a caller that holds the Workspace records
//! (the home, for a Sandbox whose daemon it reaches directly), with what
//! that side sees of the Sandbox's daemon process: whether it runs, since
//! when, and its cgroup's usage (layerfs-project#164). This file
//! is the kind's routing: its name, its request `{ sandbox, namespaces }`
//! and the home-side call. The read itself is one `lstat` of
//! `<run folder>/overlay/<namespace>.sqlite` per Workspace: the file is
//! never opened and no daemon is asked.
use super::super::deploy::Side;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_protocol::sdk::SdkValue;

/// The inspect kind.
pub const KIND: &str = "overlay_sizes";

/// The most Workspaces one request names.
const MOST: usize = 1024;

/// The overlay file sizes of the Workspaces with `namespaces` (each
/// Workspace record's `namespace`) of Sandbox `sandbox` on machine
/// `machine`, read where its run folder is: `{ overlay_sizes: [{
/// namespace, bytes }], daemon, usage, usage_reason?, workspace_usage,
/// workspace_usage_reason? }`, `bytes` null for
/// a Workspace that has no overlay file there, with what that side sees
/// of the Sandbox's daemon process (layerfs-project#164;
/// [`crate::run_folder::observed`]). `namespaces` may be empty. Refusals
/// are an inspect request's: `Busy`, `Unavailable`, `NoSuchSandbox`.
pub fn overlay_sizes(
    home: &std::path::Path,
    machine: &str,
    sandbox: &str,
    namespaces: &[i64],
) -> Outcome {
    let request = SdkValue::map([
        ("sandbox", SdkValue::text(sandbox)),
        (
            "namespaces",
            SdkValue::List(namespaces.iter().map(|n| SdkValue::Int(*n)).collect()),
        ),
    ]);
    super::super::route::inspect::ask(home, machine, KIND, request, |side, request, _| {
        serve(side, request)
    })
}

/// The request on the side that holds the run folder.
/// One `overlay_sizes` request on `side`.
pub fn serve(side: &Side<'_>, args: &SdkValue) -> Outcome {
    let namespaces: Vec<i64> = match args.get("namespaces") {
        Some(SdkValue::List(items)) if items.len() <= MOST => items
            .iter()
            .map(|item| match item {
                SdkValue::Int(namespace) if *namespace >= 0 => Some(*namespace),
                _ => None,
            })
            .collect::<Option<_>>(),
        _ => None,
    }
    .ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("overlay_sizes: `namespaces` is a list of at most {MOST} Workspace namespaces"),
        )
    })?;
    let sandbox = args.get_text("sandbox").unwrap_or("");
    // the read: one `lstat` per Workspace, in the run folder the Sandbox's
    // daemon record on this side names, and that record's process
    let run = super::super::route::inspect::run_folder(side, sandbox)?;
    let (Side::Home(root) | Side::Worker(root)) = side;
    let directory = layerfs_home::home::paths::sandbox_dir(root, sandbox);
    Ok(match crate::record::discovery::started(&directory)? {
        Some(daemon) => crate::status::run_folder::observed(&daemon, &run, &namespaces),
        None => crate::status::run_folder::sizes(&run, &namespaces),
    })
}
