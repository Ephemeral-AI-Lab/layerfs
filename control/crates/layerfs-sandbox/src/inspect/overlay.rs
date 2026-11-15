//! The inspect kind `overlay_sizes` (layerfs-project#178 `overlay_bytes`):
//! the size of each Workspace's overlay file, read on the side that holds
//! the Sandbox's run folder for a caller that holds the Workspace records
//! (the home, for a Sandbox whose daemon it reaches directly). This file
//! is the kind's routing: its name, its request `{ sandbox, namespaces }`
//! and the home-side call. The read itself is one `lstat` of
//! `<run folder>/overlay/<namespace>.sqlite` per Workspace: the file is
//! never opened and no daemon is asked.
use super::Side;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{refused, Outcome};

/// The inspect kind.
pub(super) const KIND: &str = "overlay_sizes";

/// The most Workspaces one request names.
const MOST: usize = 1024;

/// The overlay file sizes of the Workspaces with `namespaces` (each
/// Workspace record's `namespace`) of Sandbox `sandbox` on machine
/// `machine`, read where its run folder is: `{ overlay_sizes: [{
/// namespace, bytes }] }`, with no entry for a Workspace that has no
/// overlay file there. Refusals are an inspect request's: `Busy`,
/// `Unavailable`, `NoSuchSandbox`.
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
    super::ask(home, machine, KIND, request)
}

/// The request on the side that holds the run folder.
pub(super) fn serve(side: &Side<'_>, args: &SdkValue) -> Outcome {
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
    // daemon record on this side names
    let folder = super::run_folder(side, sandbox)?.join("overlay");
    let size = |namespace: &i64| {
        let found = std::fs::symlink_metadata(folder.join(format!("{namespace}.sqlite"))).ok()?;
        let bytes = i64::try_from(found.len())
            .ok()
            .filter(|_| found.is_file())?;
        Some(SdkValue::map([
            ("namespace", SdkValue::Int(*namespace)),
            ("bytes", SdkValue::Int(bytes)),
        ]))
    };
    let sizes = namespaces.iter().filter_map(size).collect();
    Ok(SdkValue::map([("overlay_sizes", SdkValue::List(sizes))]))
}
