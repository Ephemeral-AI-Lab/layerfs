//! What `sandbox.get` reads of a Sandbox's run folder (layerfs-project#178),
//! on the side that has the folder: the home on `native`, a machine's worker
//! root otherwise (also for a Sandbox whose daemon the home reaches
//! directly, #535 G4, whose records are in the home but whose run folder is
//! in its machine).
//!
//! One read per Sandbox ([`read`]) answers one map,
//! `{ overlay_sizes: [{ namespace, bytes }] }`, which [`merged`] puts on the
//! listed Workspaces as `overlay_bytes`. Another datum of the same folder
//! is one more field of that map, read in the same call. Everything is
//! found by path: no lock, no lease, no daemon, and no file is opened (an
//! overlay database least of all: closing a descriptor on it would drop
//! its owner's locks).
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::with_field;
use std::path::Path;

/// The daemon namespaces of the Workspaces `listed` holds (a
/// `sandbox.list_workspaces` answer); each names its Workspace's overlay
/// file, as its Workspace record's `namespace` does.
pub fn namespaces(listed: &SdkValue) -> Vec<i64> {
    match listed.get("workspaces") {
        Some(SdkValue::List(items)) => (items.iter())
            .filter_map(|item| item.get_int("namespace"))
            .collect(),
        _ => Vec::new(),
    }
}

/// One read of the run folder of the Sandbox recorded in `directory` (its
/// record folder on this side): the size of each of `namespaces`' overlay
/// files. `None` when this side has no run folder for it: its record names
/// none (the home's record of a Sandbox it reaches directly) or is not
/// read.
pub fn read(directory: &Path, namespaces: &[i64]) -> Option<SdkValue> {
    let daemon = crate::files::started(directory).ok()??;
    Some(sizes(Path::new(daemon.directory()?), namespaces))
}

/// The read of run folder `run` itself: `{ overlay_sizes: [{ namespace,
/// bytes }] }`, one entry for each of `namespaces`, its `bytes` null when
/// that Workspace has no overlay file there. Also what the inspect kind
/// `overlay_sizes` answers for a Sandbox whose records are in the home
/// ([`crate::inspect::overlay_sizes`]).
pub(crate) fn sizes(run: &Path, namespaces: &[i64]) -> SdkValue {
    let sizes = namespaces.iter().map(|namespace| {
        SdkValue::map([
            ("namespace", SdkValue::Int(*namespace)),
            ("bytes", overlay_bytes(run, *namespace)),
        ])
    });
    SdkValue::map([("overlay_sizes", SdkValue::List(sizes.collect()))])
}

/// The size of `<run folder>/overlay/<namespace>.sqlite`, from its `lstat`
/// alone; null when it is not a file there. The file grows until its
/// Workspace is unmounted (the overlay is never vacuumed), so this is a
/// high-water mark that includes the template's size.
fn overlay_bytes(run: &Path, namespace: i64) -> SdkValue {
    let file = run.join("overlay").join(format!("{namespace}.sqlite"));
    match std::fs::symlink_metadata(file) {
        Ok(found) if found.is_file() => {
            i64::try_from(found.len()).map_or(SdkValue::Null, SdkValue::Int)
        }
        _ => SdkValue::Null,
    }
}

/// `listed` with each Workspace's `overlay_bytes` from `read`, what the
/// side that has the run folder answered ([`read`]): null for a Workspace
/// it gave no size for, and for every Workspace when it did not answer
/// (`None`).
pub fn merged(listed: SdkValue, read: Option<&SdkValue>) -> SdkValue {
    let sizes = match read.and_then(|read| read.get("overlay_sizes")) {
        Some(SdkValue::List(sizes)) => sizes.as_slice(),
        _ => &[],
    };
    let bytes_of = |item: &SdkValue| {
        let namespace = item.get_int("namespace");
        (sizes.iter())
            .find(|size| namespace.is_some() && size.get_int("namespace") == namespace)
            .and_then(|size| size.get("bytes").cloned())
            .unwrap_or_default()
    };
    let SdkValue::Map(fields) = listed else {
        return listed;
    };
    let sized = |item: SdkValue| {
        let bytes = bytes_of(&item);
        with_field(item, "overlay_bytes", bytes)
    };
    SdkValue::Map(
        (fields.into_iter())
            .map(|(field, value)| match (field.as_str(), value) {
                ("workspaces", SdkValue::List(items)) => (
                    field,
                    SdkValue::List(items.into_iter().map(sized).collect()),
                ),
                (_, value) => (field, value),
            })
            .collect(),
    )
}
