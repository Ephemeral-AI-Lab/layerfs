//! A machine's Sandbox images (`machine.images`, `machine.prune`), read and
//! removed on the side that runs its Sandboxes, without starting it: the
//! unpacked images with their sidecars, what the archives its Sandboxes
//! read still hold (the bundle's `sandbox-image.tar`, a VM's own archive),
//! even when not unpacked yet, and what copies left (a staged part, an
//! unpack that was not recorded). The host merges those into one item per
//! image with its state (spec v19): `ready` when published, `busy` while a
//! live copy holds the image's lock (read from the lock's owner record),
//! else `broken` `partial`. It adds the Sandbox records that name each
//! image, and the host's Docker images the machine could copy
//! (`available`, read through the Docker the copy uses). macOS native runs
//! no image (`mode: 'host'`).
use crate::deploy::{self, Choice, Side};
use crate::image_artifact::{self as cache, SidecarInfo};
use crate::image_lock::{ImageLock, PruneGate};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::{failed, refused, with_next, Outcome};
use layerfs_home::state::{Reason, State};
use layerfs_sandbox_backend::rootfs::{self, Temporary};
use layerfs_sandbox_backend::Image;
use layerfs_store::place::{self, Executor};
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

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
/// size, architecture, source, last_used, state, op, reason, fix, message,
/// staged_bytes, sandboxes}]}`, one item per image, `sandboxes` the names
/// of the Sandbox records that name the image. The host's Docker
/// (`docker`, `available`) is read only when `docker` asks for it; an image
/// the machine does not hold `ready` is offered there.
pub fn list(home: &Path, name: &MachineName, docker: bool) -> Outcome {
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
    let users = |id: &str| {
        let on = records.iter();
        let using = on.filter(|r| r.machine() == machine && r.image() == Some(id));
        SdkValue::List(using.map(|r| SdkValue::text(r.name())).collect())
    };
    let images: Vec<SdkValue> = merged(answer.get("images"))
        .into_iter()
        .map(|(id, found)| {
            home::with_field(item(home, machine, &id, found), "sandboxes", users(&id))
        })
        .collect();
    if !docker {
        return Ok(home::with_field(answer, "images", SdkValue::List(images)));
    }
    // the host's Docker images this machine could copy, minus those it holds ready
    let held: Vec<(String, Vec<String>)> = images
        .iter()
        .filter(|row| row.get_text("state") == Some("ready"))
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

/// What a machine's listing found of one image: its published row, the
/// bytes of a staged part, an unpack that was not recorded.
#[derive(Default)]
struct Found {
    published: Option<SdkValue>,
    part: Option<i64>,
    stranded: bool,
}

/// A machine's listing rows by image id: a part or a stranded unpack beside
/// a published image is merged into its row.
fn merged(rows: Option<&SdkValue>) -> BTreeMap<String, Found> {
    let mut found: BTreeMap<String, Found> = BTreeMap::new();
    let Some(SdkValue::List(rows)) = rows else {
        return found;
    };
    for row in rows {
        let Some(id) = row.get_text("id").filter(|id| cache::digest(id)) else {
            continue;
        };
        let entry = found.entry(id.to_owned()).or_default();
        if row.get_bool("staged") {
            entry.part = row.get_int("size");
        } else if row.get_bool("stranded") {
            entry.stranded = true;
        } else {
            entry.published = Some(row.clone());
        }
    }
    found
}

/// One image's item and state: `ready` when published (a stale part beside
/// it shows as `staged_bytes`); otherwise `busy` while the copy whose owner
/// record names it lives (`op` `load_image`, its recorded phase and
/// `since`), else `broken` `partial`, whose fix copies it again.
fn item(home: &Path, machine: &str, id: &str, found: Found) -> SdkValue {
    let hex = &id["sha256:".len()..];
    let state = match &found.published {
        Some(_) => State::Ready,
        None => match ImageLock::holder(home, machine, hex) {
            Some(holder) => State::Busy {
                name: "load_image".to_owned(),
                phase: holder.get_text("phase").unwrap_or("lock").to_owned(),
                started_ms: holder.get_int("since_ms"),
            },
            None => State::broken(
                Reason::Partial,
                Some(&format!("layerfs machine load-image {machine} {id}")),
            ),
        },
    };
    let message = match (&state, found.part) {
        (State::Broken { .. }, Some(bytes)) => SdkValue::text(format!(
            "a copy of this image stopped after {bytes} bytes; load-image resumes it"
        )),
        (State::Broken { .. }, None) => SdkValue::text(
            "an unpack of this image stopped before it was recorded; load-image rebuilds it",
        ),
        _ => SdkValue::Null,
    };
    let mut row = found.published.unwrap_or_else(|| {
        SdkValue::map([
            ("id", SdkValue::text(id)),
            ("refs", SdkValue::List(Vec::new())),
            ("size", SdkValue::Null),
            ("architecture", SdkValue::Null),
            ("source", SdkValue::text("imported")),
            ("last_used", SdkValue::Null),
        ])
    });
    for (field, value) in state.fields() {
        row = home::with_field(row, field, value);
    }
    row = home::with_field(row, "message", message);
    let staged = found.part.map_or(SdkValue::Null, SdkValue::Int);
    home::with_field(row, "staged_bytes", staged)
}

/// The bytes of a [`list`] answer's image cache that `machine prune --unused`
/// frees: each `ready` image no Sandbox uses but the default, and what
/// copies left that no copy holds (their staged bytes; an unpack that was
/// not recorded counts nothing, its size unknown).
pub fn reclaimable(listed: &SdkValue) -> u64 {
    let default = listed.get_text("default");
    let Some(SdkValue::List(images)) = listed.get("images") else {
        return 0;
    };
    let bytes = |value: Option<i64>| value.map_or(0, |n| n.max(0) as u64);
    let unused = |row: &SdkValue| {
        matches!(row.get("sandboxes"), Some(SdkValue::List(names)) if names.is_empty())
            && row.get_text("id") != default
    };
    images
        .iter()
        .map(|row| match row.get_text("state") {
            Some("ready") if unused(row) => {
                bytes(row.get_int("size")) + bytes(row.get_int("staged_bytes"))
            }
            Some("ready") | Some("broken") => bytes(row.get_int("staged_bytes")),
            _ => 0,
        })
        .sum()
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
        .map(|row| home::with_field(env_names(row.to_value()), "staged", SdkValue::Bool(false)))
        .collect();
    append_cache_leftovers(&dir, &published, &mut images)?;
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

/// A listing row whose `config.Env` keeps only the variable names: a value can
/// be a secret (`ENV TOKEN=...`) that the answer, its traces and a UI would
/// carry. The sidecar on the machine keeps the whole record.
fn env_names(row: SdkValue) -> SdkValue {
    let Some(SdkValue::Map(config)) = row.get("config").cloned() else {
        return row;
    };
    let name = |var: SdkValue| match var {
        SdkValue::Text(text) => SdkValue::text(text.split('=').next().unwrap_or("")),
        _ => SdkValue::Null,
    };
    let config = config
        .into_iter()
        .map(|(field, value)| match (field.as_str(), value) {
            ("Env", SdkValue::List(vars)) => {
                (field, SdkValue::List(vars.into_iter().map(name).collect()))
            }
            (_, value) => (field, value),
        });
    home::with_field(row, "config", SdkValue::Map(config.collect()))
}

/// Appends what copies left in `dir`: a staged `.recv-<hex>.part` as
/// `{id, size, staged: true}` (its bytes so far), and a `<hex>.2` unpack
/// with no sidecar that no archive holds as `{id, stranded: true}`. Whether
/// a copy still holds them is the host's to read, from the copy's lock.
fn append_cache_leftovers(
    dir: &Path,
    published: &BTreeSet<String>,
    images: &mut Vec<SdkValue>,
) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(failed(e)),
    };
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let fname = entry.file_name();
        let Some(name) = fname.to_str() else { continue };
        if let Some(hex) = cache::part_hex(name) {
            let meta = match std::fs::symlink_metadata(entry.path()) {
                Ok(meta) if meta.file_type().is_file() => meta,
                Ok(_) => continue,
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(failed(e)),
            };
            images.push(SdkValue::map([
                ("id", SdkValue::text(format!("sha256:{hex}"))),
                ("size", SdkValue::Int(meta.len() as i64)),
                ("staged", SdkValue::Bool(true)),
            ]));
        } else if let Some(hex) = name.strip_suffix(".2").filter(|h| cache::is_hex(h)) {
            let id = format!("sha256:{hex}");
            if published.contains(&id) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            images.push(SdkValue::map([
                ("id", SdkValue::text(&id)),
                ("staged", SdkValue::Bool(false)),
                ("stranded", SdkValue::Bool(true)),
            ]));
        }
    }
    Ok(())
}

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
    let executor = place::on_machine(home, machine)?;
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
    args = home::with_field(args, "target", image.map_or(SdkValue::Null, SdkValue::text));
    args = home::with_field(args, "unused", SdkValue::Bool(unused));
    args = home::with_field(
        args,
        "locked",
        SdkValue::List(locked.into_iter().map(SdkValue::Text).collect()),
    );
    if !lock_errors.is_empty() {
        args = home::with_field(
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
    let acquired = home::flock::try_lock(&file, true).map_err(failed)?;
    if acquired {
        home::flock::unlock(&file);
        Ok(false)
    } else {
        Ok(true)
    }
}

/// Scans `lock_dir` for `ImageLock` files held by live processes and per-image
/// lock probe errors (e.g. a symlink or unreadable `.lock` file).
fn scan_image_locks(lock_dir: &Path) -> Result<(BTreeSet<String>, BTreeMap<String, String>)> {
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
