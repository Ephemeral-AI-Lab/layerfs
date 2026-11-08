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
//! Every companion keeps its index entry in memory (at most [`HELD`]
//! companions, each of at most [`FILE_BYTES`]). Bodies up to [`RESIDENT_BYTES`]
//! in all stay in memory; when a write or resize crosses that budget, the
//! least recently touched resident bodies are spilled to the export's own
//! scratch directory (one file per companion serial, read and written with
//! `pread`/`pwrite` outside the companion lock). A spill writes a temporary
//! `<serial>.<n>.tmp` that only the call adopting it renames to `<serial>`, so
//! two calls spilling the same companion never share a file, and no descriptor
//! is kept for a spilled body: an operation opens its file by path and closes
//! it again, so open descriptors grow with concurrent operations and not with
//! the number of spilled companions. Removing or replacing a main
//! file drops its paired `._<name>` companion and any spilled file with it;
//! when [`HELD`] is reached, orphan companions whose main file is no longer in
//! the export's name index are reclaimed first, and only if [`HELD`] live
//! companions still remain does `CREATE` answer `NFS3ERR_ACCES`.
use crate::{
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
    os::unix::fs::{DirBuilderExt, FileExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// The first serial of the export's own companions, far above any serial the
/// engine allocates.
pub const SERIALS: u64 = 1 << 62;
/// Companion index entries held at once.
pub const HELD: usize = 65_536;
/// Largest companion kept.
pub const FILE_BYTES: u64 = 1 << 20;
/// Companion payload bytes kept resident in memory before spilling to disk.
pub const RESIDENT_BYTES: usize = 32 << 20;
/// Mode of a companion created without one.
const MODE: u32 = 0o644;
/// Temporary names tried for one spilled body before the spill gives up.
const SPILL_TEMP_TRIES: u32 = 64;
/// Counter behind the distinct temporary name of each spilled body.
static SPILL_TEMP: AtomicU64 = AtomicU64::new(0);

enum Body {
    Resident(Vec<u8>),
    /// On disk in the spill directory, under the companion's own serial; each
    /// operation opens it by path instead of holding it open.
    Spilled,
}

struct Companion {
    named: (u64, Vec<u8>),
    len: usize,
    body: Body,
    metadata: PortableMetadata,
    used: u64,
}
impl Companion {
    fn resident_len(&self) -> usize {
        match &self.body {
            Body::Resident(bytes) => bytes.len(),
            Body::Spilled => 0,
        }
    }
}

/// The companions of one export.
pub(crate) struct Companions {
    next: u64,
    tick: u64,
    files: HashMap<u64, Companion>,
    names: HashMap<(u64, Vec<u8>), u64>,
    resident_bytes: usize,
    spill_dir: Option<PathBuf>,
    resident_budget: usize,
    held_limit: usize,
}
impl Default for Companions {
    fn default() -> Self {
        Self {
            next: 0,
            tick: 0,
            files: HashMap::new(),
            names: HashMap::new(),
            resident_bytes: 0,
            spill_dir: None,
            resident_budget: RESIDENT_BYTES,
            held_limit: HELD,
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
    fn take_serial(&mut self, serial: u64) -> Option<PathBuf> {
        let file = self.files.remove(&serial)?;
        self.names.remove(&file.named);
        self.resident_bytes -= file.resident_len();
        match file.body {
            Body::Resident(_) => None,
            Body::Spilled => (self.spill_dir.as_ref()).map(|dir| spill_path(dir, serial)),
        }
    }
    /// Renames the temporary file `temp`, which holds `serial`'s body of `len`
    /// bytes, to the companion's own name in the spill directory and switches
    /// the companion to it, when nothing touched it since `used` was read and
    /// its body is still resident. A body another call changed or already
    /// spilled stays: `Ok(false)` then, and `temp` is the caller's to unlink.
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
        if !spill_applies(used, file.used, matches!(file.body, Body::Resident(_))) {
            return Ok(false);
        }
        fs::rename(temp, path).map_err(|e| io_status(&e))?;
        let old = file.resident_len();
        file.body = Body::Spilled;
        file.len = file.len.max(len);
        self.resident_bytes -= old;
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
    /// Reclaims companions whose paired main file `(parent, name)` is not in
    /// `live_names`. Returns any spilled file paths to unlink outside the lock.
    fn reclaim_orphans(&mut self, live_names: &HashSet<(u64, Vec<u8>)>) -> Vec<PathBuf> {
        let orphans: Vec<u64> = self
            .files
            .iter()
            .filter(|(_, file)| {
                let (parent, ref name) = file.named;
                name.strip_prefix(b"._")
                    .is_some_and(|main| !live_names.contains(&(parent, main.to_vec())))
            })
            .map(|(serial, _)| *serial)
            .collect();
        orphans
            .into_iter()
            .filter_map(|serial| self.take_serial(serial))
            .collect()
    }
}
fn io_status(error: &io::Error) -> Status {
    match (error.kind(), error.raw_os_error()) {
        (io::ErrorKind::StorageFull, _) | (_, Some(28 | 69 | 122)) => Status::NoSpc,
        (_, Some(27)) => Status::FBig,
        (_, Some(1)) => Status::Perm,
        (io::ErrorKind::PermissionDenied, _) | (_, Some(13)) => Status::Access,
        (io::ErrorKind::ReadOnlyFilesystem, _) | (_, Some(30)) => Status::RoFs,
        _ => Status::Io,
    }
}
/// The spill directory's own file of `serial`.
fn spill_path(dir: &Path, serial: u64) -> PathBuf {
    dir.join(serial.to_string())
}
/// Creates `dir` and a temporary file in it holding `bytes` extended to `len`
/// bytes, then closes it. Two calls spilling the same `serial` write different
/// temporary files, and only the call whose file is adopted renames it to the
/// companion's own name. Only this user reads companion bodies, so the modes
/// are set here rather than left to the caller's umask.
fn create_spilled(dir: &Path, serial: u64, bytes: &[u8], len: u64) -> Result<PathBuf, Status> {
    let made = fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir);
    made.map_err(|e| io_status(&e))?;
    for _ in 0..SPILL_TEMP_TRIES {
        let name = SPILL_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("{serial}.{name}.tmp"));
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_status(&e)),
        };
        let written = (file.write_all_at(bytes, 0)).and_then(|()| file.set_len(len));
        drop(file);
        return match written {
            Ok(()) => Ok(path),
            Err(e) => {
                drop(fs::remove_file(&path));
                Err(io_status(&e))
            }
        };
    }
    Err(Status::Io)
}
/// Unlinks the spilled files a companion operation retired, outside the lock.
fn unlink_paths(paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        drop(fs::remove_file(path));
    }
}
fn now() -> PortableMetadata {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    PortableMetadata {
        mode: MODE,
        mtime_seconds: i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX),
        mtime_nanoseconds: elapsed.subsec_nanos(),
    }
}
fn touch(metadata: &mut PortableMetadata) {
    let mode = metadata.mode;
    *metadata = now();
    metadata.mode = mode;
}
/// One change to a companion body, applied to the resident bytes under the
/// lock or to the spilled file outside it.
enum Edit<'a> {
    Write { offset: usize, data: &'a [u8] },
    Size(usize),
}
impl Edit<'_> {
    /// The body length this edit needs; a write keeps what the body had beyond
    /// its end.
    fn target_len(&self) -> usize {
        match self {
            Edit::Write { offset, data } => offset + data.len(),
            Edit::Size(size) => *size,
        }
    }
    /// Applies the edit to resident bytes, which already hold `target_len`.
    fn memory(&self, bytes: &mut Vec<u8>) {
        match self {
            Edit::Write { offset, data } => {
                bytes.resize(bytes.len().max(offset + data.len()), 0);
                bytes[*offset..*offset + data.len()].copy_from_slice(data);
            }
            Edit::Size(size) => bytes.resize(*size, 0),
        }
    }
    /// Applies the edit to a spilled file, outside the companion lock.
    fn disk(&self, file: &File) -> Result<(), Status> {
        match self {
            Edit::Write { offset, data } => {
                (file.write_all_at(data, *offset as u64)).map_err(|e| io_status(&e))
            }
            Edit::Size(size) => (file.set_len(*size as u64)).map_err(|e| io_status(&e)),
        }
    }
    /// Records the edit's length on `file`.
    fn record(&self, file: &mut Companion) {
        file.len = match self {
            Edit::Write { .. } => file.len.max(self.target_len()),
            Edit::Size(size) => *size,
        };
    }
}
/// Where [`Kernel::edit_resident`] found a companion's body.
enum Room {
    /// Applied to the resident body, whose length, recency and time were
    /// recorded under the same lock: this is the companion's stat afterwards.
    Resident(ViewStat),
    /// On disk: finish with its file outside the lock.
    Spilled,
    /// Resident, but growing it needs this many more budget bytes first.
    Grow(usize),
}
/// Whether a body a spill wrote out still replaces its companion: only when the
/// companion is the one the snapshot saw (`used` is the tick read then) and its
/// body is still resident, so no call wrote to it while the file was written.
pub fn spill_applies(used: u64, file_used: u64, resident: bool) -> bool {
    used == file_used && resident
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
    /// LOOKUP of a companion name, if one is held.
    pub(crate) fn companion_named(&self, parent: u64, name: &[u8]) -> Option<ViewStat> {
        let mut companions = self.companions();
        let serial = *companions.names.get(&(parent, name.to_vec()))?;
        let stat = companions
            .files
            .get(&serial)
            .map(|file| Companions::stat(serial, file))?;
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
        if let Some(stat) = self.companion_named(parent, name) {
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
        if parent != self.root() && !self.table().parents.contains_key(&parent) {
            return Err(Status::NotDir);
        }
        let at_limit = {
            let companions = self.companions();
            companions.files.len() >= companions.held_limit
        };
        let mut reclaimed = Vec::new();
        if at_limit {
            let live: HashSet<(u64, Vec<u8>)> = self.table().by_name.keys().cloned().collect();
            let mut companions = self.companions();
            if companions.files.len() >= companions.held_limit {
                reclaimed = companions.reclaim_orphans(&live);
            }
        }
        unlink_paths(reclaimed);
        let mut companions = self.companions();
        if companions.files.len() >= companions.held_limit {
            drop(companions);
            self.names_refused.fetch_add(1, Ordering::Relaxed);
            return Err(Status::Access);
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
        let named = (parent, name.to_vec());
        companions.names.insert(named.clone(), serial);
        companions.files.insert(
            serial,
            Companion {
                named,
                len: 0,
                body: Body::Resident(Vec::new()),
                metadata,
                used,
            },
        );
        let file = &companions.files[&serial];
        let stat = Companions::stat(serial, file);
        drop(companions);
        self.names_detached.fetch_add(1, Ordering::Relaxed);
        Ok((stat, directory))
    }
    /// Takes the companion held under `(parent, name)` out of the index and
    /// unlinks its spilled file; `true` when one was held.
    fn companion_take(&self, parent: u64, name: &[u8]) -> bool {
        let spilled = {
            let mut companions = self.companions();
            let Some(serial) = companions.names.get(&(parent, name.to_vec())).copied() else {
                return false;
            };
            companions.take_serial(serial)
        };
        unlink_paths(spilled);
        true
    }
    /// REMOVE of a companion name, with the directory's `wcc_data`; `None`
    /// when no companion has that name.
    pub(crate) fn companion_remove(&self, parent: u64, name: &[u8]) -> Option<Wcc> {
        self.companion_take(parent, name)
            .then(|| self.directory_wcc(parent))
    }
    /// Drops the companion `._<name>` paired with `name` in `parent`, if one
    /// is held, when `name` itself is removed, silly-renamed or replaced by
    /// `RENAME`. A plain non-replacing `RENAME` of a main file only cleans
    /// the destination name `to/new_name` (via `finish_unlink`) and leaves
    /// `from/name` alone so a client that renames `a -> b` and then renames
    /// `._a -> ._b` can still move the companion across.
    pub(crate) fn companion_drop_for(&self, parent: u64, name: &[u8]) {
        if crate::names::apple_double(name) {
            return;
        }
        let mut paired = Vec::with_capacity(2 + name.len());
        paired.extend_from_slice(b"._");
        paired.extend_from_slice(name);
        self.companion_take(parent, &paired);
    }
    /// RENAME of a companion name: only to another AppleDouble name, which
    /// it replaces when held. `None` when no companion has that name.
    pub(crate) fn companion_rename(
        &self,
        from: u64,
        name: &[u8],
        to: u64,
        new_name: &[u8],
    ) -> Option<Result<(), Status>> {
        let mut replaced_path = None;
        {
            let mut companions = self.companions();
            let serial = *companions.names.get(&(from, name.to_vec()))?;
            if !crate::names::apple_double(new_name) || new_name.len() > 255 {
                return Some(Err(Status::Access));
            }
            let target = (to, new_name.to_vec());
            if let Some(replaced) = companions.names.get(&target).copied() {
                if replaced != serial {
                    replaced_path = companions.take_serial(replaced);
                }
            }
            companions.names.remove(&(from, name.to_vec()));
            companions.names.insert(target.clone(), serial);
            let tick = companions.tick();
            if let Some(file) = companions.files.get_mut(&serial) {
                file.named = target;
                file.used = tick;
            }
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
    /// Applies `edit` to `serial`'s resident body while the budget holds it,
    /// recording its length, recency and time under the same lock so no other
    /// call ever sees a body and a length that disagree.
    fn edit_resident(&self, serial: u64, target: usize, edit: &Edit<'_>) -> Result<Room, Status> {
        let mut guard = self.companions();
        let Companions {
            files,
            tick,
            resident_bytes,
            resident_budget,
            ..
        } = &mut *guard;
        let avail = resident_budget.saturating_sub(*resident_bytes);
        let Some(file) = files.get_mut(&serial) else {
            return Err(Status::Stale);
        };
        let before = match &mut file.body {
            Body::Spilled => return Ok(Room::Spilled),
            Body::Resident(bytes) => {
                let need = target.saturating_sub(bytes.len());
                if need > avail {
                    return Ok(Room::Grow(need));
                }
                let before = bytes.len();
                edit.memory(bytes);
                before
            }
        };
        edit.record(file);
        touch(&mut file.metadata);
        *tick += 1;
        file.used = *tick;
        *resident_bytes = *resident_bytes - before + file.resident_len();
        Ok(Room::Resident(Companions::stat(serial, file)))
    }
    /// Records an applied edit on `serial` and answers its stat.
    fn record_edit(&self, serial: u64, edit: &Edit<'_>) -> Result<ViewStat, Status> {
        let mut companions = self.companions();
        let tick = companions.tick();
        let file = companions.files.get_mut(&serial).ok_or(Status::Stale)?;
        edit.record(file);
        file.used = tick;
        touch(&mut file.metadata);
        Ok(Companions::stat(serial, file))
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
    /// how many companions it held and their resident payload bytes.
    pub(crate) fn companions_release(&self) -> (usize, usize) {
        let (held, dir) = {
            let mut companions = self.companions();
            let dir = companions.spill_dir.take();
            (std::mem::take(&mut *companions), dir)
        };
        if let Some(dir) = dir {
            drop(fs::remove_dir_all(dir));
        }
        (held.files.len(), held.resident_bytes)
    }
    /// Companions held now and their resident payload bytes.
    pub(crate) fn companions_held(&self) -> (usize, usize) {
        let companions = self.companions();
        (companions.files.len(), companions.resident_bytes)
    }
}
