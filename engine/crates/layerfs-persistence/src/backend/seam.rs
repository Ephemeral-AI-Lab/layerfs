//! The engine seam (#506): the opened engine behind a history provider and
//! the two traits through which shared semantics reach an engine.
use super::records::{BackendError, Param, Record};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::PackInfo,
    location::{InodeValueRow, LocatedObject, ObjectLocation, SignatureRow},
    port::{
        AcquiredPackRead, PackRange, PackReadChoice, PackReadPlan, PersistedPack, PersistenceError,
    },
    StoragePolicy,
};
use std::sync::Arc;

/// One past the last pooled ordinal a cursor may reach.
pub(crate) const ORDINAL_END: i64 = 4_294_967_296;

/// One opened engine session behind a history provider.
#[derive(Clone)]
pub(crate) enum Engine {
    Sqlite(Arc<super::Session>),
    Postgres(Arc<super::postgresql::session::connection::Session>),
}

/// Named history statements in one open engine transaction.
pub(crate) trait HistoryStatements {
    fn history(&self, key: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError>;
    /// The same open transaction's physical object and metadata access.
    fn mechanics(&self) -> &dyn Mechanics;
    /// One statement over parallel column arrays, where the engine binds
    /// arrays (PostgreSQL: one round trip for many rows). `None`: the
    /// engine has no such statement and the caller runs its per-row one.
    fn history_arrays(
        &self,
        _key: &str,
        _columns: Vec<Column>,
    ) -> Option<Result<Vec<Record>, BackendError>> {
        None
    }
}

/// One column of an array statement; only PostgreSQL reads them.
pub(crate) enum Column {
    I64(Vec<i64>),
    Bytes(Vec<Vec<u8>>),
}

/// Engine statements under the shared Metadata and Objects bounds; every
/// call runs inside one caller-owned transaction.
pub(crate) trait Mechanics {
    /// The stored policy row, checked against the engine's schema version.
    fn policy(&self) -> Result<StoragePolicy, PersistenceError>;
    /// Next unreserved pack id and inode-value ordinal.
    fn allocation_cursor(&self) -> Result<(i64, i64), BackendError>;
    fn advance_allocation(&self, pack: i64, ordinal: i64) -> Result<(), BackendError>;
    /// Takes `packs` pack ids and `ordinals` ordinals from the cursor and
    /// returns where each block starts. `Capacity` when either would pass
    /// its bound; nothing is taken then.
    fn allocate(&self, packs: i64, ordinals: i64) -> Result<(i64, i64), BackendError> {
        let (pack, ordinal) = self.allocation_cursor()?;
        let end_pack = pack
            .checked_add(packs)
            .filter(|v| *v > 0)
            .ok_or(BackendError::Capacity)?;
        let end_ordinal = ordinal
            .checked_add(ordinals)
            .filter(|v| *v <= ORDINAL_END)
            .ok_or(BackendError::Capacity)?;
        self.advance_allocation(end_pack, end_ordinal)?;
        Ok((pack, ordinal))
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError>;
    /// Up to `limit` stored locators that follow `after` in this engine's
    /// locator order, or the first `limit` when `after` is `None`, in that
    /// order. The order is fixed for one transaction: identities by their
    /// bytes, or as [`Self::locators_by_signed_prefix`] says. A mark reads
    /// the locators through this in bounded pieces and never holds them all.
    fn locations_after(
        &self,
        after: Option<&ObjectId>,
        limit: usize,
        out: &mut Vec<ObjectLocation>,
    ) -> Result<(), PersistenceError>;
    /// The stored locator of one object, or `None`, without the descriptor
    /// of its pack.
    fn location(&self, id: &ObjectId) -> Result<Option<ObjectLocation>, PersistenceError> {
        let mut found = Vec::with_capacity(1);
        self.locate(std::slice::from_ref(id), &mut found)?;
        Ok(found
            .first()
            .map(|located| located.location)
            .filter(|location| location.object_id == *id))
    }
    /// A hint that objects of `ids` are about to be asked through
    /// [`Self::location`], inside this transaction. An engine that pays a
    /// round trip per lookup may find them all in one and keep the answers;
    /// every answer is the same with or without the hint. Only an engine
    /// that says so ([`Self::batches_locations`]) is told.
    fn expect_locations(&self, _ids: &[ObjectId]) -> Result<(), PersistenceError> {
        Ok(())
    }
    /// Whether [`Self::expect_locations`] saves this engine anything. A
    /// caller that must read ahead to learn the ids does so only then.
    fn batches_locations(&self) -> bool {
        false
    }
    /// Whether [`Self::locations_after`] orders identities by their first
    /// eight bytes read as a signed big-endian number, then by their bytes,
    /// instead of by their bytes alone.
    fn locators_by_signed_prefix(&self) -> bool {
        false
    }
    /// A local directory for the sort files of one mark: files created,
    /// unlinked at once and gone with their handle, never Store state.
    fn scratch_directory(&self) -> std::path::PathBuf {
        std::env::temp_dir()
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError>;
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError>;
    fn read_packs(&self, ids: &[i64], out: &mut Vec<PersistedPack>)
        -> Result<(), PersistenceError>;
    /// The bounded control/directory prefix of one pack and its declared
    /// length, or `None` when this engine has no bounded prefix read.
    fn pack_prefix(
        &self,
        _id: i64,
        _limit: usize,
    ) -> Result<Option<(usize, Vec<u8>)>, PersistenceError> {
        Ok(None)
    }
    /// [`Self::pack_prefix`] for several packs in one call, in `ids` order.
    /// An engine answers from one cursor or one statement, not one per pack;
    /// a pack that is absent is an error.
    fn pack_prefixes(
        &self,
        ids: &[i64],
        limit: usize,
        out: &mut Vec<(usize, Vec<u8>)>,
    ) -> Result<(), PersistenceError> {
        for id in ids {
            out.push(
                self.pack_prefix(*id, limit)?
                    .ok_or(PersistenceError::BackendUnavailable)?,
            );
        }
        Ok(())
    }
    /// The bytes of selected ranges of several packs in one call: for each
    /// `(pack, ranges)` of `requests`, in order, the bytes of each range, in
    /// order. A pack's ranges are sorted and disjoint. The bytes are not
    /// authenticated; the consumer does that. An engine answers from one
    /// cursor or one statement; this default acquires each pack on its own.
    fn pack_ranges(
        &self,
        requests: &[(i64, Vec<PackRange>)],
        out: &mut Vec<Vec<Vec<u8>>>,
    ) -> Result<(), PersistenceError> {
        ranges_by_acquisition(self, requests, out)
    }
    /// The widest gap between two wanted ranges of one pack that costs this
    /// engine less to read through than to skip. A caller of
    /// [`Self::pack_ranges`] may join ranges no further apart than this into
    /// one, and take the bytes between them as well. `0`: every byte asked
    /// for is transferred at its own cost, so ask for exactly what is wanted.
    fn pack_range_fill(&self) -> usize {
        0
    }
    /// A hint that each pack of `ids` is about to be asked through
    /// [`Self::read_scoped_pack`], in this order, inside this transaction.
    /// An engine that pays a round trip per pack may read the small ones
    /// ahead in one; every answer is the same with or without the hint.
    fn expect_scoped_packs(&self, _ids: &[i64]) -> Result<(), PersistenceError> {
        Ok(())
    }
    /// [`Self::inode_values`], and at the engine's choice also the values of
    /// the blocks holding up to `ahead` ordinals past the last one asked, so
    /// a caller that reads leaves in ordinal order can keep them for its next
    /// calls. Rows come in ascending ordinal order; the extra ones are any
    /// rows of `out` the caller did not ask for.
    fn inode_values_ahead(
        &self,
        ordinals: &[u32],
        _ahead: u32,
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        self.inode_values(ordinals, out)
    }
    /// One pack through the caller's bounded range plan: only the control
    /// prefix and the selected complete groups are read, never the body behind
    /// them. The storage port owns the plan and the returned units.
    fn read_scoped_pack(
        &self,
        _id: i64,
        _plan: &mut dyn layerfs_storage::port::PackReadPlan,
    ) -> Result<layerfs_storage::port::AcquiredPackRead, PersistenceError> {
        Err(PersistenceError::Refused {
            status: "pack range read".to_owned(),
        })
    }
}

/// A plan that selects ranges chosen before the acquisition starts.
struct FixedRanges(Option<Vec<PackRange>>);

impl PackReadPlan for FixedRanges {
    fn select(
        &mut self,
        _info: PackInfo,
        _prefix: &[u8],
    ) -> Result<PackReadChoice, PersistenceError> {
        self.0
            .take()
            .map(PackReadChoice::Ranges)
            .ok_or(PersistenceError::Malformed)
    }
}

/// [`Mechanics::pack_ranges`] as one scoped acquisition per pack.
pub(crate) fn ranges_by_acquisition<M: Mechanics + ?Sized>(
    mechanics: &M,
    requests: &[(i64, Vec<PackRange>)],
    out: &mut Vec<Vec<Vec<u8>>>,
) -> Result<(), PersistenceError> {
    for (id, ranges) in requests {
        let mut plan = FixedRanges(Some(ranges.clone()));
        let AcquiredPackRead::Units(units) = mechanics.read_scoped_pack(*id, &mut plan)? else {
            return Err(PersistenceError::Malformed);
        };
        out.push(
            units
                .into_parts()
                .2
                .into_iter()
                .map(|(_, bytes)| bytes)
                .collect(),
        );
    }
    Ok(())
}
