//! A restored Store's project records (#668): the record `project.init`
//! writes for each of its projects, rebuilt from the Store being restored,
//! so that a Sandbox of one reads it as it reads an installed project's.
use crate::identity::{Identity, Kind};
use crate::project::add;
use crate::project::record::ProjectRecord;
use crate::record::StoreRecord;
use layerfs_history::{
    BranchId, HistoryCatalog, HistoryName, HistoryResult, LayerStackId, NamedSnapshot,
};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused};
use layerfs_protocol::provision::manifest::{StoreManifest, StoreProfile};
use layerfs_protocol::sdk::hex;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What a restored project's record is built from: the restored Store's own
/// authority and the file this home is about to hold, which `project.init`
/// also records when it writes the same record.
pub struct Restored {
    /// The restored Store's authority: its binding and cursor key.
    pub identity: Identity,
    /// The Store file it is installed as.
    pub file: PathBuf,
    /// The SQLite version its backup recorded; else the one that opened it.
    pub sqlite: String,
}

/// One project a restored Store holds, as its listing gives it.
pub struct Found {
    /// The project's name.
    pub name: String,
    /// Its LayerStack, as a listing gives it.
    pub stack: SdkValue,
    /// Its `main` Branch, absent for a Store that holds no such Branch.
    pub main: Option<Main>,
}

/// The `main` Branch a project's record names.
pub struct Main {
    /// Its LayerStack.
    pub stack: LayerStackId,
    /// Its Branch identity.
    pub branch: BranchId,
    /// Its base root: the root Init selected.
    pub root: [u8; 32],
}

impl Found {
    /// The record `project.init` writes for this project, from the restored
    /// Store's own history: its `manifest` and the `installed` manifest a
    /// Sandbox needs. `Ok(None)` when the project has no `main` Branch to
    /// name; a manifest that does not encode refuses.
    fn rebuilt(&self, store: &str, restored: &Restored) -> Result<Option<ProjectRecord>> {
        let Some(main) = &self.main else {
            return Ok(None);
        };
        let (manifest, installed) = add::manifest(
            &restored.identity,
            &restored.file,
            main.stack,
            main.branch,
            main.root,
            &restored.sqlite,
        )?;
        let fields = SdkValue::map([
            (
                "manifest",
                present(store, &self.name, "manifest", &manifest)?,
            ),
            (
                "installed",
                present(store, &self.name, "installed manifest", &installed)?,
            ),
        ]);
        Ok(Some(ProjectRecord::from_record(store, &self.name, fields)))
    }
}

/// Manifest `manifest` as a record holds it. One that does not encode
/// refuses the restore, naming the project and the field it came from, so a
/// record is never written without it.
fn present(store: &str, project: &str, field: &str, manifest: &StoreManifest) -> Result<SdkValue> {
    manifest.encode().map(SdkValue::Bytes).map_err(|error| {
        failed(format!(
            "project {project} of Store {store}: its {field} does not encode: {error}"
        ))
    })
}

/// The records a restored Store gets, decided before its install: the
/// complete record each project is written, one per project whose record is
/// not already complete. A project whose record cannot be rebuilt refuses
/// the restore here, with the Store not yet installed and no record written.
pub struct Records {
    store: String,
    names: Vec<String>,
    writes: Vec<ProjectRecord>,
}

/// [`Records`] for the restored projects `found` of Store `store` in `home`:
/// one complete record per project, as `project.init` writes it; an existing
/// complete record left as it is; an incomplete one completed, keeping what
/// it already holds.
pub fn records(home: &Path, store: &str, restored: &Restored, found: &[Found]) -> Result<Records> {
    let mut writes = Vec::new();
    let mut names = Vec::new();
    for project in found {
        let existing = ProjectRecord::read(home, store, &project.name)?;
        if !existing.as_ref().is_some_and(complete) {
            if let Some(record) = record(store, restored, project, existing)? {
                writes.push(record);
            }
        }
        names.push(project.name.clone());
    }
    Ok(Records {
        store: store.to_owned(),
        names,
        writes,
    })
}

impl Records {
    /// Writes them, and removes the entries of projects the restored Store
    /// does not hold, under the home lock the restore holds; returns the
    /// project names the restore reports.
    pub fn write(&self, home: &Path) -> Result<SdkValue> {
        for record in &self.writes {
            record.write(home)?;
        }
        let entries =
            paths::names(&paths::store_dir(home, &self.store).join("project")).map_err(failed)?;
        for old in entries {
            if !self.names.contains(&old) {
                std::fs::remove_dir_all(paths::project_dir(home, &self.store, &old))
                    .map_err(failed)?;
                layerfs_home::home::generation::moved(home);
            }
        }
        Ok(SdkValue::List(
            self.names.iter().map(SdkValue::text).collect(),
        ))
    }
}

/// The record to write for `project`: the complete one [`Found::rebuilt`]
/// makes, keeping what an existing record already holds (`created`, the name
/// and stack a listing gave it), or the listing's own when the Store's
/// history gives no `main` Branch to name. `None` leaves it as it is.
fn record(
    store: &str,
    restored: &Restored,
    project: &Found,
    existing: Option<ProjectRecord>,
) -> Result<Option<ProjectRecord>> {
    let Some(rebuilt) = project.rebuilt(store, restored)? else {
        return Ok(match existing {
            Some(_) => None,
            None => Some(ProjectRecord::found(
                store,
                &project.name,
                project.stack.clone(),
            )),
        });
    };
    let mut record = match existing {
        Some(old) => old.with_fields_of(&rebuilt),
        None => rebuilt,
    };
    // the record's own time: a restored project's is when it was restored
    if record.created().is_none() {
        record = record.created_now();
    }
    Ok(Some(record))
}

/// True when `record` holds what `discovery::project` reads: a `manifest`
/// and an `installed` manifest, each of which decodes with `Normal`.
pub fn complete(record: &ProjectRecord) -> bool {
    let decodes = |name: &str| match record.to_record().get(name) {
        Some(SdkValue::Bytes(bytes)) => {
            StoreManifest::decode(bytes).is_ok_and(|m| m.profile == StoreProfile::Normal)
        }
        _ => false,
    };
    decodes("manifest") && decodes("installed")
}

/// The projects Store history `history` holds, each with its `main`
/// Branch when it has one: what a restore and a record repair rebuild
/// records from.
pub fn found_in(history: &dyn HistoryCatalog) -> HistoryResult<Vec<Found>> {
    let main = HistoryName::new(layerfs_store_service::history::branch::MAIN)?;
    let mut found = Vec::new();
    for stack in layerfs_project::store::projects(history)? {
        let snapshot = match history.branch_snapshot_named(&stack.name, &main)? {
            NamedSnapshot::Found(snapshot) => Some(Main {
                stack: snapshot.branch.stack,
                branch: snapshot.branch.id,
                root: snapshot.base_root.to_bytes(),
            }),
            NamedSnapshot::NoStack | NamedSnapshot::NoBranch => None,
        };
        found.push(Found {
            name: stack.name.as_str().to_owned(),
            stack: SdkValue::text(hex(&stack.id.to_bytes())),
            main: snapshot,
        });
    }
    Ok(found)
}

/// Reads the live projects and SQLite version of the local SQLite Store at
/// `file` through a read-only diagnostic handle so the file's bytes and WAL
/// stay untouched.
pub fn projects_in_diagnostic(file: &Path, identity: &Identity) -> Result<(Vec<Found>, String)> {
    layerfs_store_service::kernel_guard::check(file)
        .map_err(|reason| refused("Unsupported", reason.0))?;
    let handles = layerfs_persistence::SqliteHandles::open_diagnostic(
        file,
        &identity.binding,
        identity.cursor_key,
    )
    .map_err(|error| refused("Unavailable", crate::words::persistence(&error)))?;
    let found = found_in(&handles.history).map_err(failed)?;
    Ok((found, handles.profile().sqlite_version.clone()))
}

/// The record `name` would be completed to: `existing` with the `manifest`
/// and `installed` its listed project rebuilds, when the Store lists it
/// with a `main` Branch of the recorded stack. `Ok(None)` when it does not.
fn match_rebuilt(
    store: &str,
    restored: &Restored,
    found: &[Found],
    name: &str,
    existing: ProjectRecord,
) -> Result<Option<ProjectRecord>> {
    let recorded_stack = existing.stack_id();
    let Some(item) = found.iter().find(|item| {
        item.name == name
            && recorded_stack.is_none_or(|s| item.main.as_ref().is_some_and(|m| m.stack == s))
    }) else {
        return Ok(None);
    };
    let Some(rebuilt) = item.rebuilt(store, restored)? else {
        return Ok(None);
    };
    Ok(Some(existing.with_fields_of(&rebuilt)))
}

/// A record repair needs: it holds no `op` and is not [`complete`].
fn needs_repair(record: &ProjectRecord) -> bool {
    record.op().is_none() && !complete(record)
}

/// Rebuilds `manifest` and `installed` on existing incomplete SQLite project
/// records of Store `store` under `home` from the local `file`. Records that
/// are already complete or hold an `op` are left alone, as is one whose
/// `record.json` does not read; when none need repair, `file` is not opened.
/// Returns each record it left incomplete, with why, so the caller can say
/// so: a failure here never fails the call that asked for it.
pub fn complete_existing(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    file: &Path,
) -> Vec<(String, String)> {
    let entries = match paths::names(&paths::store_dir(home, store).join("project")) {
        Ok(names) => names,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![(String::new(), error.to_string())],
    };
    let incomplete: Vec<String> = entries
        .into_iter()
        .filter(|name| {
            ProjectRecord::read(home, store, name)
                .is_ok_and(|r| r.as_ref().is_some_and(needs_repair))
        })
        .collect();
    if incomplete.is_empty() {
        return Vec::new();
    }
    complete_all(home, store, record, file, &incomplete).unwrap_or_else(|refusal| {
        (incomplete.into_iter())
            .map(|name| (name, refusal.message.clone()))
            .collect()
    })
}

/// [`complete_existing`] of the records `incomplete` names, once the Store
/// file is read.
fn complete_all(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    file: &Path,
    incomplete: &[String],
) -> Result<Vec<(String, String)>> {
    let identity = crate::identity::identity_in(record)?;
    if identity.kind != Kind::Sqlite {
        return Ok(Vec::new());
    }
    let (found, sqlite) = projects_in_diagnostic(file, &identity)?;
    let restored = Restored {
        identity,
        file: file.to_path_buf(),
        sqlite,
    };
    let _lock = home::lock::take(home)?;
    let mut left = Vec::new();
    for name in incomplete {
        let existing = match ProjectRecord::read(home, store, name) {
            Ok(Some(existing)) if needs_repair(&existing) => existing,
            _ => continue,
        };
        let why = match match_rebuilt(store, &restored, &found, name, existing) {
            Ok(Some(updated)) => match updated.write(home) {
                Ok(()) => continue,
                Err(refusal) => refusal.message,
            },
            Ok(None) => format!("Store {store} has no main Branch of its stack"),
            Err(refusal) => refusal.message,
        };
        left.push((name.clone(), why));
    }
    Ok(left)
}

/// Completes one existing incomplete SQLite project record `project` of
/// Store `store` from the local `file`; returns the record it wrote, or
/// `None` when the record needs no repair or cannot be rebuilt.
pub fn repair_project(
    home: &Path,
    store: &str,
    project: &str,
    identity: Identity,
    file: &Path,
) -> Result<Option<ProjectRecord>> {
    if identity.kind != Kind::Sqlite {
        return Ok(None);
    }
    if !ProjectRecord::read(home, store, project)?.is_some_and(|r| needs_repair(&r)) {
        return Ok(None);
    }
    let (found, sqlite) = projects_in_diagnostic(file, &identity)?;
    let restored = Restored {
        identity,
        file: file.to_path_buf(),
        sqlite,
    };
    let _lock = home::lock::take(home)?;
    let Some(existing) = ProjectRecord::read(home, store, project)?.filter(needs_repair) else {
        return Ok(None);
    };
    let Some(updated) = match_rebuilt(store, &restored, &found, project, existing)? else {
        return Ok(None);
    };
    updated.write(home)?;
    Ok(Some(updated))
}
