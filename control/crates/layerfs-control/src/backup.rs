//! The `backup` call that checks who uses a Store, `backup.restore`
//! (layerfs-project#105); the other `backup` calls are `layerfs-store`'s
//! own.
use layerfs_home::events::Events;
use layerfs_home::outcome::Outcome;
use layerfs_store::backup::family::BackupRestore;
use std::path::Path;

/// `backup.restore`: a Store from a backup, with the Store checks it makes.
pub fn restore(home: &Path, events: &mut Events<'_>, request: &BackupRestore) -> Outcome {
    layerfs_store::backup::family::restore(home, events, request, crate::store::RELATED)
}
