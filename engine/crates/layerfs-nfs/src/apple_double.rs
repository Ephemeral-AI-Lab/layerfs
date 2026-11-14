//! AppleDouble companions held by the export alone.
//!
//! On NFSv3 the macOS client keeps a file's extended attributes and resource
//! fork in a `._<name>` companion file (see [`crate::names`]). When the export
//! keeps such names out of the Workspace, a CREATE of one makes a file that
//! lives only here, in memory, under its own serial range: LOOKUP, GETATTR,
//! ACCESS, READ, WRITE, SETATTR, COMMIT, REMOVE and a RENAME to another
//! AppleDouble name work on it, READDIR does not list it, and no mutation of
//! it ever reaches the engine, so the Workspace stays clean and no Commit can
//! capture it. Everything is dropped when the export is released.
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
//! again.
//!
//! The memory is bounded: [`HELD`] companions of at most [`FILE_BYTES`] each,
//! and [`RESIDENT_BYTES`] of bodies in all, a body kept as written counted at
//! its length and a decoded one as [`Image::cost`], so a decoded image is never
//! memory the budget cannot see. Beyond the count a creation is refused
//! (`NFS3ERR_ACCES`, as before this module), beyond a file's bytes a write
//! (`NFS3ERR_FBIG`), beyond the body budget a write (`NFS3ERR_NOSPC`), which
//! leaves the companion exactly as it was.
use crate::{
    apple_double_codec::Image,
    attributes::Status,
    change::{How, Sattr, Tickets, Wcc},
    kernel::Kernel,
};
use layerfs_content::{filesystem::attributes::PortableMetadata, object::inode_leaf::InodeKind};
use layerfs_mount_ports::operations::write::Stamp;
use layerfs_workspace::ViewStat;
use std::{
    collections::HashMap,
    sync::atomic::Ordering,
    time::{SystemTime, UNIX_EPOCH},
};

/// The first serial of the export's own companions, far above any serial the
/// engine allocates.
pub const SERIALS: u64 = 1 << 62;
/// Companions held at once.
pub const HELD: usize = 4096;
/// Largest companion kept.
pub const FILE_BYTES: u64 = 1 << 20;
/// Companion payload bytes kept resident in memory before a write is refused:
/// a body kept as written counts its length, a decoded one [`Image::cost`].
pub const RESIDENT_BYTES: usize = 32 << 20;
/// Most bytes counted for one decoded companion, [`Image::cost`]; a whole
/// image costing more keeps its bytes as written, so one companion cannot take
/// the whole budget.
pub const DECODED_ENTRY_BYTES: usize = 128 << 10;
/// Mode of a companion created without one.
const MODE: u32 = 0o644;

enum Body {
    Resident(Vec<u8>),
    /// A whole image kept as its FinderInfo and attributes; a read encodes the
    /// image again, byte for byte as the client wrote it.
    Decoded(Image),
}

struct Companion {
    /// The `._` name it was created or last renamed to.
    named: (u64, Vec<u8>),
    len: usize,
    body: Body,
    metadata: PortableMetadata,
}
impl Companion {
    /// The bytes it takes of [`RESIDENT_BYTES`]: its body as written, or what
    /// its decoded form is counted as.
    fn resident_cost(&self) -> usize {
        match &self.body {
            Body::Resident(bytes) => bytes.len(),
            Body::Decoded(image) => image.cost(),
        }
    }
}

/// What an export's companion index holds (see [`crate::kernel::Released`]).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Holding {
    pub held: usize,
    pub decoded: usize,
    pub resident_bytes: usize,
}

/// The companions of one export.
#[derive(Default)]
pub(crate) struct Companions {
    next: u64,
    files: HashMap<u64, Companion>,
    names: HashMap<(u64, Vec<u8>), u64>,
    resident_bytes: usize,
    decoded: usize,
}
impl Companions {
    pub(crate) fn len(&self) -> usize {
        self.files.len()
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
    /// Drops `serial` and its name key.
    fn drop_serial(&mut self, serial: u64) {
        if let Some(file) = self.detach(serial) {
            self.names.remove(&file.named);
        }
    }
    /// Decides whether `file`'s body is a whole image: one that parses back to
    /// the same bytes and that [`DECODED_ENTRY_BYTES`] takes. `file` must be
    /// detached from the index.
    fn classify(&self, file: &mut Companion) {
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
    /// Replaces `serial`'s body by `len` bytes built from it by `apply`, kept
    /// within [`RESIDENT_BYTES`]: a body that decodes is counted as its
    /// attributes, not as its bytes, and any other body stays in memory only
    /// while the budget takes it. All of it or nothing: a body the budget
    /// cannot take leaves the companion exactly as it was.
    fn edit_body(
        &mut self,
        serial: u64,
        len: usize,
        apply: impl FnOnce(&mut Vec<u8>),
    ) -> Result<(), Status> {
        let mut file = self.detach(serial).ok_or(Status::Stale)?;
        let avail = RESIDENT_BYTES.saturating_sub(self.resident_bytes);
        let before_len = file.len;
        let original = std::mem::replace(&mut file.body, Body::Resident(Vec::new()));
        let mut bytes = match &original {
            Body::Resident(bytes) => bytes.clone(),
            Body::Decoded(image) => image.encode(),
        };
        bytes.resize(len, 0);
        apply(&mut bytes);
        file.body = Body::Resident(bytes);
        file.len = len;
        self.classify(&mut file);
        if file.resident_cost() > avail {
            // Neither as bytes nor as attributes does the body fit what the
            // budget has left: the companion keeps what it had and the caller
            // answers NOSPC.
            file.body = original;
            file.len = before_len;
            self.attach(serial, file);
            return Err(Status::NoSpc);
        }
        touch(&mut file.metadata);
        self.attach(serial, file);
        Ok(())
    }
    /// Applies a write to `serial`'s body; a write keeps what the body held
    /// beyond its end.
    fn write(&mut self, serial: u64, offset: usize, data: &[u8]) -> Result<(), Status> {
        let file = self.files.get(&serial).ok_or(Status::Stale)?;
        let len = file.len.max(offset + data.len());
        self.edit_body(serial, len, |bytes| {
            bytes[offset..offset + data.len()].copy_from_slice(data);
        })
    }
    /// Resizes `serial` to `size`, within the bounds.
    fn resize(&mut self, serial: u64, size: usize) -> Result<(), Status> {
        self.edit_body(serial, size, |_| {})
    }
    fn holding(&self) -> Holding {
        Holding {
            held: self.files.len(),
            decoded: self.decoded,
            resident_bytes: self.resident_bytes,
        }
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
    /// GETATTR of a companion serial.
    pub(crate) fn companion_stat(&self, serial: u64) -> Result<ViewStat, Status> {
        let companions = self.companions();
        let file = companions.files.get(&serial).ok_or(Status::Stale)?;
        Ok(Companions::stat(serial, file))
    }
    /// LOOKUP of a companion name, if one is held.
    pub(crate) fn companion_named(&self, parent: u64, name: &[u8]) -> Option<ViewStat> {
        let companions = self.companions();
        let serial = *companions.names.get(&(parent, name.to_vec()))?;
        companions
            .files
            .get(&serial)
            .map(|file| Companions::stat(serial, file))
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
        let mut companions = self.companions();
        if companions.len() >= HELD {
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
        let named = (parent, name.to_vec());
        let file = Companion {
            named,
            len: 0,
            body: Body::Resident(Vec::new()),
            metadata,
        };
        let stat = Companions::stat(serial, &file);
        companions.names.insert(file.named.clone(), serial);
        companions.attach(serial, file);
        drop(companions);
        self.names_detached.fetch_add(1, Ordering::Relaxed);
        Ok((stat, directory))
    }
    /// REMOVE of a companion name, with the directory's `wcc_data`; `None`
    /// when no companion has that name.
    pub(crate) fn companion_remove(&self, parent: u64, name: &[u8]) -> Option<Wcc> {
        {
            let mut companions = self.companions();
            let serial = *companions.names.get(&(parent, name.to_vec()))?;
            companions.drop_serial(serial);
        }
        Some(self.directory_wcc(parent))
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
        let mut companions = self.companions();
        let serial = *companions.names.get(&(from, name.to_vec()))?;
        if !crate::names::apple_double(new_name) || new_name.len() > 255 {
            return Some(Err(Status::Access));
        }
        let target = (to, new_name.to_vec());
        if let Some(replaced) = companions.names.get(&target).copied() {
            if replaced != serial {
                companions.drop_serial(replaced);
            }
        }
        companions.names.remove(&(from, name.to_vec()));
        companions.names.insert(target.clone(), serial);
        if let Some(file) = companions.files.get_mut(&serial) {
            file.named = target;
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
        let companions = self.companions();
        let file = companions.files.get(&serial).ok_or(Status::Stale)?;
        let length = file.len as u64;
        let start = offset.min(length) as usize;
        let end = offset.saturating_add(u64::from(count)).min(length) as usize;
        match &file.body {
            Body::Resident(bytes) => {
                // A body and its length are recorded under one lock; clamp
                // anyway so no reply slices past the bytes it holds.
                let stop = end.min(bytes.len());
                out.extend_from_slice(bytes.get(start..stop).unwrap_or_default());
            }
            Body::Decoded(image) => {
                // The image the client wrote, synthesized again over the
                // window this reply asks for rather than in full.
                let stop = end.min(image.len());
                out.extend_from_slice(&image.encode_range(start, stop));
            }
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
        let mut companions = self.companions();
        let before = {
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            Companions::stat(serial, file)
        };
        companions.write(serial, offset as usize, data)?;
        let file = companions.files.get(&serial).ok_or(Status::Stale)?;
        let after = Companions::stat(serial, file);
        Ok((data.len() as u32, (Some(before), Some(after))))
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
            if size > FILE_BYTES {
                return Err(Status::FBig);
            }
            self.companions().resize(serial, size as usize)?;
        }
        let mut companions = self.companions();
        let file = companions.files.get_mut(&serial).ok_or(Status::Stale)?;
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
    /// Drops every companion; what the index held.
    pub(crate) fn companions_release(&self) -> Holding {
        let mut companions = self.companions();
        let holding = companions.holding();
        *companions = Companions::default();
        holding
    }
    /// What the companion index holds now.
    pub(crate) fn companions_held(&self) -> Holding {
        self.companions().holding()
    }
}
