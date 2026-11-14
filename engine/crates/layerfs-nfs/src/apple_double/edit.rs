//! Companion bodies: READ, WRITE and SETATTR, and the spill that moves a body
//! from memory to the export's scratch directory.
//!
//! One edit of a companion runs at a time, under that companion's own edit
//! gate (`Companion::editing` in the parent module): it applies to the resident
//! bytes under the
//! companion lock or to the spilled file outside it, and records the length
//! its own bytes leave, so a truncate and a write of the same companion can
//! never leave a length its file does not have and no round has to decide
//! again on a body another call changed under it.
//!
//! A body that does not fit the resident budget is spilled first, least
//! recently touched first; the body that still does not fit is written out
//! itself and edited in its file, which needs no budget. The file I/O never
//! holds the companion lock, and no descriptor is kept for a spilled body:
//! each operation opens its file by path and closes it again.
use super::{edited_len, record, Body, Companions, DECODED_ENTRY_BYTES, FILE_BYTES};
use crate::{
    apple_double_spill::{create_spilled, io_status, spill_path, touch, unlink_paths, Edit, Room},
    attributes::Status,
    change::{Sattr, Tickets, Wcc},
    kernel::Kernel,
};
use layerfs_mount_ports::operations::write::Stamp;
use layerfs_workspace::ViewStat;
use std::{
    fs::{File, OpenOptions},
    io,
    os::unix::fs::FileExt,
};

impl Kernel {
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
                    // The image the client wrote, synthesized again over the
                    // window this reply asks for rather than in full.
                    let stop = end.min(image.len());
                    out.extend_from_slice(&image.encode_range(start, stop));
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
        if companions.files.get(&serial).is_none() {
            return Err(Status::Stale);
        }
        companions.use_serial(serial);
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
    /// Applies `edit` to `serial` and answers its stat afterwards: in the
    /// resident body while the budget holds it, else in its spilled file
    /// outside the companion lock. This companion's gate makes the edit run
    /// alone, so the length recorded is the one its own bytes leave.
    fn edit_companion(&self, serial: u64, edit: Edit<'_>) -> Result<ViewStat, Status> {
        let gate = {
            let companions = self.companions();
            companions
                .files
                .get(&serial)
                .ok_or(Status::Stale)?
                .editing
                .clone()
        };
        // Held to the end: no other edit of this companion runs meanwhile.
        let _editing = gate.lock().unwrap_or_else(|poison| poison.into_inner());
        let target = edit.target_len();
        loop {
            match self.edit_resident(serial, target, &edit)? {
                Room::Resident(stat) => return Ok(stat),
                Room::Spilled => break,
                Room::Grow(need) => {
                    // No other call changes this body, so a round that finds
                    // room leaves the next edit in it.
                    if !self.spill_for(serial, need)? {
                        break;
                    }
                }
            }
        }
        // The body still does not fit resident memory: write it out and edit
        // its file, which needs no body budget.
        self.spill_gated(serial, target)?;
        edit.disk(&self.spill_file(serial, true)?)?;
        self.record_edit(serial, &edit)
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
    /// stat.
    fn record_edit(&self, serial: u64, edit: &Edit<'_>) -> Result<ViewStat, Status> {
        let mut companions = self.companions();
        let tick = companions.tick();
        let mut file = companions.detach(serial).ok_or(Status::Stale)?;
        record(edit, &mut file);
        file.used = tick;
        touch(&mut file.metadata);
        let stat = Companions::stat(serial, &file);
        companions.attach(serial, file);
        Ok(stat)
    }
    /// Spills the least recently used bodies to disk (outside the companion
    /// lock) until adding `need` bytes stays within `resident_budget`. Returns
    /// `true` if `need` fits in resident memory afterwards. Each round takes
    /// the oldest entry of the eviction order, so a round costs one step into
    /// it rather than a walk of every companion, and the rounds end when no
    /// companion holding memory is left: a victim another call changed
    /// meanwhile has moved to the newest end of the order, so it is not picked
    /// twice, and the caller then writes out its own body, which needs no
    /// budget at all.
    fn spill_for(&self, target: u64, need: usize) -> Result<bool, Status> {
        loop {
            let victim = {
                let companions = self.companions();
                if companions.resident_bytes + need <= companions.resident_budget {
                    return Ok(true);
                }
                companions.oldest_resident(Some(target))
            };
            let Some(victim) = victim else {
                return Ok(false);
            };
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
        let adopted = self
            .companions()
            .adopt_spilled(serial, Some(used), &temp, size);
        if matches!(adopted, Ok(true)) {
            return Ok(true);
        }
        unlink_paths([temp]);
        adopted.map(|_| false)
    }
    /// Writes `serial`'s body out while its own edit runs alone, keeping
    /// `size` bytes: no other edit can change the body, so the file written is
    /// the body this edit then applies to.
    fn spill_gated(&self, serial: u64, size: usize) -> Result<(), Status> {
        let (dir, bytes, size) = {
            let companions = self.companions();
            let file = companions.files.get(&serial).ok_or(Status::Stale)?;
            let bytes = match &file.body {
                Body::Resident(bytes) => bytes.clone(),
                Body::Decoded(image) => image.encode(),
                Body::Spilled => return Ok(()),
            };
            let dir = companions.spill_dir.clone().ok_or(Status::NoSpc)?;
            let size = size.max(bytes.len());
            (dir, bytes, size)
        };
        let temp = create_spilled(&dir, serial, &bytes, size as u64)?;
        let adopted = self.companions().adopt_spilled(serial, None, &temp, size);
        if !matches!(adopted, Ok(true)) {
            unlink_paths([temp]);
        }
        adopted.map(|_| ())
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
}
