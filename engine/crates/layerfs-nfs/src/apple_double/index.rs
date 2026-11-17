//! The companion index of one export: its names, main inodes, slots of the
//! index bound, resident budget and eviction order.
use super::{
    Body, Companion, Companions, Decoded, Holding, Named, Orphan, Slot,
    DECODED_ENTRY_BYTES,
};
use crate::{
    apple_double_codec::Image,
    apple_double_spill::{spill_path, touch},
    attributes::Status,
    trace::Route as Trace,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_workspace::ViewStat;
use std::{collections::BTreeSet, path::PathBuf};

impl Companions {
    pub(super) fn tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }
    /// Marks `serial` as used now, which is the order eviction follows. Only a
    /// companion holding memory is in the order: a spilled body has nothing to
    /// give back.
    pub(super) fn use_serial(&mut self, serial: u64) {
        let tick = self.tick();
        self.set_used(serial, tick);
    }
    /// Moves `serial`'s index entry to `used`.
    pub(super) fn set_used(&mut self, serial: u64, used: u64) {
        let Some(file) = self.files.get_mut(&serial) else {
            return;
        };
        let old = std::mem::replace(&mut file.used, used);
        self.by_use.remove(&(old, serial));
        if file.resident_cost() > 0 {
            self.by_use.insert((used, serial));
        }
    }
    pub(super) fn stat(serial: u64, file: &Companion) -> ViewStat {
        ViewStat {
            serial,
            kind: InodeKind::RegularFile,
            metadata: file.metadata,
            namespace_refs: 1,
            logical_len: file.len as u64,
            subdirs: 0,
        }
    }
    /// Adds `file` to the index, the slot and byte counts and the eviction
    /// order.
    pub(super) fn attach(&mut self, serial: u64, file: Companion) {
        if file.slotted() {
            self.slots.insert(serial);
        }
        if file.resident_cost() > 0 {
            self.by_use.insert((file.used, serial));
        }
        self.resident_bytes += file.resident_cost();
        self.decoded += usize::from(matches!(file.body, Body::Decoded(_)));
        self.files.insert(serial, file);
    }
    /// Takes `serial` out of the index and the counts; its name keys stay for
    /// the caller to unindex.
    pub(super) fn detach(&mut self, serial: u64) -> Option<Companion> {
        let file = self.files.remove(&serial)?;
        self.slots.remove(&serial);
        self.by_use.remove(&(file.used, serial));
        self.resident_bytes -= file.resident_cost();
        self.decoded -= usize::from(matches!(file.body, Body::Decoded(_)));
        Some(file)
    }
    /// Makes every name of `file` and its main inode find `serial`.
    pub(super) fn index(&mut self, serial: u64, file: &Companion) {
        self.names.insert(file.named.clone(), serial);
        for named in &file.also {
            self.names.insert(named.clone(), serial);
        }
        if let Some(inode) = file.inode {
            self.inodes.insert(inode, serial);
        }
    }
    /// Drops every name of `file` and its inode key that still find `serial`.
    pub(super) fn unindex(&mut self, serial: u64, file: &Companion) {
        for named in std::iter::once(&file.named).chain(&file.also) {
            if self.names.get(named) == Some(&serial) {
                self.names.remove(named);
            }
        }
        if let Some(inode) = file.inode {
            if self.inodes.get(&inode) == Some(&serial) {
                self.inodes.remove(&inode);
            }
        }
    }
    /// Whether `serial` holds a name other than `named`.
    pub(super) fn holds_another(&self, serial: u64, named: &Named) -> bool {
        self.files.get(&serial).is_some_and(|file| {
            (file.named != *named && self.names.get(&file.named) == Some(&serial))
                || file.also.iter().any(|also| also != named)
        })
    }
    /// Drops `named`, one of `serial`'s names, and answers whether it held it.
    /// A companion left with no name is still reached through its main inode,
    /// so it keeps its record; another held name becomes the one its record
    /// names, so a later `index` puts back only names it still holds.
    pub(super) fn drop_name(&mut self, serial: u64, named: &Named) -> bool {
        let Some(file) = self.files.get_mut(&serial) else {
            return false;
        };
        let primary = file.named == *named;
        if !primary && !file.also.iter().any(|also| also == named) {
            return false;
        }
        if self.names.get(named) == Some(&serial) {
            self.names.remove(named);
        }
        if primary {
            if let Some(next) = file.also.pop() {
                file.named = next;
            }
        } else {
            file.also.retain(|also| also != named);
        }
        true
    }
    /// The companion `named` reaches when its main file is `main`: the one
    /// held under that name, unless it belongs to another inode than `main`,
    /// else the one its main inode already has.
    pub(super) fn resolve(&self, named: &Named, main: Option<u64>) -> Option<u64> {
        self.resolve_route(named, main).0
    }
    /// [`Self::resolve`], and which part of the index answered, for the trace.
    pub(super) fn resolve_route(&self, named: &Named, main: Option<u64>) -> (Option<u64>, Trace) {
        if let Some(&serial) = self.names.get(named) {
            let inode = self.files.get(&serial).and_then(|file| file.inode);
            if inode.is_none() || main.is_none() || inode == main {
                return (Some(serial), Trace::Name);
            }
        }
        match main.and_then(|main| self.inodes.get(&main).copied()) {
            Some(serial) => (Some(serial), Trace::Inode),
            None => (None, Trace::None),
        }
    }
    /// Replaces `serial`'s body by the decoded form of `bytes`, the caller's
    /// edited body of `len` bytes, while the resident budget takes it and the
    /// export knows the companion's main inode. The decoded form is counted as
    /// its attributes rather than its image, so a body that decodes usually
    /// needs much less room than its bytes, and the caller is told the room it
    /// would need instead of spilling a body that fits after one eviction.
    pub(super) fn keep_decoded(&mut self, serial: u64, bytes: &[u8], len: usize) -> Decoded {
        // Only an inode's companion is decoded: one whose main file the export
        // never saw keeps the client's bytes as written and a slot of the
        // index bound, so that bound and orphan reclamation still reach it.
        let Some(file) = self.files.get(&serial) else {
            return Decoded::Unchanged;
        };
        let (inode, before) = (file.inode, file.resident_cost());
        if inode.is_none() {
            return Decoded::Unchanged;
        }
        let Some(image) = Image::compact(bytes) else {
            return Decoded::Unchanged;
        };
        if !worth_decoding(&image, bytes.len()) {
            return Decoded::Unchanged;
        }
        let cost = image.cost();
        let tick = self.tick();
        let Some(mut file) = self.detach(serial) else {
            return Decoded::Unchanged;
        };
        if self.resident_bytes + cost > self.resident_budget {
            self.attach(serial, file);
            // The body decodes, so the room to ask for is what the decoded
            // form adds to the count, not the length of the bytes edited.
            return Decoded::Room(cost.saturating_sub(before));
        }
        file.len = len;
        file.body = Body::Decoded(image);
        file.whole = true;
        file.used = tick;
        touch(&mut file.metadata);
        self.attach(serial, file);
        Decoded::Kept
    }
    /// Decides whether `file`'s body is a whole image: a decoded body of an
    /// inode's companion always is; a resident body of one is decoded when it
    /// parses back to the same bytes and [`DECODED_ENTRY_BYTES`] takes it. A
    /// companion whose main file the export does not know is never a whole
    /// image: it keeps the client's bytes as written and a slot of the index
    /// bound, so that bound and orphan reclamation still reach it. `file` must
    /// be detached from the index.
    pub(super) fn classify(&self, file: &mut Companion) {
        if file.inode.is_none() {
            // Only a decoded body has to become bytes again; a body on disk
            // keeps its file and takes a slot.
            if let Body::Decoded(image) = &file.body {
                file.body = Body::Resident(image.encode());
            }
            file.whole = false;
            return;
        }
        // A body on disk cannot be decoded here (that reads its file under
        // this lock): it keeps the class the write that spilled it left it.
        file.whole = match &file.body {
            Body::Decoded(_) => true,
            Body::Spilled => file.whole,
            Body::Resident(_) => false,
        };
        let Body::Resident(bytes) = &file.body else {
            return;
        };
        let Some(image) = Image::compact(bytes) else {
            return;
        };
        file.whole = true;
        if worth_decoding(&image, bytes.len()) {
            file.body = Body::Decoded(image);
        }
    }
    pub(super) fn take_serial(&mut self, serial: u64) -> Option<PathBuf> {
        self.reserved.remove(&serial);
        let file = self.detach(serial)?;
        self.unindex(serial, &file);
        match file.body {
            Body::Resident(_) | Body::Decoded(_) => None,
            Body::Spilled => (self.spill_dir.as_ref()).map(|dir| spill_path(dir, serial)),
        }
    }
    /// Where a body a spill wrote out replaces its companion: the companion's
    /// own file, or `None` when another call changed the body since the
    /// snapshot (`used` is the tick read then, and only a body still in memory
    /// can be replaced). The caller renames its file to that path itself,
    /// outside this lock, and then commits the body with
    /// [`Self::commit_spilled`].
    pub(super) fn spill_target(&self, serial: u64, used: Option<u64>) -> Result<Option<PathBuf>, Status> {
        let file = self.files.get(&serial).ok_or(Status::Stale)?;
        // A body already on disk is never replaced: its file is the body.
        if matches!(file.body, Body::Spilled) || used.is_some_and(|used| used != file.used) {
            return Ok(None);
        }
        let dir = self.spill_dir.as_ref().ok_or(Status::NoSpc)?;
        Ok(Some(spill_path(dir, serial)))
    }
    /// Commits the body whose bytes now sit at the companion's spill file: it
    /// holds them on disk from now on, `len` bytes of them, which were the
    /// whole body the file was written from. `false` when the companion is gone
    /// or already holds a file, and the file just renamed into place is the
    /// caller's to drop.
    ///
    /// The caller holds the companion's edit gate and no other call writes this
    /// companion's file, so a body that changed between the snapshot and here
    /// cannot be the body this file was written from.
    pub(super) fn commit_spilled(&mut self, serial: u64, len: usize) -> bool {
        let Some(mut file) = self.detach(serial) else {
            return false;
        };
        if matches!(file.body, Body::Spilled) {
            self.attach(serial, file);
            return false;
        }
        file.body = Body::Spilled;
        // A body written out whole is still a whole image; a shorter file
        // than the companion had described is not.
        file.whole &= len <= file.len;
        file.len = file.len.max(len);
        self.attach(serial, file);
        true
    }
    /// The least recently used companion holding memory, a body kept as
    /// written or a decoded one: the budget counts both, so both are evicted.
    /// The companions the caller will not take are passed over, one step into
    /// the eviction order rather than a walk of every companion.
    pub(super) fn oldest_resident(&self, skip: &BTreeSet<u64>) -> Option<u64> {
        self.by_use
            .iter()
            .find(|(_, serial)| !skip.contains(serial))
            .map(|(_, serial)| *serial)
    }
    /// Whether the index takes a companion that takes a slot of the bound: the
    /// one place the bound is decided. A companion already holding a slot
    /// keeps it, and a body that takes no slot is always admitted.
    pub(super) fn admits(&self, serial: Option<u64>, slotted: bool) -> bool {
        !slotted
            || serial.is_some_and(|serial| {
                self.slots.contains(&serial) || self.reserved.contains(&serial)
            })
            || self.slots.len() + self.reserved.len() < self.held_limit
    }
    /// Decides whether `serial` may take a slot of the bound and holds it when
    /// it may, in one step under this lock: a call that needs I/O before its
    /// commit keeps the slot for the commit, so two calls cannot both be told
    /// the bound has room for one. [`Slot::Held`] means it already has one
    /// (or a reservation) and nothing was taken.
    pub(super) fn reserve(&mut self, serial: u64) -> Slot {
        // A companion another call removed takes nothing: its commit finds it
        // gone, and a reservation left for it would shrink the bound for good.
        if self.slots.contains(&serial)
            || self.reserved.contains(&serial)
            || !self.files.contains_key(&serial)
        {
            return Slot::Held;
        }
        if self.slots.len() + self.reserved.len() >= self.held_limit {
            return Slot::Full;
        }
        self.reserved.insert(serial);
        Slot::Taken
    }
    /// Gives back a slot reserved for a commit that did not land.
    pub(super) fn unreserve(&mut self, serial: u64) {
        self.reserved.remove(&serial);
    }
    /// The companions no name of the export reaches: neither the name it
    /// records nor another name of its inode finds it. Such a companion holds
    /// its body with no way for a client to ask for it, which is one of the two
    /// shapes a lost attribute takes (the other is no companion at all). A
    /// companion reached only through its main inode - one whose `._` name the
    /// client removed, or whose main file went under that name - is reachable,
    /// which is why a name key kept for an inode's companion is not a leak.
    /// Counted with the other holdings, by who reports them: the release line
    /// says which shape a loss had, and a status read says so while serving.
    pub(super) fn unreachable(&self) -> usize {
        self.files
            .iter()
            .filter(|(serial, file)| {
                self.names.get(&file.named) != Some(serial)
                    && !file
                        .also
                        .iter()
                        .any(|also| self.names.get(also) == Some(serial))
                    && file
                        .inode
                        .is_none_or(|inode| self.inodes.get(&inode) != Some(serial))
            })
            .count()
    }
    /// Whether `orphan` is still the slot reclamation found: the same main
    /// inode, still taking a slot, and not used by any call since.
    pub(super) fn unchanged(&self, orphan: &Orphan) -> bool {
        self.files.get(&orphan.serial).is_some_and(|file| {
            file.inode == orphan.inode && file.slotted() && file.used == orphan.used
        })
    }
    pub(super) fn holding(&self) -> Holding {
        Holding {
            held: self.files.len(),
            slotted: self.slots.len(),
            resident_bytes: self.resident_bytes,
            decoded: self.decoded,
            unreachable: self.unreachable(),
        }
    }
}
/// Whether a whole image of `len` bytes is kept decoded: only when its decoded
/// form costs less than the bytes it replaces, and at most
/// [`DECODED_ENTRY_BYTES`]. Many small attributes cost more decoded than as
/// written, and such an image is kept as the client wrote it.
fn worth_decoding(image: &Image, len: usize) -> bool {
    image.cost() <= DECODED_ENTRY_BYTES && image.cost() < len
}
