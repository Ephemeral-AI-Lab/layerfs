//! The synthetic kernel: the lookup references, open descriptors and parent
//! index a FUSE kernel would hold, kept by the export for a stateless client.
//!
//! The engine fences every request on a kernel reference: the root's implicit
//! one, a lookup reference, or an open descriptor. An NFS client presents a
//! handle at any time and never sends FORGET, OPEN or RELEASE, so:
//!
//! - every serial the export hands out in a handle holds exactly one lookup
//!   reference, taken by the LOOKUP visit or the creating mutation that
//!   decided it; a later visit that adds one more for the same serial is
//!   answered by forgetting the extra one at once;
//! - READ and WRITE use one implicit descriptor per file (read-only until the
//!   first WRITE, which reopens it writable), and READDIR one implicit
//!   directory descriptor per enumeration (`cookieverf`); both are bounded
//!   and the least recently used is closed when the bound is reached;
//! - `..` is answered from the parent recorded when a directory was looked
//!   up, created, moved or listed, since the engine resolves names only
//!   downward;
//! - a serial whose last name a REMOVE, RMDIR or replacing RENAME of this
//!   export took away gives its reference and descriptor back at once, so the
//!   engine can reclaim it; its handle is stale afterwards, as on any server;
//! - a client's silly rename (`.nfs…`) is not a namespace change: the source
//!   name is unlinked and the inode is kept alive by the export alone, under
//!   a name only this export knows (see [`crate::names`]).
//!
//! - the references are bounded (reference collection, "plan A" of the A2
//!   design, with no engine interface added): every handed-out serial keeps
//!   the name it was last reached by. When more than the configured number
//!   of serials hold a reference, the least recently used quarter that holds
//!   no descriptor and no silly name is forgotten, its name memory kept. A
//!   later handle of a forgotten serial is resolved again by the ordinary
//!   LOOKUP of that remembered name (its parent first, recursively): the
//!   same serial continues; another serial or none is a stale handle, as for
//!   a file renamed or removed elsewhere.
//!
//! Everything still held is given back by [`Kernel::release`], which runs
//! before the engine mount is revoked.
use crate::{
    attributes::{refusal, Identity, Status},
    block::wait,
    handle::{HandleError, Handles},
};
use layerfs_content::{filesystem::PathName, object::inode_leaf::InodeKind};
use layerfs_fuse::{
    operations::{KernelData, KernelRead, MutationFailure, ReadDataInput, ReadFailure},
    ports::{Fence, MountServices, RequestServices, ServiceError},
};
use layerfs_overlay::{KernelMount, OverlayError, READ_WINDOW};
use layerfs_workspace::{KernelReadOperation, ViewStat};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

/// Implicit file descriptors kept open at once.
pub const FILES_HELD: usize = 256;
/// Implicit directory descriptors (enumerations) kept open at once.
pub const DIRECTORIES_HELD: usize = 256;
/// EXCLUSIVE CREATE verifiers remembered for retransmissions.
pub const VERIFIERS_HELD: usize = 1024;
/// Lookup references an export keeps by default before collecting.
pub const LOOKUPS_HELD: usize = 65_536;
/// Remembered names beyond which a handle of a forgotten serial is stale.
const NAMES_HELD: usize = 4 * 1024 * 1024;
/// Parents resolved again for one handle at most (directory depth).
const DEPTH: usize = 4096;
/// Serials used within this many most recent uses are never collected: a
/// call another connection is serving may be about to use them.
pub const RECENT_KEPT: u64 = 64;

/// A name in one directory: (directory serial, name bytes).
pub(crate) type Named = (u64, Vec<u8>);
/// An EXCLUSIVE CREATE's verifier, the serial it created and its last use.
pub(crate) type Verified = ([u8; 8], u64, u64);

/// One implicit file descriptor.
#[derive(Clone, Copy)]
pub(crate) struct Held {
    pub handle: u64,
    /// The size last published or observed through this export.
    pub size: u64,
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
    /// Serial -> last use, for the collection order.
    pub used: HashMap<u64, u64>,
    /// Serials marked for a cache refresh, and when (see [`crate::refresh`]).
    pub refreshing: HashMap<u64, std::time::Instant>,
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
}
/// One opened file as READ and WRITE need it.
#[derive(Clone, Copy)]
pub(crate) struct OpenFile {
    pub handle: u64,
    pub size: u64,
}

/// One kernel mount served over NFS: its ports and its synthetic kernel state.
pub struct Kernel {
    services: Arc<dyn MountServices>,
    fence: Fence,
    mount: KernelMount,
    identity: Identity,
    handles: Handles,
    table: Mutex<Table>,
    requests: AtomicU64,
    /// Mutations whose failure keeps engine custody (possibly a publication
    /// ticket): kept, never released or undone on a guess, as the FUSE
    /// connection retains them.
    retained: Mutex<Vec<MutationFailure>>,
    /// Refuse to create macOS AppleDouble (`._*`) names.
    refuse_apple_double: bool,
    /// Lookup references kept before the least recently used are collected.
    lookups_held: usize,
    /// References collected, handles resolved again, and resolutions that
    /// found another serial or none (stale), over the export's life.
    collected: AtomicU64,
    resolved: AtomicU64,
    unresolved: AtomicU64,
    /// Silly renames served as unlinks, and creations refused by name.
    pub(crate) silly_renamed: AtomicU64,
    pub(crate) names_refused: AtomicU64,
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
            handles: Handles::new(mount.owner_id()),
            table: Mutex::default(),
            requests: AtomicU64::new(1),
            retained: Mutex::default(),
            refuse_apple_double: false,
            lookups_held: LOOKUPS_HELD,
            collected: AtomicU64::new(0),
            resolved: AtomicU64::new(0),
            unresolved: AtomicU64::new(0),
            silly_renamed: AtomicU64::new(0),
            names_refused: AtomicU64::new(0),
        }
    }
    /// Whether creating `._*` names is refused (see [`crate::names`]).
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
    /// The serial a client handle names, holding its reference: a collected
    /// one is resolved again by its remembered name.
    pub fn decode(&self, handle: &[u8]) -> Result<u64, Status> {
        let serial = self.handles.decode(handle).map_err(|error| match error {
            HandleError::Bad => Status::BadHandle,
            HandleError::Stale => Status::Stale,
        })?;
        self.held_reference(serial)?;
        Ok(serial)
    }
    /// Ensures the export holds `serial`'s reference, resolving a collected
    /// serial again; marks it used.
    pub(crate) fn held_reference(&self, serial: u64) -> Result<(), Status> {
        if serial == self.root() {
            return Ok(());
        }
        {
            let mut table = self.table();
            if table.lookups.contains(&serial) {
                let tick = table.tick();
                table.used.insert(serial, tick);
                return Ok(());
            }
        }
        self.resolve(serial, 0)
    }
    /// One collected serial resolved by the LOOKUP of its remembered name,
    /// after its parent. Another serial, or none, there is a stale handle.
    fn resolve(&self, serial: u64, depth: usize) -> Result<(), Status> {
        let named = self.table().names.get(&serial).cloned();
        let Some((parent, name)) = named else {
            return Err(Status::Stale);
        };
        if depth >= DEPTH {
            return Err(Status::Stale);
        }
        let parent_held = parent == self.root() || self.table().lookups.contains(&parent);
        if !parent_held {
            self.resolve(parent, depth + 1)?;
        }
        let found = PathName::from_bytes(&name)
            .map_err(|_| Status::Stale)
            .and_then(|path| {
                self.observe(
                    parent,
                    None,
                    KernelReadOperation::Lookup { parent, name: path },
                )
            });
        match found {
            Ok((stat, _, _)) => {
                self.entry_taken(&stat, parent, &name);
                if stat.serial == serial {
                    self.resolved.fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
            }
            Err(Status::NoEnt | Status::Stale | Status::NotDir) => {}
            Err(status) => return Err(status),
        }
        let mut table = self.table();
        if table
            .names
            .get(&serial)
            .is_some_and(|(p, n)| *p == parent && *n == name)
        {
            table.names.remove(&serial);
        }
        self.unresolved.fetch_add(1, Ordering::Relaxed);
        Err(Status::Stale)
    }
    pub fn request(&self) -> Result<(Arc<dyn RequestServices>, u64), Status> {
        let services = self.services.request(&self.fence).map_err(failure)?;
        Ok((services, self.requests.fetch_add(1, Ordering::Relaxed)))
    }
    fn observe(
        &self,
        protected: u64,
        handle: Option<u64>,
        operation: KernelReadOperation,
    ) -> Result<(ViewStat, Option<u64>, Option<u64>), Status> {
        let (services, request) = self.request()?;
        let found = wait(KernelRead::prepare(
            services, self.mount, request, protected, handle, operation,
        ))
        .map_err(read_failure)?;
        let answer = found.value().map(|value| {
            (
                value.stat.clone(),
                value.file.map(|file| file.owner_id()),
                value.directory.map(|directory| directory.owner_id()),
            )
        });
        wait(found.dispose()).map_err(read_failure)?;
        answer.map_err(refusal)
    }
    /// GETATTR. An inode that lost its last name while a silly name keeps it
    /// is observed through its descriptor; any other inode no name reaches
    /// any more is stale.
    pub fn getattr(&self, serial: u64) -> Result<ViewStat, Status> {
        let handle = {
            let table = self.table();
            if table.pinned(serial) {
                table.files.get(&serial).map(|held| held.handle)
            } else {
                None
            }
        };
        let stat = self
            .observe(serial, handle, KernelReadOperation::Getattr { serial })
            .map(|(stat, _, _)| stat)
            .map_err(|status| {
                if status == Status::NoEnt {
                    Status::Stale
                } else {
                    status
                }
            })?;
        self.observed_size(&stat);
        Ok(stat)
    }
    /// LOOKUP of `name` in directory `parent`, including `.`, `..` and the
    /// export's own silly names.
    pub fn lookup(&self, parent: u64, name: &[u8]) -> Result<ViewStat, Status> {
        match name {
            b"" => return Err(Status::NoEnt),
            b"." => return self.getattr(parent),
            b".." => {
                let up = if parent == self.root() {
                    parent
                } else {
                    let known = self.table().parents.get(&parent).copied();
                    known.ok_or(Status::NoEnt)?
                };
                self.held_reference(up)?;
                return self.getattr(up);
            }
            _ if name.len() > 255 => return Err(Status::NameTooLong),
            _ => {}
        }
        let silly = self.table().silly.get(&(parent, name.to_vec())).copied();
        if let Some(serial) = silly {
            return self.getattr(serial);
        }
        let path = PathName::from_bytes(name).map_err(|_| Status::Inval)?;
        let (stat, _, _) = self.observe(
            parent,
            None,
            KernelReadOperation::Lookup { parent, name: path },
        )?;
        // The deciding visit took one engine reference; keep exactly one.
        self.entry_taken(&stat, parent, name);
        Ok(stat)
    }
    /// A visit or mutation that decided `stat` as the entry `name` of
    /// `parent` took one engine lookup reference on it: the export keeps
    /// exactly one per serial, remembers the name and records a directory's
    /// parent. Beyond the bound, the least recently used are collected.
    pub(crate) fn entry_taken(&self, stat: &ViewStat, parent: u64, name: &[u8]) {
        let (extra, collect) = {
            let mut table = self.table();
            if stat.kind == InodeKind::Directory {
                table.parents.insert(stat.serial, parent);
            }
            let root = stat.serial == self.root();
            if !root {
                let tick = table.tick();
                table.used.insert(stat.serial, tick);
                if table.names.len() < NAMES_HELD || table.names.contains_key(&stat.serial) {
                    table.names.insert(stat.serial, (parent, name.to_vec()));
                }
            }
            let extra = root || !table.lookups.insert(stat.serial);
            let collect = if table.lookups.len() > self.lookups_held {
                collectable(&mut table, self.lookups_held - self.lookups_held / 4)
            } else {
                Vec::new()
            };
            (extra, collect)
        };
        if extra {
            self.forget(stat.serial, 1);
        }
        self.collected
            .fetch_add(collect.len() as u64, Ordering::Relaxed);
        for serial in collect {
            self.forget(serial, 1);
        }
        self.observed_size(stat);
    }
    /// Records that `serial` is now reached as `name` in `parent` (RENAME).
    pub(crate) fn renamed(&self, serial: u64, parent: u64, name: &[u8]) {
        let mut table = self.table();
        if table.names.contains_key(&serial) {
            table.names.insert(serial, (parent, name.to_vec()));
        }
    }
    pub(crate) fn forget(&self, serial: u64, count: u64) {
        if let Ok((services, _)) = self.request() {
            drop(wait(services.forget(self.mount, serial, count)));
        }
    }
    /// Gives back everything the export holds for `serial`, whose last name
    /// is gone: its descriptor, its reference and its parent record. Later
    /// handles of it are stale.
    pub(crate) fn unlinked(&self, serial: u64) {
        if serial == self.root() {
            return;
        }
        let (file, directories, held) = {
            let mut table = self.table();
            table.parents.remove(&serial);
            table.names.remove(&serial);
            table.used.remove(&serial);
            table.verifiers.retain(|_, entry| entry.1 != serial);
            let directories: Vec<u64> = table
                .directories
                .iter()
                .filter(|(_, entry)| entry.0 == serial)
                .map(|(handle, _)| *handle)
                .collect();
            for handle in &directories {
                table.directories.remove(handle);
            }
            (
                table.files.remove(&serial),
                directories,
                table.lookups.remove(&serial),
            )
        };
        if let Some(file) = file {
            self.close_file(serial, file.handle);
        }
        for handle in directories {
            self.close_directory(serial, handle);
        }
        if held {
            self.forget(serial, 1);
        }
    }
    /// Records the parent of a directory seen in a listing or moved by RENAME.
    pub fn saw_directory(&self, serial: u64, parent: u64) {
        self.table().parents.insert(serial, parent);
    }
    /// The size a published or observed stat reports, for READ's EOF.
    pub(crate) fn observed_size(&self, stat: &ViewStat) {
        if let Some(held) = self.table().files.get_mut(&stat.serial) {
            held.size = stat.logical_len;
        }
    }
    /// READLINK: the raw target bytes.
    pub fn readlink(&self, serial: u64) -> Result<Vec<u8>, Status> {
        let (services, request) = self.request()?;
        let data = wait(KernelData::read(
            services,
            self.mount,
            request,
            serial,
            None,
            ReadDataInput::Link,
        ))
        .map_err(read_failure)?
        .map_err(refusal)?;
        Ok(data.bytes().to_vec())
    }
    /// The implicit descriptor of `serial`, opened (or reopened writable)
    /// when needed.
    pub(crate) fn open_file(&self, serial: u64, writable: bool) -> Result<OpenFile, Status> {
        {
            let mut table = self.table();
            let tick = table.tick();
            if let Some(held) = table.files.get_mut(&serial) {
                if held.writable || !writable {
                    held.tick = tick;
                    return Ok(OpenFile {
                        handle: held.handle,
                        size: held.size,
                    });
                }
            }
        }
        let operation = KernelReadOperation::Open { serial, writable };
        let (stat, file, _) = self.observe(serial, None, operation)?;
        let handle = file.ok_or(Status::ServerFault)?;
        let (evicted, closed, chosen) = {
            let mut table = self.table();
            let tick = table.tick();
            let current = table.files.get(&serial).copied();
            match current {
                // Another connection opened a good enough one meanwhile.
                Some(theirs) if theirs.writable || !writable => {
                    (None, Some((serial, handle)), theirs)
                }
                _ => {
                    let evicted = if current.is_none() && table.files.len() >= FILES_HELD {
                        let pinned: HashSet<u64> = table.silly.values().copied().collect();
                        oldest(
                            table
                                .files
                                .iter()
                                .filter(|(serial, _)| !pinned.contains(serial))
                                .map(|(serial, held)| (*serial, held.tick)),
                        )
                        .and_then(|serial| {
                            table
                                .files
                                .remove(&serial)
                                .map(|held| (serial, held.handle))
                        })
                    } else {
                        None
                    };
                    let held = Held {
                        handle,
                        size: stat.logical_len,
                        tick,
                        writable,
                    };
                    table.files.insert(serial, held);
                    (evicted, current.map(|old| (serial, old.handle)), held)
                }
            }
        };
        if let Some((old, descriptor)) = evicted {
            self.close_file(old, descriptor);
        }
        if let Some((old, descriptor)) = closed {
            self.close_file(old, descriptor);
        }
        Ok(OpenFile {
            handle: chosen.handle,
            size: chosen.size,
        })
    }
    pub(crate) fn close_file(&self, serial: u64, handle: u64) {
        if let Ok((services, _)) = self.request() {
            drop(wait(services.close_file(self.mount, serial, handle)));
        }
    }
    /// READ of at most one engine window. `eof` is set when the reply reaches
    /// the size last seen for the file, or comes back short.
    pub fn read(&self, serial: u64, offset: u64, count: u32) -> Result<(Vec<u8>, bool), Status> {
        let length = count.min(READ_WINDOW as u32);
        let open = self.open_file(serial, false)?;
        if offset >= open.size {
            return Ok((Vec::new(), true));
        }
        let (services, request) = self.request()?;
        let data = wait(KernelData::read(
            services,
            self.mount,
            request,
            serial,
            Some(open.handle),
            ReadDataInput::File { offset, length },
        ))
        .map_err(read_failure)?
        .map_err(refusal)?;
        let bytes = data.bytes().to_vec();
        let end = offset.saturating_add(bytes.len() as u64);
        let eof = bytes.len() < length as usize || end >= open.size;
        Ok((bytes, eof))
    }
    /// A fresh implicit directory descriptor for an enumeration from cookie 0.
    pub fn open_directory(&self, serial: u64) -> Result<u64, Status> {
        let (_, _, directory) =
            self.observe(serial, None, KernelReadOperation::Opendir { serial })?;
        let handle = directory.ok_or(Status::ServerFault)?;
        let evicted = {
            let mut table = self.table();
            let tick = table.tick();
            let evicted = if table.directories.len() >= DIRECTORIES_HELD {
                oldest(
                    table
                        .directories
                        .iter()
                        .map(|(handle, entry)| (*handle, entry.1)),
                )
                .and_then(|handle| {
                    table
                        .directories
                        .remove(&handle)
                        .map(|entry| (entry.0, handle))
                })
            } else {
                None
            };
            table.directories.insert(handle, (serial, tick));
            evicted
        };
        if let Some((old, descriptor)) = evicted {
            self.close_directory(old, descriptor);
        }
        Ok(handle)
    }
    /// The descriptor of a continuing enumeration, if it is still open.
    pub fn directory(&self, serial: u64, handle: u64) -> Option<u64> {
        let mut table = self.table();
        let tick = table.tick();
        let entry = table.directories.get_mut(&handle)?;
        if entry.0 != serial {
            return None;
        }
        entry.1 = tick;
        Some(handle)
    }
    fn close_directory(&self, serial: u64, handle: u64) {
        if let Ok((services, _)) = self.request() {
            drop(wait(services.close_directory(self.mount, serial, handle)));
        }
    }
    /// Gives back everything the synthetic kernel holds: descriptors first,
    /// then lookup references. Run once, before the engine mount is revoked.
    /// Retained mutation failures keep their custody and are only counted.
    pub fn release(&self) -> Released {
        let table = std::mem::take(&mut *self.table());
        let released = Released {
            files: table.files.len(),
            directories: table.directories.len(),
            lookups: table.lookups.len(),
            silly: table.silly.len(),
            retained: self.retained_count(),
            silly_renamed: self.silly_renamed.load(Ordering::Relaxed),
            names_refused: self.names_refused.load(Ordering::Relaxed),
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
            files: table.files.len(),
            directories: table.directories.len(),
            lookups: table.lookups.len(),
            silly: table.silly.len(),
            retained: self.retained_count(),
            silly_renamed: self.silly_renamed.load(Ordering::Relaxed),
            names_refused: self.names_refused.load(Ordering::Relaxed),
            collected: self.collected.load(Ordering::Relaxed),
            resolved: self.resolved.load(Ordering::Relaxed),
            unresolved: self.unresolved.load(Ordering::Relaxed),
        }
    }
    /// Closes every implicit file descriptor that no silly name needs, so
    /// nothing the export opened is held across a Commit. Returns how many.
    pub fn close_files(&self) -> usize {
        let closing: Vec<(u64, u64)> = {
            let mut table = self.table();
            let pinned: HashSet<u64> = table.silly.values().copied().collect();
            let closing: Vec<(u64, u64)> = table
                .files
                .iter()
                .filter(|(serial, _)| !pinned.contains(serial))
                .map(|(serial, held)| (*serial, held.handle))
                .collect();
            for (serial, _) in &closing {
                table.files.remove(serial);
            }
            closing
        };
        for (serial, handle) in &closing {
            self.close_file(*serial, *handle);
        }
        closing.len()
    }
    /// Closes every enumeration descriptor; a continuing READDIR then
    /// answers `NFS3ERR_BAD_COOKIE` and the client lists again from the
    /// start. Used after a Commit installed another root. Returns how many.
    pub fn close_directories(&self) -> usize {
        let closing: Vec<(u64, u64)> = std::mem::take(&mut self.table().directories)
            .into_iter()
            .map(|(handle, (serial, _))| (serial, handle))
            .collect();
        for (serial, handle) in &closing {
            self.close_directory(*serial, *handle);
        }
        closing.len()
    }
}
/// Takes the least recently used references out of the table until `keep`
/// remain, skipping recently used serials, serials that hold a descriptor
/// or a silly name, and
/// directories a held serial was reached through: the engine's own parent
/// index of a held inode never loses its link, so collection proceeds from
/// the leaves. Names and parent records stay for a later resolution.
fn collectable(table: &mut Table, keep: usize) -> Vec<u64> {
    let held_parents = table.lookups.iter().filter_map(|serial| {
        table
            .names
            .get(serial)
            .map(|named| named.0)
            .or_else(|| table.parents.get(serial).copied())
    });
    let busy: HashSet<u64> = table
        .silly
        .values()
        .copied()
        .chain(table.files.keys().copied())
        .chain(table.directories.values().map(|entry| entry.0))
        .chain(held_parents)
        .collect();
    let recent = table.tick.saturating_sub(RECENT_KEPT);
    let mut candidates: Vec<(u64, u64)> = table
        .lookups
        .iter()
        .filter(|serial| !busy.contains(serial))
        .map(|serial| (table.used.get(serial).copied().unwrap_or(0), *serial))
        .filter(|(used, _)| *used < recent)
        .collect();
    let excess = table.lookups.len().saturating_sub(keep);
    candidates.sort_unstable();
    candidates.truncate(excess);
    for (_, serial) in &candidates {
        table.lookups.remove(serial);
        table.used.remove(serial);
    }
    candidates.into_iter().map(|(_, serial)| serial).collect()
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
    /// References collected beyond the bound, over the export's life.
    pub collected: u64,
    /// Handles of collected serials resolved again by their name.
    pub resolved: u64,
    /// Handles of collected serials whose name no longer reaches them.
    pub unresolved: u64,
}
fn oldest(entries: impl Iterator<Item = (u64, u64)>) -> Option<u64> {
    entries.min_by_key(|(_, tick)| *tick).map(|(key, _)| key)
}
fn stale(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if matches!(
            error.downcast_ref::<OverlayError>(),
            Some(OverlayError::Stale)
        ) {
            return true;
        }
        current = error.source();
    }
    false
}
/// Whether a failed step's cause chain holds an engine `Stale`.
pub(crate) fn failure_is_stale(error: &ServiceError) -> bool {
    stale(error.as_ref())
}
/// A failed port step: an engine `Stale` is the client's stale handle; any
/// other failure is an I/O error of this request alone.
pub fn failure(error: ServiceError) -> Status {
    if stale(error.as_ref()) {
        Status::Stale
    } else {
        Status::Io
    }
}
fn read_failure(failure: ReadFailure) -> Status {
    if stale(failure.reason.as_ref()) {
        Status::Stale
    } else {
        Status::Io
    }
}
