//! Every record of a layout 3 home, read once: whether it parses, whether
//! an entity record has `"v": 1`, and the modes of the home's folders
//! (0700, or 0701 where a namespaces Sandbox passes through) and record
//! files (0600). A Store folder with projects and no Store record is
//! reported here too.
use super::report::Findings;
use crate::common::home::{paths, read_record, RECORD_VERSION};
use crate::common::outcome::failed;
use crate::machine::record::MachineRecord;
use crate::sandbox::record::SandboxRecord;
use crate::store::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What the scan read.
#[derive(Default)]
pub(super) struct Scanned {
    /// Machines whose record parses.
    pub(super) machines: Vec<MachineRecord>,
    /// Machines with a record file, parsing or not.
    pub(super) machine_names: BTreeSet<String>,
    /// Stores whose record parses.
    pub(super) stores: Vec<StoreRecord>,
    /// Stores with a record file, parsing or not.
    pub(super) store_names: BTreeSet<String>,
    /// Projects (`store`, `project`) with a record file, parsing or not.
    pub(super) projects: BTreeSet<(String, String)>,
    /// Sandboxes whose record parses.
    pub(super) sandboxes: Vec<SandboxRecord>,
}

fn names(dir: &Path) -> Result<Vec<String>> {
    paths::names(dir).map_err(failed)
}

/// Reads the home's records and modes.
pub(super) fn scan(home: &Path, findings: &mut Findings) -> Result<Scanned> {
    let mut scanned = Scanned::default();
    let shown = home.display().to_string();
    folder(home, ("home", &shown), findings);
    file_mode(&home.join("layerfs.json"), ("home", &shown), findings);
    for top in ["machine", "store", "sandbox", "backup"] {
        folder(&home.join(top), ("home", &shown), findings);
    }
    for name in names(&home.join("machine"))? {
        let entity = ("machine", name.as_str());
        folder(&paths::machine_dir(home, &name), entity, findings);
        let path = paths::machine_record(home, &name);
        if path.exists() {
            scanned.machine_names.insert(name.clone());
        }
        if let Some(value) = record(&path, entity, true, findings) {
            scanned
                .machines
                .push(MachineRecord::from_record(&name, value));
        }
        record(&paths::worker_record(home, &name), entity, false, findings);
    }
    for name in names(&home.join("store"))? {
        store(home, &name, &mut scanned, findings)?;
    }
    for name in names(&home.join("sandbox"))? {
        sandbox(home, &name, &mut scanned, findings)?;
    }
    for name in names(&home.join("backup"))? {
        let entity = ("bucket", name.as_str());
        let path = paths::bucket_record(home, &name);
        folder(path.parent().unwrap_or(home), entity, findings);
        record(&path, entity, true, findings);
    }
    Ok(scanned)
}

/// One Store's folder: its record and its projects'.
fn store(home: &Path, name: &str, scanned: &mut Scanned, findings: &mut Findings) -> Result<()> {
    let entity = ("store", name);
    store_folder(&paths::store_dir(home, name), entity, findings);
    let path = paths::store_record(home, name);
    let has_record = path.exists();
    if has_record {
        scanned.store_names.insert(name.to_owned());
    }
    if let Some(value) = record(&path, entity, true, findings) {
        scanned.stores.push(StoreRecord::from_record(name, value));
    }
    let projects = paths::store_dir(home, name).join("project");
    folder(&projects, entity, findings);
    for project in names(&projects)? {
        let full = format!("{name}/{project}");
        let entity = ("project", full.as_str());
        folder(&projects.join(&project), entity, findings);
        let path = paths::project_record(home, name, &project);
        if path.exists() {
            scanned.projects.insert((name.to_owned(), project.clone()));
        }
        record(&path, entity, true, findings);
        if !has_record {
            findings.error(
                entity,
                format!("project {full} is in store/{name}, which has no Store record"),
                format!(
                    "no call reads it: restore {}, or delete {} by hand",
                    paths::store_record(home, name).display(),
                    paths::store_dir(home, name).display()
                ),
            );
        }
    }
    Ok(())
}

/// One Sandbox's folder: its record, daemon keys and Workspace records.
fn sandbox(home: &Path, name: &str, scanned: &mut Scanned, findings: &mut Findings) -> Result<()> {
    let entity = ("sandbox", name);
    folder(&paths::sandbox_dir(home, name), entity, findings);
    if let Some(value) = record(&paths::sandbox_record(home, name), entity, true, findings) {
        scanned
            .sandboxes
            .push(SandboxRecord::from_record(name, value));
    }
    record(&paths::sandbox_control(home, name), entity, false, findings);
    let workspaces = paths::workspace_dir(home, name);
    folder(&workspaces, entity, findings);
    let entries = match std::fs::read_dir(&workspaces) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        other => other.map_err(failed)?,
    };
    let mut files: Vec<_> = (entries.filter_map(|e| e.ok()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    for file in files {
        let id = file.file_stem().unwrap_or_default().to_string_lossy();
        let workspace = format!("{name}:{id}");
        record(&file, ("workspace", &workspace), true, findings);
    }
    Ok(())
}

/// The record at `path`, if it is there and parses; a record that does not
/// parse, or (`versioned`) whose `v` is not this build's, is an error, and
/// so is a mode other than 0600.
fn record(
    path: &Path,
    entity: (&str, &str),
    versioned: bool,
    findings: &mut Findings,
) -> Option<SdkValue> {
    file_mode(path, entity, findings);
    let value = match read_record(path) {
        Ok(value) => value?,
        Err(refusal) => {
            findings.error(
                entity,
                format!("{} does not parse: {}", path.display(), refusal.message),
                format!(
                    "no call reads it: repair or remove {} by hand",
                    path.display()
                ),
            );
            return None;
        }
    };
    if versioned && value.get_int("v") != Some(RECORD_VERSION) {
        let v = value
            .get("v")
            .map_or("none".to_owned(), crate::common::json::print);
        findings.error(
            entity,
            format!(
                "{} has \"v\" {v}; this build reads \"v\": {RECORD_VERSION}",
                path.display()
            ),
            format!(
                "use the LayerFS build that wrote {}, or repair it by hand",
                path.display()
            ),
        );
    }
    Some(value)
}

/// The mode bits of `path` itself, if it is there.
fn mode(path: &Path) -> Option<u32> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    Some(metadata.permissions().mode() & 0o7777)
}

/// A folder others may list or change, or its group may use, is an error;
/// others may pass through (0701).
fn folder(path: &Path, entity: (&str, &str), findings: &mut Findings) {
    if let Some(mode) = mode(path).filter(|mode| mode & 0o076 != 0) {
        findings.error(
            entity,
            format!(
                "{} has mode {mode:04o}; the home's folders are 0700",
                path.display()
            ),
            format!("`chmod g-rwx,o-rw {}`", path.display()),
        );
    }
}

/// A Store's own folder is held to the daemon's rule (kernel_guard): no
/// group or other bits at all, not even pass-through, so a folder doctor
/// calls fine is one the daemon opens.
fn store_folder(path: &Path, entity: (&str, &str), findings: &mut Findings) {
    if let Some(mode) = mode(path).filter(|mode| mode & 0o077 != 0) {
        findings.error(
            entity,
            format!(
                "{} has mode {mode:04o}; a Store's folder is 0700",
                path.display()
            ),
            format!("`chmod 700 {}`", path.display()),
        );
    }
}

/// A record file others or its group may read is an error.
fn file_mode(path: &Path, entity: (&str, &str), findings: &mut Findings) {
    if let Some(mode) = mode(path).filter(|mode| mode & 0o077 != 0) {
        findings.error(
            entity,
            format!(
                "{} has mode {mode:04o}; the home's records are 0600",
                path.display()
            ),
            format!("`chmod 600 {}`", path.display()),
        );
    }
}
