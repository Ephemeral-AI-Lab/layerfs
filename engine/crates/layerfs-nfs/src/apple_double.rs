//! AppleDouble companions held by the export alone.
//!
//! On NFSv3 the macOS client keeps a file's extended attributes and resource
//! fork in a `._<name>` companion file (see [`crate::names`]). When the export
//! keeps such names out of the Workspace, a CREATE of one makes a file that
//! lives only here, under its own serial range: LOOKUP, GETATTR, ACCESS, READ,
//! WRITE, SETATTR, COMMIT, REMOVE and a RENAME to another AppleDouble name
//! work on it, READDIR does not list it, and no mutation of it ever reaches
//! the engine, so the Workspace stays clean and no Commit can capture it.
//! Everything is dropped when the export is released.
//!
//! The tree is not consulted for such a name: while this mode is on, a
//! LOOKUP of a `._` name answers `NFS3ERR_NOENT` unless the export holds that
//! companion, which is what a READDIR of that directory already shows by not
//! listing them, so the two agree. Such a change also never touches the
//! directory it names, so its `wcc_data` answers both halves from the last
//! facts the export saw for it (see [`Kernel::directory_wcc`]).
//!
//! A companion created while the export knows its main file (`<name>` in the
//! same directory) belongs to that inode: any name of the inode reaches it, so
//! hard links share one companion, it follows the inode through RENAME, and it
//! goes with the inode's last name rather than with one name. Its FinderInfo
//! and extended attributes are kept under that inode's serial, not under the
//! `._` name. Whenever a write leaves such a companion holding a whole
//! AppleDouble image the client laid out itself (see
//! [`crate::apple_double_codec`]), it is decoded: a small one keeps only its
//! FinderInfo and attributes in memory (at most [`DECODED_BYTES`] in all, each
//! at most [`DECODED_ENTRY_BYTES`]), and every read synthesizes the same bytes
//! again. Such a whole image takes no slot of the index bound. Any other body
//! (a resource fork, another layout, a half-written image, or a companion whose
//! main file is unknown) is kept as the client wrote it and takes a slot: the
//! index bound, the refusal, its rate-limited log and orphan reclamation stay
//! the fallback for everything decoding does not compress.
//!
//! Every companion keeps its index entry in memory (at most [`HELD`] of them
//! not holding a whole image, each companion at most [`FILE_BYTES`]). Bodies up
//! to [`RESIDENT_BYTES`] in all stay in memory; when a write or resize crosses
//! that budget, the least recently touched resident bodies are spilled to the
//! export's own scratch directory (see [`crate::apple_double_spill`]). Removing
//! or replacing a main file drops its paired `._<name>` companion, or only that
//! name when the companion is its inode's, which goes with the inode's last
//! name. When [`HELD`] is reached, orphan companions whose main inode or main
//! file the export no longer knows are reclaimed first, and only if [`HELD`]
//! live companions still remain does `CREATE` answer `NFS3ERR_NOSPC`.
use crate::{
    apple_double_codec::Image,
    apple_double_spill::{
        create_spilled, io_status, now, spill_path, touch, unlink_paths, Edit, Room,
    },
    attributes::Status,
    change::{How, Sattr, Tickets, Wcc},
    kernel::Kernel,
};
use layerfs_content::{filesystem::attributes::PortableMetadata, object::inode_leaf::InodeKind};
use layerfs_mount_ports::operations::write::Stamp;
use layerfs_workspace::ViewStat;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

/// The first serial of the export's own companions, far above any serial the
/// engine allocates.
pub const SERIALS: u64 = 1 << 62;
/// Companion index entries not holding a whole image, held at once.
pub const HELD: usize = 65_536;
/// Largest companion kept.
pub const FILE_BYTES: u64 = 1 << 20;
/// Companion payload bytes kept resident in memory before spilling to disk.
pub const RESIDENT_BYTES: usize = 32 << 20;
/// Bytes counted for all decoded companions (see [`Image::cost`]).
pub const DECODED_BYTES: usize = 512 << 20;
/// Bytes counted for one decoded companion; a larger whole image keeps its
/// bytes as written.
pub const DECODED_ENTRY_BYTES: usize = 128 << 10;

/// A name in one directory: (directory serial, name bytes).
type Named = (u64, Vec<u8>);

enum Body {
    Resident(Vec<u8>),
    /// On disk in the spill directory, under the companion's own serial; each
    /// operation opens it by path instead of holding it open.
    Spilled,
    /// A whole image kept as its FinderInfo and attributes; a read encodes the
    /// image again, byte for byte as the client wrote it.
    Decoded(Image),
}

struct Companion {
    /// The `._` name it was created or last renamed to.
    named: Named,
    /// The main inode it belongs to, when the export knew it at creation.
    inode: Option<u64>,
    /// Its body is a whole image: any name of the inode reads it, and it takes
    /// no slot of the index bound.
    whole: bool,
    len: usize,
    body: Body,
    metadata: PortableMetadata,
    used: u64,
}
impl Companion {
    fn resident_len(&self) -> usize {
        match &self.body {
            Body::Resident(bytes) => bytes.len(),
            Body::Spilled | Body::Decoded(_) => 0,
        }
    }
    fn decoded_cost(&self) -> usize {
        match &self.body {
            Body::Decoded(image) => image.cost(),
            Body::Resident(_) | Body::Spilled => 0,
        }
    }
    /// Whether it takes a slot of the index bound.
    fn slotted(&self) -> bool {
        !self.whole
    }
}

/// What an export's companion index holds (see [`crate::kernel::Released`]).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Holding {
    pub held: usize,
    pub slotted: usize,
    pub resident_bytes: usize,
    pub decoded: usize,
    pub decoded_bytes: usize,
}

/// The companions of one export.
pub(crate) struct Companions {
    next: u64,
    tick: u64,
    files: HashMap<u64, Companion>,
    names: HashMap<Named, u64>,
    /// Main inode serial -> the companion holding its FinderInfo and
    /// attributes.
    inodes: HashMap<u64, u64>,
    slotted: usize,
    resident_bytes: usize,
    decoded: usize,
    decoded_bytes: usize,
    spill_dir: Option<PathBuf>,
    resident_budget: usize,
    held_limit: usize,
    last_refusal_log: Option<Instant>,
}
impl Default for Companions {
    fn default() -> Self {
        Self {
            next: 0,
            tick: 0,
            files: HashMap::new(),
            names: HashMap::new(),
            inodes: HashMap::new(),
            slotted: 0,
            resident_bytes: 0,
            decoded: 0,
            decoded_bytes: 0,
            spill_dir: None,
            resident_budget: RESIDENT_BYTES,
            held_limit: HELD,
            last_refusal_log: None,
        }
    }
}
impl Companions {
    fn tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }
    fn use_serial(&mut self, serial: u64) {
        let tick = self.tick();
        if let Some(file) = self.files.get_mut(&serial) {
            file.used = tick;
        }
    }
    fn stat(serial: u64, file: &Companion) -> ViewStat {
        ViewStat {
            serial,
            kind: InodeKind::RegularFile,
            metadata: file.metadata,
            namespace_refs: 1,
            logical_len: file.len as u64,
            subdirs: 0,
        }
    }
    /// Adds `file` to the index and to the slot and byte counts.
    fn attach(&mut self, serial: u64, file: Companion) {
        self.slotted += usize::from(file.slotted());
        self.resident_bytes += file.resident_len();
        self.decoded += usize::from(matches!(file.body, Body::Decoded(_)));
        self.decoded_bytes += file.decoded_cost();
        self.files.insert(serial, file);
    }
    /// Takes `serial` out of the index and the counts; its name keys stay for
    /// the caller to unindex.
    fn detach(&mut self, serial: u64) -> Option<Companion> {
        let file = self.files.remove(&serial)?;
        self.slotted -= usize::from(file.slotted());
        self.resident_bytes -= file.resident_len();
        self.decoded -= usize::from(matches!(file.body, Body::Decoded(_)));
        self.decoded_bytes -= file.decoded_cost();
        Some(file)
    }
    /// Makes `file`'s name and main inode find `serial`.
    fn index(&mut self, serial: u64, file: &Companion) {
        self.names.insert(file.named.clone(), serial);
        if let Some(inode) = file.inode {
            self.inodes.insert(inode, serial);
        }
    }
    /// Drops `file`'s name and inode keys that still find `serial`.
    fn unindex(&mut self, serial: u64, file: &Companion) {
        if self.names.get(&file.named) == Some(&serial) {
            self.names.remove(&file.named);
        }
        if let Some(inode) = file.inode {
            if self.inodes.get(&inode) == Some(&serial) {
                self.inodes.remove(&inode);
            }
        }
    }
    /// The companion `named` reaches when its main file is `main`: the one
    /// held under that name, unless it belongs to another inode than `main`,
    /// else the one its main inode already has.
    fn resolve(&self, named: &Named, main: Option<u64>) -> Option<u64> {
        if let Some(&serial) = self.names.get(named) {
            let inode = self.files.get(&serial).and_then(|file| file.inode);
            if inode.is_none() || main.is_none() || inode == main {
                return Some(serial);
            }
        }
        main.and_then(|main| self.inodes.get(&main).copied())
    }
    /// Replaces `serial`'s body by the decoded form of `bytes`, the caller's
    /// edited body of `len` bytes, when the decoded budget takes it and the
    /// export knows the companion's main inode. The decoded form costs its
    /// attributes rather than its image, so a body that decodes needs no
    /// resident budget at all.
    fn keep_decoded(&mut self, serial: u64, bytes: &[u8], len: usize) -> bool {
        // Only an inode's companion is decoded: one whose main file the export
        // never saw keeps the client's bytes as written and a slot of the
        // index bound, so that bound and orphan reclamation still reach it.
        let inode = self.files.get(&serial).and_then(|file| file.inode);
        if inode.is_none() {
            return false;
        }
        let Some(image) = Image::compact(bytes) else {
            return false;
        };
        if image.cost() > DECODED_ENTRY_BYTES {
            return false;
        }
        let cost = image.cost();
        let tick = self.tick();
        let Some(mut file) = self.detach(serial) else {
            return false;
        };
        if self.decoded_bytes + cost > DECODED_BYTES {
            self.attach(serial, file);
            return false;
        }
        file.len = len;
        file.body = Body::Decoded(image);
        file.whole = true;
        file.used = tick;
        touch(&mut file.metadata);
        self.attach(serial, file);
        true
    }
    /// Decides whether `file`'s body is a whole image: a decoded body of an
    /// inode's companion always is; a resident body of one is decoded when it
    /// parses back to the same bytes and the decoded budget takes it. A
    /// companion whose main file the export does not know is never a whole
    /// image: it keeps the client's bytes as written and a slot of the index
    /// bound. `file` must be detached from the index.
    fn classify(&self, file: &mut Companion) {
        if file.inode.is_none() {
            let decoded = match &file.body {
                Body::Decoded(image) => Some(image.encode()),
                Body::Resident(_) | Body::Spilled => None,
            };
            if let Some(bytes) = decoded {
                file.body = Body::Resident(bytes);
            }
            file.whole = false;
            return;
        }
        file.whole = matches!(file.body, Body::Decoded(_));
        let Body::Resident(bytes) = &file.body else {
            return;
        };
        let Some(image) = Image::compact(bytes) else {
            return;
        };
        file.whole = true;
        let cost = image.cost();
        if cost <= DECODED_ENTRY_BYTES && self.decoded_bytes + cost <= DECODED_BYTES {
            file.body = Body::Decoded(image);
        }
    }
    fn take_serial(&mut self, serial: u64) -> Option<PathBuf> {
        let file = self.detach(serial)?;
        self.unindex(serial, &file);
        match file.body {
            Body::Resident(_) | Body::Decoded(_) => None,
            Body::Spilled => (self.spill_dir.as_ref()).map(|dir| spill_path(dir, serial)),
        }
    }
    /// Renames the temporary file `temp`, which holds `serial`'s body of `len`
    /// bytes, to the companion's own name in the spill directory and switches
    /// the companion to it, when nothing touched it since `used` was read and
    /// its body is still in memory (resident or decoded). A body another call
    /// changed or already spilled stays: `Ok(false)` then, and `temp` is the
    /// caller's to unlink.
    fn adopt_spilled(
        &mut self,
        serial: u64,
        used: u64,
        temp: &Path,
        len: usize,
    ) -> Result<bool, Status> {
        let path = self
            .spill_dir
            .as_ref()
            .map(|dir| spill_path(dir, serial))
            .ok_or(Status::NoSpc)?;
        let Some(file) = self.files.get_mut(&serial) else {
            return Err(Status::Stale);
        };
        if !spill_applies(used, file.used, !matches!(file.body, Body::Spilled)) {
            return Ok(false);
        }
        fs::rename(temp, path).map_err(|e| io_status(&e))?;
        let mut file = self.detach(serial).ok_or(Status::Stale)?;
        file.body = Body::Spilled;
        // A body written out whole is still a whole image; a shorter file
        // than the companion had described is not.
        file.whole &= len <= file.len;
        file.len = file.len.max(len);
        self.attach(serial, file);
        Ok(true)
    }
    fn oldest_resident(&self, skip: Option<u64>) -> Option<u64> {
        self.files
            .iter()
            .filter(|(serial, file)| {
                Some(**serial) != skip
                    && matches!(file.body, Body::Resident(ref b) if !b.is_empty())
            })
            .min_by_key(|(_, file)| file.used)
            .map(|(serial, _)| *serial)
    }
    /// Reclaims companions whose main inode is not in `live_serials`, or whose
    /// main file `(parent, name)` is not in `live_names` when the export never
    /// knew its inode. Returns any spilled file paths to unlink outside the
    /// lock.
    fn reclaim_orphans(
        &mut self,
        live_names: &HashSet<Named>,
        live_serials: &HashSet<u64>,
    ) -> Vec<PathBuf> {
        let orphans: Vec<u64> = self
            .files
            .iter()
            .filter(|(_, file)| match file.inode {
                Some(inode) => !live_serials.contains(&inode),
                None => {
                    let (parent, ref name) = file.named;
                    name.strip_prefix(b"._")
                        .is_some_and(|main| !live_names.contains(&(parent, main.to_vec())))
                }
            })
            .map(|(serial, _)| *serial)
            .collect();
        orphans
            .into_iter()
            .filter_map(|serial| self.take_serial(serial))
            .collect()
    }
    fn holding(&self) -> Holding {
        Holding {
            held: self.files.len(),
            slotted: self.slotted,
            resident_bytes: self.resident_bytes,
            decoded: self.decoded,
            decoded_bytes: self.decoded_bytes,
        }
    }
}
/// Whether a body a spill wrote out still replaces its companion: only when the
/// companion is the one the snapshot saw (`used` is the tick read then) and its
/// body is still in memory, so no call wrote to it while the file was written.
pub fn spill_applies(used: u64, file_used: u64, resident: bool) -> bool {
    used == file_used && resident
}
/// The body length `edit` leaves on a body that had `len` bytes.
fn edited_len(edit: &Edit<'_>, len: usize) -> usize {
    match edit {
        Edit::Write { .. } => len.max(edit.target_len()),
        Edit::Size(size) => *size,
    }
}
/// Records the length `edit` leaves on `file`.
fn record(edit: &Edit<'_>, file: &mut Companion) {
    file.len = edited_len(edit, file.len);
}
/// A serial of the export's own companions.
pub fn companion(serial: u64) -> bool {
    serial >= SERIALS
}

impl Kernel {
    fn companions(&self) -> std::sync::MutexGuard<'_, Companions> {
        self.companions
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
    pub(crate) fn configure_companions(
        &self,
        spill_dir: Option<PathBuf>,
        resident_budget: usize,
        held_limit: usize,
    ) {
        if let Some(dir) = &spill_dir {
            drop(fs::remove_dir_all(dir));
        }
        let mut companions = self.companions();
        companions.spill_dir = spill_dir;
        companions.resident_budget = resident_budget;
        companions.held_limit = held_limit.max(1);
    }
    /// The main inode `._<name>` in `parent` pairs with, as the export's own
    /// name index knows it. Takes the kernel table: never call it under the
    /// companion lock.
    fn main_of(&self, parent: u64, name: &[u8]) -> Option<u64> {
        let main = name.strip_prefix(b"._").filter(|main| !main.is_empty())?;
        self.table().by_name.get(&(parent, main.to_vec())).copied()
    }
    /// Spills the least recently used resident companions to disk (outside the
    /// companion lock) until adding `need` bytes stays within `resident_budget`.
    /// Returns `true` if `need` fits in resident memory afterwards.
    fn spill_for(&self, target: u64, need: usize) -> Result<bool, Status> {
        loop {
            let victim = {
                let companions = self.companions();
                let fits = companions.resident_bytes + need <= companions.resident_budget;
                if fits {
                    return Ok(true);
                }
                companions.oldest_resident(Some(target))
            };
            let Some(victim) = victim else {
                return Ok(false);
            };
            // A victim another call changed meanwhile leaves the next round to
            // pick again; only a disk failure ends the loop.
            let _ = self.spill_one(victim, None)?;
        }
    }
    /// Writes `serial`'s body out to the spill directory and switches it to
    /// that file, keeping at least `keep` bytes (all of them when `None`),
    /// with the disk I/O outside the companion lock. `Ok(false)` when another
    /// call changed the companion, or already spilled it, while the file was
    /// written: that temporary file is dropped and the caller decides again on
    /// the body it now finds.
    fn spill_one(&self, serial: u64, keep: Option<usize>) -> Result<bool, Status> {
        let (dir, bytes, size, used) = {
            let companions = self.companions();
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            let bytes = match &file.body {
                Body::Resident(bytes) => bytes.clone(),
                Body::Decoded(image) => image.encode(),
                Body::Spilled => return Ok(true),
            };
            let dir = companions.spill_dir.clone().ok_or(Status::NoSpc)?;
            let size = keep.unwrap_or(0).max(bytes.len());
            (dir, bytes, size, file.used)
        };
        let temp = create_spilled(&dir, serial, &bytes, size as u64)?;
        let adopted = self.companions().adopt_spilled(serial, used, &temp, size);
        if matches!(adopted, Ok(true)) {
            return Ok(true);
        }
        unlink_paths([temp]);
        adopted.map(|_| false)
    }
    /// The spill directory's file of `serial`, opened for one operation and
    /// closed again when the returned descriptor drops: the export holds no
    /// descriptor per spilled companion. A file or directory that is gone
    /// means the companion itself is gone.
    fn spill_file(&self, serial: u64, write: bool) -> Result<File, Status> {
        let dir = {
            let companions = self.companions();
            companions.spill_dir.clone().ok_or(Status::Stale)?
        };
        OpenOptions::new()
            .read(true)
            .write(write)
            .open(spill_path(&dir, serial))
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => Status::Stale,
                _ => io_status(&e),
            })
    }
    /// GETATTR of a companion serial.
    pub(crate) fn companion_stat(&self, serial: u64) -> Result<ViewStat, Status> {
        let mut companions = self.companions();
        let file = companions.files.get(&serial).ok_or(Status::Stale)?;
        let stat = Companions::stat(serial, file);
        companions.use_serial(serial);
        Ok(stat)
    }
    /// LOOKUP of a companion name, if one is held for it or for its main
    /// inode.
    pub(crate) fn companion_named(&self, parent: u64, name: &[u8]) -> Option<ViewStat> {
        let main = self.main_of(parent, name);
        let mut companions = self.companions();
        let serial = companions.resolve(&(parent, name.to_vec()), main)?;
        let stat = Companions::stat(serial, companions.files.get(&serial)?);
        companions.use_serial(serial);
        Some(stat)
    }
    /// CREATE of an AppleDouble name.
    pub(crate) fn companion_create(
        &self,
        parent: u64,
        name: &[u8],
        how: How,
        tickets: &mut Tickets,
    ) -> Result<(ViewStat, Wcc), Status> {
        let directory = self.directory_wcc(parent);
        if parent != self.root() && !self.table().parents.contains_key(&parent) {
            return Err(Status::NotDir);
        }
        let main = self.main_of(parent, name);
        let mut companions = self.companions();
        // Never acquire the kernel table under the companion lock. If the
        // index bound is reached, snapshot the live names and inodes first,
        // then recheck everything under one lock.
        let live = if companions.slotted >= companions.held_limit {
            drop(companions);
            let live = {
                let table = self.table();
                (
                    table.by_name.keys().cloned().collect::<HashSet<_>>(),
                    table.by_name.values().copied().collect::<HashSet<_>>(),
                )
            };
            companions = self.companions();
            Some(live)
        } else {
            None
        };
        let named = (parent, name.to_vec());
        if let Some(serial) = companions.resolve(&named, main) {
            let stat = Companions::stat(serial, &companions.files[&serial]);
            companions.use_serial(serial);
            drop(companions);
            // Opened again: UNCHECKED applies its size, as an open with
            // truncation would; GUARDED and EXCLUSIVE find it taken.
            let How::Unchecked(sattr) = how else {
                return Err(Status::Exist);
            };
            return match sattr.size {
                Some(size) => {
                    let sized = Sattr {
                        size: Some(size),
                        ..Sattr::default()
                    };
                    let (stat, _) = self.companion_setattr(stat.serial, sized, None, tickets)?;
                    Ok((stat, directory))
                }
                None => Ok((stat, directory)),
            };
        }
        let reclaimed = match (&live, companions.slotted >= companions.held_limit) {
            (Some((names, serials)), true) => companions.reclaim_orphans(names, serials),
            _ => Vec::new(),
        };
        if companions.slotted >= companions.held_limit {
            let held = companions.slotted;
            let limit = companions.held_limit;
            let log = companions
                .last_refusal_log
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(60));
            let namespace = log.then(|| companions.spill_dir.clone());
            if log {
                companions.last_refusal_log = Some(Instant::now());
            }
            drop(companions);
            unlink_paths(reclaimed);
            let refused = self.names_refused.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(namespace) = namespace {
                eprintln!(
                    "layerfs-nfs: AppleDouble index full: export={:?} namespace={namespace:?} parent={parent} companions={held} limit={limit} names_refused={refused}; NFS3ERR_NOSPC",
                    self.mount()
                );
            }
            return Err(Status::NoSpc);
        }
        let mut metadata = now();
        if let How::Unchecked(sattr) | How::Guarded(sattr) = how {
            if let Some(mode) = sattr.mode {
                metadata.mode = mode & 0o777;
            }
        }
        let serial = SERIALS + companions.next;
        companions.next += 1;
        let used = companions.tick();
        let file = Companion {
            named,
            inode: main,
            whole: false,
            len: 0,
            body: Body::Resident(Vec::new()),
            metadata,
            used,
        };
        let stat = Companions::stat(serial, &file);
        companions.index(serial, &file);
        companions.attach(serial, file);
        drop(companions);
        unlink_paths(reclaimed);
        self.names_detached.fetch_add(1, Ordering::Relaxed);
        Ok((stat, directory))
    }
    /// REMOVE of a companion name, with the directory's `wcc_data`; `None`
    /// when no companion has that name.
    pub(crate) fn companion_remove(&self, parent: u64, name: &[u8]) -> Option<Wcc> {
        let main = self.main_of(parent, name);
        let spilled = {
            let mut companions = self.companions();
            let serial = companions.resolve(&(parent, name.to_vec()), main)?;
            companions.take_serial(serial)
        };
        unlink_paths(spilled);
        Some(self.directory_wcc(parent))
    }
    /// The main file `name` in `parent` was removed, silly-renamed or replaced
    /// by `RENAME`: drops the companion held under `._<name>`, or only that
    /// name when the companion belongs to an inode, which goes with the
    /// inode's last name instead (see [`Kernel::companion_drop_inode`]). A
    /// plain non-replacing `RENAME` of a main file only cleans the destination
    /// name `to/new_name` (via `finish_unlink`) and leaves `from/name` alone so
    /// a client that renames `a -> b` and then renames `._a -> ._b` can still
    /// move the companion across.
    pub(crate) fn companion_drop_for(&self, parent: u64, name: &[u8]) {
        if crate::names::apple_double(name) {
            return;
        }
        let mut paired = Vec::with_capacity(2 + name.len());
        paired.extend_from_slice(b"._");
        paired.extend_from_slice(name);
        let named = (parent, paired);
        let spilled = {
            let mut companions = self.companions();
            let Some(&serial) = companions.names.get(&named) else {
                return;
            };
            if companions
                .files
                .get(&serial)
                .is_some_and(|file| file.inode.is_some())
            {
                companions.names.remove(&named);
                None
            } else {
                companions.take_serial(serial)
            }
        };
        unlink_paths(spilled);
    }
    /// Drops the companion of inode `main`, whose last name is gone.
    pub(crate) fn companion_drop_inode(&self, main: u64) {
        let spilled = {
            let mut companions = self.companions();
            let Some(serial) = companions.inodes.get(&main).copied() else {
                return;
            };
            companions.take_serial(serial)
        };
        unlink_paths(spilled);
    }
    /// RENAME of a companion name: only to another AppleDouble name, which it
    /// replaces when held. It then belongs to the destination's main inode, or
    /// to none when that is unknown. `None` when no companion has that name.
    pub(crate) fn companion_rename(
        &self,
        from: u64,
        name: &[u8],
        to: u64,
        new_name: &[u8],
    ) -> Option<Result<(), Status>> {
        let from_main = self.main_of(from, name);
        let to_main = self.main_of(to, new_name);
        let mut replaced_path = None;
        {
            let mut companions = self.companions();
            let serial = companions.resolve(&(from, name.to_vec()), from_main)?;
            if !crate::names::apple_double(new_name) || new_name.len() > 255 {
                return Some(Err(Status::Access));
            }
            let target = (to, new_name.to_vec());
            if let Some(replaced) = companions.resolve(&target, to_main) {
                if replaced != serial {
                    replaced_path = companions.take_serial(replaced);
                }
            }
            let tick = companions.tick();
            let mut file = companions.detach(serial)?;
            companions.unindex(serial, &file);
            file.named = target;
            file.inode = to_main;
            file.used = tick;
            // A client may write a companion under another name first.
            if matches!(file.body, Body::Resident(_) | Body::Decoded(_)) {
                companions.classify(&mut file);
            }
            companions.index(serial, &file);
            companions.attach(serial, file);
        }
        unlink_paths(replaced_path);
        Some(Ok(()))
    }
    /// READ of a companion, at most `count` bytes from `offset`, appended to
    /// `out`; `eof` when the reply reaches the companion's end.
    pub(crate) fn companion_read(
        &self,
        serial: u64,
        offset: u64,
        count: u32,
        out: &mut Vec<u8>,
    ) -> Result<bool, Status> {
        let (spilled, length, start, end) = {
            let mut companions = self.companions();
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            let length = file.len as u64;
            let start = offset.min(length) as usize;
            let end = offset.saturating_add(u64::from(count)).min(length) as usize;
            let spilled = match &file.body {
                Body::Resident(bytes) => {
                    // A body and its length are recorded under one lock; clamp
                    // anyway so no reply slices past the bytes it holds.
                    let stop = end.min(bytes.len());
                    out.extend_from_slice(bytes.get(start..stop).unwrap_or_default());
                    false
                }
                Body::Decoded(image) => {
                    // The image the client wrote, synthesized again.
                    let bytes = image.encode();
                    let stop = end.min(bytes.len());
                    out.extend_from_slice(bytes.get(start..stop).unwrap_or_default());
                    false
                }
                Body::Spilled => true,
            };
            companions.use_serial(serial);
            (spilled, length, start, end)
        };
        if spilled {
            let mut buf = vec![0u8; end - start];
            self.spill_file(serial, false)?
                .read_exact_at(&mut buf, start as u64)
                .map_err(|e| io_status(&e))?;
            out.extend_from_slice(&buf);
        }
        Ok(end as u64 >= length)
    }
    /// WRITE to a companion: all of it or nothing.
    pub(crate) fn companion_write(
        &self,
        serial: u64,
        offset: u64,
        data: &[u8],
    ) -> Result<(u32, Wcc), Status> {
        let end = offset.checked_add(data.len() as u64).ok_or(Status::FBig)?;
        if end > FILE_BYTES {
            return Err(Status::FBig);
        }
        let before = self.companion_stat(serial)?;
        let after = self.edit_companion(
            serial,
            Edit::Write {
                offset: offset as usize,
                data,
            },
        )?;
        Ok((data.len() as u32, (Some(before), Some(after))))
    }
    /// Applies `edit` to `serial` and answers its stat afterwards: in the
    /// resident body while the budget holds it, else in its spilled file
    /// outside the companion lock.
    fn edit_companion(&self, serial: u64, edit: Edit<'_>) -> Result<ViewStat, Status> {
        let target = edit.target_len();
        // A round ends in the body, or in another call having changed this
        // companion while its body was written out, which decides again.
        for _ in 0..3 {
            let spilled = match self.edit_resident(serial, target, &edit)? {
                Room::Resident(stat) => return Ok(stat),
                Room::Spilled => true,
                Room::Grow(need) => {
                    if self.spill_for(serial, need)? {
                        continue;
                    }
                    self.spill_one(serial, Some(target))?
                }
            };
            if spilled {
                edit.disk(&self.spill_file(serial, true)?)?;
                return self.record_edit(serial, &edit);
            }
        }
        // Only other calls changing this companion in every round get here.
        Err(Status::Io)
    }
    /// Applies `edit` to `serial`'s body in memory while the budget holds it
    /// (a decoded body is encoded first), recording its length, recency and
    /// time and deciding again whether it is a whole image, under the same
    /// lock, so no other call ever sees a body and a length that disagree.
    fn edit_resident(&self, serial: u64, target: usize, edit: &Edit<'_>) -> Result<Room, Status> {
        let mut companions = self.companions();
        let avail = companions
            .resident_budget
            .saturating_sub(companions.resident_bytes);
        let file = companions.files.get(&serial).ok_or(Status::Stale)?;
        let need = match &file.body {
            Body::Spilled => return Ok(Room::Spilled),
            Body::Resident(bytes) => target.saturating_sub(bytes.len()),
            Body::Decoded(image) => edited_len(edit, image.len()),
        };
        if need > avail {
            // The body does not fit the resident budget, but the edit may
            // complete a whole image, whose decoded form costs its attributes
            // and no body budget. Only a body small enough to decode is tried.
            if target > DECODED_ENTRY_BYTES + crate::apple_double_codec::IMAGE_UNIT {
                return Ok(Room::Grow(need));
            }
            let mut bytes = match &file.body {
                Body::Resident(bytes) => bytes.clone(),
                Body::Decoded(image) => image.encode(),
                Body::Spilled => return Ok(Room::Spilled),
            };
            let len = edited_len(edit, file.len);
            edit.memory(&mut bytes);
            if companions.keep_decoded(serial, &bytes, len) {
                let stat = companions
                    .files
                    .get(&serial)
                    .map(|file| Companions::stat(serial, file));
                if let Some(stat) = stat {
                    return Ok(Room::Resident(stat));
                }
            }
            return Ok(Room::Grow(need));
        }
        let tick = companions.tick();
        let mut file = companions.detach(serial).ok_or(Status::Stale)?;
        let mut bytes = match std::mem::replace(&mut file.body, Body::Spilled) {
            Body::Resident(bytes) => bytes,
            Body::Decoded(image) => image.encode(),
            Body::Spilled => {
                companions.attach(serial, file);
                return Ok(Room::Spilled);
            }
        };
        edit.memory(&mut bytes);
        file.body = Body::Resident(bytes);
        record(edit, &mut file);
        touch(&mut file.metadata);
        file.used = tick;
        companions.classify(&mut file);
        let stat = Companions::stat(serial, &file);
        companions.attach(serial, file);
        Ok(Room::Resident(stat))
    }
    /// Records an edit applied to `serial`'s spilled file and answers its
    /// stat. A body changed on disk is no longer known to be a whole image.
    fn record_edit(&self, serial: u64, edit: &Edit<'_>) -> Result<ViewStat, Status> {
        let mut companions = self.companions();
        let tick = companions.tick();
        let mut file = companions.detach(serial).ok_or(Status::Stale)?;
        record(edit, &mut file);
        file.used = tick;
        file.whole = false;
        touch(&mut file.metadata);
        let stat = Companions::stat(serial, &file);
        companions.attach(serial, file);
        Ok(stat)
    }
    /// SETATTR of a companion: mode, size and modification time; the
    /// owner is the export's identity, as for every inode.
    pub(crate) fn companion_setattr(
        &self,
        serial: u64,
        sattr: Sattr,
        guard: Option<(u32, u32)>,
        _tickets: &mut Tickets,
    ) -> Result<(ViewStat, Wcc), Status> {
        let identity = self.identity();
        if sattr.uid.is_some_and(|uid| uid != identity.uid)
            || sattr.gid.is_some_and(|gid| gid != identity.gid)
        {
            return Err(Status::Perm);
        }
        let before = {
            let companions = self.companions();
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            let before = Companions::stat(serial, file);
            if let Some(expected) = guard {
                if crate::attributes::ctime(&before) != expected {
                    return Err(Status::NotSync);
                }
            }
            before
        };
        if let Some(size) = sattr.size {
            if size > FILE_BYTES {
                return Err(Status::FBig);
            }
            self.edit_companion(serial, Edit::Size(size as usize))?;
        }
        let mut companions = self.companions();
        let tick = companions.tick();
        let file = companions.files.get_mut(&serial).ok_or(Status::Stale)?;
        file.used = tick;
        if let Some(mode) = sattr.mode {
            file.metadata.mode = mode & 0o777;
        }
        match sattr.mtime {
            Some(Stamp::At(time)) => {
                file.metadata.mtime_seconds = time.seconds;
                file.metadata.mtime_nanoseconds = time.nanoseconds;
            }
            Some(Stamp::Processing) => touch(&mut file.metadata),
            None if sattr.size.is_some() => touch(&mut file.metadata),
            None => {}
        }
        let after = Companions::stat(serial, file);
        Ok((after.clone(), (Some(before), Some(after))))
    }
    /// Drops every companion and removes the export's spill directory; returns
    /// what the index held.
    pub(crate) fn companions_release(&self) -> Holding {
        let (held, dir) = {
            let mut companions = self.companions();
            let dir = companions.spill_dir.take();
            (std::mem::take(&mut *companions), dir)
        };
        if let Some(dir) = dir {
            drop(fs::remove_dir_all(dir));
        }
        let holding = held.holding();
        if holding.held > 0 {
            eprintln!(
                "layerfs-nfs: AppleDouble companions released: export={:?} held={} slotted={} decoded={} decoded_bytes={} resident_bytes={}",
                self.mount(),
                holding.held,
                holding.slotted,
                holding.decoded,
                holding.decoded_bytes,
                holding.resident_bytes
            );
        }
        holding
    }
    /// What the companion index holds now.
    pub(crate) fn companions_held(&self) -> Holding {
        self.companions().holding()
    }
}
