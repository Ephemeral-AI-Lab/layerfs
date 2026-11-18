//! The export containers killed image copies left in the host's Docker: a
//! `home leftovers` entry of kind `export_container`, which `home prune`
//! removes (call surface 2.0, spec v19). They are the containers labelled as
//! this home's image exports (`layerfs.role=image-export`), and the legacy
//! `layerfs-export-<machine>-<hex>` containers of builds before the labels.
//! A container is removable while no copy of its image to its machine holds
//! the image's [`ImageLock`]; the listing reads that lock's owner record
//! ([`ImageLock::holder`]) and the removal takes the lock without waiting.
use crate::docker::{Docker, Failure};
use crate::image_artifact as cache;
use crate::image_load::{existing_home_id, owned_for_home_id, remove_container, EXPORT_ROLE};
use crate::image_lock::ImageLock;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;
use std::path::Path;
use std::time::Duration;

/// How long one listing read of Docker may take: a `home leftovers` walk is
/// bounded to 2 s per side.
const LIST: Duration = Duration::from_secs(2);

/// The prefix every export container's name has.
const PREFIX: &str = "layerfs-export-";

/// One export container a killed copy left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftExport {
    /// Its Docker name: the id's last part.
    pub name: String,
    /// The machine its copy went to.
    pub machine: String,
    /// The image it exported, `sha256:<hex>`.
    pub image: String,
    /// When Docker created it, in ms since 1970, when Docker said.
    pub created_ms: Option<i64>,
    /// Whether a live copy of its image to its machine holds the image's lock.
    pub held: bool,
}

/// Whether `name` is a name an export container is given: the prefix, then
/// letters, digits, `_`, `.` and `-` only.
pub fn admits(name: &str) -> bool {
    name.len() <= 255
        && name.strip_prefix(PREFIX).is_some_and(|rest| {
            !rest.is_empty()
                && (rest.bytes())
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        })
}

/// The machine and image a legacy container's name `layerfs-export-<m>-<hex>`
/// gives (its image's 64 hex digits last).
fn legacy(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix(PREFIX)?;
    let (machine, hex) = rest.rsplit_once('-')?;
    (cache::is_hex(hex) && layerfs_home::names::name("machine", machine).is_ok())
        .then(|| (machine.to_owned(), format!("sha256:{hex}")))
}

/// Every export container a killed copy left: this home's labelled ones and
/// the legacy unlabelled ones, by name. A Docker that does not answer is the
/// caller's to report.
pub fn left_exports(docker: &Docker, home: &Path) -> Result<Vec<LeftExport>, Failure> {
    // one read first: a Docker that is not there or does not answer says
    // so here, with its own failure
    let filter = format!("name={PREFIX}");
    let role = "{{.Names}}\t{{.Label \"layerfs.role\"}}";
    let listed = docker.read(&["ps", "-a", "--filter", &filter, "--format", role], LIST)?;
    let mut found: Vec<(String, String, String)> = Vec::new();
    if let Ok(Some(home_id)) = existing_home_id(home) {
        let owned = owned_for_home_id(docker, &home_id, None, None)
            .map_err(|refusal| Failure::Error(refusal.message))?;
        found.extend(owned.into_iter().map(|c| (c.name, c.machine, c.image)));
    }
    for line in listed.lines() {
        let (name, label) = line.split_once('\t').unwrap_or((line, ""));
        if label != EXPORT_ROLE && !found.iter().any(|(held, ..)| held == name) {
            if let Some((machine, image)) = legacy(name) {
                found.push((name.to_owned(), machine, image));
            }
        }
    }
    found.sort();
    let created = created(docker, found.iter().map(|(name, ..)| name.as_str()));
    Ok(found
        .into_iter()
        .map(|(name, machine, image)| {
            let hex = &image["sha256:".len()..];
            LeftExport {
                created_ms: created
                    .iter()
                    .find(|(n, _)| *n == name)
                    .and_then(|(_, at)| *at),
                held: ImageLock::holder(home, &machine, hex).is_some(),
                name,
                machine,
                image,
            }
        })
        .collect())
}

/// When Docker created each of `names`, in one `docker inspect`.
fn created<'a>(
    docker: &Docker,
    names: impl Iterator<Item = &'a str>,
) -> Vec<(String, Option<i64>)> {
    let names: Vec<&str> = names.collect();
    if names.is_empty() {
        return Vec::new();
    }
    let mut args = vec![
        "container",
        "inspect",
        "--format",
        "{{.Name}}\t{{.Created}}",
    ];
    args.extend(names.iter().copied());
    let Ok(out) = docker.read(&args, LIST) else {
        return Vec::new();
    };
    (out.lines())
        .filter_map(|line| {
            let (name, at) = line.split_once('\t')?;
            Some((
                name.trim_start_matches('/').to_owned(),
                rfc3339_ms(at.trim()),
            ))
        })
        .collect()
}

/// `2026-11-19T09:58:35.123456789Z` (Docker's `Created`, UTC) in ms since 1970.
fn rfc3339_ms(text: &str) -> Option<i64> {
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !text.ends_with('Z') || !(1..=12).contains(&month) {
        return None;
    }
    // days from civil (proleptic Gregorian), Howard Hinnant's algorithm
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400) + hour * 3_600 + minute * 60 + second) * 1_000)
}

/// Removes export container `name` with `docker rm -v`, found again among
/// [`left_exports`] (`NotFound` when it is gone), under its image's copy
/// lock taken without waiting (`InUse` while a copy holds it).
pub fn remove_left_export(docker: &Docker, home: &Path, name: &str) -> Result<(), SdkRefusal> {
    let gone = || refused("NotFound", "no such leftover");
    let listed = left_exports(docker, home).map_err(|failure| failure.refusal(""))?;
    let left = listed
        .into_iter()
        .find(|left| left.name == name)
        .ok_or_else(gone)?;
    // a guard, not a copy: it writes no owner record, so a listing meanwhile
    // does not show a copy that is not running
    let Some(_lock) = ImageLock::try_hold(home, &left.machine, &left.image)? else {
        return Err(refused(
            "InUse",
            format!(
                "a copy of image {} to machine {} holds its lock and uses this container; nothing was removed",
                left.image, left.machine
            ),
        ));
    };
    match remove_container(docker, &left.name) {
        true => Ok(()),
        false => Err(refused(
            "Failed",
            format!("docker rm -v {} did not remove it", left.name),
        )),
    }
}

/// Whether `failure` means Docker is not installed here: then nothing is
/// listed and nothing is named as not listed.
pub fn not_installed(failure: &Failure) -> bool {
    matches!(failure, Failure::NotInstalled)
}
