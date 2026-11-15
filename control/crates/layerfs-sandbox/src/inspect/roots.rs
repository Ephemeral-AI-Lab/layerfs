//! Where a Files view starts and what it keeps private
//! (layerfs-project#178 "Sandbox files", security rule 5).
//!
//! [`sandbox`] is the no-path answer of `sandbox.files`: the Sandbox's
//! roots, each with a `role`. Its Workspaces come from the Workspace
//! records and are never `stat`ed, so a stopped daemon's mount cannot hang
//! the answer; `home` and `log` are entries of the run folder.
//!
//! [`kept`] is the one place a path is judged against LayerFS's own
//! layout: every listing and every read asks it, for the path and for each
//! folder above it.
use super::files::{entry, refusal, CALL};
use super::Side;
use crate::record::SandboxRecord;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::paths;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_sandbox_backend::local::confined::{Base, Kind, View};
use std::ffi::OsStr;
use std::path::{Component, Path};

/// The commands' `HOME` in a run folder (the backends' `tmp`).
pub(super) const HOME: &str = "tmp";
/// The daemon's log in a run folder.
const LOG: &str = "daemon.log";
/// The folder of a run folder that holds its Workspace mounts.
const WORKSPACES: &str = "workspaces";
/// The uid a Sandbox's commands run as when its record names none.
const COMMAND_UID: u32 = 501;
/// The most Workspaces a request sends.
const MOST: usize = 1024;

/// What LayerFS keeps of a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kept {
    /// Nothing: it is listed and read as it is.
    Open,
    /// It holds a secret, a database or a lock: listed with its name and
    /// size, never entered or read.
    Restricted,
    /// A log a `logs` call reads by cursor: listed, not read here.
    Log,
    /// A Workspace mount, or a path inside one: listed as a mount from the
    /// Workspace records, never `stat`ed, entered or read.
    Workspace,
}

/// What LayerFS keeps of `path` on the side whose root is `root` (a home,
/// or a worker's root), from the path alone. A read asks this of the path
/// and of every folder above it.
///
/// - A Store's database and its companions, `store.sqlite*`, wherever
///   they are.
/// - In a run folder (`machine/native/run/<name>/`): `config/` (the
///   daemon's private key) and `overlay/` (the Workspaces' databases) are
///   restricted, `daemon.log` is a log, and what is under `workspaces/` is
///   a Workspace mount. Its `tmp/` is the commands' home: theirs, open.
/// - Elsewhere under `root`: `control.json` and `worker.json` (keys),
///   `lock` and `*.lock` (locks), a Store's `open/` (its leases), and
///   `log/worker.log` with `worker.1.log` (logs).
///
/// A view that shows `/layerfs/state` adds its `secret` here.
pub(super) fn kept(root: &Path, path: &Path) -> Kept {
    let inside = path.strip_prefix(root).ok();
    let names: Vec<&OsStr> = (inside.unwrap_or(path).components())
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let starts =
        |name: &OsStr, prefix: &str| name.as_encoded_bytes().starts_with(prefix.as_bytes());
    if names.iter().any(|name| starts(name, "store.sqlite")) {
        return Kept::Restricted;
    }
    if inside.is_none() {
        return Kept::Open;
    }
    let named = |at: usize, name: &str| names.get(at).is_some_and(|found| *found == name);
    if named(0, "machine") && named(1, "native") && named(2, "run") && names.len() > 4 {
        return match names[4].to_str() {
            Some("config" | "overlay") => Kept::Restricted,
            Some(LOG) if names.len() == 5 => Kept::Log,
            Some(WORKSPACES) if names.len() > 5 => Kept::Workspace,
            _ => Kept::Open,
        };
    }
    if named(0, "store") && named(2, "open") {
        return Kept::Restricted;
    }
    if named(0, "log") && names.len() == 2 && (named(1, "worker.log") || named(1, "worker.1.log")) {
        return Kept::Log;
    }
    let ends = |name: &OsStr, suffix: &str| name.as_encoded_bytes().ends_with(suffix.as_bytes());
    match names.last() {
        Some(name) if *name == "control.json" || *name == "worker.json" => Kept::Restricted,
        Some(name) if *name == "lock" || ends(name, ".lock") => Kept::Restricted,
        _ => Kept::Open,
    }
}

/// The uid the commands of Sandbox `name` run as: the numeric id its
/// daemon record on `side` names, else 501. It is never looked up in an
/// image's `/etc/passwd`.
pub(super) fn command_uid(side: &Side<'_>, name: &str) -> u32 {
    let (Side::Home(root) | Side::Worker(root)) = side;
    (SandboxRecord::read_in(&paths::sandbox_dir(root, name), name).ok())
        .flatten()
        .and_then(|record| record.daemon()?.uid())
        .and_then(|uid| u32::try_from(uid).ok())
        .unwrap_or(COMMAND_UID)
}

/// The Workspace records of the Sandbox folder `directory`, as a request
/// carries them from the side that keeps them to the side that has the run
/// folder: `[{ id, path }]`.
pub(super) fn recorded(directory: &Path) -> Outcome {
    let mut sent = Vec::new();
    for (_, record) in crate::files::workspaces(directory)? {
        if let (Some(id), Some(path)) = (record.id(), record.path()) {
            sent.push(SdkValue::map([
                ("id", SdkValue::text(id)),
                ("path", SdkValue::text(path)),
            ]));
        }
    }
    Ok(SdkValue::List(sent))
}

/// The roots of Sandbox `name`, whose run folder on `side` is `run`: one
/// `workspace` per Workspace record (those `sent` with the request, else
/// this side's own), then `home` and `log` when the run folder has them.
/// The answer is a folder page that is never continued.
pub(super) fn sandbox(side: &Side<'_>, name: &str, run: &Path, sent: Option<&SdkValue>) -> Outcome {
    let (Side::Home(root) | Side::Worker(root)) = side;
    let own;
    let recorded = match sent {
        Some(SdkValue::List(items)) if items.len() <= MOST => items,
        Some(_) => {
            return Err(refused(
                "InvalidInput",
                format!("{CALL}: `workspaces` is a list of at most {MOST} Workspace records"),
            ))
        }
        None => {
            own = recorded(&paths::sandbox_dir(root, name))?;
            match &own {
                SdkValue::List(items) => items,
                _ => unreachable!("recorded answers a list"),
            }
        }
    };
    let mut mounts: Vec<(&str, &str)> = (recorded.iter())
        .filter_map(|item| Some((item.get_text("path")?, item.get_text("id")?)))
        .collect();
    // `workspaces/2` before `workspaces/10`
    mounts.sort_by_key(|(path, _)| (path.len(), *path));
    let mut entries: Vec<SdkValue> = (mounts.into_iter())
        .map(|(path, id)| {
            let mut mount = entry(path, 1, "mount", None);
            mount.push(("role".to_owned(), SdkValue::text("workspace")));
            mount.push(("workspace".to_owned(), SdkValue::text(id)));
            SdkValue::Map(mount)
        })
        .collect();
    // the run folder is LayerFS's own; its two entries are `stat`ed as
    // entries, nothing is followed
    let folder = Base::open(run, View::Beneath)
        .and_then(|base| base.open_at(Path::new("")))
        .map_err(|why| refusal(CALL, run, why))?;
    for (name, kind, shown, role) in [
        (HOME, Kind::Folder, "folder", "home"),
        (LOG, Kind::File, "file", "log"),
    ] {
        let Ok(stat) = folder.stat_of(OsStr::new(name)) else {
            continue;
        };
        if stat.kind == kind {
            let path = run.join(name);
            let mut root = entry(&path.to_string_lossy(), 1, shown, Some(&stat));
            root.push(("role".to_owned(), SdkValue::text(role)));
            entries.push(SdkValue::Map(root));
        }
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("folder")),
        ("entries", SdkValue::List(entries)),
        ("next", SdkValue::Null),
        ("has_more", SdkValue::Bool(false)),
    ]))
}
