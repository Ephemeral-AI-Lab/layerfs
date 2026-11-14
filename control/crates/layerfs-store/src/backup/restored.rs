//! A restored Store's project records (#668): the record `project.init`
//! writes for each of its projects, rebuilt from the Store being restored,
//! so that a Sandbox of one reads it as it reads an installed project's.
use crate::identity::Identity;
use crate::project::add;
use crate::project::record::ProjectRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_history::{BranchId, LayerStackId};
use layerfs_home::home::paths;
use layerfs_home::outcome::failed;
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
/// and an `installed` manifest, each of which decodes.
fn complete(record: &ProjectRecord) -> bool {
    let decodes = |name: &str| match record.to_record().get(name) {
        Some(SdkValue::Bytes(bytes)) => StoreManifest::decode(bytes).is_ok(),
        _ => false,
    };
    decodes("manifest") && decodes("installed")
}
