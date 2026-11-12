//! What a machine holds that its own records do not say: its Stores and
//! Sandboxes, read from their records by `layerfs-control`, which passes
//! these reads in (layerfs-project#104). A machine call runs each at the
//! point it ran before, under the same locks.
use layerfs_bridge::control::SdkRefusal;
use std::path::Path;

/// A read of the names on machine `name` in home `home`.
type Names = fn(home: &Path, name: &str) -> Result<Vec<String>, SdkRefusal>;
/// An `InUse` check on machine `name` in home `home`.
type Check = fn(home: &Path, name: &str) -> Result<(), SdkRefusal>;

/// The reads and `InUse` checks over a machine's Stores and Sandboxes.
#[derive(Clone, Copy)]
pub struct Related {
    /// The Stores whose record names the machine.
    pub stores_on: Names,
    /// The PostgreSQL Stores attached to the machine.
    pub postgresql_on: Names,
    /// The Sandboxes whose record names the machine.
    pub sandboxes_on: Names,
    /// `machine.remove`: refused while the machine
    /// holds Stores or Sandboxes.
    pub can_remove: Check,
    /// `machine.stop`: refused while Sandboxes run on it.
    pub can_stop: Check,
    /// `machine.update`: refused while a running Sandbox uses its settings.
    pub can_change: Check,
}
