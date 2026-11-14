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
//! Whenever a write leaves a companion holding a whole AppleDouble image the
//! client laid out itself (see [`crate::apple_double_codec`]), it is decoded: a
//! small one keeps only its FinderInfo and attributes in memory (each at most
//! [`DECODED_ENTRY_BYTES`] counted), and every read synthesizes the same bytes
//! again. Any other body (a resource fork, another layout, a half-written
//! image) is kept as the client wrote it.
//!
//! The memory is bounded: [`HELD`] companions of at most [`FILE_BYTES`] each,
//! and [`RESIDENT_BYTES`] of bodies in all, a body kept as written counted at
//! its length and a decoded one as [`Image::cost`], so a decoded image is never
//! memory the budget cannot see. Bodies up to that budget stay in memory; when
//! a write or resize crosses it, the least recently touched resident bodies are
//! spilled to the export's own scratch directory (see
//! [`crate::apple_double_spill`]), and a body that still does not fit is
//! written out itself. Everything is dropped when the export is released,
//! which removes that directory too.
use crate::{
    apple_double_codec::Image,
    apple_double_spill::{io_status, now, spill_path, touch, unlink_paths, Edit},
    attributes::Status,
    change::{How, Sattr, Tickets, Wcc},
    kernel::Kernel,
};
use layerfs_content::{filesystem::attributes::PortableMetadata, object::inode_leaf::InodeKind};
use layerfs_workspace::ViewStat;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc, Mutex},
};

mod edit;

/// The first serial of the export's own companions, far above any serial the
/// engine allocates.
pub const SERIALS: u64 = 1 << 62;
/// Companion index entries held at once.
pub const HELD: usize = 4096;
/// Largest companion kept.
pub const FILE_BYTES: u64 = 1 << 20;
/// Companion body bytes kept resident in memory before spilling to disk: a
/// body kept as written counts its length, a decoded one [`Image::cost`].
pub const RESIDENT_BYTES: usize = 32 << 20;
/// Most bytes counted for one decoded companion, [`Image::cost`]; a whole
/// image costing more keeps its bytes as written, so one companion cannot take
/// the whole budget.
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
    len: usize,
    body: Body,
    metadata: PortableMetadata,
    used: u64,
    /// One edit of this companion at a time: its body's bytes, its length and
    /// its spilled file change together, so a truncate and a write can never
    /// leave a length its file does not have, and no round has to decide
    /// again on a body another call changed under it.
    editing: Arc<Mutex<()>>,
}
impl Companion {
    /// The bytes it takes of [`RESIDENT_BYTES`]: its body as written, or what
    /// its decoded form is counted as.
    fn resident_cost(&self) -> usize {
        match &self.body {
            Body::Resident(bytes) => bytes.len(),
            Body::Spilled => 0,
            Body::Decoded(image) => image.cost(),
        }
    }
}

/// What an export's companion index holds (see [`crate::kernel::Released`]).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Holding {
    pub held: usize,
    pub resident_bytes: usize,
    pub decoded: usize,
}

/// The companions of one export.
pub(crate) struct Companions {
    next: u64,
    tick: u64,
    files: HashMap<u64, Companion>,
    names: HashMap<Named, u64>,
    resident_bytes: usize,
    decoded: usize,
    spill_dir: Option<PathBuf>,
    resident_budget: usize,
}
impl Default for Companions {
    fn default() -> Self {
        Self {
            next: 0,
            tick: 0,
            files: HashMap::new(),
            names: HashMap::new(),
            resident_bytes: 0,
            decoded: 0,
            spill_dir: None,
            resident_budget: RESIDENT_BYTES,
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
    /// Adds `file` to the index and to the byte counts.
    fn attach(&mut self, serial: u64, file: Companion) {
        self.resident_bytes += file.resident_cost();
        self.decoded += usize::from(matches!(file.body, Body::Decoded(_)));
        self.files.insert(serial, file);
    }
    /// Takes `serial` out of the index and the counts; its name key stays for
    /// the caller to unindex.
    fn detach(&mut self, serial: u64) -> Option<Companion> {
        let file = self.files.remove(&serial)?;
        self.resident_bytes -= file.resident_cost();
        self.decoded -= usize::from(matches!(file.body, Body::Decoded(_)));
        Some(file)
    }
    /// Replaces `serial`'s body by the decoded form of `bytes`, the caller's
    /// edited body of `len` bytes, while the resident budget takes it. The
    /// decoded form is counted as its attributes rather than its image, so a
    /// body that decodes usually needs much less room than its bytes.
    fn keep_decoded(&mut self, serial: u64, bytes: &[u8], len: usize) -> bool {
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
        if self.resident_bytes + cost > self.resident_budget {
            self.attach(serial, file);
            return false;
        }
        file.len = len;
        file.body = Body::Decoded(image);
        file.used = tick;
        touch(&mut file.metadata);
        self.attach(serial, file);
        true
    }
    /// Decides whether `file`'s body is a whole image: a resident one is
    /// decoded when it parses back to the same bytes and [`DECODED_ENTRY_BYTES`]
    /// takes it. `file` must be detached from the index.
    fn classify(&self, file: &mut Companion) {
        // A body on disk cannot be decoded here (that reads its file under
        // this lock): it keeps the class the write that spilled it left it.
        let Body::Resident(bytes) = &file.body else {
            return;
        };
        let Some(image) = Image::compact(bytes) else {
            return;
        };
        if image.cost() <= DECODED_ENTRY_BYTES {
            file.body = Body::Decoded(image);
        }
    }
    fn take_serial(&mut self, serial: u64) -> Option<PathBuf> {
        let file = self.detach(serial)?;
        self.names.remove(&file.named);
        match file.body {
            Body::Resident(_) | Body::Decoded(_) => None,
            Body::Spilled => (self.spill_dir.as_ref()).map(|dir| spill_path(dir, serial)),
        }
    }
    /// Renames the temporary file `temp`, which holds `serial`'s body of `len`
    /// bytes, to the companion's own name in the spill directory and switches
    /// the companion to it. With `used`, the tick the snapshot was read at,
    /// only a companion nothing touched since and whose body is still in memory
    /// (resident or decoded) takes it: a body another call changed or already
    /// spilled stays, `Ok(false)` then, and `temp` is the caller's to unlink.
    /// `None` is an edit's own body written out while it holds the companion's
    /// edit gate, so no other edit can have changed it.
    fn adopt_spilled(
        &mut self,
        serial: u64,
        used: Option<u64>,
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
        if let Some(used) = used {
            if !spill_applies(used, file.used, !matches!(file.body, Body::Spilled)) {
                return Ok(false);
            }
        }
        fs::rename(temp, path).map_err(|e| io_status(&e))?;
        let mut file = self.detach(serial).ok_or(Status::Stale)?;
        file.body = Body::Spilled;
        // A shorter file than the companion had described is not the body any
        // more.
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
    fn holding(&self) -> Holding {
        Holding {
            held: self.files.len(),
            resident_bytes: self.resident_bytes,
            decoded: self.decoded,
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
    pub(crate) fn configure_companions(&self, spill_dir: Option<PathBuf>, resident_budget: usize) {
        if let Some(dir) = &spill_dir {
            drop(fs::remove_dir_all(dir));
        }
        let mut companions = self.companions();
        companions.spill_dir = spill_dir;
        companions.resident_budget = resident_budget;
    }
    /// GETATTR of a companion serial.
    pub(crate) fn companion_stat(&self, serial: u64) -> Result<ViewStat, Status> {
        let mut companions = self.companions();
        let file = companions.files.get(&serial).ok_or(Status::Stale)?;
        let stat = Companions::stat(serial, file);
        companions.use_serial(serial);
        Ok(stat)
    }
    /// LOOKUP of a companion name, if one is held for it.
    pub(crate) fn companion_named(&self, parent: u64, name: &[u8]) -> Option<ViewStat> {
        let mut companions = self.companions();
        let serial = *companions.names.get(&(parent, name.to_vec()))?;
        let stat = Companions::stat(serial, companions.files.get(&serial)?);
        companions.use_serial(serial);
        Some(stat)
    }
    /// CREATE of an already-held companion name: UNCHECKED applies its size,
    /// as an open with truncation would; GUARDED and EXCLUSIVE find it taken.
    fn companion_open(
        &self,
        serial: u64,
        how: How,
        directory: Wcc,
        mut companions: std::sync::MutexGuard<'_, Companions>,
        tickets: &mut Tickets,
    ) -> Result<(ViewStat, Wcc), Status> {
        let stat = Companions::stat(serial, &companions.files[&serial]);
        companions.use_serial(serial);
        drop(companions);
        let How::Unchecked(sattr) = how else {
            return Err(Status::Exist);
        };
        match sattr.size {
            Some(size) => {
                let sized = Sattr {
                    size: Some(size),
                    ..Sattr::default()
                };
                let (stat, _) = self.companion_setattr(stat.serial, sized, None, tickets)?;
                Ok((stat, directory))
            }
            None => Ok((stat, directory)),
        }
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
        let named = (parent, name.to_vec());
        let mut companions = self.companions();
        if let Some(serial) = companions.names.get(&named).copied() {
            return self.companion_open(serial, how, directory, companions, tickets);
        }
        if companions.files.len() >= HELD {
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
        let file = Companion {
            named: named.clone(),
            len: 0,
            body: Body::Resident(Vec::new()),
            metadata,
            used,
            editing: Arc::new(Mutex::new(())),
        };
        let stat = Companions::stat(serial, &file);
        companions.names.insert(named, serial);
        companions.attach(serial, file);
        drop(companions);
        self.names_detached.fetch_add(1, Ordering::Relaxed);
        Ok((stat, directory))
    }
    /// REMOVE of a companion name, with the directory's `wcc_data`; `None`
    /// when no companion has that name.
    pub(crate) fn companion_remove(&self, parent: u64, name: &[u8]) -> Option<Wcc> {
        let spilled = {
            let mut companions = self.companions();
            let serial = *companions.names.get(&(parent, name.to_vec()))?;
            companions.take_serial(serial)
        };
        unlink_paths(spilled);
        Some(self.directory_wcc(parent))
    }
    /// RENAME of a companion name: only to another AppleDouble name, which it
    /// replaces when held. `None` when no companion has that name.
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
            let tick = companions.tick();
            let mut file = companions.detach(serial)?;
            companions.names.remove(&file.named);
            file.named = target.clone();
            file.used = tick;
            companions.names.insert(target, serial);
            companions.attach(serial, file);
        }
        unlink_paths(replaced_path);
        Some(Ok(()))
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
                "layerfs-nfs: AppleDouble companions released: export={:?} held={} decoded={} resident_bytes={}",
                self.mount(),
                holding.held,
                holding.decoded,
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
