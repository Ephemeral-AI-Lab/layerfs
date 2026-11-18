//! `home.prune(leftover)` (call surface 2.0, layerfs-project#180): removes
//! one leftover `home leftovers` listed, one attempt, under that kind's
//! guard. It is a mutation: it goes where a machine's mutations go
//! (`place::on_machine`, which refuses while an operation holds the
//! machine), to this process on `native` and to the worker's `home.prune`
//! operation otherwise, sent once. A stopped machine is `Unavailable`; a
//! lost answer is `Unknown` and nothing is sent again.
//!
//! The caller sends a kind and a name, never a path. The removing side
//! derives the path ([`Kind::path`]), checks every folder on the way is a
//! real directory under its own root, finds the entry again and checks its
//! guard at that moment:
//!
//! | kind | guard (`InUse` when it does not hold) |
//! | --- | --- |
//! | `run_folder` | no Sandbox record names it; its `owner` marker is there and older than a minute; the record that marker names has no live daemon (pid, start time and boot); the mount table lists nothing at or under it or its `<name>.root`. Removed entry by entry, through no link, and only directories on the folder's own device |
//! | `empty_root` | its run folder is gone; not a mount point (mount table, and its device is its parent's); `rmdir` only |
//! | `backup_staging` | the Store's `backup.lock` in the home, taken without waiting and held for the removal |
//! | `image_tmp` | the image's `<hex>.2.lock`, taken without waiting; the lock file stays |
//! | `export_container` | (host's Docker, `native` only) the copy lock of its machine and image in the home, taken without waiting and held while `docker rm -v` removes it |
//! | `init_copy` | a line of the home's `log/uncertain.jsonl` names this machine and copy, and its deadline passed more than an hour ago; then the worker's `project.stage` discard removes it. A copy no line names, or one with no deadline, is report-only (`InvalidInput`) |
//!
//! `NotFound`: an id no walk gives, or an entry that is gone. `InvalidInput`:
//! a folder copy that is only reported (`report_only`).
use super::leftover_list::{named, named_copies, INIT_GRACE_MS};
use super::leftovers::{self, Kind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{now_ms, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_sandbox::record::SandboxRecord;
use layerfs_sandbox_backend::rootfs::{self, Temporary};
use layerfs_store::place::{self, Executor};
use layerfs_store::project::frame::{StageFrame, StageStep};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// How long a run folder's `owner` marker must have been there: a start
/// writes it just before its record.
pub const OWNER_QUIET: Duration = Duration::from_secs(60);

fn in_use(why: impl Into<String>) -> SdkRefusal {
    refused("InUse", format!("{}; nothing was removed", why.into()))
}

fn gone(id: &str) -> SdkRefusal {
    refused(
        "NotFound",
        format!("no leftover has the id {id} now; `layerfs home leftovers` lists them"),
    )
}

/// `home.prune(leftover)` on `home`: `{ freed_bytes, outcome: removed,
/// leftover, kind, machine, path }`; `freed_bytes` is null when the size
/// was not measured.
pub fn prune(home: &Path, id: &str) -> Outcome {
    let Some((machine, kind, name)) = leftovers::parse(id) else {
        return Err(gone(id));
    };
    let built_in = matches!(machine, "native" | "docker");
    if !built_in && layerfs_machine::record::MachineRecord::read(home, machine)?.is_none() {
        return Err(gone(id));
    }
    let removed = match kind {
        Kind::InitCopy => init_copy(home, machine, name, id)?,
        Kind::ExportContainer => export_container(home, name).map_err(|r| renamed(r, id))?,
        _ => {
            // a create of this Store stages at these paths: held until the
            // removal ends
            let _backup = match kind {
                Kind::BackupStaging => Some(backup_lock(home, name, id)?),
                _ => None,
            };
            match place::on_machine(home, machine)? {
                Executor::Local => prune_on(home, kind, name, &[]).map_err(|r| renamed(r, id))?,
                Executor::Worker(mut worker) => {
                    let known = (layerfs_sandbox::record::sandboxes(home)?.iter())
                        .filter(|sandbox| sandbox.machine() == machine)
                        .map(|sandbox| SdkValue::text(sandbox.name()))
                        .collect();
                    let args = SdkValue::map([
                        ("kind", SdkValue::text(kind.as_str())),
                        ("name", SdkValue::text(name)),
                        ("known", SdkValue::List(known)),
                    ]);
                    worker
                        .attempt("home.prune", args, &mut |_| {})
                        .map_err(|unreached| unreached.0)?
                        .map_err(|r| renamed(r, id))?
                }
            }
        }
    };
    let field = |key: &str| removed.get(key).cloned().unwrap_or_default();
    Ok(SdkValue::map([
        ("freed_bytes", field("freed_bytes")),
        ("outcome", SdkValue::text("removed")),
        ("leftover", SdkValue::text(id)),
        ("kind", SdkValue::text(kind.as_str())),
        ("machine", SdkValue::text(machine)),
        ("path", field("path")),
    ]))
}

/// A removing side's `NotFound` names the id the caller gave.
fn renamed(refusal: SdkRefusal, id: &str) -> SdkRefusal {
    match refusal.code.as_str() {
        "NotFound" => gone(id),
        _ => refusal,
    }
}

/// The home's `backup.lock` of the Store a staged snapshot belongs to.
fn backup_lock(
    home: &Path,
    name: &str,
    id: &str,
) -> Result<layerfs_store::backup::create::Creating> {
    let store = name.split_once('/').map_or(name, |(store, _)| store);
    if !real_dir(&paths::store_dir(home, store)) {
        return Err(gone(id));
    }
    layerfs_store::backup::create::Creating::take(home, store).map_err(|refusal| {
        match refusal.code.as_str() {
            "Busy" => in_use(format!("a `backup create` of Store {store} is running")),
            _ => refusal,
        }
    })
}

/// An image export container in the host's Docker, under its image's copy
/// lock: `{ path: null, freed_bytes: null }`.
fn export_container(home: &Path, name: &str) -> Outcome {
    let docker = layerfs_sandbox::docker::Docker::of(home)?;
    layerfs_sandbox::image_exports::remove_left_export(&docker, home, name)?;
    Ok(SdkValue::map([
        ("path", SdkValue::Null),
        ("freed_bytes", SdkValue::Null),
    ]))
}

/// A folder copy: removable only when an uncertain line names it and its
/// deadline passed more than an hour ago; then the worker's own discard.
fn init_copy(home: &Path, machine: &str, copy: &str, id: &str) -> Outcome {
    let copies = named_copies(home);
    let Some(line) = named(&copies, machine, copy) else {
        return Err(refused(
            "InvalidInput",
            format!(
                "no line of log/uncertain.jsonl names copy {copy} on machine {machine}, so it may belong to a `project init` that is still running: it is only reported. Once none runs there, remove incoming/init-{copy} under that machine's worker root by hand"
            ),
        ));
    };
    let Some(deadline) = line.deadline else {
        return Err(refused(
            "InvalidInput",
            format!("the uncertain line of copy {copy} records no deadline for its Init: it is only reported"),
        ));
    };
    let wait = deadline
        .saturating_add(INIT_GRACE_MS)
        .saturating_sub(now_ms());
    if wait > 0 {
        return Err(in_use(format!(
            "copy {copy}'s `project init` of {}/{} may run until its deadline; it is removable in {} s (an hour after that deadline)",
            line.store,
            line.project,
            wait.saturating_add(999) / 1000
        )));
    }
    let frame = StageFrame {
        store: line.store.clone(),
        name: line.project.clone(),
        id: copy.to_owned(),
        step: StageStep::Discard,
    };
    let folder = |root: &Path| layerfs_store::project::staged::folder(root, copy);
    let (discarded, path) = match place::on_machine(home, machine)? {
        Executor::Local => (
            layerfs_store::project::staged::run(home, &frame)?,
            folder(home).display().to_string(),
        ),
        Executor::Worker(mut worker) => (
            worker
                .attempt("project.stage", frame.encode(), &mut |_| {})
                .map_err(|unreached| unreached.0)??,
            format!("incoming/init-{copy} under machine {machine}'s worker root"),
        ),
    };
    if !discarded.get_bool("discarded") {
        return Err(gone(id));
    }
    Ok(SdkValue::map([
        ("path", SdkValue::text(path)),
        ("freed_bytes", SdkValue::Null),
    ]))
}

/// The worker's `home.prune` operation: `{ kind, name, known }` under its
/// own root.
pub(crate) fn prune_in_worker(root: &Path, args: &SdkValue) -> Outcome {
    let kind = args.get_text("kind").and_then(Kind::named);
    let (Some(kind), Some(name)) = (kind, args.get_text("name")) else {
        return Err(refused("InvalidInput", "home.prune: `kind` and `name`"));
    };
    let known: Vec<String> = match args.get("known") {
        Some(SdkValue::List(names)) => (names.iter())
            .filter_map(|name| match name {
                SdkValue::Text(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    prune_on(root, kind, name, &known)
}

/// Removes entry `name` of `kind` under side root `root`, found again and
/// checked under its guard now. `known` names the Sandboxes the caller's
/// home records on this machine: `{ path, freed_bytes }`.
pub fn prune_on(root: &Path, kind: Kind, name: &str, known: &[String]) -> Outcome {
    let absent = || refused("NotFound", "no such leftover");
    let path = kind.path(root, name).ok_or_else(absent)?;
    // every folder from the root down to the entry's own is a real
    // directory: a link on the way would lead a removal elsewhere
    let within = path.parent().filter(|parent| beneath(root, parent));
    let Some(parent) = within else {
        return Err(absent());
    };
    let found = match std::fs::symlink_metadata(&path) {
        Ok(found) => found,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(absent()),
        Err(error) => return Err(failed(error)),
    };
    let bytes = match kind {
        Kind::RunFolder if found.is_dir() => run_folder(root, &path, known)?,
        Kind::EmptyRoot if found.is_dir() => empty_root(parent, &path, name)?,
        Kind::BackupStaging if found.is_file() => {
            std::fs::remove_file(&path).map_err(failed)?;
            Some(found.len())
        }
        Kind::ImageTmp => match rootfs::remove_temporary(parent, &format!(".tmp-{name}")) {
            Ok(Temporary::Removed) => None,
            Ok(Temporary::Absent) => return Err(absent()),
            Ok(Temporary::Held) => {
                return Err(in_use(
                    "another process holds that image's lock: an unpack of it is running",
                ))
            }
            Err(error) => return Err(failed(error)),
        },
        Kind::InitCopy | Kind::ExportContainer => {
            return Err(refused(
                "InvalidInput",
                format!("`{}` entries are not removed here", kind.as_str()),
            ))
        }
        // a link or another kind of file under the entry's name is not it
        _ => return Err(absent()),
    };
    Ok(SdkValue::map([
        ("path", SdkValue::text(path.display().to_string())),
        (
            "freed_bytes",
            (bytes.and_then(|bytes| i64::try_from(bytes).ok()))
                .map_or(SdkValue::Null, SdkValue::Int),
        ),
    ]))
}

/// Whether `path` is a real directory, not a link to one.
fn real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|found| found.is_dir())
}

/// Whether `folder` is under `root` with every folder between them a real
/// directory.
fn beneath(root: &Path, folder: &Path) -> bool {
    let Ok(rest) = folder.strip_prefix(root) else {
        return false;
    };
    let mut at = root.to_owned();
    real_dir(&at)
        && rest.components().all(|part| {
            at.push(part);
            matches!(part, std::path::Component::Normal(_)) && real_dir(&at)
        })
}

/// The mount points this process's table lists at or under `path`.
fn mounts(path: &Path) -> Result<Vec<PathBuf>> {
    layerfs_sandbox_backend::local::mounts_under(path).map_err(|error| {
        refused(
            "Failed",
            format!("the mount table cannot be read ({error}); nothing was removed"),
        )
    })
}

/// A run folder no record names: its guard, then its removal.
fn run_folder(root: &Path, path: &Path, known: &[String]) -> Result<Option<u64>> {
    let shown = path.display();
    if leftovers::recorded(path, &leftovers::records(root), known) {
        return Err(in_use(format!(
            "a Sandbox record names {shown}; `layerfs sandbox remove` removes that Sandbox"
        )));
    }
    let marker = path.join(leftovers::OWNER);
    let written = match std::fs::symlink_metadata(&marker) {
        Ok(found) if found.is_file() => found.modified().map_err(failed)?,
        _ => {
            return Err(in_use(format!(
                "{shown} has no `owner` marker, so it is not known to be a Sandbox's run folder LayerFS finished starting"
            )))
        }
    };
    if SystemTime::now()
        .duration_since(written)
        .unwrap_or_default()
        < OWNER_QUIET
    {
        return Err(in_use(format!(
            "{shown}'s `owner` marker was written less than {} s ago: its Sandbox may be starting",
            OWNER_QUIET.as_secs()
        )));
    }
    if let Some(pid) = live_daemon(root, path) {
        return Err(in_use(format!(
            "the Sandbox whose record folder {shown}'s `owner` marker names has a live daemon (process {pid})"
        )));
    }
    let mut private_root = path.as_os_str().to_owned();
    private_root.push(".root");
    let private_root = PathBuf::from(private_root);
    for point in [path, private_root.as_path()] {
        if let Some(mount) = mounts(point)?.first() {
            return Err(in_use(format!("{} is a mount point", mount.display())));
        }
    }
    let device = std::fs::symlink_metadata(path).map_err(failed)?.dev();
    remove_tree(path, device).map(Some)
}

/// The live daemon process of the Sandbox whose record folder `folder`'s
/// `owner` marker names, when that folder is one of this side's.
fn live_daemon(root: &Path, folder: &Path) -> Option<u32> {
    let owner = leftovers::owner(folder)?;
    let records = std::fs::canonicalize(root.join("sandbox")).ok()?;
    if owner.parent() != Some(records.as_path()) {
        return None;
    }
    let record = SandboxRecord::read_in(&owner, owner.file_name()?.to_str()?).ok()??;
    let process = layerfs_sandbox::reopen::recorded_process(record.daemon()?).ok()?;
    process.alive().then_some(process.pid)
}

/// An empty private root whose run folder is gone: `rmdir`, never of a
/// mount point.
fn empty_root(run: &Path, path: &Path, name: &str) -> Result<Option<u64>> {
    let shown = path.display();
    if std::fs::symlink_metadata(run.join(name)).is_ok() {
        return Err(in_use(format!(
            "{shown}'s run folder is there: its Sandbox may be running"
        )));
    }
    let device = |at: &Path| std::fs::symlink_metadata(at).map(|found| found.dev());
    let (own, parent) = (device(path).map_err(failed)?, device(run).map_err(failed)?);
    if leftovers::is_mount(own, parent, !mounts(path)?.is_empty()) {
        return Err(in_use(format!("{shown} is a mount point")));
    }
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(Some(0)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(refused("NotFound", "no such leftover"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            Err(in_use(format!("{shown} is not empty")))
        }
        Err(error) => Err(failed(error)),
    }
}

/// Removes directory `top` and all it holds, through no link: a link is
/// unlinked, never followed, and a directory is entered only while it is on
/// `device`, `top`'s own (a mount below ends the removal `InUse`; the mount
/// table was read first, so none is expected). Answers the bytes of the
/// files it removed.
fn remove_tree(top: &Path, device: u64) -> Result<u64> {
    let io = |at: &Path, error: std::io::Error| {
        refused(
            "Failed",
            format!("{}: {error}; the removal stopped there", at.display()),
        )
    };
    let mut bytes = 0u64;
    let mut pending = vec![(top.to_owned(), false)];
    while let Some((dir, emptied)) = pending.pop() {
        if emptied {
            std::fs::remove_dir(&dir).map_err(|error| io(&dir, error))?;
            continue;
        }
        let found = std::fs::symlink_metadata(&dir).map_err(|error| io(&dir, error))?;
        if found.dev() != device {
            return Err(in_use(format!(
                "{} is on another filesystem than its run folder (a mount); the removal stopped there",
                dir.display()
            )));
        }
        // a copied mode may close a directory to its own owner
        if found.permissions().mode() & 0o700 != 0o700 {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| io(&dir, error))?;
        }
        pending.push((dir.clone(), true));
        for entry in std::fs::read_dir(&dir).map_err(|error| io(&dir, error))? {
            let at = entry.map_err(|error| io(&dir, error))?.path();
            let found = std::fs::symlink_metadata(&at).map_err(|error| io(&at, error))?;
            if found.is_dir() {
                pending.push((at, false));
            } else {
                std::fs::remove_file(&at).map_err(|error| io(&at, error))?;
                bytes = bytes.saturating_add(found.len());
            }
        }
    }
    Ok(bytes)
}
