//! The Store calls whose checks read who uses a Store (layerfs-project#105):
//! each runs `layerfs-store`'s call with the checks from
//! [`crate::inventory`]. The Store calls proper (`branch`, `commit`, `file`)
//! and the rest are `layerfs-store`'s own.
use crate::inventory;
use layerfs_home::events::Events;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;
use layerfs_store::sqlite::family::StorePush;
use layerfs_store::Related;
use std::path::Path;

pub use layerfs_store::close_idle;

/// Who uses a Store or a project, read from the Sandbox records.
pub const RELATED: Related = Related {
    store_idle: inventory::store_idle,
    can_remove_project: inventory::can_remove_project,
};

/// `store.remove`: a sqlite Store no Sandbox uses, with its file.
pub fn remove(home: &Path, name: &StoreName) -> Outcome {
    layerfs_store::sqlite::family::remove(home, name, RELATED)
}

/// `store.push`: a sqlite Store from native to a machine.
pub fn push(home: &Path, events: &mut Events<'_>, request: &StorePush) -> Outcome {
    layerfs_store::sqlite::family::push(home, events, request, RELATED)
}

/// `store.pull`: a sqlite Store from its machine to native.
pub fn pull(home: &Path, events: &mut Events<'_>, name: &str) -> Outcome {
    layerfs_store::sqlite::family::pull(home, events, name, RELATED)
}

/// `store.disconnect`: forget a PostgreSQL Store; its data stays.
pub fn disconnect(home: &Path, name: &StoreName) -> Outcome {
    layerfs_store::postgresql::connect::disconnect(home, name, RELATED)
}
