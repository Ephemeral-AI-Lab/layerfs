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
//! `pread`/`pwrite` outside the companion lock). Removing or replacing a main
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
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
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

enum Body {
    Resident(Vec<u8>),
    Spilled(Arc<File>),
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
            Body::Spilled(_) => 0,
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
    pub(crate) fn len(&self) -> usize {
        self.files.len()
    }
    pub(crate) fn spilled(&self) -> usize {
        self.files
            .values()
            .filter(|file| matches!(file.body, Body::Spilled(_)))
            .count()
    }
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
    fn spill_path(&self, serial: u64) -> Option<PathBuf> {
        self.spill_dir
            .as_ref()
            .map(|dir| dir.join(serial.to_string()))
    }
    fn take_serial(&mut self, serial: u64) -> Option<PathBuf> {
        let file = self.files.remove(&serial)?;
        self.names.remove(&file.named);
        self.resident_bytes -= file.resident_len();
        match file.body {
            Body::Resident(_) => None,
            Body::Spilled(_) => self.spill_path(serial),
        }
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
fn create_spilled(dir: &Path, serial: u64, bytes: &[u8], len: u64) -> Result<Arc<File>, Status> {
    fs::create_dir_all(dir).map_err(|e| io_status(&e))?;
    let path = dir.join(serial.to_string());
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| io_status(&e))?;
    if !bytes.is_empty() {
        if let Err(e) = file.write_all_at(bytes, 0) {
            drop(file);
            drop(fs::remove_file(&path));
            return Err(io_status(&e));
        }
    }
    if len > bytes.len() as u64 {
        if let Err(e) = file.set_len(len) {
            drop(file);
            drop(fs::remove_file(&path));
            return Err(io_status(&e));
        }
    }
    Ok(Arc::new(file))
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
    let fresh = now();
    metadata.mtime_seconds = fresh.mtime_seconds;
    metadata.mtime_nanoseconds = fresh.mtime_nanoseconds;
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
            let (candidate, dir, bytes, used) = {
                let companions = self.companions();
                if companions
                    .resident_bytes
                    .checked_add(need)
                    .is_some_and(|total| total <= companions.resident_budget)
                {
                    return Ok(true);
                }
                let Some(dir) = companions.spill_dir.clone() else {
                    return Ok(false);
                };
                let Some(victim) = companions.oldest_resident(Some(target)) else {
                    return Ok(false);
                };
                let file = &companions.files[&victim];
                let Body::Resident(ref data) = file.body else {
                    continue;
                };
                (victim, dir, data.clone(), file.used)
            };
            let spilled = create_spilled(&dir, candidate, &bytes, bytes.len() as u64)?;
            let mut unlink = None;
            {
                let mut companions = self.companions();
                match companions.files.get_mut(&candidate) {
                    Some(file) if file.used == used && matches!(file.body, Body::Resident(_)) => {
                        let old = file.resident_len();
                        file.body = Body::Spilled(spilled);
                        companions.resident_bytes -= old;
                    }
                    _ => {
                        unlink = Some(dir.join(candidate.to_string()));
                    }
                }
            }
            if let Some(path) = unlink {
                drop(fs::remove_file(path));
            }
        }
    }
    /// Transitions `serial` itself to a spilled file of length `new_len`
    /// without holding the companion lock across disk I/O.
    fn spill_self(&self, serial: u64, new_len: usize) -> Result<Arc<File>, Status> {
        let (dir, existing, old_len) = {
            let companions = self.companions();
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            if let Body::Spilled(ref handle) = file.body {
                return Ok(handle.clone());
            }
            let dir = companions.spill_dir.clone().ok_or(Status::NoSpc)?;
            let Body::Resident(ref data) = file.body else {
                unreachable!()
            };
            let copy = data.len().min(new_len);
            (dir, data[..copy].to_vec(), file.len)
        };
        let target_len = new_len.max(old_len) as u64;
        let handle = create_spilled(&dir, serial, &existing, target_len)?;
        let mut unlink = None;
        let chosen = {
            let mut companions = self.companions();
            match companions.files.get_mut(&serial) {
                Some(file) => match &file.body {
                    Body::Resident(_) => {
                        let old_res = file.resident_len();
                        file.body = Body::Spilled(handle.clone());
                        if target_len as usize > file.len {
                            file.len = target_len as usize;
                        }
                        companions.resident_bytes -= old_res;
                        Ok(handle)
                    }
                    Body::Spilled(already) => {
                        unlink = Some(dir.join(serial.to_string()));
                        Ok(already.clone())
                    }
                },
                None => {
                    unlink = Some(dir.join(serial.to_string()));
                    Err(Status::Stale)
                }
            }
        };
        if let Some(path) = unlink {
            drop(fs::remove_file(path));
        }
        chosen
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
            companions.len() >= companions.held_limit
        };
        let mut reclaimed = Vec::new();
        if at_limit {
            let live: HashSet<(u64, Vec<u8>)> = self.table().by_name.keys().cloned().collect();
            let mut companions = self.companions();
            if companions.len() >= companions.held_limit {
                reclaimed = companions.reclaim_orphans(&live);
            }
        }
        for path in reclaimed {
            drop(fs::remove_file(path));
        }
        let mut companions = self.companions();
        if companions.len() >= companions.held_limit {
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
    /// REMOVE of a companion name, with the directory's `wcc_data`; `None`
    /// when no companion has that name.
    pub(crate) fn companion_remove(&self, parent: u64, name: &[u8]) -> Option<Wcc> {
        let spilled = {
            let mut companions = self.companions();
            let serial = companions.names.get(&(parent, name.to_vec())).copied()?;
            companions.take_serial(serial)
        };
        if let Some(path) = spilled {
            drop(fs::remove_file(path));
        }
        Some(self.directory_wcc(parent))
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
        let spilled = {
            let mut companions = self.companions();
            companions
                .names
                .get(&(parent, paired))
                .copied()
                .and_then(|serial| companions.take_serial(serial))
        };
        if let Some(path) = spilled {
            drop(fs::remove_file(path));
        }
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
        if let Some(path) = replaced_path {
            drop(fs::remove_file(path));
        }
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
            match &file.body {
                Body::Resident(bytes) => {
                    out.extend_from_slice(&bytes[start..end]);
                    companions.use_serial(serial);
                    return Ok(end as u64 >= length);
                }
                Body::Spilled(handle) => {
                    let cloned = handle.clone();
                    companions.use_serial(serial);
                    (cloned, length, start, end)
                }
            }
        };
        let mut buf = vec![0u8; end - start];
        if !buf.is_empty() {
            spilled
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
        let end_usize = end as usize;
        let (before, spilled_handle, need_bytes) = {
            let mut companions = self.companions();
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            let before = Companions::stat(serial, file);
            match &file.body {
                Body::Spilled(handle) => (before, Some(handle.clone()), 0),
                Body::Resident(bytes) => {
                    let add = end_usize.saturating_sub(bytes.len());
                    if companions.resident_bytes + add <= companions.resident_budget {
                        let tick = companions.tick();
                        if let Some(file) = companions.files.get_mut(&serial) {
                            if let Body::Resident(ref mut buf) = file.body {
                                if end_usize > buf.len() {
                                    buf.resize(end_usize, 0);
                                }
                                buf[offset as usize..end_usize].copy_from_slice(data);
                            }
                            file.len = file.len.max(end_usize);
                            file.used = tick;
                            touch(&mut file.metadata);
                        }
                        companions.resident_bytes += add;
                        let after = Companions::stat(serial, &companions.files[&serial]);
                        return Ok((data.len() as u32, (Some(before), Some(after))));
                    }
                    (before, None, add)
                }
            }
        };
        if let Some(handle) = spilled_handle {
            handle
                .write_all_at(data, offset)
                .map_err(|e| io_status(&e))?;
            let mut companions = self.companions();
            let tick = companions.tick();
            let file = companions.files.get_mut(&serial).ok_or(Status::Stale)?;
            file.len = file.len.max(end_usize);
            file.used = tick;
            touch(&mut file.metadata);
            let after = Companions::stat(serial, file);
            return Ok((data.len() as u32, (Some(before), Some(after))));
        }
        if self.spill_for(serial, need_bytes)? {
            let mut companions = self.companions();
            let avail = companions
                .resident_budget
                .saturating_sub(companions.resident_bytes);
            if let Some(file) = companions.files.get_mut(&serial) {
                if let Body::Resident(ref mut buf) = file.body {
                    let add = end_usize.saturating_sub(buf.len());
                    if add <= avail {
                        if end_usize > buf.len() {
                            buf.resize(end_usize, 0);
                        }
                        buf[offset as usize..end_usize].copy_from_slice(data);
                        file.len = file.len.max(end_usize);
                        touch(&mut file.metadata);
                        let after = Companions::stat(serial, file);
                        companions.resident_bytes += add;
                        companions.use_serial(serial);
                        return Ok((data.len() as u32, (Some(before), Some(after))));
                    }
                }
            }
        }
        let handle = self.spill_self(serial, end_usize)?;
        handle
            .write_all_at(data, offset)
            .map_err(|e| io_status(&e))?;
        let mut companions = self.companions();
        let tick = companions.tick();
        let file = companions.files.get_mut(&serial).ok_or(Status::Stale)?;
        file.len = file.len.max(end_usize);
        file.used = tick;
        touch(&mut file.metadata);
        let after = Companions::stat(serial, file);
        Ok((data.len() as u32, (Some(before), Some(after))))
    }
    /// Resizes `serial` to `size` for SETATTR / UNCHECKED CREATE, spilling if
    /// needed outside the companion lock.
    fn companion_resize(&self, serial: u64, size: u64) -> Result<(), Status> {
        if size > FILE_BYTES {
            return Err(Status::FBig);
        }
        let new_len = size as usize;
        let (spilled_handle, need_bytes) = {
            let mut companions = self.companions();
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            match &file.body {
                Body::Spilled(handle) => (Some(handle.clone()), 0),
                Body::Resident(bytes) => {
                    if new_len <= bytes.len() {
                        let sub = bytes.len() - new_len;
                        let file = companions.files.get_mut(&serial).unwrap();
                        if let Body::Resident(ref mut buf) = file.body {
                            buf.truncate(new_len);
                        }
                        file.len = new_len;
                        companions.resident_bytes -= sub;
                        return Ok(());
                    }
                    let add = new_len - bytes.len();
                    if companions.resident_bytes + add <= companions.resident_budget {
                        if let Some(file) = companions.files.get_mut(&serial) {
                            if let Body::Resident(ref mut buf) = file.body {
                                buf.resize(new_len, 0);
                            }
                            file.len = new_len;
                        }
                        companions.resident_bytes += add;
                        return Ok(());
                    }
                    (None, add)
                }
            }
        };
        if let Some(handle) = spilled_handle {
            handle.set_len(size).map_err(|e| io_status(&e))?;
            let mut companions = self.companions();
            let file = companions.files.get_mut(&serial).ok_or(Status::Stale)?;
            file.len = new_len;
            return Ok(());
        }
        if self.spill_for(serial, need_bytes)? {
            let mut companions = self.companions();
            let avail = companions
                .resident_budget
                .saturating_sub(companions.resident_bytes);
            if let Some(file) = companions.files.get_mut(&serial) {
                if let Body::Resident(ref mut buf) = file.body {
                    let add = new_len.saturating_sub(buf.len());
                    if add <= avail {
                        buf.resize(new_len, 0);
                        file.len = new_len;
                        companions.resident_bytes += add;
                        return Ok(());
                    }
                }
            }
        }
        let handle = self.spill_self(serial, new_len)?;
        handle.set_len(size).map_err(|e| io_status(&e))?;
        let mut companions = self.companions();
        let file = companions.files.get_mut(&serial).ok_or(Status::Stale)?;
        file.len = new_len;
        Ok(())
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
        let foreign = sattr.uid.is_some_and(|uid| uid != identity.uid)
            || sattr.gid.is_some_and(|gid| gid != identity.gid);
        if foreign {
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
            self.companion_resize(serial, size)?;
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
    /// `(companions, companion_bytes, companions_spilled)`.
    pub(crate) fn companions_release(&self) -> (usize, usize, usize) {
        let (held, bytes, spilled, dir) = {
            let mut companions = self.companions();
            let held = companions.len();
            let bytes = companions.resident_bytes;
            let spilled = companions.spilled();
            let dir = companions.spill_dir.take();
            *companions = Companions::default();
            (held, bytes, spilled, dir)
        };
        if let Some(dir) = dir {
            drop(fs::remove_dir_all(dir));
        }
        (held, bytes, spilled)
    }
    /// Companion holdings now: `(companions, companion_bytes, companions_spilled)`.
    pub(crate) fn companions_held(&self) -> (usize, usize, usize) {
        let companions = self.companions();
        (
            companions.len(),
            companions.resident_bytes,
            companions.spilled(),
        )
    }
}
