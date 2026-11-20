//! A restored Store's project records (#668): the record `project.init`
//! writes for each of its projects, rebuilt from the Store being restored,
//! so that a Sandbox of one reads it as it reads an installed project's.
use crate::identity::{Identity, Kind};
use crate::project::add;
use crate::project::record::ProjectRecord;
use crate::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::{StoreManifest, StoreProfile};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{BranchId, HistoryCatalog, HistoryName, LayerStackId, NamedSnapshot};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused};
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
/// and an `installed` manifest, each of which decodes with `Disposable`.
pub fn complete(record: &ProjectRecord) -> bool {
    let decodes = |name: &str| match record.to_record().get(name) {
        Some(SdkValue::Bytes(bytes)) => {
            StoreManifest::decode(bytes).is_ok_and(|m| m.profile == StoreProfile::Disposable)
        }
        _ => false,
    };
    decodes("manifest") && decodes("installed")
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
    let main = HistoryName::new(layerfs_store_service::history::branch::MAIN).map_err(failed)?;
    let mut found = Vec::new();
    for stack in layerfs_project::store::projects(&handles.history).map_err(failed)? {
        let snapshot = match handles
            .history
            .branch_snapshot_named(&stack.name, &main)
            .map_err(failed)?
        {
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
    Ok((found, handles.profile().sqlite_version.clone()))
}

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

/// Rebuilds `manifest` and `installed` on existing incomplete SQLite project
/// records of Store `store` under `home` from the local `file`. Records that
/// are already complete or hold an `op` are left alone; when none need repair,
/// `file` is not opened.
pub fn complete_existing(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    file: &Path,
) -> Result<()> {
    let entries = match paths::names(&paths::store_dir(home, store).join("project")) {
        Ok(names) => names,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(failed(error)),
    };
    let mut incomplete = Vec::new();
    for name in entries {
        if let Some(existing) = ProjectRecord::read(home, store, &name)? {
            if existing.op().is_none() && !complete(&existing) {
                incomplete.push(name);
            }
        }
    }
    if incomplete.is_empty() {
        return Ok(());
    }
    let identity = crate::identity::identity_in(record)?;
    if identity.kind != Kind::Sqlite {
        return Ok(());
    }
    let (found, sqlite) = projects_in_diagnostic(file, &identity)?;
    let restored = Restored {
        identity,
        file: file.to_path_buf(),
        sqlite,
    };
    let _lock = home::lock::take(home)?;
    for name in incomplete {
        let Some(existing) = ProjectRecord::read(home, store, &name)? else {
            continue;
        };
        if existing.op().is_some() || complete(&existing) {
            continue;
        }
        if let Some(updated) = match_rebuilt(store, &restored, &found, &name, existing)? {
            updated.write(home)?;
        }
    }
    Ok(())
}

/// Completes one existing incomplete SQLite project record `project` of
/// Store `store` from the local `file`; returns whether the record was written.
pub fn repair_project(
    home: &Path,
    store: &str,
    project: &str,
    identity: Identity,
    file: &Path,
) -> Result<bool> {
    if identity.kind != Kind::Sqlite {
        return Ok(false);
    }
    let Some(existing) = ProjectRecord::read(home, store, project)? else {
        return Ok(false);
    };
    if existing.op().is_some() || complete(&existing) {
        return Ok(false);
    }
    let (found, sqlite) = projects_in_diagnostic(file, &identity)?;
    let restored = Restored {
        identity,
        file: file.to_path_buf(),
        sqlite,
    };
    let _lock = home::lock::take(home)?;
    let Some(existing) = ProjectRecord::read(home, store, project)? else {
        return Ok(false);
    };
    if existing.op().is_some() || complete(&existing) {
        return Ok(false);
    }
    let Some(updated) = match_rebuilt(store, &restored, &found, project, existing)? else {
        return Ok(false);
    };
    updated.write(home)?;
    Ok(true)
}
