//! Where a Files view starts and what it keeps private
//! (layerfs-project#178 "Sandbox files", security rule 5).
//!
//! [`sandbox`] is the no-path answer of `sandbox.files`: the Sandbox's
//! roots, each with a `role`. Its Workspaces come from the Workspace
//! records and are never `stat`ed, so a stopped daemon's mount cannot hang
//! the answer; `home` and `log` are entries of the run folder; `/` is the
//! root of a Sandbox that has one of its own.
//!
//! [`machine`] is the no-path answer of `machine.files`: the machine's
//! LayerFS folders ([`folders`]) ranked by role, then `/`.
//!
//! [`kept`] is the one place a path is judged against LayerFS's own
//! layout: every listing and every read asks it, for the path and for each
//! folder above it.
use super::files::{entry, refusal, CALL};
use super::Side;
use crate::record::{DaemonRecord, SandboxRecord};
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::paths;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_sandbox_backend::shared::confined::{Base, Kind, View};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// The commands' `HOME` in a run folder (the backends' `tmp`).
pub(super) const HOME: &str = "tmp";
/// The daemon's log in a run folder.
const LOG: &str = "daemon.log";
/// The folder of a run folder that holds its Workspace mounts.
const WORKSPACES: &str = "workspaces";
/// The uid a Sandbox's commands run as when its record names none.
const COMMAND_UID: u32 = 501;
/// The most Workspaces a request sends.
pub(super) const MOST: usize = 1024;
/// The folder beside a worker's root that holds its secret and its pid.
const STATE: &str = "state";

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
/// Beside a worker's root (`/layerfs/data`), `state/secret`: the secret
/// its callers prove themselves with.
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
        // the worker's secret, in the folder beside a worker's root
        let state = root.parent().map(|layerfs| layerfs.join(STATE));
        let secret = state.is_some_and(|state| path.strip_prefix(state) == Ok(Path::new("secret")));
        return match secret {
            true => Kept::Restricted,
            false => Kept::Open,
        };
    }
    let named = |at: usize, name: &str| names.get(at).is_some_and(|found| *found == name);
    if named(0, "machine") && named(1, "native") && named(2, "run") && names.len() > 4 {
        return match names[4].to_str() {
            Some("config" | "overlay") => Kept::Restricted,
            Some(LOG) if names.len() == 5 => Kept::Log,
            Some(WORKSPACES) if names.len() > 5 => Kept::Workspace,
            // the image cache's own locks, beside its unpacked images
            Some(lock) if names.len() == 5 && named(3, "images") && lock.ends_with(".lock") => {
                Kept::Restricted
            }
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

/// The daemon record of Sandbox `name` on `side`, read as a file.
pub(super) fn daemon(side: &Side<'_>, name: &str) -> Option<DaemonRecord> {
    let (Side::Home(root) | Side::Worker(root)) = side;
    let record = SandboxRecord::read_in(&paths::sandbox_dir(root, name), name);
    record.ok().flatten()?.daemon().cloned()
}

/// The uid the commands of the Sandbox `daemon` records run as: the
/// numeric id the record names, else 501. It is never looked up in an
/// image's `/etc/passwd`.
pub(super) fn uid_of(daemon: Option<&DaemonRecord>) -> u32 {
    (daemon.and_then(DaemonRecord::uid))
        .and_then(|uid| u32::try_from(uid).ok())
        .unwrap_or(COMMAND_UID)
}

/// [`uid_of`] the daemon record of Sandbox `name` on `side`.
pub(super) fn command_uid(side: &Side<'_>, name: &str) -> u32 {
    uid_of(daemon(side, name).as_ref())
}

/// The Workspace records of the Sandbox folder `directory`, as a request
/// carries them from the side that keeps them to the side that has the run
/// folder: `[{ id, path, workspace }]`, `workspace` its handle
/// (`<sandbox>:<name>`, or `<sandbox>:<path>` with no name), the folder's
/// name being the Sandbox's.
pub(super) fn recorded(directory: &Path) -> Outcome {
    let sandbox = (directory.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let mut sent = Vec::new();
    for (_, record) in crate::files::workspaces(directory)? {
        let handle = crate::list::handle(sandbox, &record);
        if let (Some(id), Some(path), Some(handle)) = (record.id(), record.path(), handle) {
            sent.push(SdkValue::map([
                ("id", SdkValue::text(id)),
                ("path", SdkValue::text(path)),
                ("workspace", SdkValue::Text(handle)),
            ]));
        }
    }
    Ok(SdkValue::List(sent))
}

/// The Workspace records of every Sandbox recorded under `root`, as
/// [`recorded`] answers one Sandbox's; a folder that cannot be read has
/// none.
pub(super) fn recorded_under(root: &Path) -> SdkValue {
    let names = paths::names(&root.join("sandbox")).unwrap_or_default();
    let mut all = Vec::new();
    for name in names {
        if let Ok(SdkValue::List(items)) = recorded(&paths::sandbox_dir(root, &name)) {
            all.extend(items);
        }
    }
    SdkValue::List(all)
}

/// The LayerFS folders of the machine `side` serves, in the order a
/// client shows them, each with its role. On a worker's root
/// (`/layerfs/data`): the root itself (`data`), in it `store` (`stores`),
/// `sandbox` (`sandbox_records`), `machine/native/run` (`run_folders`)
/// with its `images` (`images`), `incoming` (`staging`) and `log`
/// (`logs`), then beside it `state` (`state`), `bundle` (`bundle`) and
/// `image` (`image`). In a home, which is its owner's own computer:
/// `store`, `sandbox`, `machine/native/run` and `log` only.
pub(super) fn folders(side: &Side<'_>) -> Vec<(PathBuf, &'static str)> {
    let (Side::Home(root) | Side::Worker(root)) = side;
    let run = paths::run_root(root);
    let mut folders = vec![
        (root.join("store"), "stores"),
        (root.join("sandbox"), "sandbox_records"),
        (run.clone(), "run_folders"),
    ];
    match side {
        Side::Home(_) => folders.push((paths::log_dir(root), "logs")),
        Side::Worker(_) => {
            folders.insert(0, (root.to_path_buf(), "data"));
            folders.push((run.join("images"), "images"));
            folders.push((root.join("incoming"), "staging"));
            folders.push((paths::log_dir(root), "logs"));
            if let Some(layerfs) = root.parent() {
                folders.push((layerfs.join(STATE), "state"));
                folders.push((layerfs.join("bundle"), "bundle"));
                folders.push((layerfs.join("image"), "image"));
            }
        }
    }
    folders
}

/// The roots of the machine `side` serves, the no-path answer of
/// `machine.files`: each LayerFS folder that is there, with its `role`
/// (`data` with `free_bytes`, what its filesystem has left), then on a
/// worker's side `/` (`machine`). Each is opened as a folder and `stat`ed
/// through its descriptor, whatever filesystem it is on; one that is a
/// link or no folder is left out. The answer is a folder page that is
/// never continued.
pub(super) fn machine(side: &Side<'_>) -> Outcome {
    let mut listed = folders(side);
    if let Side::Worker(_) = side {
        listed.push((PathBuf::from("/"), "machine"));
    }
    let mut entries = Vec::new();
    for (folder, role) in listed {
        let opened =
            Base::open(&folder, View::Beneath).and_then(|base| base.open_at(Path::new("")));
        let Ok(opened) = opened else { continue };
        let mut root = entry(&folder.to_string_lossy(), 1, "folder", Some(&opened.stat));
        root.push(("role".to_owned(), SdkValue::text(role)));
        if role == "data" {
            let free = layerfs_home::home::filesystem(&folder).ok();
            let free = free.and_then(|disk| i64::try_from(disk.free).ok());
            root.push((
                "free_bytes".to_owned(),
                free.map_or(SdkValue::Null, SdkValue::Int),
            ));
        }
        entries.push(SdkValue::Map(root));
    }
    Ok(page_of(entries))
}

/// A folder page that is never continued.
fn page_of(entries: Vec<SdkValue>) -> SdkValue {
    SdkValue::map([
        ("kind", SdkValue::text("folder")),
        ("entries", SdkValue::List(entries)),
        ("next", SdkValue::Null),
        ("has_more", SdkValue::Bool(false)),
    ])
}

/// The roots of Sandbox `name`, whose run folder on `side` is `run`: one
/// `workspace` per Workspace record (those `sent` with the request, else
/// this side's own), then `home` and `log` when the run folder has them,
/// then, for a Sandbox with a root of its own (`rooted`: a namespaces
/// Sandbox), `/` with role `root`. That entry is from the record alone:
/// nothing of the root is opened, so a stopped Sandbox lists it too, and a
/// read of it then says why it cannot be read. The answer is a folder page
/// that is never continued.
pub(super) fn sandbox(
    side: &Side<'_>,
    name: &str,
    run: &Path,
    sent: Option<&SdkValue>,
    rooted: bool,
) -> Outcome {
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
    // each Workspace by its handle; a record sent by a caller of a build
    // before handles names it by its id
    let mut mounts: Vec<(&str, &str)> = (recorded.iter())
        .filter_map(|item| {
            let handle = item.get_text("workspace").or(item.get_text("id"))?;
            Some((item.get_text("path")?, handle))
        })
        .collect();
    // `workspaces/2` before `workspaces/10`
    mounts.sort_by_key(|(path, _)| (path.len(), *path));
    let mut entries: Vec<SdkValue> = (mounts.into_iter())
        .map(|(path, handle)| {
            let mut mount = entry(path, 1, "mount", None);
            mount.push(("role".to_owned(), SdkValue::text("workspace")));
            mount.push(("workspace".to_owned(), SdkValue::text(handle)));
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
    if rooted {
        let mut root = entry("/", 1, "folder", None);
        root.push(("role".to_owned(), SdkValue::text("root")));
        entries.push(SdkValue::Map(root));
    }
    Ok(page_of(entries))
}
