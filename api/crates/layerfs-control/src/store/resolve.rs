//! The Store and project a call names, read from their records, and the
//! checks every Store call makes on them first.
use crate::common::home::lease;
use crate::common::home::{paths, read_record};
use crate::common::outcome::refused;
use crate::store::record::StoreRef;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

/// The Store and project a call names, with their records.
pub(crate) struct Target {
    /// The Store's name.
    pub store: String,
    /// The project's name in its Store.
    pub project: String,
    /// `store/<store>/record.json`.
    pub record: SdkValue,
    /// The record, parsed once.
    pub store_ref: StoreRef,
}

/// Splits `<store>/<project>` and reads both records: no Store is
/// `NoSuchStore`, no project `NoSuchProject` (1.1.0's codes, S §2.9).
pub(crate) fn target(home: &Path, project: &str) -> Result<Target, SdkRefusal> {
    let (store, name) = project.split_once('/').ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("project `{project}` must be written with its Store"),
        )
    })?;
    let record = read_record(&paths::store_record(home, store))?
        .ok_or_else(|| refused("NoSuchStore", format!("no Store named {store}")))?;
    if read_record(&paths::project_record(home, store, name))?.is_none() {
        return Err(refused("NoSuchProject", format!("no project {project}")));
    }
    Ok(Target {
        store: store.to_owned(),
        project: name.to_owned(),
        store_ref: StoreRef::from_record(store, &record)?,
        record,
    })
}

/// The machine a sqlite Store's record names; `native` for a Store with no
/// machine field, refused for one the 1.1.0 conversion could not place.
pub(crate) fn machine(record: &SdkValue, store: &str) -> Result<String, SdkRefusal> {
    StoreRef::from_record(store, record)?
        .machine()
        .map(str::to_owned)
}

/// Refuses `InUse` while a live process moves or removes the Store (its
/// record's `in_progress`), this process included: a move's own steps take
/// no lease and make no Store call, so nothing else in its process may
/// either. A move whose process is gone does not block.
pub(crate) fn moving(record: &SdkValue, store: &str) -> Result<(), SdkRefusal> {
    let Some(progress @ SdkValue::Map(_)) = record.get("in_progress") else {
        return Ok(());
    };
    if lease::recorded_alive(progress) {
        let pid = progress.get_int("pid").unwrap_or(0);
        let doing = match progress.get_text("op") {
            Some("remove") => "removed",
            _ => "moved",
        };
        return Err(refused(
            "InUse",
            format!("Store {store} is being {doing} by process {pid}"),
        ));
    }
    Ok(())
}
