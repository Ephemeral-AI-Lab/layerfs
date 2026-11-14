//! What the synthetic kernel gives back, counted.
//!
//! [`Kernel::release`] runs once, before the engine mount is revoked; [`Kernel::held`]
//! reports the same counts while the export serves, for diagnostics.
use super::Kernel;
use std::sync::atomic::Ordering;

impl Kernel {
    /// Gives back everything the synthetic kernel holds: descriptors first,
    /// then lookup references. Run once, before the engine mount is revoked.
    /// Retained mutation failures keep their custody and are only counted.
    pub fn release(&self) -> Released {
        let table = std::mem::take(&mut *self.table());
        let companions = self.companions_release();
        let released = Released {
            companions,
            files: table.files.len(),
            directories: table.directories.len(),
            lookups: table.lookups.len(),
            silly: table.silly.len(),
            retained: self.retained_count(),
            silly_renamed: self.silly_renamed.load(Ordering::Relaxed),
            names_refused: self.names_refused.load(Ordering::Relaxed),
            names_detached: self.names_detached.load(Ordering::Relaxed),
            collected: self.collected.load(Ordering::Relaxed),
            resolved: self.resolved.load(Ordering::Relaxed),
            unresolved: self.unresolved.load(Ordering::Relaxed),
        };
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
        Released {
            companions: self.companions_held(),
            files: table.files.len(),
            directories: table.directories.len(),
            lookups: table.lookups.len(),
            silly: table.silly.len(),
            retained: self.retained_count(),
            silly_renamed: self.silly_renamed.load(Ordering::Relaxed),
            names_refused: self.names_refused.load(Ordering::Relaxed),
            names_detached: self.names_detached.load(Ordering::Relaxed),
            collected: self.collected.load(Ordering::Relaxed),
            resolved: self.resolved.load(Ordering::Relaxed),
            unresolved: self.unresolved.load(Ordering::Relaxed),
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
    /// AppleDouble companions held in memory (see [`crate::apple_double`]).
    pub companions: usize,
    /// AppleDouble companions created, over the export's life.
    pub names_detached: u64,
    /// References collected beyond the bound, over the export's life.
    pub collected: u64,
    /// SqliteHandles of collected serials resolved again by their name.
    pub resolved: u64,
    /// SqliteHandles of collected serials whose name no longer reaches them.
    pub unresolved: u64,
}
