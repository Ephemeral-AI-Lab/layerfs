//! `store.create`: an empty sqlite Store on `native`.
use super::family::public;
use crate::store::record::StoreRecord;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{refused, Outcome, View};
use std::path::Path;

/// `store.create`: an empty sqlite Store on `native`.
pub(super) fn create(home: &Path, name: &str) -> Outcome {
    layerfs_home::names::StoreName::parse(name)?;
    let _lock = home::lock::take(home)?;
    if paths::store_record(home, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    let directory = paths::store_dir(home, name);
    home::private_dir(&directory)?;
    let identity = crate::store::identity::Identity {
        kind: crate::store::identity::Kind::Sqlite,
        binding: layerfs_home::helpers::random()?.to_vec(),
        cursor_key: layerfs_home::helpers::random()?,
    };
    let file = paths::store_file(home, name);
    let policy = layerfs_storage::StoragePolicy::frozen_default();
    let sealed = match crate::store::identity::make_empty(&directory, &file, &identity, policy) {
        Ok(sealed) => sealed,
        Err(refusal) => {
            // nothing was published: this call's own directory goes
            let _ = std::fs::remove_dir_all(&directory);
            return Err(refusal);
        }
    };
    crate::store::identity::save_record(home, name, &identity)?;
    let record = StoreRecord::read(home, name)?
        .unwrap_or_else(|| StoreRecord::from_record(name, SdkValue::Null))
        .created_on_native(&sealed.sqlite_version);
    record.write(home)?;
    public(home, &record, View::Full, None)
}
