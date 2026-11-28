//! `home.move`: moves a current home into another folder on the same
//! filesystem, as `~/.layerfs` into `~/.layerfs/home`. It renames the
//! home's own top-level entries (LayerFS's names and the integration
//! folders) one by one, so the new folder may sit inside the old one;
//! every other entry, such as a client's `dsh/` or `connect/`, stays where
//! it is and is answered as `left`. It never copies: a copy keeps the id
//! and would drive the same machines.
//!
//! Then it rewrites the absolute paths into the moved entries that records
//! hold: a machine record's `daemon` and `image_archive`, every text in a
//! Sandbox record's `daemon`, and each run folder's `owner` marker
//! (`machine/<m>/run/<dir>/owner`). A run folder's `config/daemon.setup`
//! and `config/helper.spec` are read only by the start that wrote them, so
//! they are left as they are: the move is refused while a host-machine
//! Sandbox is open, the only one that could read them again.
//!
//! Refused before any effect while anything runs in the home: an open
//! host-machine Sandbox (its record has a runtime `id` or an `op`), a live
//! Store lease, or the host worker. It holds the home lock throughout.
use layerfs_home::disk::layout::{detect, Layout, INTEGRATION, LAYOUT_VERSION, OWN};
use layerfs_home::disk::{paths, private_dir, read_record, write_record};
use layerfs_home::reply::outcome::{failed, refused, with_fix, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Where the moved entries were and are: as the call named them and as
/// the filesystem resolves them, since records may hold either.
struct Moved {
    from: [PathBuf; 2],
    to: [PathBuf; 2],
    names: Vec<String>,
}

impl Moved {
    /// `text` under the new folder when it names a path inside a moved
    /// entry of the old one.
    fn mapped(&self, text: &str) -> Option<String> {
        for (from, to) in self.from.iter().zip(&self.to) {
            let Ok(rest) = Path::new(text).strip_prefix(from) else {
                continue;
            };
            let first = rest.components().next()?.as_os_str().to_string_lossy();
            if self.names.iter().any(|name| *name == first) {
                return Some(to.join(rest).display().to_string());
            }
        }
        None
    }

    /// Rewrites every text inside `value` that [`Self::mapped`] moves.
    fn rewrite(&self, value: &mut SdkValue) -> bool {
        match value {
            SdkValue::Text(text) => match self.mapped(text) {
                Some(new) => {
                    *text = new;
                    true
                }
                None => false,
            },
            SdkValue::List(items) => {
                (items.iter_mut()).fold(false, |any, item| self.rewrite(item) | any)
            }
            SdkValue::Map(fields) => {
                (fields.iter_mut()).fold(false, |any, (_, item)| self.rewrite(item) | any)
            }
            _ => false,
        }
    }
}

/// `home.move`: `{ home, from, outcome, moved, left, rewritten }`.
pub fn relocate(home: &Path, to: &Path) -> Outcome {
    if layerfs_home::disk::guest::is_root(home) {
        return Err(refused(
            "InvalidInput",
            "a machine's worker root is not a home",
        ));
    }
    if !to.is_absolute() || to.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(refused(
            "InvalidInput",
            "the folder to move the home to must be an absolute path with no `..`",
        ));
    }
    let id = current(home)?;
    let from_real = std::fs::canonicalize(home).map_err(failed)?;
    let to_real = resolved(to)?;
    target_free(to, &to_real)?;
    same_filesystem(&from_real, &to_real)?;
    let _lock = layerfs_home::lock::home::take(home)?;
    if current(home)? != id {
        return Err(refused(
            "Unavailable",
            "home identity changed while moving it",
        ));
    }
    let names = own_entries(&from_real)?;
    if let Some(name) = names
        .iter()
        .find(|name| to_real.starts_with(from_real.join(name)))
    {
        return Err(refused(
            "InvalidInput",
            format!(
                "{} is inside the home's own `{name}`, which moves",
                to.display()
            ),
        ));
    }
    nothing_runs(home)?;
    private_dir(to)?;
    let moved = Moved {
        from: [home.to_owned(), from_real.clone()],
        to: [to.to_owned(), to_real.clone()],
        names: names.clone(),
    };
    rename_all(&from_real, &to_real, &names)?;
    let rewritten = rewrite_records(&moved, &to_real).map_err(|refusal| {
        refused(
            "Failed",
            format!(
                "the home moved to {}, but a path in it was not rewritten: {}",
                to.display(),
                refusal.message
            ),
        )
    })?;
    layerfs_home::record::generation::moved(&to_real);
    let mut left = left_entries(&from_real, &names)?;
    // the new home itself, when it sits in the old folder, did not stay behind
    if to_real.parent() == Some(from_real.as_path()) {
        left.retain(|name| to_real.file_name().is_none_or(|own| own != name.as_str()));
    }
    let texts =
        |items: Vec<String>| SdkValue::List(items.into_iter().map(SdkValue::Text).collect());
    Ok(SdkValue::map([
        ("home", SdkValue::text(to.display().to_string())),
        ("from", SdkValue::text(home.display().to_string())),
        ("outcome", SdkValue::text("moved")),
        ("moved", texts(names)),
        ("left", texts(left)),
        ("rewritten", texts(rewritten)),
    ]))
}

/// The id of the current home at `home`; anything else is refused.
fn current(home: &Path) -> Result<String> {
    match detect(home)? {
        Layout::Current(file) if file.layout == LAYOUT_VERSION => Ok(file.id),
        Layout::Current(_) => Err(with_fix(
            refused("InvalidInput", "an older layout home: open it once first"),
            "layerfs home init",
        )),
        Layout::Empty => Err(refused(
            "NotFound",
            format!("no LayerFS home at {}", home.display()),
        )),
        Layout::Foreign => Err(layerfs_home::disk::layout::foreign(home)),
        Layout::Container => Err(layerfs_home::disk::layout::container(home)),
    }
}

/// `to` as the filesystem resolves it: its parent's real path and its name.
fn resolved(to: &Path) -> Result<PathBuf> {
    let (Some(parent), Some(name)) = (to.parent(), to.file_name()) else {
        return Err(refused("InvalidInput", "the home cannot move to `/`"));
    };
    match std::fs::canonicalize(parent) {
        Ok(parent) => Ok(parent.join(name)),
        Err(error) => Err(refused(
            "InvalidInput",
            format!(
                "{}: {error}; make the folder that holds it first",
                parent.display()
            ),
        )),
    }
}

/// The home's own top-level entries, which move: LayerFS's names and the
/// integration folders.
fn own_entries(home: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(home).map_err(failed)? {
        let name = entry
            .map_err(failed)?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if OWN.contains(&name.as_str()) || INTEGRATION.contains(&name.as_str()) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Every other top-level entry of the old folder, which stays.
fn left_entries(home: &Path, moved: &[String]) -> Result<Vec<String>> {
    let mut left = Vec::new();
    for entry in std::fs::read_dir(home).map_err(failed)? {
        let name = entry
            .map_err(failed)?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if name != ".DS_Store" && !moved.contains(&name) {
            left.push(name);
        }
    }
    left.sort();
    Ok(left)
}

/// `to` is free: not there, or an empty folder (macOS's `.DS_Store` aside).
fn target_free(to: &Path, real: &Path) -> Result<()> {
    let entries = match std::fs::read_dir(real) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(refused(
                "InvalidInput",
                format!("{}: {error}", to.display()),
            ))
        }
    };
    for entry in entries {
        if entry.map_err(failed)?.file_name() != ".DS_Store" {
            return Err(refused(
                "Exists",
                format!(
                    "{} is not empty; a home moves only into a new or empty folder",
                    to.display()
                ),
            ));
        }
    }
    Ok(())
}

/// A home moves by rename only: `to` (or the folder that will hold it)
/// is on the home's filesystem.
fn same_filesystem(from: &Path, to: &Path) -> Result<()> {
    let at = match std::fs::metadata(to) {
        Ok(_) => to,
        Err(_) => to.parent().unwrap_or(to),
    };
    let device = |path: &Path| std::fs::metadata(path).map(|m| m.dev()).map_err(failed);
    if device(from)? != device(at)? {
        return Err(refused(
            "Unsupported",
            format!(
                "{} is on another filesystem; a home moves only by rename, never by copy",
                to.display()
            ),
        ));
    }
    Ok(())
}

/// Refuses while anything runs in the home: the host worker, a live Store
/// lease or an open host-machine Sandbox.
fn nothing_runs(home: &Path) -> Result<()> {
    for lock in [
        paths::host_worker_startup_lock(home),
        paths::host_worker_lock(home),
    ] {
        let Ok(file) = std::fs::File::open(&lock) else {
            continue;
        };
        match layerfs_home::lock::flock::try_lock(&file, true) {
            Ok(true) => layerfs_home::lock::flock::unlock(&file),
            Ok(false) => {
                return Err(refused(
                    "InUse",
                    "the host worker serves this home; stop what uses it, then move the home",
                ))
            }
            Err(error) => return Err(failed(format!("{}: {error}", lock.display()))),
        }
    }
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let (live, _) = layerfs_store::admin::open::lease::read_holders(home, &store)?;
        if let Some(pid) = live.first() {
            return Err(refused(
                "InUse",
                format!(
                    "process {pid} has Store {store} open; a home moves only while nothing uses it"
                ),
            ));
        }
    }
    let mut open = Vec::new();
    for name in paths::names(&home.join("sandbox")).map_err(failed)? {
        let Some(record) = read_record(&paths::sandbox_dir(home, &name).join("record.json"))?
        else {
            continue;
        };
        let on_host = record.get_text("machine") == Some(layerfs_home::names::HOST_MACHINE);
        let running = record.get_text("id").is_some()
            || record
                .get(layerfs_home::record::intent::FIELD)
                .is_some_and(|op| *op != SdkValue::Null);
        if on_host && running {
            open.push(name);
        }
    }
    if let Some(first) = open.first() {
        return Err(with_fix(
            refused(
                "InUse",
                format!(
                    "host-machine Sandboxes are open: {}; remove them, then move the home",
                    open.join(", ")
                ),
            ),
            format!("layerfs sandbox remove {first}"),
        ));
    }
    Ok(())
}

/// Renames each entry into `to`; when one fails, the ones already moved
/// are put back and the failure is answered.
fn rename_all(from: &Path, to: &Path, names: &[String]) -> Result<()> {
    // `layerfs.json` last: until it moves, the old path is still the home
    let mut ordered: Vec<&String> = names.iter().filter(|n| *n != "layerfs.json").collect();
    ordered.extend(names.iter().filter(|n| *n == "layerfs.json"));
    for (done, name) in ordered.iter().enumerate() {
        if let Err(error) = std::fs::rename(from.join(name), to.join(name)) {
            let mut message = format!("moving {name}: {error}; nothing moved");
            for back in &ordered[..done] {
                if let Err(undo) = std::fs::rename(to.join(back), from.join(back)) {
                    message = format!(
                        "moving {name}: {error}; putting {back} back failed too ({undo}): \
                         the home is split between {} and {}",
                        from.display(),
                        to.display()
                    );
                    break;
                }
            }
            return Err(refused("Failed", message));
        }
    }
    Ok(())
}

/// Rewrites the paths into moved entries that the moved home's records
/// and run folders hold; answers each rewritten file, relative to `home`.
fn rewrite_records(moved: &Moved, home: &Path) -> Result<Vec<String>> {
    let mut rewritten = Vec::new();
    let mut record = |path: PathBuf, keys: &[&str]| -> Result<()> {
        let Some(mut value) = read_record(&path)? else {
            return Ok(());
        };
        let SdkValue::Map(fields) = &mut value else {
            return Ok(());
        };
        let mut changed = false;
        for (key, item) in fields.iter_mut() {
            if keys.contains(&key.as_str()) {
                changed |= moved.rewrite(item);
            }
        }
        if changed {
            write_record(&path, &value)?;
            rewritten.push(relative(home, &path));
        }
        Ok(())
    };
    let machines = paths::names(&home.join("machine")).map_err(failed)?;
    for machine in &machines {
        record(
            paths::machine_record(home, machine),
            &["daemon", "image_archive"],
        )?;
    }
    for sandbox in paths::names(&home.join("sandbox")).map_err(failed)? {
        record(
            paths::sandbox_dir(home, &sandbox).join("record.json"),
            &["daemon"],
        )?;
    }
    for machine in &machines {
        let run = paths::machine_dir(home, machine).join("run");
        for folder in paths::names(&run).map_err(failed)? {
            let owner = run.join(folder).join("owner");
            if rewrite_owner(moved, &owner)? {
                rewritten.push(relative(home, &owner));
            }
        }
    }
    Ok(rewritten)
}

/// Rewrites one run folder's `owner` marker (a regular file, never
/// followed) when it names a moved path: a new 0600 file renamed over it.
fn rewrite_owner(moved: &Moved, owner: &Path) -> Result<bool> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    match std::fs::symlink_metadata(owner) {
        Ok(meta) if meta.is_file() => {}
        _ => return Ok(false),
    }
    let text = std::fs::read(owner).map_err(failed)?;
    let Some(new) = moved.mapped(&String::from_utf8_lossy(&text)) else {
        return Ok(false);
    };
    let temp = owner.with_file_name(format!(".owner.{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(failed)?;
    file.write_all(new.as_bytes()).map_err(failed)?;
    drop(file);
    std::fs::rename(&temp, owner).map_err(failed)?;
    Ok(true)
}

/// `path` relative to `home`, as the answer shows it.
fn relative(home: &Path, path: &Path) -> String {
    path.strip_prefix(home)
        .unwrap_or(path)
        .display()
        .to_string()
}
