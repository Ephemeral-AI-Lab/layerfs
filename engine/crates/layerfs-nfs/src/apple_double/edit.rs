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
//! holds the companion lock: the file is written from a body read under the
//! lock and renamed to the companion's own name outside it, before the body is
//! committed as spilled under the lock again. No descriptor is kept for a
//! spilled body: each operation opens its file by path and closes it again.
use super::{edited_len, record, Body, Companions, Decoded, Slot, DECODED_ENTRY_BYTES, FILE_BYTES};
use crate::{
    apple_double_spill::{
        create_spilled, io_status, spill_path, touch, unlink_paths, Edit, Edited, Room,
    },
    attributes::Status,
    change::{Sattr, Tickets, Wcc},
    kernel::Kernel,
};
use layerfs_mount_ports::operations::write::Stamp;
use layerfs_workspace::ViewStat;
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::FileExt,
    path::Path,
};

/// One `pread` of `buf` at `offset`: a short read is the answer, and a read a
/// signal interrupted is asked again, since no bytes of it moved.
fn read_window(file: &File, buf: &mut [u8], offset: u64) -> Result<usize, Status> {
    loop {
        match file.read_at(buf, offset) {
            Ok(read) => return Ok(read),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(io_status(&e)),
        }
    }
}

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
            // One `pread` of the window the last lock saw, outside it: another
            // call may have truncated the file since, and the bytes the file
            // still holds are the reply - a short read, not an I/O error.
            let mut buf = vec![0u8; end - start];
            let read = read_window(&self.spill_file(serial, false)?, &mut buf, start as u64)?;
            buf.truncate(read);
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
        let gate = self.edit_gate(serial)?;
        // Held to the end: the reply's before is the body this edit changes.
        let _editing = gate.lock().unwrap_or_else(|poison| poison.into_inner());
        let before = self.companion_stat(serial)?;
        let offset = offset as usize;
        let after = self.edit_gated(serial, Edit::Write { offset, data })?;
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
        // One SETATTR runs as one edit: the guard is checked against the body
        // its size change then applies to, with no WRITE in between.
        let gate = self.edit_gate(serial)?;
        let _editing = gate.lock().unwrap_or_else(|poison| poison.into_inner());
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
            if crate::trace::enabled() {
                crate::trace::handle("SETATTR", serial, &format!("size={size}"));
            }
            self.edit_gated(serial, Edit::Size(size as usize))?;
        }
        let mut companions = self.companions();
        if !companions.files.contains_key(&serial) {
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
    /// `serial`'s edit gate: whoever holds it is the companion's one edit.
    fn edit_gate(&self, serial: u64) -> Result<std::sync::Arc<std::sync::Mutex<()>>, Status> {
        let companions = self.companions();
        let file = companions.files.get(&serial).ok_or(Status::Stale)?;
        Ok(file.editing.clone())
    }
    /// Applies `edit` to `serial` and answers its stat afterwards: in the
    /// resident body while the budget holds it, else in its spilled file
    /// outside the companion lock. The caller holds this companion's gate, so
    /// the edit runs alone and the length recorded is the one its own bytes
    /// leave.
    fn edit_gated(&self, serial: u64, edit: Edit<'_>) -> Result<ViewStat, Status> {
        match self.apply_edit(serial, &edit)? {
            Edited::Done(stat) => Ok(stat),
            Edited::NoSlot => {
                // A body that takes a slot of the index bound has none:
                // reclaim the companions no name reaches any more, once,
                // outside the companion lock, and apply the same edit again.
                // A bound that still has no room refuses the whole edit, with
                // the body, its length and its name as they were.
                if self.reclaim_orphans() == 0 {
                    return Err(Status::NoSpc);
                }
                match self.apply_edit(serial, &edit)? {
                    Edited::Done(stat) => Ok(stat),
                    Edited::NoSlot => Err(Status::NoSpc),
                }
            }
        }
    }
    /// Applies `edit` to `serial` once: in the resident body while the budget
    /// holds it, else in its spilled file. It answers that a slot of the index
    /// bound is missing rather than changing anything, so the caller can make
    /// room and try again.
    fn apply_edit(&self, serial: u64, edit: &Edit<'_>) -> Result<Edited, Status> {
        let target = edit.target_len();
        loop {
            match self.edit_resident(serial, target, edit)? {
                Room::Resident(stat) => return Ok(Edited::Done(stat)),
                Room::NoSlot => return Ok(Edited::NoSlot),
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
        // its file, which needs no body budget. A body edited on its file is no
        // longer known to be a whole image, so it takes a slot of the index
        // bound: decide that before the file is written, and leave the body,
        // its file and its name as they were when the bound has none. The slot
        // is held from the decision, not taken by the commit after the I/O, so
        // another call cannot be told the same last slot is free.
        if self.companions().reserve(serial) == Slot::Full {
            return Ok(Edited::NoSlot);
        }
        // The body is written out at the length it has, and the edit grows
        // its file itself: the companion's recorded length moves only once
        // the edit's bytes are in the file, so no GETATTR or READ meanwhile
        // sees a length whose bytes are not there, and a failed file
        // operation leaves the length it had.
        let written = self.spill_gated(serial, 0).and_then(|()| {
            let file = self.spill_file(serial, true)?;
            edit.disk(&file)
        });
        if let Err(status) = written {
            self.companions().unreserve(serial);
            return Err(status);
        }
        self.record_edit(serial, edit).map(Edited::Done)
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
            // The decoded form's own charge comes back when it becomes bytes.
            Body::Decoded(image) => edited_len(edit, image.len()).saturating_sub(image.cost()),
        };
        if need > avail {
            // The body does not fit the resident budget, but the edit may
            // complete a whole image, whose decoded form costs its attributes
            // rather than its bytes. Only a body small enough to decode is
            // tried.
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
            match companions.keep_decoded(serial, &bytes, len) {
                Decoded::Kept => {
                    let stat = companions
                        .files
                        .get(&serial)
                        .map(|file| Companions::stat(serial, file));
                    if let Some(stat) = stat {
                        return Ok(Room::Resident(stat));
                    }
                }
                // Make room for the decoded form rather than for the bytes the
                // edit holds: one eviction usually takes it.
                Decoded::Room(needed) => return Ok(Room::Grow(needed)),
                Decoded::Unchanged => {}
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
        // A companion holding no slot while the bound is full may take one with
        // this edit, and only the edited body says whether it does: its bytes
        // are kept when that is the case, so a refusal puts them back exactly.
        let restore = (!file.slotted() && !companions.admits(Some(serial), true))
            .then(|| (bytes.clone(), file.whole, file.len));
        edit.memory(&mut bytes);
        file.body = Body::Resident(bytes);
        record(edit, &mut file);
        companions.classify(&mut file);
        if let Some((bytes, whole, len)) = restore {
            if !companions.admits(Some(serial), file.slotted()) {
                file.body = Body::Resident(bytes);
                file.whole = whole;
                file.len = len;
                companions.attach(serial, file);
                return Ok(Room::NoSlot);
            }
        }
        touch(&mut file.metadata);
        file.used = tick;
        let stat = Companions::stat(serial, &file);
        companions.attach(serial, file);
        Ok(Room::Resident(stat))
    }
    /// Records an edit applied to `serial`'s spilled file and answers its
    /// stat. A body changed on disk is no longer known to be a whole image.
    fn record_edit(&self, serial: u64, edit: &Edit<'_>) -> Result<ViewStat, Status> {
        let mut companions = self.companions();
        let tick = companions.tick();
        // The slot held for this commit is given back whether or not the
        // companion is still there to take it.
        companions.unreserve(serial);
        let mut file = companions.detach(serial).ok_or(Status::Stale)?;
        record(edit, &mut file);
        file.used = tick;
        file.whole = false;
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
    ///
    /// A victim that cannot be written out is no failure of this call: one
    /// another call removed has nothing left to give back, and one whose file
    /// cannot be written keeps its body in memory, so the round passes over it
    /// and looks at the body after it. Only a call that finds no room and then
    /// cannot write out its own body reports that error, and it is its own.
    fn spill_for(&self, target: u64, need: usize) -> Result<bool, Status> {
        let mut skip = BTreeSet::from([target]);
        loop {
            let victim = {
                let companions = self.companions();
                if companions.resident_bytes + need <= companions.resident_budget {
                    return Ok(true);
                }
                // With nowhere to spill to, no victim can give memory back.
                if companions.spill_dir.is_none() {
                    return Ok(false);
                }
                companions.oldest_resident(&skip)
            };
            let Some(victim) = victim else {
                return Ok(false);
            };
            if self.spill_one(victim, None) != Ok(true) {
                // The body changed under the file written from it, the
                // companion's own edit holds it, or its file could not be
                // written: this call does not pick it again, and the next
                // round looks at the body after it.
                skip.insert(victim);
            }
        }
    }
    /// Writes `serial`'s body out to the spill directory and switches it to
    /// that file, keeping at least `keep` bytes (all of them when `None`),
    /// with the disk I/O outside the companion lock. `Ok(false)` when another
    /// call changed the companion, or already spilled it, while the file was
    /// written, or when the companion's own edit holds it: that temporary file
    /// is dropped and the caller decides again on the body it now finds.
    fn spill_one(&self, serial: u64, keep: Option<usize>) -> Result<bool, Status> {
        let gate = {
            let companions = self.companions();
            match companions.files.get(&serial) {
                Some(file) => file.editing.clone(),
                None => return Ok(true),
            }
        };
        // One writer of a companion's file at a time: an edit of this companion
        // is writing to it, and a spill may not wait for another call's I/O. A
        // gate poisoned by a panic is taken anyway, as every other holder of it
        // does, so one panicked edit does not make its body undevictable.
        let _editing = match gate.try_lock() {
            Ok(editing) => editing,
            Err(std::sync::TryLockError::Poisoned(poison)) => poison.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
        };
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
        let adopted = self.adopt_spilled(serial, Some(used), &temp, size);
        if matches!(adopted, Ok(true)) {
            return Ok(true);
        }
        unlink_paths([temp]);
        adopted.map(|_| false)
    }
    /// Writes `serial`'s body out while its own edit runs alone, keeping
    /// `size` bytes: no other edit can change the body, so the file written is
    /// the body this edit then applies to.
    pub(super) fn spill_gated(&self, serial: u64, size: usize) -> Result<(), Status> {
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
        let adopted = self.adopt_spilled(serial, None, &temp, size);
        if !matches!(adopted, Ok(true)) {
            unlink_paths([temp]);
        }
        adopted.map(|_| ())
    }
    /// Renames `temp`, holding `serial`'s body written out, onto the
    /// companion's own spill file and switches the companion to it. The
    /// rename runs outside the companion lock: a reader keeps reading the
    /// body in memory until the rename is done and the body is committed
    /// under the lock again, and the file is never visible half written. The
    /// caller holds the companion's edit gate for the whole adoption, so no
    /// other call writes this companion's file.
    fn adopt_spilled(
        &self,
        serial: u64,
        used: Option<u64>,
        temp: &Path,
        len: usize,
    ) -> Result<bool, Status> {
        let path = match self.companions().spill_target(serial, used) {
            Ok(Some(path)) => path,
            Ok(None) => return Ok(false),
            Err(status) => return Err(status),
        };
        if let Err(e) = fs::rename(temp, &path) {
            // The temporary file is the caller's to drop: nothing was adopted.
            return Err(io_status(&e));
        }
        let committed = self.companions().commit_spilled(serial, len);
        if !committed {
            // The companion is gone or already holds a file: the file just put
            // in place is this call's to drop.
            unlink_paths([path]);
        }
        Ok(committed)
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
