//! LayerFS removes what interrupted operations left behind itself (spec
//! v20, owner 2026-11-19: `home leftovers` and `home prune` are gone). This
//! module is the run root's part: a Sandbox run folder no record names, and
//! an empty private root `<name>.root` whose run folder is gone. One side
//! sweeps its own run root (`machine/native/run` under the home on
//! `native`, under the worker's root on a machine):
//!
//! - a worker when it starts serving (`machine start`), and before each
//!   `sandbox create` it runs;
//! - this process before a `sandbox create` on `native` starts its daemon.
//!
//! An entry is removed only where nothing can own it. A run folder: no
//! Sandbox record of this side names it, and none of its Sandbox names no
//! run folder yet (a create in flight); its `owner` marker is there and was
//! written more than a minute ago (a start writes it just before its
//! record); the record folder that marker names is this side's and has no
//! live daemon (its pid, start time and boot), or is gone; the mount table
//! lists nothing at or under it or its private root. It is removed entry by
//! entry through no link, only on its own device, its `owner` marker last,
//! so a removal a bound stopped is found and finished by the next sweep. An
//! empty private root: its run folder is gone and it is not a mount point;
//! `rmdir` only. Anything that cannot be proven free is kept.
//!
//! A worker starting to serve ([`worker_start`]) first removes each Sandbox
//! record folder of its root whose recorded daemon is known gone (its pid,
//! start time and boot name no live process: the machine stopped since, or
//! the daemon ended), under that folder's guard: the home may have removed
//! its own record while the machine was stopped (`sandbox remove --force`),
//! and nothing else would reach this one. Its run folder is then no
//! record's, and goes by the rules above. A record with no daemon, or one
//! whose daemon may live or cannot be told, is kept.
//!
//! A sweep is one attempt inside [`BOUNDS`] (20,000 directory entries or
//! 2 s, the bounded walk the leftover listing used), and it never fails the
//! call it rides on: what it removed, and each removal that failed, is a
//! line of `log/sweep.jsonl` under the side's root (the home's log on
//! `native`, the machine's beside `worker.log`), and what is left waits for
//! the next sweep. The Workspace mount points of a run folder
//! (`workspaces/`) are never entered while it is only listed.
use crate::record::SandboxRecord;
use layerfs_home::home::{log_swept, paths};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Where a sweep stops: after this many directory entries, or this long,
/// whichever comes first.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// Directory entries listed or removed.
    pub entries: usize,
    /// Wall time.
    pub within: Duration,
}

/// The bounds every sweep runs in.
pub const BOUNDS: Bounds = Bounds {
    entries: 20_000,
    within: Duration::from_secs(2),
};

/// How long a run folder's `owner` marker must have been there: a start
/// writes it just before its record.
pub const OWNER_QUIET: Duration = Duration::from_secs(60);

/// A run folder's marker naming the record folder that owns it.
const OWNER: &str = "owner";

/// What one sweep is still allowed to do.
pub struct Budget {
    left: usize,
    deadline: Instant,
}

impl Budget {
    /// A fresh budget of `bounds`.
    pub fn new(bounds: Bounds) -> Self {
        Self {
            left: bounds.entries,
            deadline: Instant::now() + bounds.within,
        }
    }

    /// Counts one more entry; `false` once a bound is reached.
    pub fn more(&mut self) -> bool {
        if self.left == 0 || Instant::now() >= self.deadline {
            self.left = 0;
            return false;
        }
        self.left -= 1;
        true
    }

    /// The time left, zero once a bound is reached.
    pub fn time_left(&self) -> Duration {
        match self.left {
            0 => Duration::ZERO,
            _ => self.deadline.saturating_duration_since(Instant::now()),
        }
    }
}

/// What one sweep did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Swept {
    /// The entries removed, by path.
    pub removed: Vec<PathBuf>,
    /// The entries whose removal failed or stopped at the bound, with why.
    pub failed: Vec<(PathBuf, String)>,
    /// Whether the sweep ended before a bound did.
    pub complete: bool,
}

impl Swept {
    fn note(
        &mut self,
        root: &Path,
        kind: &str,
        path: PathBuf,
        outcome: Result<Option<u64>, String>,
    ) {
        log_swept(root, kind, &path, &outcome);
        match outcome {
            Ok(_) => self.removed.push(path),
            Err(why) => self.failed.push((path, why)),
        }
    }
}

/// Sweeps the run root of side root `root` inside [`BOUNDS`].
pub fn run_root(root: &Path) -> Swept {
    run_root_within(root, BOUNDS)
}

/// A worker's sweep of its root `root` when it starts serving: the Sandbox
/// records whose daemon is gone, then [`run_root`] (see the module).
pub fn worker_start(root: &Path) -> Swept {
    let mut swept = Swept::default();
    for name in paths::names(&root.join("sandbox")).unwrap_or_default() {
        let folder = paths::sandbox_dir(root, &name);
        let ended = || match SandboxRecord::read(root, &name) {
            Ok(Some(record)) => record.daemon().and_then(alive) == Some(false),
            _ => false,
        };
        if !ended() {
            continue;
        }
        // read again under its guard: a call that holds it may change it
        let Ok(_guard) = layerfs_store::project::discovery::guard(&folder) else {
            continue;
        };
        if !ended() {
            continue;
        }
        let outcome = std::fs::remove_dir_all(&folder).map(|()| None);
        let outcome = outcome.map_err(|error| format!("{}: {error}", folder.display()));
        swept.note(root, "record", folder, outcome);
    }
    let run = run_root(root);
    swept.removed.extend(run.removed);
    swept.failed.extend(run.failed);
    swept.complete = run.complete;
    swept
}

/// [`run_root`] inside `bounds`.
pub fn run_root_within(root: &Path, bounds: Bounds) -> Swept {
    let mut budget = Budget::new(bounds);
    let mut swept = Swept::default();
    let run = paths::run_root(root);
    let listed = list(&run, &mut budget);
    let records = records(root);
    for (name, directory) in &listed {
        if !directory || name == "images" || name.starts_with('.') {
            continue;
        }
        if let Some(base) = name.strip_suffix(".root") {
            if listed.iter().any(|(other, _)| other == base) {
                continue;
            }
            let path = run.join(name);
            if let Some(outcome) = empty_root(&run, &path) {
                swept.note(root, "empty_root", path, outcome);
            }
            continue;
        }
        let path = run.join(name);
        if !free(root, &path, &records) {
            continue;
        }
        let device = match std::fs::symlink_metadata(&path) {
            Ok(found) if found.is_dir() => found.dev(),
            _ => continue,
        };
        let outcome = remove_tree(&path, device, &mut budget);
        swept.note(root, "run_folder", path, outcome);
    }
    swept.complete = budget.more();
    swept
}

/// The entries of real directory `dir` (a link is not entered; an absent
/// one is empty): each name and whether it is a directory, sorted.
fn list(dir: &Path, budget: &mut Budget) -> Vec<(String, bool)> {
    let mut listed = Vec::new();
    if !std::fs::symlink_metadata(dir).is_ok_and(|found| found.is_dir()) {
        return listed;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return listed;
    };
    for entry in entries.flatten() {
        if !budget.more() {
            break;
        }
        let directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if let Ok(name) = entry.file_name().into_string() {
            listed.push((name, directory));
        }
    }
    listed.sort();
    listed
}

/// Lowercase hex digits only.
fn hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The record folder a run folder's `owner` marker names: read through no
/// link, at most 4096 bytes.
pub fn owner(folder: &Path) -> Option<PathBuf> {
    use std::io::Read;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::OpenOptionsExt;
    let mut marker = Vec::new();
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(folder.join(OWNER))
        .and_then(|file| file.take(4096).read_to_end(&mut marker))
        .ok()?;
    let path = PathBuf::from(std::ffi::OsString::from_vec(marker));
    path.is_absolute().then_some(path)
}

/// The Sandbox name a run folder belongs to: the folder its `owner` marker
/// names (`<root>/sandbox/<name>`), else what follows the prefix LayerFS
/// gives run folders (`layerfs-<8 hex>-`).
pub fn sandbox_of(folder: &Path) -> Option<String> {
    if let Some(owner) = owner(folder) {
        let named = owner.file_name()?.to_str()?.to_owned();
        let parent = owner.parent()?.file_name()?;
        return (parent == "sandbox").then_some(named);
    }
    let name = folder.file_name()?.to_str()?;
    let rest = name.strip_prefix("layerfs-")?;
    let (home, sandbox) = (rest.get(..8)?, rest.get(8..)?.strip_prefix('-')?);
    (hex(home) && !sandbox.is_empty()).then(|| sandbox.to_owned())
}

/// One Sandbox folder of a side: its name, and the name of the run folder
/// its record's daemon holds; `Err` when its record could not be read.
type Recorded = (String, Result<Option<String>, ()>);

/// The Sandbox folders of side root `root`, as a sweep reads them.
fn records(root: &Path) -> Vec<Recorded> {
    let names = paths::names(&root.join("sandbox")).unwrap_or_default();
    (names.into_iter())
        .map(|name| {
            let run = match SandboxRecord::read(root, &name) {
                Ok(Some(record)) => Ok((record.daemon())
                    .and_then(|daemon| daemon.directory())
                    .and_then(|directory| Path::new(directory).file_name()?.to_str())
                    .map(str::to_owned)),
                // a folder with no record yet is a create claiming its name
                Ok(None) => Ok(None),
                Err(_) => Err(()),
            };
            (name, run)
        })
        .collect()
}

/// Whether run folder `folder` under `root` can be proven to belong to no
/// one (see the module).
fn free(root: &Path, folder: &Path, records: &[Recorded]) -> bool {
    let name = folder.file_name().and_then(|name| name.to_str());
    // a record names it, or a record that could not be read might
    if (records.iter()).any(|(_, run)| match run {
        Ok(run) => run.as_deref().is_some() && run.as_deref() == name,
        Err(()) => true,
    }) {
        return false;
    }
    // its Sandbox has a folder here that names no run folder yet
    if sandbox_of(folder).is_some_and(|sandbox| {
        (records.iter()).any(|(record, run)| *record == sandbox && matches!(run, Ok(None)))
    }) {
        return false;
    }
    let written = match std::fs::symlink_metadata(folder.join(OWNER)) {
        Ok(found) if found.is_file() => found.modified(),
        _ => return false,
    };
    let quiet = written
        .ok()
        .and_then(|at| SystemTime::now().duration_since(at).ok())
        .is_some_and(|age| age >= OWNER_QUIET);
    if !quiet || !daemonless(root, folder) {
        return false;
    }
    let mut private_root = folder.as_os_str().to_owned();
    private_root.push(".root");
    let unmounted = |point: &Path| {
        layerfs_sandbox_backend::local::mounts_under(point).is_ok_and(|mounts| mounts.is_empty())
    };
    unmounted(folder) && unmounted(Path::new(&private_root))
}

/// Whether the record folder run folder `folder`'s `owner` marker names
/// is this side's and has no live daemon, or is gone.
fn daemonless(root: &Path, folder: &Path) -> bool {
    let Some(owner) = owner(folder) else {
        return false;
    };
    let Ok(records) = std::fs::canonicalize(root.join("sandbox")) else {
        // no Sandbox folder at all on this side: no record holds a daemon
        return !root.join("sandbox").exists();
    };
    if owner.parent() != Some(records.as_path()) {
        return false;
    }
    if !owner.exists() {
        return true;
    }
    let Some(name) = owner.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    match SandboxRecord::read_in(&owner, name) {
        Ok(Some(record)) => match record.daemon() {
            None => true,
            Some(daemon) => alive(daemon) == Some(false),
        },
        Ok(None) => true,
        Err(_) => false,
    }
}

/// Whether the daemon a record names lives; `None` when it cannot be told.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn alive(daemon: &crate::record::DaemonRecord) -> Option<bool> {
    let process = crate::reopen::recorded_process(daemon).ok()?;
    Some(process.alive())
}

/// Elsewhere it cannot be told.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn alive(_daemon: &crate::record::DaemonRecord) -> Option<bool> {
    None
}

/// An empty private root `path` in run root `run` whose run folder is gone:
/// `rmdir`, never of a mount point. `None` when it is kept (not empty, or
/// a mount point).
fn empty_root(run: &Path, path: &Path) -> Option<Result<Option<u64>, String>> {
    let device = |at: &Path| std::fs::symlink_metadata(at).map(|found| found.dev());
    let (own, parent) = (device(path).ok()?, device(run).ok()?);
    let listed = layerfs_sandbox_backend::local::mounts_under(path).ok()?;
    if own != parent || !listed.is_empty() {
        return None;
    }
    if !std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none()) {
        return None;
    }
    match std::fs::remove_dir(path) {
        Ok(()) => Some(Ok(Some(0))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => None,
        Err(error) => Some(Err(error.to_string())),
    }
}

/// Removes directory `top` and all it holds, through no link, its `owner`
/// marker last: a link is unlinked, never followed, and a directory is
/// entered only while it is on `device`, `top`'s own. Answers the bytes of
/// the files it removed; an entry another remover took first is gone
/// already. Stops at `budget`'s bound, saying so.
fn remove_tree(top: &Path, device: u64, budget: &mut Budget) -> Result<Option<u64>, String> {
    let io = |at: &Path, error: std::io::Error| format!("{}: {error}", at.display());
    let absent = |error: &std::io::Error| error.kind() == std::io::ErrorKind::NotFound;
    let mut bytes = 0u64;
    let mut pending = vec![(top.to_owned(), false)];
    while let Some((dir, emptied)) = pending.pop() {
        if emptied {
            if dir == top {
                match std::fs::remove_file(top.join(OWNER)) {
                    Err(error) if !absent(&error) => return Err(io(&top.join(OWNER), error)),
                    _ => {}
                }
            }
            match std::fs::remove_dir(&dir) {
                Err(error) if !absent(&error) => return Err(io(&dir, error)),
                _ => continue,
            }
        }
        let found = match std::fs::symlink_metadata(&dir) {
            Ok(found) => found,
            Err(error) if absent(&error) => continue,
            Err(error) => return Err(io(&dir, error)),
        };
        if found.dev() != device {
            return Err(format!(
                "{} is on another filesystem than its run folder (a mount); the removal stopped there",
                dir.display()
            ));
        }
        // a copied mode may close a directory to its own owner
        if found.permissions().mode() & 0o700 != 0o700 {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| io(&dir, error))?;
        }
        pending.push((dir.clone(), true));
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if absent(&error) => continue,
            Err(error) => return Err(io(&dir, error)),
        };
        for entry in entries {
            if !budget.more() {
                return Err(format!(
                    "the sweep's bound ({} entries or {} s) ended the removal; the next sweep goes on",
                    BOUNDS.entries,
                    BOUNDS.within.as_secs()
                ));
            }
            let at = entry.map_err(|error| io(&dir, error))?.path();
            if dir == top && at.file_name().is_some_and(|name| name == OWNER) {
                continue;
            }
            let found = match std::fs::symlink_metadata(&at) {
                Ok(found) => found,
                Err(error) if absent(&error) => continue,
                Err(error) => return Err(io(&at, error)),
            };
            if found.is_dir() {
                pending.push((at, false));
                continue;
            }
            match std::fs::remove_file(&at) {
                Ok(()) => bytes = bytes.saturating_add(found.len()),
                Err(error) if absent(&error) => {}
                Err(error) => return Err(io(&at, error)),
            }
        }
    }
    Ok(Some(bytes))
}
