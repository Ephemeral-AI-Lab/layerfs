//! What the synthetic kernel gives back, counted.
//!
//! [`Kernel::release`] runs once, before the engine mount is revoked; [`Kernel::held`]
//! reports the same counts while the export serves, for diagnostics.
use super::{Kernel, Table};
use std::sync::atomic::{AtomicU64, Ordering};

impl Kernel {
    /// Gives back everything the synthetic kernel holds: descriptors first,
    /// then lookup references. Run once, before the engine mount is revoked.
    /// Retained mutation failures keep their custody and are only counted.
    pub fn release(&self) -> Released {
        let table = std::mem::take(&mut *self.table());
        let companions = self.companions_release();
        let released = self.holdings(&table, companions);
        for (serial, held) in table.files {
            self.close_file(serial, held.handle);
        }
        for (handle, (serial, _)) in table.directories {
            self.close_directory(serial, handle);
        }
        for serial in table.lookups {
            self.forget(serial, 1);
        }
        released
    }
    fn retained_count(&self) -> usize {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .len()
    }
    /// Current holdings, for diagnostics.
    pub fn held(&self) -> Released {
        let table = self.table();
        let companions = self.companions_held();
        self.holdings(&table, companions)
    }
    /// What this export holds now, as [`Released`] reports it.
    fn holdings(&self, table: &Table, companions: crate::apple_double::Holding) -> Released {
        let count = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        Released {
            companions: companions.held,
            companion_bytes: companions.resident_bytes,
            companions_slotted: companions.slotted,
            companions_decoded: companions.decoded,
            companion_decoded_bytes: companions.decoded_bytes,
            files: table.files.len(),
            directories: table.directories.len(),
            lookups: table.lookups.len(),
            silly: table.silly.len(),
            retained: self.retained_count(),
            silly_renamed: count(&self.silly_renamed),
            names_refused: count(&self.names_refused),
            names_detached: count(&self.names_detached),
            collected: count(&self.collected),
            resolved: count(&self.resolved),
            unresolved: count(&self.unresolved),
        }
    }
}

/// What [`Kernel::release`] gave back.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Released {
    pub files: usize,
    pub directories: usize,
    pub lookups: usize,
    /// Silly names whose inode was kept alive until release.
    pub silly: usize,
    /// Failed mutations still holding engine custody.
    pub retained: usize,
    /// Client silly renames served as unlinks, over the export's life.
    pub silly_renamed: u64,
    /// Creations refused for their AppleDouble name, over the export's life.
    pub names_refused: u64,
    /// AppleDouble companions held in the export's index (see
    /// [`crate::apple_double`]), whole images included.
    pub companions: usize,
    /// Companion payload bytes currently resident in memory.
    pub companion_bytes: usize,
    /// Of the companions held, those taking a slot of the index bound: all but
    /// the ones holding a whole image.
    pub companions_slotted: usize,
    /// Of the companions held, those kept decoded, and the bytes counted for
    /// them.
    pub companions_decoded: usize,
    pub companion_decoded_bytes: usize,
    /// AppleDouble companions created, over the export's life.
    pub names_detached: u64,
    /// References collected beyond the bound, over the export's life.
    pub collected: u64,
    /// SqliteHandles of collected serials resolved again by their name.
    pub resolved: u64,
    /// SqliteHandles of collected serials whose name no longer reaches them.
    pub unresolved: u64,
}
