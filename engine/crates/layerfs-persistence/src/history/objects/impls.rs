//! The inherent accessors of the mark result types.
//!
//! Kept in a submodule so `objects.rs` stays within the product source-line
//! boundary; the types stay next to their construction.

use super::records::Count;
use super::IdSet;
use super::KeptObjects;
use super::KeptWaste;
use super::MarkResult;
use super::PartlyDead;
use super::MAX_KEPT_WASTE_REFERENCES;
use crate::history::kept::KeptHistory;
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use std::sync::{Arc, Mutex};

impl KeptWaste {
    /// One mark's kept-waste accounting. The reference list is the full set the
    /// mark reached; it is cut at [`MAX_KEPT_WASTE_REFERENCES`] here, and the
    /// cut is recorded so the §11 projection still renders `{value, cap}` even
    /// though the stored list is already bounded.
    #[doc(hidden)]
    pub fn from_parts(
        dead_in_retained_packs_bytes: u64,
        delta_bases_bytes: u64,
        mut delta_base_references: Vec<ObjectId>,
        inode_blocks_bytes: u64,
    ) -> Self {
        let delta_base_references_capped = delta_base_references.len() > MAX_KEPT_WASTE_REFERENCES;
        delta_base_references.truncate(MAX_KEPT_WASTE_REFERENCES);
        Self {
            dead_in_retained_packs_bytes,
            delta_bases_bytes,
            delta_base_references,
            delta_base_references_capped,
            inode_blocks_bytes,
        }
    }

    /// Stored bytes of records in retained packs that no reached object owns.
    pub fn dead_in_retained_packs_bytes(&self) -> u64 {
        self.dead_in_retained_packs_bytes
    }

    /// Stored bytes of objects reached only as a delta base.
    pub fn delta_bases_bytes(&self) -> u64 {
        self.delta_bases_bytes
    }

    /// The objects counted by [`Self::delta_bases_bytes`], in ascending order.
    ///
    /// The list carries at most [`MAX_KEPT_WASTE_REFERENCES`] identities; the
    /// byte total above stays exact, so a pathological delta graph cannot make
    /// the report payload unbounded.
    pub fn delta_base_references(&self) -> &[ObjectId] {
        &self.delta_base_references
    }

    /// Whether [`Self::delta_base_references`] was cut at
    /// [`MAX_KEPT_WASTE_REFERENCES`] before it was stored.
    pub fn delta_base_references_capped(&self) -> bool {
        self.delta_base_references_capped
    }

    /// Bytes of `inode_value_block` rows, which 2.0 keeps in full (§6.4).
    pub fn inode_blocks_bytes(&self) -> u64 {
        self.inode_blocks_bytes
    }

    /// Whether every part is zero.
    pub fn is_empty(&self) -> bool {
        self.dead_in_retained_packs_bytes == 0
            && self.delta_bases_bytes == 0
            && self.inode_blocks_bytes == 0
    }
}
/// Two marks kept the same when their sets and counts agree. The partly dead
/// packs are compared as lists when both marks listed them, and by their
/// count always.
impl PartialEq for KeptObjects {
    fn eq(&self, other: &Self) -> bool {
        self.objects == other.objects
            && self.count == other.count
            && self.retained == other.retained
            && self.waste == other.waste
            && self.partly_dead.count == other.partly_dead.count
            && self.partly_dead_listed == other.partly_dead_listed
            && self.ordinals == other.ordinals
    }
}
impl Eq for KeptObjects {}
impl KeptObjects {
    /// Every reached canonical object, in ascending identity order, when
    /// the mark listed them ([`crate::HistoryProvider::mark`]). A SQLite
    /// compaction mark holds no list, and this is empty: [`Self::len`] still
    /// counts them.
    pub fn objects(&self) -> &[ObjectId] {
        &self.objects
    }

    /// Every pack holding a reached object, in ascending pack order. The
    /// list is made the first time it is asked for and then held, eight
    /// bytes a pack: a caller that must not hold a list reads
    /// [`Self::retained_packs`].
    pub fn packs(&self) -> &[i64] {
        self.packs.get_or_init(|| {
            let mut packs = Vec::with_capacity(usize::try_from(self.retained.len()).unwrap_or(0));
            packs.extend(self.retained.iter().map(|pack| pack as i64));
            packs
        })
    }

    /// The packs holding a reached object, as a set of pack ids: one bit a
    /// pack, with membership, a count and an ascending iterator.
    pub fn retained_packs(&self) -> &IdSet {
        &self.retained
    }

    /// The bytes the mark keeps without a retained root needing them.
    pub fn waste(&self) -> &KeptWaste {
        &self.waste
    }

    /// Every retained pack that holds a record no reached object owns, in
    /// ascending pack order, with the bytes it adds to
    /// [`KeptWaste::dead_in_retained_packs_bytes`]. A pack named with zero
    /// bytes holds an unreached record inside a compressed group.
    ///
    /// Listed by a mark that lists what it kept
    /// ([`crate::HistoryProvider::mark`], `mark_with_scratch`); a
    /// compaction's own mark leaves it empty, while
    /// [`Self::partly_dead_count`] and [`Self::each_partly_dead`] always
    /// answer.
    pub fn partly_dead_packs(&self) -> &[(i64, u64)] {
        &self.partly_dead_listed
    }

    /// How many retained packs hold a record no reached object owns.
    pub fn partly_dead_count(&self) -> u64 {
        self.partly_dead.count
    }

    /// Hands every partly dead pack and its dead bytes to `each`, in
    /// ascending pack order, without holding the list: beyond a bound the
    /// mark keeps it in a sort file.
    pub fn each_partly_dead(
        &self,
        each: &mut dyn FnMut(i64, u64) -> HistoryResult<()>,
    ) -> HistoryResult<()> {
        let Some(packs) = &self.partly_dead.packs else {
            return Ok(());
        };
        let mut packs = packs
            .lock()
            .map_err(|_| HistoryError::Integrity("mark pack list"))?;
        let mut sorted = packs.sorted()?;
        while let Some(pack) = sorted.next()? {
            each(pack.pack, pack.records)?;
        }
        Ok(())
    }

    /// Fills [`Self::partly_dead_packs`].
    pub(in crate::history) fn list_partly_dead(&mut self) -> HistoryResult<()> {
        let mut listed = Vec::with_capacity(usize::try_from(self.partly_dead.count).unwrap_or(0));
        self.each_partly_dead(&mut |pack, bytes| {
            listed.push((pack, bytes));
            Ok(())
        })?;
        self.partly_dead_listed = listed;
        Ok(())
    }

    /// The inode value ordinals the reached leaves read, as inclusive runs in
    /// ascending order. A read of a kept leaf asks for no other ordinal, so a
    /// rebuild keeps only the `inode_value_block` rows that hold one of them.
    ///
    /// The list is made the first time it is asked for and then held, eight
    /// bytes a run: a caller that must not hold a list reads the runs of
    /// [`Self::inode_ordinal_set`].
    pub fn inode_ordinals(&self) -> &[(u32, u32)] {
        self.inode_ordinals.get_or_init(|| {
            self.ordinals
                .runs()
                .map(|(first, last)| (first as u32, last as u32))
                .collect()
        })
    }

    /// The inode value ordinals the reached leaves read, as a set: one bit
    /// an ordinal, with membership and its runs in ascending order.
    pub fn inode_ordinal_set(&self) -> &IdSet {
        &self.ordinals
    }

    /// Whether `id` is among the listed objects ([`Self::objects`]).
    pub fn contains(&self, id: ObjectId) -> bool {
        self.objects.binary_search(&id).is_ok()
    }

    /// How many objects the closure holds.
    pub fn len(&self) -> usize {
        usize::try_from(self.count).unwrap_or(usize::MAX)
    }

    /// Whether the closure holds no object.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}
impl MarkResult {
    /// The retained history closure, exactly as written to `kept_node`.
    pub fn history(&self) -> &KeptHistory {
        &self.history
    }

    /// The retained object and pack closure.
    pub fn objects(&self) -> &KeptObjects {
        &self.objects
    }

    /// BLAKE3 over the §6.4 live-heads vector at the mark's fixed generation.
    pub fn live_heads_digest(&self) -> [u8; 32] {
        self.live_heads_digest
    }

    /// The distinct live-pack body bytes this walk read: the §11 `markBytes`
    /// transfer metric on PostgreSQL (a #52 measurement, not a report field).
    ///
    /// A whole-body read counts the pack's body; a scoped acquisition counts
    /// the group extents it transferred, which the group cache asks for only
    /// once. The bounded directory prefix every retained pack costs the
    /// `keptWaste` attribution is not a body read, and a later whole-body read
    /// replaces the partial extents it covers. A metadata header read still
    /// acquires the record's group, so any non-empty closure transfers some
    /// body bytes; the SQLite §7 payload sweep runs in the build copy, so
    /// SQLite does not report this value.
    pub fn mark_bytes(&self) -> u64 {
        self.mark_bytes
    }

    /// The most bytes the mark's sort files held at one time. The mark sorts
    /// what it reached in bounded memory and writes what does not fit to
    /// unnamed files beside a SQLite Store (a local temporary directory for
    /// PostgreSQL); zero when everything fitted. A compaction's own sort
    /// files are in the same space and count here from then on.
    pub fn scratch_bytes(&self) -> u64 {
        self.space.peak_bytes()
    }

    /// Where the sort files of this mark went: the one space every sort
    /// file of a compaction is created in and counted by.
    pub(crate) fn sort_space(&self) -> &Arc<super::Space> {
        &self.space
    }
}

/// Two marks are equal when they kept the same and read the same; where
/// their sort files went is not part of what they found.
impl PartialEq for MarkResult {
    fn eq(&self, other: &Self) -> bool {
        self.history == other.history
            && self.objects == other.objects
            && self.live_heads_digest == other.live_heads_digest
            && self.mark_bytes == other.mark_bytes
    }
}
impl Eq for MarkResult {}

impl PartlyDead {
    /// The list a kept-waste pass adds to, in ascending pack order.
    pub(super) fn new(packs: super::Sorter<Count>, count: u64) -> Self {
        Self {
            packs: Some(Arc::new(Mutex::new(packs))),
            count,
        }
    }
}
