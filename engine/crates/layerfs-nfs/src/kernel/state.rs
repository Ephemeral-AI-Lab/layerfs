//! The synthetic kernel's state: the export's bounds, the table of what it
//! holds for a stateless client, and the [`Kernel`] that owns both.
use crate::{
    kernel::read::failure,
    protocol::{
        attributes::{Identity, Status},
        handle::SqliteHandles,
    },
};
use layerfs_overlay::KernelMount;
use layerfs_vfs::ports::mount::{Fence, MountServices, RequestServices};
use layerfs_vfs::{requests::MutationFailure, ViewStat};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

/// Implicit file descriptors kept open at once.
pub const FILES_HELD: usize = 1024;
/// Implicit directory descriptors (enumerations) kept open at once.
pub const DIRECTORIES_HELD: usize = 256;
/// EXCLUSIVE CREATE verifiers remembered for retransmissions.
pub const VERIFIERS_HELD: usize = 1024;
/// Lookup references an export keeps by default before collecting.
pub const LOOKUPS_HELD: usize = 65_536;
/// Remembered names beyond which a handle of a forgotten serial is stale.
pub(super) const NAMES_HELD: usize = 4 * 1024 * 1024;
/// Parents resolved again for one handle at most (directory depth).
pub(super) const DEPTH: usize = 4096;
/// Serials used within this many most recent uses are never collected: a
/// call another connection is serving may be about to use them.
pub const RECENT_KEPT: u64 = 64;

/// A name in one directory: (directory serial, name bytes).
pub(crate) type Named = (u64, Vec<u8>);
/// An EXCLUSIVE CREATE's verifier, the serial it created and its last use.
pub(crate) type Verified = ([u8; 8], u64, u64);

/// One implicit file descriptor.
#[derive(Clone)]
pub(crate) struct Held {
    pub handle: u64,
    /// The facts last published or observed for the file through this
    /// descriptor. A COMMIT is answered from them.
    pub stat: ViewStat,
    pub tick: u64,
    pub writable: bool,
}

#[derive(Default)]
pub(crate) struct Table {
    /// Serials holding the export's one lookup reference (the root's is implicit).
    pub lookups: HashSet<u64>,
    /// Directory serial -> the parent it was last seen under.
    pub parents: HashMap<u64, u64>,
    /// File serial -> its implicit descriptor.
    pub files: HashMap<u64, Held>,
    /// Directory descriptor -> (directory serial, last use).
    pub directories: HashMap<u64, (u64, u64)>,
    /// Silly-renamed inodes: (directory, client name) -> serial. Their
    /// descriptors are never evicted; their source name is gone.
    pub silly: HashMap<Named, u64>,
    /// EXCLUSIVE CREATE: (directory, name) -> (verifier, serial, last use).
    pub verifiers: HashMap<Named, Verified>,
    /// Serial -> the name it was last reached by, kept after its reference
    /// is collected so a later handle can be resolved again.
    pub names: HashMap<u64, Named>,
    /// Every observed (directory, name) -> serial, including hard-link aliases.
    /// A second name does not retire the first; unlink/rename remove known
    /// vanished names, and retiring a serial removes all its keys.
    pub by_name: HashMap<Named, u64>,
    /// Serial -> the names it is reached by besides [`Table::names`]: the
    /// hard-link aliases it was also observed under. Every key of
    /// [`Table::by_name`] is one of the two, so dropping one record removes
    /// exactly its own keys instead of scanning the whole index.
    pub aliases: HashMap<u64, Vec<Named>>,
    /// Serial -> last use, for the collection order.
    pub used: HashMap<u64, u64>,
    /// Serials marked for a cache refresh: the mode asked, and when.
    pub refreshing: HashMap<u64, (u32, std::time::Instant)>,
    /// Directory serial -> the facts last published or observed for it. A
    /// change of an AppleDouble companion names a directory but changes
    /// nothing in it, so its `wcc_data` answers from here (see
    /// [`Kernel::directory_wcc`]).
    pub directory_stats: HashMap<u64, ViewStat>,
    pub tick: u64,
}
impl Table {
    pub fn tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }
    /// Serials kept alive by a silly name.
    pub fn pinned(&self, serial: u64) -> bool {
        self.silly.values().any(|held| *held == serial)
    }
    /// Records that `serial` is reached as `named`.
    pub(crate) fn bind_name(&mut self, named: Named, serial: u64) {
        self.by_name.insert(named, serial);
    }
    /// Remembers that `serial` is also reached as `named`, a name beyond its
    /// remembered one, so dropping the serial needs no scan of `by_name`.
    pub(crate) fn remember_alias(&mut self, serial: u64, named: &Named) {
        let aliases = self.aliases.entry(serial).or_default();
        if !aliases.iter().any(|alias| alias == named) {
            aliases.push(named.clone());
        }
    }
    /// Drops `named`, which `serial` no longer holds. Returns whether it did.
    pub(crate) fn unbind_name(&mut self, named: &Named, serial: u64) -> bool {
        if self.by_name.get(named) != Some(&serial) {
            return false;
        }
        self.by_name.remove(named);
        self.drop_alias(serial, named);
        true
    }
    /// Drops `named` whatever serial holds it, and its alias record.
    pub(crate) fn unbind_any(&mut self, named: &Named) -> Option<u64> {
        let serial = self.by_name.remove(named)?;
        self.drop_alias(serial, named);
        Some(serial)
    }
    fn drop_alias(&mut self, serial: u64, named: &Named) {
        let Some(aliases) = self.aliases.get_mut(&serial) else {
            return;
        };
        aliases.retain(|alias| alias != named);
        if aliases.is_empty() {
            self.aliases.remove(&serial);
        }
    }
    /// Drops every key `serial` holds: its remembered name and its aliases.
    pub(crate) fn unbind_serial(&mut self, serial: u64) {
        if let Some(named) = self.names.remove(&serial) {
            if self.by_name.get(&named) == Some(&serial) {
                self.by_name.remove(&named);
            }
        }
        for named in self.aliases.remove(&serial).unwrap_or_default() {
            if self.by_name.get(&named) == Some(&serial) {
                self.by_name.remove(&named);
            }
        }
    }
}
/// One opened file as READ and WRITE need it.
#[derive(Clone, Copy)]
pub(crate) struct OpenFile {
    pub handle: u64,
    pub size: u64,
}
/// One kernel mount served over NFS: its ports and its synthetic kernel state.
pub struct Kernel {
    pub(super) services: Arc<dyn MountServices>,
    pub(super) fence: Fence,
    pub(super) mount: KernelMount,
    pub(super) identity: Identity,
    pub(super) handles: SqliteHandles,
    pub(super) table: Mutex<Table>,
    pub(super) requests: AtomicU64,
    /// Mutations whose failure keeps engine custody (possibly a publication
    /// ticket): kept, never released or undone on a guess, as the FUSE
    /// connection retains them.
    pub(super) retained: Mutex<Vec<MutationFailure>>,
    /// Keep macOS AppleDouble (`._*`) names out of the Workspace: a regular
    /// file lives in the export alone (see [`crate::apple_double`]), any
    /// other kind is refused.
    pub(super) refuse_apple_double: bool,
    /// The AppleDouble companions held in memory.
    pub(crate) companions: Mutex<crate::apple_double::companions::Companions>,
    /// Lookup references kept before the least recently used are collected.
    pub(super) lookups_held: usize,
    /// References collected, handles resolved again, and resolutions that
    /// found another serial or none (stale), over the export's life.
    pub(super) collected: AtomicU64,
    pub(super) resolved: AtomicU64,
    pub(super) unresolved: AtomicU64,
    /// Silly renames served as unlinks, and creations refused by name.
    pub(crate) silly_renamed: AtomicU64,
    pub(crate) names_refused: AtomicU64,
    /// AppleDouble companions created in memory.
    pub(crate) names_detached: AtomicU64,
}
impl Kernel {
    pub fn new(services: Arc<dyn MountServices>, mount: KernelMount, uid: u32, gid: u32) -> Self {
        let root = mount.root_serial();
        Self {
            services,
            fence: Fence::default(),
            mount,
            identity: Identity {
                root,
                uid,
                gid,
                fsid: mount.owner_id(),
            },
            handles: SqliteHandles::new(mount.owner_id()),
            table: Mutex::default(),
            requests: AtomicU64::new(1),
            retained: Mutex::default(),
            refuse_apple_double: false,
            companions: Mutex::default(),
            lookups_held: LOOKUPS_HELD,
            collected: AtomicU64::new(0),
            resolved: AtomicU64::new(0),
            unresolved: AtomicU64::new(0),
            silly_renamed: AtomicU64::new(0),
            names_refused: AtomicU64::new(0),
            names_detached: AtomicU64::new(0),
        }
    }
    /// Whether creating `._*` names is refused (see [`crate::kernel::names`]).
    pub fn refusing_apple_double(mut self, refuse: bool) -> Self {
        self.refuse_apple_double = refuse;
        self
    }
    pub fn refuses_apple_double(&self) -> bool {
        self.refuse_apple_double
    }
    /// How many lookup references are kept before collection (at least 4).
    pub fn holding(mut self, lookups: usize) -> Self {
        self.lookups_held = lookups.max(4);
        self
    }
    pub fn identity(&self) -> Identity {
        self.identity
    }
    pub fn root(&self) -> u64 {
        self.identity.root
    }
    pub fn mount(&self) -> KernelMount {
        self.mount
    }
    pub fn encode(&self, serial: u64) -> Vec<u8> {
        self.handles.encode(serial).to_vec()
    }
    pub(crate) fn table(&self) -> MutexGuard<'_, Table> {
        self.table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
    /// Keeps a failed mutation's remaining custody.
    pub(crate) fn retain(&self, failure: MutationFailure) {
        self.retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(failure);
    }
    pub fn request(&self) -> Result<(Arc<dyn RequestServices>, u64), Status> {
        let services = self.services.request(&self.fence).map_err(failure)?;
        Ok((services, self.requests.fetch_add(1, Ordering::Relaxed)))
    }
}
