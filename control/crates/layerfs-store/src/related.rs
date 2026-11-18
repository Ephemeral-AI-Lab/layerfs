//! What uses a Store from outside the Store's records: the Sandboxes of its
//! projects, read from their records by `layerfs-control`, which passes
//! these checks in (layerfs-project#105). Each runs at the point it ran
//! before, under the same locks.
use layerfs_bridge::control::SdkRefusal;
use std::path::Path;

/// A related-use check on the Store or project `name` in home `home`.
type Check = fn(home: &Path, name: &str) -> Result<(), SdkRefusal>;

/// The checks over a Store's users.
#[derive(Clone, Copy)]
pub struct Related {
    /// `store.move`, `store.remove`, `store.disconnect` and
    /// `backup.restore`: refused while another live process holds the Store
    /// or a Sandbox of one of its projects is recorded.
    pub store_idle: Check,
    /// `project.delete`: refused `Busy` while a Sandbox of the project is
    /// recorded, even with nothing mounted.
    pub can_remove_project: Check,
}
