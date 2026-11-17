//! A machine's Sandbox images (`machine.images`, `machine.prune`), read and
//! removed on the side that runs its Sandboxes, without starting it: the
//! unpacked images with their sidecars, and what the archives its Sandboxes
//! read still hold (the bundle's `sandbox-image.tar`, a VM's own archive),
//! even when not unpacked yet. The host adds how many Sandbox records name
//! each image, and the host's Docker images the machine could copy
//! (`available`, read through the Docker the copy uses). macOS native runs
//! no image (`mode: 'host'`).
use crate::deploy::{self, Choice, Side};
use crate::image_artifact::{self as cache, SidecarInfo};
use crate::image_load::ImageLock;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_sandbox_backend::rootfs::{self, Temporary};
use layerfs_sandbox_backend::Image;
use layerfs_store::place::{self, Executor};
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::SystemTime;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A refusal with a machine-readable `detail`.
pub fn refused_with(code: &str, message: impl Into<String>, detail: SdkValue) -> SdkRefusal {
    SdkRefusal {
        detail,
        ..refused(code, message)
    }
}

/// `Unsupported` `{reason: 'host'}`: macOS native runs Sandboxes on the Mac
/// itself, with no image.
pub fn host_only(machine: &str) -> SdkRefusal {
    refused_with(
        "Unsupported",
        format!("machine {machine} runs Sandboxes as host processes; it has no Sandbox images"),
        SdkValue::map([("reason", SdkValue::text("host"))]),
    )
}

/// `Unavailable` `{what: 'machine'}`: the machine's worker was not reached.
pub fn stopped(refusal: SdkRefusal) -> SdkRefusal {
    SdkRefusal {
        detail: SdkValue::map([("what", SdkValue::text("machine"))]),
        ..refusal
    }
}

/// Whether Sandboxes run by `executor` are host processes (macOS native).
pub fn host_mode(executor: &Executor) -> bool {
    matches!(executor, Executor::Local) && cfg!(target_os = "macos")
}

/// `machine.images`: `{machine, mode, default, free, images: [{id, refs,
/// size, architecture, source, sandboxes, last_used}]}`.
pub fn list(home: &Path, name: &MachineName) -> Outcome {
    let machine = name.as_str();
    let choice = deploy::choose(home, machine, None)?;
    let executor = place::on_machine(home, machine)?;
    if host_mode(&executor) {
        return Ok(SdkValue::map([
            ("machine", SdkValue::text(machine)),
            ("mode", SdkValue::text("host")),
            ("architecture", SdkValue::Null),
            ("default", SdkValue::Null),
            ("free", SdkValue::Null),
            ("images", SdkValue::List(Vec::new())),
            ("docker", SdkValue::Null),
            ("available", SdkValue::List(Vec::new())),
        ]));
    }
    let answer = match executor {
        Executor::Local => listing(machine, &Side::Home(home), &choice)?,
        Executor::Worker(mut worker) => worker
            .attempt("machine.images", request(machine, &choice), &mut |_| {})
            .map_err(|unreached| stopped(unreached.0))??,
    };
    let records = crate::record::sandboxes(home)?;
    let count = |id: &str| {
        records
            .iter()
            .filter(|r| r.machine() == machine && r.image() == Some(id))
            .count() as i64
    };
    let images = match answer.get("images") {
        Some(SdkValue::List(rows)) => {
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let id = row.get_text("id").unwrap_or("");
                let mut enriched =
                    home::with_field(row.clone(), "sandboxes", SdkValue::Int(count(id)));
                if enriched.get_bool("staged") {
                    if let Some(hex) = id.strip_prefix("sha256:").filter(|h| cache::is_hex(h)) {
                        if is_flock_held(&ImageLock::path(home, machine, hex))? {
                            enriched =
                                home::with_field(enriched, "resumable", SdkValue::Bool(false));
                        }
                    }
                }
                out.push(enriched);
            }
            out
        }
        _ => Vec::new(),
    };
    // the host's Docker images this machine could copy, minus those it holds completely
    let held: Vec<(String, Vec<String>)> = images
        .iter()
        .filter(|row| !row.get_bool("staged") && !row.get_bool("stranded"))
        .map(|row| {
            (
                row.get_text("id").unwrap_or("").to_owned(),
                cache::texts(row.get("refs")),
            )
        })
        .collect();
    let architecture = answer
        .get_text("architecture")
        .unwrap_or(cache::architecture())
        .to_owned();
    let free = answer.get_int("free").map(|n| n.max(0) as u64);
    let docker = crate::docker::Docker::of(home)?;
    let (docker, available) = crate::docker::candidates(&docker, &architecture, free, &held);
    let answer = layerfs_home::home::with_field(answer, "images", SdkValue::List(images));
    let answer = layerfs_home::home::with_field(answer, "docker", docker);
    Ok(layerfs_home::home::with_field(
        answer,
        "available",
        SdkValue::List(available),
    ))
}

/// The worker request that carries what the machine's record chooses.
fn request(machine: &str, choice: &Choice) -> SdkValue {
    SdkValue::map([
        ("machine", SdkValue::text(machine)),
        (
            "archive",
            choice
                .archive
                .clone()
                .map_or(SdkValue::Null, SdkValue::Text),
        ),
        (
            "image",
            choice.image.clone().map_or(SdkValue::Null, SdkValue::Text),
        ),
    ])
}

/// The choice a worker request carries.
fn choice_of(args: &SdkValue) -> Choice {
    Choice {
        archive: args.get_text("archive").map(str::to_owned),
        image: args.get_text("image").map(str::to_owned),
        platform: None,
    }
}

/// `machine.images` in this worker, without home records.
pub fn in_worker(root: &Path, args: SdkValue) -> Outcome {
    let machine = args.get_text("machine").unwrap_or("");
    MachineName::parse(machine)?;
    listing(machine, &Side::Worker(root), &choice_of(&args))
}

/// Every image `side` holds or can unpack, its default and its free bytes.
fn listing(machine: &str, side: &Side<'_>, choice: &Choice) -> Outcome {
    let root = match side {
        Side::Home(root) | Side::Worker(root) => *root,
    };
    let dir = cache::cache(root);
    let mut rows: Vec<SidecarInfo> = cache::sidecar_infos(&dir)?;
    // the archives a Sandbox here reads: the machine's own, and the bundle's
    let mut archives = Vec::new();
    if choice.archive.is_some() {
        archives.push(choice.archive.as_deref());
    }
    archives.push(None);
    let mut default = choice.image.clone();
    for (index, archive) in archives.into_iter().enumerate() {
        let path = deploy::archive(side, archive)?;
        let Ok(entries) = rootfs::images(&path) else {
            continue;
        };
        if index == 0 && default.is_none() && entries.len() == 1 {
            default = Some(entries[0].digest.as_str().to_owned());
        }
        for entry in entries {
            let id = entry.digest.as_str();
            if rows.iter().any(|row| row.sidecar.id == id) {
                continue;
            }
            if let Some(row) = cache::from_archive(side, archive, id)? {
                rows.push(SidecarInfo {
                    sidecar: row,
                    variant: None,
                });
            }
        }
    }
    let published: BTreeSet<String> = rows.iter().map(|r| r.sidecar.id.clone()).collect();
    let mut images: Vec<SdkValue> = rows
        .into_iter()
        .map(|row| home::with_field(row.to_value(), "staged", SdkValue::Bool(false)))
        .collect();
    append_cache_leftovers(root, machine, &dir, &published, &mut images)?;
    images.sort_by(|a, b| {
        a.get_text("id")
            .unwrap_or("")
            .cmp(b.get_text("id").unwrap_or(""))
            .then_with(|| a.get_bool("staged").cmp(&b.get_bool("staged")))
    });
    Ok(SdkValue::map([
        ("machine", SdkValue::text(machine)),
        ("mode", SdkValue::text("images")),
        ("architecture", SdkValue::text(cache::architecture())),
        ("default", default.map_or(SdkValue::Null, SdkValue::Text)),
        (
            "free",
            cache::free(&dir).map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
        ),
        ("images", SdkValue::List(images)),
    ]))
}

/// Appends staged `.recv-<hex>.part` rows and stranded `<hex>.2` rows found in `dir`.
fn append_cache_leftovers(
    root: &Path,
    machine: &str,
    dir: &Path,
    published: &BTreeSet<String>,
    images: &mut Vec<SdkValue>,
) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(failed(e)),
    };
    let now = SystemTime::now();
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let fname = entry.file_name();
        let Some(name) = fname.to_str() else { continue };
        if let Some(hex) = cache::part_hex(name) {
            let meta = match std::fs::symlink_metadata(entry.path()) {
                Ok(meta) => meta,
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(failed(e)),
            };
            let id = format!("sha256:{hex}");
            let age = meta
                .modified()
                .ok()
                .and_then(|mtime| now.duration_since(mtime).ok())
                .map_or(0, |d| d.as_secs() as i64);
            let lock_held = is_flock_held(&dir.join(format!("{hex}.2.lock")))?
                || is_flock_held(&ImageLock::path(root, machine, hex))?;
            let resumable = meta.file_type().is_file()
                && meta.len() > 0
                && !published.contains(&id)
                && !lock_held;
            images.push(SdkValue::map([
                ("id", SdkValue::text(&id)),
                ("refs", SdkValue::List(Vec::new())),
                ("size", SdkValue::Int(meta.len() as i64)),
                ("age", SdkValue::Int(age)),
                ("resumable", SdkValue::Bool(resumable)),
                ("architecture", SdkValue::Null),
                ("source", SdkValue::text("staged")),
                ("variant", SdkValue::Null),
                ("config", SdkValue::Null),
                ("last_used", SdkValue::Null),
                ("staged", SdkValue::Bool(true)),
            ]));
        } else if let Some(hex) = name.strip_suffix(".2").filter(|h| cache::is_hex(h)) {
            let id = format!("sha256:{hex}");
            if published.contains(&id) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            images.push(SdkValue::map([
                ("id", SdkValue::text(&id)),
                ("refs", SdkValue::List(Vec::new())),
                ("size", SdkValue::Null),
                ("architecture", SdkValue::Null),
                ("source", SdkValue::text("stranded")),
                ("variant", SdkValue::Null),
                ("config", SdkValue::Null),
                ("last_used", SdkValue::Null),
                ("staged", SdkValue::Bool(false)),
                ("stranded", SdkValue::Bool(true)),
            ]));
        }
    }
    Ok(())
}

/// `machine.prune(name, image?, { unused })`: removes image `image`, or with
/// `unused` every image no Sandbox uses except the machine's default:
/// `{removed: [id], freed}`, `freed` measured on the filesystem.
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
    let executor = place::on_machine(home, machine)?;
    if host_mode(&executor) {
        return Err(host_only(machine));
    }
    let locked = locked_image_ids(&ImageLock::dir(home, machine))?;
    let _target_lock = if let Some(target) = image {
        if locked.contains(target) {
            return Err(refused_with(
                "InUse",
                format!("image {target} has an active copy lock on this machine"),
                SdkValue::map([("count", SdkValue::Int(1))]),
            ));
        }
        Some(ImageLock::try_take(home, machine, target)?.ok_or_else(|| {
            refused_with(
                "InUse",
                format!("image {target} has an active copy lock on this machine"),
                SdkValue::map([("count", SdkValue::Int(1))]),
            )
        })?)
    } else {
        None
    };
    let mut args = request(machine, &choice);
    args = home::with_field(args, "target", image.map_or(SdkValue::Null, SdkValue::text));
    args = home::with_field(args, "unused", SdkValue::Bool(unused));
    args = home::with_field(
        args,
        "locked",
        SdkValue::List(locked.into_iter().map(SdkValue::Text).collect()),
    );
    if image.is_some() {
        args = home::with_field(args, "target_guarded", SdkValue::Bool(true));
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
    if !machine.is_empty() {
        for id in locked_image_ids(&ImageLock::dir(root, machine))? {
            if !(target_guarded && target == Some(id.as_str())) {
                locked.insert(id);
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
    for (hex, item) in entries {
        let id = format!("sha256:{hex}");
        let held = [dir.join(format!("{hex}.2")), dir.join(&hex)];
        let lower_users = lowers.iter().filter(|l| held.contains(l)).count();
        let img_locked = locked.contains(&id);
        let hold_locked = is_flock_held(&dir.join(format!("{hex}.2.lock")))?;
        if img_locked || lower_users > 0 || hold_locked {
            if target.is_some() {
                let count = lower_users.max(1);
                return Err(refused_with(
                    "InUse",
                    format!(
                        "image {id} is used by a Sandbox on this machine (or one being created)"
                    ),
                    SdkValue::map([("count", SdkValue::Int(count as i64))]),
                ));
            }
            continue;
        }
        let is_default = target.is_none() && unused && default.as_deref() == Some(id.as_str());
        let _local_guard = if target.is_none()
            && matches!(side, Side::Home(_))
            && !machine.is_empty()
            && ImageLock::dir(root, machine).is_dir()
        {
            match ImageLock::try_take(root, machine, &id)? {
                Some(guard) => Some(guard),
                None => continue,
            }
        } else {
            None
        };
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
                    SdkValue::map([("count", SdkValue::Int(1))]),
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
                    let count = lower_users.max(1);
                    return Err(refused_with(
                        "InUse",
                        format!(
                            "image {id} is used by a Sandbox on this machine (or one being created)"
                        ),
                        SdkValue::map([("count", SdkValue::Int(count as i64))]),
                    ));
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
    Ok(SdkValue::map([
        ("removed", SdkValue::List(removed)),
        ("freed", SdkValue::Int(freed)),
    ]))
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
    let acquired = home::flock::try_lock(&file, true).map_err(failed)?;
    if acquired {
        home::flock::unlock(&file);
        Ok(false)
    } else {
        Ok(true)
    }
}

/// Every `sha256:<hex>` image digest whose `ImageLock` file in `lock_dir` is
/// held by a live process or open file description.
fn locked_image_ids(lock_dir: &Path) -> Result<BTreeSet<String>> {
    let mut locked = BTreeSet::new();
    let entries = match std::fs::read_dir(lock_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(locked),
        Err(e) => return Err(failed(e)),
    };
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let fname = entry.file_name();
        let Some(name) = fname.to_str() else { continue };
        let Some(hex) = name.strip_suffix(".lock").filter(|h| cache::is_hex(h)) else {
            continue;
        };
        if is_flock_held(&entry.path())? {
            locked.insert(format!("sha256:{hex}"));
        }
    }
    Ok(locked)
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
