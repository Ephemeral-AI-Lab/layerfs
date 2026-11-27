//! A machine's Sandbox images (`machine.images`), read on the side that
//! runs its Sandboxes, without starting it (their removal, `machine.prune`,
//! is `prune.rs`): the
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
use crate::image::cache::{self as cache, SidecarInfo};
use crate::image::copy::lock::ImageLock;
use crate::rootfs;
use layerfs_home::home;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::state::{Reason, State};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::place::{self, Executor};
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
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
/// What the refusal already carries stays beside `what`, its `fix` (`layerfs
/// machine start <m>`) among it (RC-B F2).
pub fn stopped(refusal: SdkRefusal) -> SdkRefusal {
    let detail = layerfs_home::home::with_field(refusal.detail, "what", SdkValue::text("machine"));
    SdkRefusal { detail, ..refusal }
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
    let docker = crate::image::copy::docker::Docker::of(home)?;
    let (docker, available) =
        crate::image::copy::docker::candidates(&docker, &architecture, free, &held);
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
                Some(&format!(
                    "layerfs machine load-image {machine} {id}",
                    machine = shell_word(machine),
                    id = shell_word(id)
                )),
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
pub(crate) fn request(machine: &str, choice: &Choice) -> SdkValue {
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
pub(crate) fn choice_of(args: &SdkValue) -> Choice {
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
