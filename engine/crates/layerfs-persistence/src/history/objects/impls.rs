//! The inherent accessors of the mark result types.
//!
//! Kept in a submodule so `objects.rs` stays within the product source-line
//! boundary; the types stay next to their construction.

use super::KeptObjects;
use super::KeptWaste;
use super::MarkResult;
use super::MAX_KEPT_WASTE_REFERENCES;
use crate::history::kept::KeptHistory;
use layerfs_content::ObjectId;

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
impl KeptObjects {
    /// Every reached canonical object, in ascending identity order, when
    /// the mark listed them ([`crate::HistoryProvider::mark`]). A SQLite
    /// compaction mark holds no list, and this is empty: [`Self::len`] still
    /// counts them.
    pub fn objects(&self) -> &[ObjectId] {
        &self.objects
    }

    /// Every pack holding a reached object, in ascending pack order.
    pub fn packs(&self) -> &[i64] {
        &self.packs
    }

    /// The bytes the mark keeps without a retained root needing them.
    pub fn waste(&self) -> &KeptWaste {
        &self.waste
    }

    /// Every retained pack that holds a record no reached object owns, in
    /// ascending pack order, with the bytes it adds to
    /// [`KeptWaste::dead_in_retained_packs_bytes`]. A pack named with zero
    /// bytes holds an unreached record inside a compressed group.
    pub fn partly_dead_packs(&self) -> &[(i64, u64)] {
        &self.partly_dead
    }

    /// The inode value ordinals the reached leaves read, as inclusive runs in
    /// ascending order. A read of a kept leaf asks for no other ordinal, so a
    /// rebuild keeps only the `inode_value_block` rows that hold one of them.
    pub fn inode_ordinals(&self) -> &[(u32, u32)] {
        &self.inode_ordinals
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

    /// The most bytes the mark's sort files held, summed over the files. The
    /// mark sorts what it reached in bounded memory and writes what does not
    /// fit to unnamed files beside a SQLite Store (the temporary directory
    /// for PostgreSQL); zero when everything fitted.
    pub fn scratch_bytes(&self) -> u64 {
        self.scratch_bytes
    }
}
