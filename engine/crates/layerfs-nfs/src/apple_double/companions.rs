//! AppleDouble companions held by the export alone.
//!
//! On NFSv3 the macOS client keeps a file's extended attributes and resource
//! fork in a `._<name>` companion file (see [`crate::kernel::names`]). When the export
//! keeps such names out of the Workspace, a CREATE of one makes a file that
//! lives only here, under its own serial range: LOOKUP, GETATTR, ACCESS, READ,
//! WRITE, SETATTR, COMMIT, REMOVE, a LINK to another AppleDouble name and a
//! RENAME to another AppleDouble name
//! work on it, READDIR does not list it, and no mutation of it ever reaches
//! the engine (a LOOKUP of its name in the view is the one engine call), so
//! the Workspace stays clean and no Commit can capture it.
//! Everything is dropped when the export is released.
//!
//! Only a name the Workspace view does not hold is a companion's: a `._` file
//! that is really in the tree (committed, or made in the view by another
//! route) is an ordinary Workspace file, which READDIR lists. So a LOOKUP of
//! a `._` name asks the view first and answers the companion only when the
//! view has no such name ([`Kernel::lookup_apple_double`]), and LOOKUP and
//! READDIR agree. CREATE, REMOVE and RENAME of a `._` name ask the view the
//! same way: CREATE of a name the view holds is the ordinary CREATE of that
//! file (UNCHECKED opens it, GUARDED and EXCLUSIVE answer `NFS3ERR_EXIST`),
//! so no companion is made to shadow it, and the attributes a macOS client
//! then stores there go into the Workspace file, as on any NFSv3 server; a
//! RENAME may replace such a file. A companion whose name the view gains
//! later (an install, another route) is shadowed by the real file and stays
//! unreachable by that name. A companion change never touches the directory
//! it names, so its `wcc_data` answers both halves from the last facts the
//! export saw for it (see [`Kernel::directory_wcc`]).
//!
//! A companion created while the export knows its main file (`<name>` in the
//! same directory) belongs to that inode: any name of the inode reaches it, so
//! hard links share one companion, it follows the inode through RENAME, and it
//! goes with the inode's last name rather than with one name. Its FinderInfo
//! and extended attributes are kept under that inode's serial, not under the
//! `._` name. It may hold several of the inode's `._` names at once (a LINK of
//! a companion name makes one, and a client that links a file keeps the
//! attributes aside under each name), and removing one of them leaves the
//! companion to the inode's other names: only its last name, on an inode with
//! no other name the export knows, is the client clearing the attributes. Whenever a write leaves such a companion holding a whole
//! AppleDouble image the client laid out itself (see
//! [`crate::apple_double::codec`]), it is decoded: one whose attributes cost
//! less than its bytes keeps only its FinderInfo and attributes in memory
//! (each at most [`DECODED_ENTRY_BYTES`] counted), and every read synthesizes
//! the same bytes again. Such a whole
//! image takes no slot of the index bound. Any other body
//! (a resource fork, another layout, a half-written image, or a companion whose
//! main file is unknown) is kept as the client wrote it and takes a slot: the
//! index bound, the refusal, its rate-limited log and orphan reclamation stay
//! the fallback for everything decoding does not compress.
//!
//! Every companion keeps its index entry in memory (at most [`HELD`] of them
//! not holding a whole image, each companion at most [`FILE_BYTES`]). Bodies up
//! to [`RESIDENT_BYTES`] in all stay in memory, a body kept as written counted
//! at its length and a decoded one as [`Image::cost`], so a decoded image is
//! memory the budget sees. An edit that leaves a body whole decodes it as it
//! records it rather than asking for room first, so the pool may stand one
//! decoded body over until the next write or resize needs room; that one then
//! spills the least recently touched bodies, decoded ones included, to the
//! export's own scratch directory (see [`crate::apple_double::spill`]). Removing
//! or replacing a main file drops its paired `._<name>` companion when no main
//! inode owns it; a companion that belongs to an inode goes with the inode, so
//! `._<name>` still answers for that inode after the name went, and only the
//! inode's last name takes the companion and every `._` name it holds. When
//! [`HELD`] is reached, orphan companions whose main inode or main
//! file the export no longer knows are reclaimed first, and only if [`HELD`]
//! live companions still remain does `CREATE` answer `NFS3ERR_NOSPC`.
use crate::{
    apple_double::{
        codec::Image,
        spill::{now, unlink_paths, Edit},
        trace::{self, Route as Trace},
    },
    kernel::{
        mutation::{How, Sattr, Tickets, Wcc},
        Kernel,
    },
    protocol::attributes::Status,
};
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_vfs::ViewStat;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{atomic::Ordering, Arc, Mutex},
    time::{Duration, Instant},
};

/// The first serial of the export's own companions, far above any serial the
/// engine allocates.
pub const SERIALS: u64 = 1 << 62;
/// Companion index entries not holding a whole image, held at once.
pub const HELD: usize = 65_536;
/// Largest companion kept.
pub const FILE_BYTES: u64 = 1 << 20;
/// Companion body bytes kept resident in memory before spilling to disk: a
/// body kept as written counts its length, a decoded one [`Image::cost`].
pub const RESIDENT_BYTES: usize = 32 << 20;
/// Most bytes counted for one decoded companion, [`Image::cost`]: a cap on
/// what one decoded body is charged to [`RESIDENT_BYTES`], not on what a
/// companion may store. A whole image costing more keeps its bytes as written
/// and is charged at their length, so one companion cannot take the whole
/// budget.
pub const DECODED_ENTRY_BYTES: usize = 128 << 10;

/// A name in one directory: (directory serial, name bytes).
pub(crate) type Named = (u64, Vec<u8>);

pub(crate) enum Body {
    Resident(Vec<u8>),
    /// On disk in the spill directory, under the companion's own serial; each
    /// operation opens it by path instead of holding it open.
    Spilled,
    /// A whole image kept as its FinderInfo and attributes; a read encodes the
    /// image again, byte for byte as the client wrote it.
    Decoded(Image),
}

pub(crate) struct Companion {
    /// The `._` name it was created or last renamed to.
    pub(crate) named: Named,
    /// The other `._` names of the same companion, which a LINK made: a client
    /// that links a file keeps its attributes aside under each of the file's
    /// names, and every one of them is a name of this one companion.
    pub(crate) also: Vec<Named>,
    /// The main inode it belongs to, when the export knew it at creation.
    pub(crate) inode: Option<u64>,
    /// Its body is a whole image: any name of the inode reads it, and it takes
    /// no slot of the index bound.
    pub(crate) whole: bool,
    pub(crate) len: usize,
    pub(crate) body: Body,
    pub(crate) metadata: PortableMetadata,
    pub(crate) used: u64,
    /// One edit of this companion at a time: its body's bytes, its length and
    /// its spilled file change together, so a truncate and a write can never
    /// leave a length its file does not have, and no round has to decide
    /// again on a body another call changed under it.
    pub(crate) editing: Arc<Mutex<()>>,
}
impl Companion {
    /// The bytes it takes of [`RESIDENT_BYTES`]: its body as written, or what
    /// its decoded form is counted as.
    pub(crate) fn resident_cost(&self) -> usize {
        match &self.body {
            Body::Resident(bytes) => bytes.len(),
            Body::Spilled => 0,
            Body::Decoded(image) => image.cost(),
        }
    }
    /// Whether it takes a slot of the index bound.
    pub(crate) fn slotted(&self) -> bool {
        !self.whole
    }
}

/// A slot of the index the export's own name index cannot reach, and what the
/// reclaim needs to decide about it.
#[derive(Clone)]
pub(crate) struct Orphan {
    pub(crate) serial: u64,
    /// The main inode the export knows for it; an inode holding no name in the
    /// kernel table is gone.
    pub(crate) inode: Option<u64>,
    /// Its main file's directory and name when it has no inode: the name index
    /// is lossy, so only the engine can say whether that file is there.
    pub(crate) main: Option<Named>,
    /// Its last use when it was found: a companion used since keeps itself.
    pub(crate) used: u64,
}

/// What [`Companions::reserve`] made of a call's claim on a slot of the index
/// bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Slot {
    /// It took the slot, which it gives back if its commit does not land.
    Taken,
    /// It already held a slot (or a reservation for one), or the companion
    /// is gone: nothing was taken.
    Held,
    /// The bound has none.
    Full,
}

/// What [`Companions::keep_decoded`] made of an edited body.
pub(crate) enum Decoded {
    /// The companion holds the body's decoded form.
    Kept,
    /// The body decodes, but the resident budget needs this many more bytes
    /// before the decoded form fits.
    Room(usize),
    /// Not a body the export decodes: not a whole image, too large to decode,
    /// or no main inode.
    Unchanged,
}

/// What an export's companion index holds (see [`crate::kernel::Released`]).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Holding {
    pub held: usize,
    pub slotted: usize,
    pub resident_bytes: usize,
    pub decoded: usize,
    /// The companions no name reaches, so that a status read and the release
    /// line report the same number.
    pub unreachable: usize,
}

/// The companions of one export.
pub(crate) struct Companions {
    next: u64,
    pub(crate) tick: u64,
    pub(crate) files: HashMap<u64, Companion>,
    pub(crate) names: HashMap<Named, u64>,
    /// Main inode serial -> the companion holding its FinderInfo and
    /// attributes.
    pub(crate) inodes: HashMap<u64, u64>,
    /// The serials taking a slot of the index bound, so reclamation walks only
    /// the [`HELD`]-bounded slots rather than every companion: a whole image
    /// takes no slot, whether it is decoded or kept as written bytes, so whole
    /// images add nothing to the walk.
    pub(crate) slots: HashSet<u64>,
    /// The serials holding a slot for a commit that is still on its way, whose
    /// I/O runs outside this lock. A reserved slot counts against the bound, so
    /// two calls cannot both be told the bound has room for one.
    pub(crate) reserved: HashSet<u64>,
    /// Every companion holding memory, oldest use first: (used tick, serial).
    /// One tick is one touch and no serial takes two ticks, so the keys are
    /// unique and the oldest entry is the first one.
    pub(crate) by_use: BTreeSet<(u64, u64)>,
    pub(crate) resident_bytes: usize,
    pub(crate) decoded: usize,
    pub(crate) spill_dir: Option<PathBuf>,
    pub(crate) resident_budget: usize,
    pub(crate) held_limit: usize,
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
            slots: HashSet::new(),
            reserved: HashSet::new(),
            by_use: BTreeSet::new(),
            resident_bytes: 0,
            decoded: 0,
            spill_dir: None,
            resident_budget: RESIDENT_BYTES,
            held_limit: HELD,
            last_refusal_log: None,
        }
    }
}
/// How a CREATE that found the companion already held is traced: an UNCHECKED
/// create with an explicit size truncates the body every name of the inode
/// shares, and a GUARDED or EXCLUSIVE one is answered `NFS3ERR_EXIST`.
fn opening(how: &How) -> &'static str {
    match how {
        How::Unchecked(sattr) if sattr.size.is_some() => "existing-truncate",
        How::Unchecked(_) | How::Guarded(_) => "existing",
        How::Exclusive(_) => "existing-exclusive",
    }
}
/// The body length `edit` leaves on a body that had `len` bytes.
pub(crate) fn edited_len(edit: &Edit<'_>, len: usize) -> usize {
    match edit {
        Edit::Write { .. } => len.max(edit.target_len()),
        Edit::Size(size) => *size,
    }
}
/// Records the length `edit` leaves on `file`.
pub(crate) fn record(edit: &Edit<'_>, file: &mut Companion) {
    file.len = edited_len(edit, file.len);
}
/// A serial of the export's own companions.
pub fn companion(serial: u64) -> bool {
    serial >= SERIALS
}

impl Kernel {
    pub(crate) fn companions(&self) -> std::sync::MutexGuard<'_, Companions> {
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
        let (serial, route) = companions.resolve_route(&(parent, name.to_vec()), main);
        if crate::kernel::names::apple_double(name) {
            trace::companion(
                "LOOKUP",
                parent,
                name,
                main,
                route,
                serial,
                match serial {
                    Some(_) => "Ok",
                    None => "NoEnt",
                },
            );
        }
        let stat = Companions::stat(serial?, companions.files.get(&serial?)?);
        companions.use_serial(serial?);
        Some(stat)
    }
    /// LOOKUP of a `._` name while companions are kept: the view answers
    /// first, so a real Workspace file of that name (one READDIR lists) is
    /// never hidden or shadowed, and only a name the view does not hold
    /// answers the companion, if one is held, a companion answering with no
    /// directory row.
    pub(crate) fn lookup_apple_double(
        &self,
        parent: u64,
        name: &[u8],
    ) -> Result<(ViewStat, Option<ViewStat>), crate::kernel::read::Refused> {
        match self.lookup_view(parent, name) {
            Err(refused) if refused.status == Status::NoEnt => {
                match self.companion_named(parent, name) {
                    Some(stat) => Ok((stat, None)),
                    None => Err(refused),
                }
            }
            answer => answer,
        }
    }
    /// Whether the Workspace view holds `name` in `parent`: such a `._` name
    /// is an ordinary Workspace file, never a companion's. A view that holds
    /// it takes the export's one engine reference, as any LOOKUP does.
    pub(crate) fn view_holds(&self, parent: u64, name: &[u8]) -> Result<bool, Status> {
        match self.lookup_view(parent, name) {
            Ok(_) => Ok(true),
            Err(refused) if refused.status == Status::NoEnt => Ok(false),
            Err(refused) => Err(refused.status),
        }
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
        let mut main = self.main_of(parent, name);
        let mut companions = self.companions();
        let (held, route) = companions.resolve_route(&named, main);
        if let Some(serial) = held {
            trace::companion("CREATE", parent, name, main, route, held, opening(&how));
            return self.companion_open(serial, how, directory, companions, tickets);
        }
        // At the index bound, reclaim the companions no name reaches any more
        // first, outside this lock.
        if !companions.admits(None, true) {
            drop(companions);
            self.reclaim_orphans();
            // The reclaim held no companion lock, so another CREATE of this
            // name may have inserted it meanwhile and the main file may have
            // changed: both are read again, and the name is inserted under the
            // lock this call now holds without releasing it again.
            main = self.main_of(parent, name);
            companions = self.companions();
            let (held, route) = companions.resolve_route(&named, main);
            if let Some(serial) = held {
                trace::companion("CREATE", parent, name, main, route, held, opening(&how));
                return self.companion_open(serial, how, directory, companions, tickets);
            }
        }
        if !companions.admits(None, true) {
            let held = companions.slots.len();
            let limit = companions.held_limit;
            let log = companions
                .last_refusal_log
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(60));
            let namespace = log.then(|| companions.spill_dir.clone());
            if log {
                companions.last_refusal_log = Some(Instant::now());
            }
            trace::companion("CREATE", parent, name, main, Trace::None, None, "NoSpc");
            drop(companions);
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
            also: Vec::new(),
            inode: main,
            whole: false,
            len: 0,
            body: Body::Resident(Vec::new()),
            metadata,
            used,
            editing: Arc::new(Mutex::new(())),
        };
        let stat = Companions::stat(serial, &file);
        trace::companion(
            "CREATE",
            parent,
            name,
            main,
            Trace::None,
            Some(serial),
            "Ok",
        );
        companions.index(serial, &file);
        companions.attach(serial, file);
        drop(companions);
        self.names_detached.fetch_add(1, Ordering::Relaxed);
        Ok((stat, directory))
    }
    /// REMOVE of a companion name, with the directory's `wcc_data`; `None`
    /// when no companion has that name.
    ///
    /// A companion the export pairs with a main inode is the inode's, not one
    /// name's: its FinderInfo and attributes belong to the file, so a client
    /// that links the file (or removes one of its names) and drops the `._`
    /// name it no longer uses leaves the companion to the inode's remaining
    /// names. Only while this name is the companion's last one, and its inode
    /// has no other name the export knows, is the REMOVE the client clearing
    /// the attributes, which takes the companion with it. A companion no main
    /// inode owns has nothing else to go with and always goes.
    pub(crate) fn companion_remove(&self, parent: u64, name: &[u8]) -> Option<Wcc> {
        let named = (parent, name.to_vec());
        let main = self.main_of(parent, name);
        let resolved = {
            let companions = self.companions();
            let (serial, route) = companions.resolve_route(&named, main);
            serial.and_then(|serial| {
                let file = companions.files.get(&serial)?;
                Some((
                    serial,
                    route,
                    file.inode,
                    !companions.holds_another(serial, &named),
                ))
            })
        };
        let Some((serial, route, inode, one_name)) = resolved else {
            if crate::kernel::names::apple_double(name) {
                trace::companion("REMOVE", parent, name, main, Trace::None, None, "NoEnt");
            }
            return None;
        };
        let kept = !one_name || inode.is_some_and(|inode| self.knows_another_name(inode));
        trace::companion(
            "REMOVE",
            parent,
            name,
            main,
            route,
            Some(serial),
            match kept {
                true => "name-dropped",
                false => "companion-dropped",
            },
        );
        let spilled = {
            let mut companions = self.companions();
            if kept {
                companions.drop_name(serial, &named);
                Vec::new()
            } else {
                companions.take_serial(serial).into_iter().collect()
            }
        };
        unlink_paths(spilled);
        Some(self.directory_wcc(parent))
    }
    /// Whether the export's name index knows a name of `inode` beyond the one
    /// it remembers: a hard link the client made, which the companion of an
    /// inode follows.
    fn knows_another_name(&self, inode: u64) -> bool {
        let table = self.table();
        let remembered = usize::from(table.names.contains_key(&inode));
        remembered + table.aliases.get(&inode).map_or(0, Vec::len) > 1
    }
    /// The main file `name` in `parent` was removed, silly-renamed or replaced
    /// by `RENAME`: drops the companion held under `._<name>` when no main
    /// inode owns it, and leaves an inode's companion to the inode - it goes
    /// with the inode's last name instead (see
    /// [`Kernel::companion_drop_inode`]), which is also where the `._` names it
    /// held go (`Companions::unindex`). A plain non-replacing `RENAME` of a
    /// main file only cleans the destination name `to/new_name` (via
    /// `finish_unlink`) and leaves `from/name` alone so a client that renames
    /// `a -> b` and then renames `._a -> ._b` can still move the companion
    /// across.
    ///
    /// The `._` name stays after its main name for as long as the inode does,
    /// because the inode's FinderInfo and attributes are the file's: `/rm a`
    /// on a file `b` still names leaves `._a` answering the companion of the
    /// inode, rather than reporting attributes that the file `b` still has as
    /// gone. A new file under that name reaches the old companion nowhere:
    /// [`Companions::resolve`] takes a name key only for the inode its
    /// companion belongs to, so the answer falls back to that inode's
    /// companion, which a new file does not have.
    pub(crate) fn companion_drop_for(&self, parent: u64, name: &[u8]) {
        if crate::kernel::names::apple_double(name) {
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
            let of_inode = companions
                .files
                .get(&serial)
                .is_some_and(|file| file.inode.is_some());
            trace::companion(
                "UNPAIR",
                parent,
                name,
                companions.files.get(&serial).and_then(|file| file.inode),
                Trace::Name,
                Some(serial),
                match of_inode {
                    true => "name-kept",
                    false => "companion-dropped",
                },
            );
            match of_inode {
                true => None,
                false => companions.take_serial(serial),
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
    /// LINK of a companion name: the same companion under another AppleDouble
    /// name, as a client that links a file makes to carry its attributes to
    /// the new name. The name it names must be free, and a name that is not
    /// another companion name is refused, so a companion never becomes a file
    /// of the Workspace.
    pub(crate) fn companion_link(
        &self,
        serial: u64,
        parent: u64,
        name: &[u8],
    ) -> Result<(ViewStat, Wcc), Status> {
        if !crate::kernel::names::apple_double(name) || name.len() > 255 {
            return Err(Status::Access);
        }
        if parent != self.root() && !self.table().parents.contains_key(&parent) {
            return Err(Status::NotDir);
        }
        let named = (parent, name.to_vec());
        let directory = self.directory_wcc(parent);
        let main = self.main_of(parent, name);
        let mut companions = self.companions();
        if companions.names.contains_key(&named)
            || companions
                .resolve(&named, main)
                .is_some_and(|other| other != serial)
        {
            trace::companion("LINK", parent, name, main, Trace::Name, None, "Exist");
            return Err(Status::Exist);
        }
        let Some(mut file) = companions.detach(serial) else {
            return Err(Status::Stale);
        };
        file.also.push(named);
        file.used = companions.tick();
        let stat = Companions::stat(serial, &file);
        trace::companion("LINK", parent, name, main, Trace::None, Some(serial), "Ok");
        companions.index(serial, &file);
        companions.attach(serial, file);
        Ok((stat, directory))
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
        let serial = {
            let companions = self.companions();
            companions.resolve(&(from, name.to_vec()), from_main)?
        };
        if !crate::kernel::names::apple_double(new_name) || new_name.len() > 255 {
            return Some(Err(Status::Access));
        }
        let source = (from, name.to_vec());
        let renamed = self.rename_companion(serial, &source, (to, new_name.to_vec()), to_main);
        trace::companion(
            "RENAME",
            to,
            new_name,
            to_main,
            Trace::None,
            Some(serial),
            match &renamed {
                Ok(()) => "Ok",
                Err(status) => trace::status(*status),
            },
        );
        Some(renamed)
    }
    /// Moves `serial`'s companion from its name `source` to `target`, under
    /// `to_main`'s inode. Only `source` is replaced: its other names (a LINK
    /// made them) stay, and a companion reached only through its inode keeps
    /// every name it holds and gains `target`.
    ///
    /// The body is classified again under the destination, so a whole image
    /// renamed onto an unknown main is the client's bytes again and takes a
    /// slot: such a body no longer fits the resident budget as attributes, and
    /// is written out first (outside the companion lock, with the companion's
    /// edit gate held), so the rename itself only records a name and a class.
    /// A companion that needs a new slot of the index bound takes the same
    /// decision the edit path takes: the orphan slots are reclaimed once and
    /// the rename is refused if the bound still has none.
    fn rename_companion(
        &self,
        serial: u64,
        source: &Named,
        target: Named,
        to_main: Option<u64>,
    ) -> Result<(), Status> {
        let gate = {
            let companions = self.companions();
            companions
                .files
                .get(&serial)
                .ok_or(Status::Stale)?
                .editing
                .clone()
        };
        // Held to the end: the body written out below is the body renamed.
        let _editing = gate.lock().unwrap_or_else(|poison| poison.into_inner());
        // A decoded body renamed onto an unknown main would cost its whole
        // image of the resident budget: write it out instead, which costs none.
        let leaves_memory = {
            let companions = self.companions();
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            match &file.body {
                Body::Decoded(image) if to_main.is_none() => {
                    let cost = image.len();
                    companions
                        .resident_bytes
                        .saturating_sub(image.cost())
                        .saturating_add(cost)
                        > companions.resident_budget
                }
                _ => false,
            }
        };
        if leaves_memory {
            self.spill_gated(serial, 0)?;
        }
        // A companion moved onto an unknown main takes a slot; one already
        // holding a slot keeps it, and one that stays a whole image keeps its
        // exemption. The slot is held from here, so the commit below cannot
        // find another call took the last one while this rename waited.
        if to_main.is_none() && self.companions().reserve(serial) == Slot::Full {
            self.reclaim_orphans();
            if self.companions().reserve(serial) == Slot::Full {
                return Err(Status::NoSpc);
            }
        }
        let mut replaced_path = None;
        {
            let mut companions = self.companions();
            let Some(mut file) = companions.detach(serial) else {
                companions.unreserve(serial);
                return Err(Status::Stale);
            };
            companions.unindex(serial, &file);
            if let Some(replaced) = companions.resolve(&target, to_main) {
                if replaced != serial {
                    replaced_path = companions.take_serial(replaced);
                }
            }
            if file.named == *source {
                file.named = target.clone();
            } else if let Some(named) = file.also.iter_mut().find(|named| **named == *source) {
                *named = target.clone();
            } else if file.named != target {
                file.also.push(target.clone());
            }
            file.also.retain(|named| *named != file.named);
            file.also.dedup();
            file.inode = to_main;
            file.used = companions.tick();
            // A client may write a companion under another name first, and a
            // name it moves onto may belong to another main inode, or to
            // none: the body is classified again under the destination, so
            // one whose main file is unknown takes its slot and none keeps a
            // whole image's exemption.
            companions.classify(&mut file);
            companions.index(serial, &file);
            companions.attach(serial, file);
            companions.unreserve(serial);
        }
        unlink_paths(replaced_path);
        Ok(())
    }
    /// Drops every companion and removes the export's spill directory; returns
    /// what the index held.
    pub(crate) fn companions_release(&self) -> Holding {
        let (holding, dir) = {
            let mut companions = self.companions();
            let holding = companions.holding();
            let dir = companions.spill_dir.take();
            drop(std::mem::take(&mut *companions));
            (holding, dir)
        };
        if let Some(dir) = dir {
            drop(fs::remove_dir_all(dir));
        }
        if holding.held > 0 {
            eprintln!(
                "layerfs-nfs: AppleDouble companions released: export={:?} held={} slotted={} decoded={} resident_bytes={} unreachable={}",
                self.mount(),
                holding.held,
                holding.slotted,
                holding.decoded,
                holding.resident_bytes,
                holding.unreachable
            );
        }
        holding
    }
    /// What the companion index holds now.
    pub(crate) fn companions_held(&self) -> Holding {
        self.companions().holding()
    }
    /// Traces a request about a main file (`op`, its directory and name) when
    /// the export holds a companion for that inode: these are the requests
    /// that make a `._` name resolvable (see [`crate::apple_double::trace`]).
    pub(crate) fn trace_main_file(
        &self,
        op: &str,
        parent: u64,
        name: &[u8],
        inode: u64,
        status: &str,
    ) {
        if !trace::enabled() {
            return;
        }
        let companion = self.companions().inodes.get(&inode).copied();
        if companion.is_some() {
            trace::main_file(op, parent, name, Some(inode), companion, status);
        }
    }
    /// Traces a name the export's kernel table bound to `inode` (`alias`: kept
    /// as another name of it) or dropped from it (`unbound`), when the inode
    /// has a companion: a `._` name resolves through those names.
    pub(crate) fn trace_bind(&self, parent: u64, name: &[u8], inode: u64, status: &str) {
        if !trace::enabled() {
            return;
        }
        let companion = self.companions().inodes.get(&inode).copied();
        if companion.is_some() {
            trace::main_file("BIND", parent, name, Some(inode), companion, status);
        }
    }
}
