//! A machine's Sandbox images (`machine.images`, `machine.prune`), read and
//! removed on the side that runs its Sandboxes, without starting it: the
//! unpacked images with their sidecars, and what the archives its Sandboxes
//! read still hold (the bundle's `sandbox-image.tar`, a VM's own archive),
//! even when not unpacked yet. The host adds how many Sandbox records name
//! each image, and the host's Docker images the machine could copy
//! (`available`, read through the Docker the copy uses). macOS native runs
//! no image (`mode: 'host'`).
use crate::deploy::{self, Choice, Side};
use crate::image_artifact::{self as cache, Sidecar};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::names::MachineName;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_sandbox_backend::rootfs;
use layerfs_sandbox_backend::Image;
use layerfs_store::place::{self, Executor};
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
        Some(SdkValue::List(rows)) => rows
            .iter()
            .map(|row| {
                let n = count(row.get_text("id").unwrap_or(""));
                layerfs_home::home::with_field(row.clone(), "sandboxes", SdkValue::Int(n))
            })
            .collect(),
        _ => Vec::new(),
    };
    // the host's Docker images this machine could copy, minus those it holds
    let held: Vec<(String, Vec<String>)> = images
        .iter()
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
            choice.archive.clone().map_or(SdkValue::Null, SdkValue::Text),
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
    let mut rows: Vec<Sidecar> = cache::sidecars(&dir)?;
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
            if rows.iter().any(|row| row.id == id) {
                continue;
            }
            if let Some(row) = cache::from_archive(side, archive, id)? {
                rows.push(row);
            }
        }
    }
    let images = rows
        .into_iter()
        .map(|row| {
            let mut value = row.to_value();
            if let SdkValue::Map(fields) = &mut value {
                fields.retain(|(name, _)| name != "config");
            }
            value
        })
        .collect();
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
    let mut args = request(machine, &choice);
    args = layerfs_home::home::with_field(
        args,
        "target",
        image.map_or(SdkValue::Null, SdkValue::text),
    );
    args = layerfs_home::home::with_field(args, "unused", SdkValue::Bool(unused));
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

/// Removes the images `args` names on `side`.
fn pruning(side: &Side<'_>, args: &SdkValue) -> Outcome {
    let root = match side {
        Side::Home(root) | Side::Worker(root) => *root,
    };
    let dir = cache::cache(root);
    let choice = choice_of(args);
    let target = args.get_text("target").filter(|t| cache::digest(t));
    let unused = args.get_bool("unused");
    let default = match &choice.image {
        Some(image) => Some(image.clone()),
        None => deploy::archive(side, choice.archive.as_deref())
            .ok()
            .and_then(|path| rootfs::images(&path).ok())
            .filter(|entries| entries.len() == 1)
            .map(|entries| entries[0].digest.as_str().to_owned()),
    };
    let before = cache::free(&dir);
    let candidates: Vec<String> = match target {
        Some(target) => vec![target.to_owned()],
        None => unpacked(&dir)?,
    };
    let lowers = lowers(dir.parent().unwrap_or(&dir))?;
    let mut removed = Vec::new();
    for id in candidates {
        if target.is_none() && unused && default.as_deref() == Some(id.as_str()) {
            continue;
        }
        let hex = &id["sha256:".len()..];
        let held = [dir.join(format!("{hex}.2")), dir.join(hex)];
        if target.is_some() && !held.iter().any(|p| p.exists()) {
            return Err(refused(
                "NotFound",
                format!("machine has no unpacked image {id}"),
            ));
        }
        let image = Image::parse(&id).map_err(|e| refused("InvalidInput", e.to_string()))?;
        let gone = rootfs::remove(&image, &dir, |path| lowers.iter().any(|l| l == path))
            .map_err(failed)?;
        if gone {
            match std::fs::remove_file(cache::sidecar_path(&dir, hex)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(e)),
                _ => {}
            }
            removed.push(SdkValue::Text(id));
        } else if target.is_some() {
            let users = lowers.iter().filter(|l| held.contains(l)).count();
            return Err(refused_with(
                "InUse",
                format!("image {id} is used by a Sandbox on this machine (or one being created)"),
                SdkValue::map([("count", SdkValue::Int(users as i64))]),
            ));
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

/// Every image unpacked in `dir`, the current layout's and older ones'.
fn unpacked(dir: &Path) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ids),
        Err(e) => return Err(failed(e)),
    };
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let hex = name.strip_suffix(".2").unwrap_or(name);
        let id = format!("sha256:{hex}");
        if cache::digest(&id) && entry.path().is_dir() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.sort();
    Ok(ids)
}

/// The image directories the Sandboxes under runtime root `run` are built on:
/// each Sandbox directory's `config/helper.spec` `lower=` line.
fn lowers(run: &Path) -> Result<Vec<std::path::PathBuf>> {
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
