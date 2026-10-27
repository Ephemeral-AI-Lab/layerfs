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
//! The memory is bounded: [`HELD`] companions of at most [`FILE_BYTES`]
//! each and [`TOTAL_BYTES`] in all. Beyond the count a creation is refused
//! (`NFS3ERR_ACCES`, as before this module), beyond the bytes a write
//! (`NFS3ERR_FBIG` per file, `NFS3ERR_NOSPC` in all).
use crate::{
    attributes::Status,
    change::{How, Sattr, Tickets},
    kernel::Kernel,
};
use layerfs_content::{filesystem::attributes::PortableMetadata, object::inode_leaf::InodeKind};
use layerfs_fuse::operations::write::Stamp;
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
/// Bytes kept over all companions.
pub const TOTAL_BYTES: usize = 64 << 20;
/// Mode of a companion created without one.
const MODE: u32 = 0o644;

struct Companion {
    named: (u64, Vec<u8>),
    data: Vec<u8>,
    metadata: PortableMetadata,
}
/// The companions of one export.
#[derive(Default)]
pub(crate) struct Companions {
    next: u64,
    files: HashMap<u64, Companion>,
    names: HashMap<(u64, Vec<u8>), u64>,
    bytes: usize,
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
            logical_len: file.data.len() as u64,
            subdirs: 0,
        }
    }
    fn drop_serial(&mut self, serial: u64) {
        if let Some(file) = self.files.remove(&serial) {
            self.names.remove(&file.named);
            self.bytes -= file.data.len();
        }
    }
    /// Resizes `serial` to `size`, within the bounds.
    fn resize(&mut self, serial: u64, size: u64) -> Result<(), Status> {
        if size > FILE_BYTES {
            return Err(Status::FBig);
        }
        let file = self.files.get_mut(&serial).ok_or(Status::Stale)?;
        let old = file.data.len();
        let new = size as usize;
        if new > old && self.bytes + (new - old) > TOTAL_BYTES {
            return Err(Status::NoSpc);
        }
        file.data.resize(new, 0);
        self.bytes = self.bytes + new - old;
        Ok(())
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
    ) -> Result<ViewStat, Status> {
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
                    self.companion_setattr(stat.serial, sized, None, tickets)
                }
                None => Ok(stat),
            };
        }
        // A directory must exist and a companion the tree itself carries
        // stays the tree's: neither is ours to shadow.
        let directory = self.getattr(parent)?;
        if directory.kind != InodeKind::Directory {
            return Err(Status::NotDir);
        }
        let taken = match self.lookup(parent, name) {
            Ok(_) => true,
            Err(Status::NoEnt) => false,
            Err(status) => return Err(status),
        };
        let mut companions = self.companions();
        if taken || companions.len() >= HELD {
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
        companions.names.insert(named.clone(), serial);
        companions.files.insert(
            serial,
            Companion {
                named,
                data: Vec::new(),
                metadata,
            },
        );
        let file = &companions.files[&serial];
        let stat = Companions::stat(serial, file);
        drop(companions);
        self.names_detached.fetch_add(1, Ordering::Relaxed);
        Ok(stat)
    }
    /// REMOVE of a companion name; `false` when none is held.
    pub(crate) fn companion_remove(&self, parent: u64, name: &[u8]) -> bool {
        let mut companions = self.companions();
        match companions.names.get(&(parent, name.to_vec())).copied() {
            Some(serial) => {
                companions.drop_serial(serial);
                true
            }
            None => false,
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
    /// READ of a companion.
    pub(crate) fn companion_read(
        &self,
        serial: u64,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), Status> {
        let companions = self.companions();
        let file = companions.files.get(&serial).ok_or(Status::Stale)?;
        let length = file.data.len() as u64;
        let start = offset.min(length) as usize;
        let end = offset.saturating_add(u64::from(count)).min(length) as usize;
        Ok((file.data[start..end].to_vec(), end as u64 >= length))
    }
    /// WRITE to a companion: all of it or nothing.
    pub(crate) fn companion_write(
        &self,
        serial: u64,
        offset: u64,
        data: &[u8],
    ) -> Result<(u32, ViewStat), Status> {
        let mut companions = self.companions();
        let end = offset.checked_add(data.len() as u64).ok_or(Status::FBig)?;
        let length = companions
            .files
            .get(&serial)
            .ok_or(Status::Stale)?
            .data
            .len() as u64;
        if end > length {
            companions.resize(serial, end)?;
        }
        let file = companions.files.get_mut(&serial).ok_or(Status::Stale)?;
        file.data[offset as usize..end as usize].copy_from_slice(data);
        touch(&mut file.metadata);
        let stat = Companions::stat(serial, file);
        Ok((data.len() as u32, stat))
    }
    /// SETATTR of a companion: mode, size and modification time; the
    /// owner is the export's identity, as for every inode.
    pub(crate) fn companion_setattr(
        &self,
        serial: u64,
        sattr: Sattr,
        guard: Option<(u32, u32)>,
        _tickets: &mut Tickets,
    ) -> Result<ViewStat, Status> {
        let identity = self.identity();
        let foreign = sattr.uid.is_some_and(|uid| uid != identity.uid)
            || sattr.gid.is_some_and(|gid| gid != identity.gid);
        if foreign {
            return Err(Status::Perm);
        }
        let mut companions = self.companions();
        if let Some(expected) = guard {
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            let current = Companions::stat(serial, file);
            if crate::attributes::ctime(&current) != expected {
                return Err(Status::NotSync);
            }
        }
        if let Some(size) = sattr.size {
            companions.resize(serial, size)?;
        }
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
        Ok(Companions::stat(serial, file))
    }
    /// Drops every companion; their count.
    pub(crate) fn companions_release(&self) -> usize {
        let mut companions = self.companions();
        let held = companions.len();
        *companions = Companions::default();
        held
    }
    /// Companions held now.
    pub(crate) fn companions_held(&self) -> usize {
        self.companions().len()
    }
}
