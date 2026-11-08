//! The Store and project a call names, read from their records, and the
//! checks every Store call makes on them first.
use crate::project::record::ProjectRecord;
use crate::record::{StoreRecord, StoreRef};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;
use std::path::Path;

/// The Store and project a call names, with their records.
pub struct Target {
    /// The Store's name.
    pub store: String,
    /// The project's name in its Store.
    pub project: String,
    /// `store/<store>/record.json`.
    pub record: StoreRecord,
    /// The record, parsed once.
    pub store_ref: StoreRef,
}

/// Splits `<store>/<project>` and reads both records: no Store is
/// `NoSuchStore`, no project `NoSuchProject` (1.1.0's codes, S §2.9).
pub fn target(home: &Path, project: &str) -> Result<Target, SdkRefusal> {
    let (store, name) = project.split_once('/').ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("project `{project}` must be written with its Store"),
        )
    })?;
    let record = StoreRecord::read(home, store)?
        .ok_or_else(|| refused("NoSuchStore", format!("no Store named {store}")))?;
    if ProjectRecord::read(home, store, name)?.is_none() {
        return Err(refused("NoSuchProject", format!("no project {project}")));
    }
    Ok(Target {
        store: store.to_owned(),
        project: name.to_owned(),
        store_ref: record.store_ref()?.clone(),
        record,
    })
}
