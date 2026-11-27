//! `machine.prune`: a machine's Sandbox images and what copies left,
//! removed on the side that runs its Sandboxes, without starting it, unless
//! a live copy holds an image (`Busy`) or a Sandbox uses it (`InUse`). The
//! listing is `list.rs`.
use super::list::{choice_of, host_mode, host_only, refused_with, request, stopped};
use crate::deploy::{self, Side};
use crate::image::cache;
use crate::image::copy::lock::{ImageLock, PruneGate};
use crate::rootfs::{self, Temporary};
use crate::Image;
use layerfs_home::disk;
use layerfs_home::names::MachineName;
use layerfs_home::reply::outcome::{failed, refused, with_next, Outcome};
use layerfs_home::reply::state::State;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::admin::route::executor::{self, Executor};
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `Busy` for image `id` on `machine`, which a live copy holds: the copy as
/// `op` `{name: load_image, phase, started_at}` when its owner record says.
fn copied(home: &Path, machine: &str, id: &str) -> SdkRefusal {
    let hex = &id["sha256:".len()..];
    let holder = ImageLock::holder(home, machine, hex);
    let phase = holder
        .as_ref()
        .and_then(|h| h.get_text("phase"))
        .unwrap_or("lock");
    let op = State::Busy {
        name: "load_image".to_owned(),
        phase: phase.to_owned(),
        started_ms: holder.as_ref().and_then(|h| h.get_int("since_ms")),
    };
    let refusal = refused(
        "Busy",
        format!(
            "image {id} is being copied to machine {machine} (phase {phase}); nothing was removed"
        ),
    );
    let op = op
        .fields()
        .into_iter()
        .find(|(key, _)| *key == "op")
        .map(|(_, op)| op);
    with_next(refusal, "op", op.unwrap_or_default())
}

/// `InUse` for image `id`, which `count` Sandboxes (at least one) use.
fn used(id: &str, count: usize) -> SdkRefusal {
    refused_with(
        "InUse",
        format!("image {id} is used by a Sandbox on this machine (or one being created)"),
        SdkValue::map([("sandbox_count", SdkValue::Int(count.max(1) as i64))]),
    )
}

/// `machine.prune(name, image?, { unused })`: removes image `image`, or with
/// `unused` every image no Sandbox uses except the machine's default, and
/// what copies left: `{removed: [id], freed, skipped?: [{id, reason}]}`,
/// `freed` measured on the filesystem. A named image a live copy holds is
/// `Busy` with the copy's `op`; one a Sandbox uses is `InUse`
/// (`{sandbox_count}`).
pub fn prune(home: &Path, name: &MachineName, image: Option<&str>, unused: bool) -> Outcome {
    let machine = name.as_str();
    if image.is_none() && !unused {
        return Err(refused(
            "InvalidInput",
            "name an image to remove, or pass `unused` to remove every unused one",
        ));
    }
    if image.is_some_and(|i| !cache::digest(i)) {
        return Err(refused(
            "InvalidInput",
            "machine.prune takes an image id, sha256:<64 hex> (machine.images lists them)",
        ));
    }
    let choice = deploy::choose(home, machine, None)?;
    let executor = executor::on_machine(home, machine)?;
    if host_mode(&executor) {
        return Err(host_only(machine));
    }
    // no copy starts writing until this prune is over, so the copy locks read
    // here are every copy it must leave alone, on a worker machine too
    let _gate = PruneGate::hold(home, machine)?;
    let (locked, lock_errors) = scan_image_locks(&ImageLock::dir(home, machine))?;
    let _target_lock = if let Some(target) = image {
        if let Some(reason) = lock_errors.get(target) {
            return Err(refused("Failed", reason.clone()));
        }
        if locked.contains(target) {
            return Err(copied(home, machine, target));
        }
        Some(
            ImageLock::try_hold(home, machine, target)?
                .ok_or_else(|| copied(home, machine, target))?,
        )
    } else {
        None
    };
    let mut args = request(machine, &choice);
    args = disk::with_field(args, "target", image.map_or(SdkValue::Null, SdkValue::text));
    args = disk::with_field(args, "unused", SdkValue::Bool(unused));
    args = disk::with_field(
        args,
        "locked",
        SdkValue::List(locked.into_iter().map(SdkValue::Text).collect()),
    );
    if !lock_errors.is_empty() {
        args = disk::with_field(
            args,
            "lock_errors",
            SdkValue::List(
                lock_errors
                    .into_iter()
                    .map(|(id, reason)| {
                        SdkValue::map([
                            ("id", SdkValue::text(id)),
                            ("reason", SdkValue::text(reason)),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    if image.is_some() {
        args = disk::with_field(args, "target_guarded", SdkValue::Bool(true));
    }
    match executor {
        Executor::Local => pruning(&Side::Home(home), &args),
        Executor::Worker(mut worker) => worker
            .attempt("machine.prune", args, &mut |_| {})
            .map_err(|unreached| stopped(unreached.0))?,
    }
}

/// `machine.prune` in this worker.
pub fn prune_in_worker(root: &Path, args: SdkValue) -> Outcome {
    MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    pruning(&Side::Worker(root), &args)
}

/// Artifacts present in the image cache directory for one digest hex.
#[derive(Default)]
struct CacheEntry {
    has_unpacked: bool,
    has_sidecar: bool,
    has_part: bool,
    temps: Vec<String>,
}

/// Removes the images `args` names on `side`, plus unheld staged parts,
/// temporary unpack directories, stranded unpacked directories, and dangling
/// sidecars, while never touching live-locked or in-use entries.
fn pruning(side: &Side<'_>, args: &SdkValue) -> Outcome {
    let root = match side {
        Side::Home(root) | Side::Worker(root) => *root,
    };
    let dir = cache::cache(root);
    let choice = choice_of(args);
    let machine = args.get_text("machine").unwrap_or("");
    let target = args.get_text("target").filter(|t| cache::digest(t));
    let unused = args.get_bool("unused");
    let target_guarded = args.get_bool("target_guarded");
    let mut locked: BTreeSet<String> = cache::texts(args.get("locked")).into_iter().collect();
    let mut lock_errors: BTreeMap<String, String> = match args.get("lock_errors") {
        Some(SdkValue::List(items)) => items
            .iter()
            .filter_map(|item| {
                Some((
                    item.get_text("id")?.to_owned(),
                    item.get_text("reason")?.to_owned(),
                ))
            })
            .collect(),
        _ => BTreeMap::new(),
    };
    if !machine.is_empty() {
        let (local_locked, local_errors) = scan_image_locks(&ImageLock::dir(root, machine))?;
        for id in local_locked {
            if !(target_guarded && target == Some(id.as_str())) {
                locked.insert(id);
            }
        }
        for (id, reason) in local_errors {
            if !(target_guarded && target == Some(id.as_str())) {
                lock_errors.insert(id, reason);
            }
        }
    }
    let default = match &choice.image {
        Some(image) => Some(image.clone()),
        None => deploy::archive(side, choice.archive.as_deref())
            .ok()
            .and_then(|path| rootfs::images(&path).ok())
            .filter(|entries| entries.len() == 1)
            .map(|entries| entries[0].digest.as_str().to_owned()),
    };
    let before = cache::free(&dir);
    let target_hex = target.map(|t| &t["sha256:".len()..]);
    let entries = collect_cache_entries(&dir, target_hex)?;
    if target.is_some() && entries.is_empty() {
        return Err(refused(
            "NotFound",
            format!("machine has no unpacked image {}", target.unwrap_or("")),
        ));
    }
    let lowers = lowers(dir.parent().unwrap_or(&dir))?;
    let mut removed = Vec::new();
    let mut skipped = Vec::new();
    for (hex, item) in entries {
        let id = format!("sha256:{hex}");
        if let Some(reason) = lock_errors.get(&id) {
            if target.is_some() {
                return Err(refused("Failed", reason.clone()));
            }
            skipped.push(SdkValue::map([
                ("id", SdkValue::text(&id)),
                ("reason", SdkValue::text(reason)),
            ]));
            continue;
        }
        let held = [dir.join(format!("{hex}.2")), dir.join(&hex)];
        let lower_users = lowers.iter().filter(|l| held.contains(l)).count();
        let img_locked = locked.contains(&id);
        let hold_locked = match is_flock_held(&dir.join(format!("{hex}.2.lock"))) {
            Ok(held) => held,
            Err(err) => {
                if target.is_some() {
                    return Err(err);
                }
                skipped.push(SdkValue::map([
                    ("id", SdkValue::text(&id)),
                    ("reason", SdkValue::text(err.message)),
                ]));
                continue;
            }
        };
        // a live copy's part and image are left whole
        if img_locked {
            if target.is_some() {
                return Err(refused(
                    "Busy",
                    format!("image {id} is being copied to this machine; nothing was removed"),
                ));
            }
            continue;
        }
        // an image a Sandbox uses stays; with `unused`, a part no copy holds
        // beside it goes
        if lower_users > 0 || hold_locked {
            if target.is_some() {
                return Err(used(&id, lower_users));
            }
            if item.has_part && remove_file_if_present(&cache::part(&dir, &hex))? {
                removed.push(SdkValue::Text(id));
            }
            continue;
        }
        let is_default = target.is_none() && unused && default.as_deref() == Some(id.as_str());
        let mut removed_any = false;
        let mut temp_held = false;
        for temp_name in &item.temps {
            match rootfs::remove_temporary(&dir, temp_name).map_err(failed)? {
                Temporary::Removed => removed_any = true,
                Temporary::Absent => {}
                Temporary::Held => {
                    temp_held = true;
                    break;
                }
            }
        }
        if temp_held {
            if target.is_some() {
                return Err(refused_with(
                    "InUse",
                    format!("image {id} is being unpacked on this machine"),
                    SdkValue::map([("sandbox_count", SdkValue::Int(1))]),
                ));
            }
            continue;
        }
        if item.has_part && remove_file_if_present(&cache::part(&dir, &hex))? {
            removed_any = true;
        }
        if is_default {
            if removed_any && !item.has_unpacked {
                removed.push(SdkValue::Text(id));
            }
            continue;
        }
        if item.has_unpacked {
            let image = Image::parse(&id).map_err(|e| refused("InvalidInput", e.to_string()))?;
            let gone = rootfs::remove(&image, &dir, |path| lowers.iter().any(|l| l == path))
                .map_err(failed)?;
            if gone {
                removed_any = true;
            } else {
                if target.is_some() {
                    return Err(used(&id, lower_users));
                }
                continue;
            }
        }
        if item.has_sidecar && remove_file_if_present(&cache::sidecar_path(&dir, &hex))? {
            removed_any = true;
        }
        if removed_any {
            removed.push(SdkValue::Text(id));
        }
    }
    let after = cache::free(&dir);
    let freed = match (before, after) {
        (Some(before), Some(after)) => after.saturating_sub(before) as i64,
        _ => 0,
    };
    let mut out = vec![
        ("removed".to_owned(), SdkValue::List(removed)),
        ("freed".to_owned(), SdkValue::Int(freed)),
    ];
    if !skipped.is_empty() {
        out.push(("skipped".to_owned(), SdkValue::List(skipped)));
    }
    Ok(SdkValue::Map(out))
}

/// Scans `dir` for unpacked images, sidecars, staged `.part` files, and temporary
/// `.tmp-*` directories, optionally filtered to a single `target_hex`.
fn collect_cache_entries(
    dir: &Path,
    target_hex: Option<&str>,
) -> Result<BTreeMap<String, CacheEntry>> {
    let mut found: BTreeMap<String, CacheEntry> = BTreeMap::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(failed(e)),
    };
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let fname = entry.file_name();
        let Some(name) = fname.to_str() else { continue };
        if let Some(hex) = cache::part_hex(name) {
            if target_hex.is_none_or(|t| t == hex) {
                found.entry(hex.to_owned()).or_default().has_part = true;
            }
        } else if let Some(hex) = rootfs::temporary_digest(name) {
            if target_hex.is_none_or(|t| t == hex) {
                found
                    .entry(hex.to_owned())
                    .or_default()
                    .temps
                    .push(name.to_owned());
            }
        } else if let Some(hex) = name.strip_suffix(".2.json").filter(|h| cache::is_hex(h)) {
            if target_hex.is_none_or(|t| t == hex) {
                found.entry(hex.to_owned()).or_default().has_sidecar = true;
            }
        } else {
            let hex = name.strip_suffix(".2").unwrap_or(name);
            if cache::is_hex(hex)
                && target_hex.is_none_or(|t| t == hex)
                && entry.file_type().is_ok_and(|t| t.is_dir())
            {
                found.entry(hex.to_owned()).or_default().has_unpacked = true;
            }
        }
    }
    Ok(found)
}

fn remove_file_if_present(path: &Path) -> Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(failed(e)),
    }
}

/// Checks whether an `flock` exists and is currently held on `path` without
/// creating or mutating any file on disk.
fn is_flock_held(path: &Path) -> Result<bool> {
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(failed(e)),
    };
    let acquired = layerfs_home::lock::flock::try_lock(&file, true).map_err(failed)?;
    if acquired {
        layerfs_home::lock::flock::unlock(&file);
        Ok(false)
    } else {
        Ok(true)
    }
}

/// Scans `lock_dir` for `ImageLock` files held by live processes and per-image
/// lock probe errors (e.g. a symlink or unreadable `.lock` file).
pub(crate) fn scan_image_locks(
    lock_dir: &Path,
) -> Result<(BTreeSet<String>, BTreeMap<String, String>)> {
    let mut locked = BTreeSet::new();
    let mut errors = BTreeMap::new();
    let entries = match std::fs::read_dir(lock_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok((locked, errors)),
        Err(e) => return Err(failed(e)),
    };
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let fname = entry.file_name();
        let Some(name) = fname.to_str() else { continue };
        let Some(hex) = name.strip_suffix(".lock").filter(|h| cache::is_hex(h)) else {
            continue;
        };
        let id = format!("sha256:{hex}");
        match is_flock_held(&entry.path()) {
            Ok(true) => {
                locked.insert(id);
            }
            Ok(false) => {}
            Err(err) => {
                errors.insert(id, err.message);
            }
        }
    }
    Ok((locked, errors))
}

/// The image directories the Sandboxes under runtime root `run` are built on:
/// each Sandbox directory's `config/helper.spec` `lower=` line.
pub(crate) fn lowers(run: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(run) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(failed(e)),
    };
    for entry in entries {
        let spec = entry.map_err(failed)?.path().join("config/helper.spec");
        let Ok(text) = std::fs::read_to_string(&spec) else {
            continue;
        };
        for line in text.lines() {
            if let Some(lower) = line.strip_prefix("lower=") {
                found.push(std::path::PathBuf::from(lower));
            }
        }
    }
    Ok(found)
}
