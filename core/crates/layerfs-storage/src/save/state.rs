//! One producer's bounded pending, framing and reconstruction state.
use super::seal::Packer;
use crate::{
    encoding::{
        delta::{
            candidates::Candidates,
            read::ChainCounters,
            select::{DeltaCounters, DepthCache},
        },
        pool::PoolReader,
        CompressionWorkspace, DecompressionWorkspace, GroupCache,
    },
    error::StorageResult,
    location::SignatureRow,
    save::{PendingBatch, SaveProfile},
    storage::Storage,
};
use layerfs_content::inode_leaf::INODE_VALUE_BYTES;
use std::{
    cell::{RefCell, RefMut},
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    sync::{Mutex, PoisonError},
};

/// Encode and decode regions of finished Saves, kept for the next Save
/// (F-043).
///
/// Each Save used to allocate and zero its own 16 MiB encode region (and its
/// 1 MiB decode region) and free them at the end. glibc then kept a freed
/// region in the allocator arena of the thread that ran it, so a process
/// making Commits on a thread pool held about one region per thread arena:
/// 1.2 GB measured in the SDK runner. Only plain regions are kept, never a
/// zstd context, and at most one per available CPU per kind; a region past
/// that is freed. Releasing them when the Store closes is a follow-up once
/// `store/handle.rs` is free.
static IDLE_ENCODE: Mutex<Vec<Vec<u64>>> = Mutex::new(Vec::new());
static IDLE_DECODE: Mutex<Vec<Vec<u64>>> = Mutex::new(Vec::new());

/// A codec workspace built in a plain region.
pub(super) trait Region: Sized {
    fn fresh() -> StorageResult<Self>;
    fn in_region(region: Vec<u64>) -> StorageResult<Self>;
    fn into_region(self) -> Option<Vec<u64>>;
}
impl Region for CompressionWorkspace {
    fn fresh() -> StorageResult<Self> {
        Self::new()
    }
    fn in_region(region: Vec<u64>) -> StorageResult<Self> {
        Self::in_region(region)
    }
    fn into_region(self) -> Option<Vec<u64>> {
        Some(self.into_region())
    }
}
impl Region for DecompressionWorkspace {
    fn fresh() -> StorageResult<Self> {
        Self::new()
    }
    fn in_region(region: Vec<u64>) -> StorageResult<Self> {
        Self::in_region(region)
    }
    fn into_region(self) -> Option<Vec<u64>> {
        self.into_region()
    }
}

/// A Save's workspace; its region returns to `idle` when dropped.
pub(super) struct Kept<W: Region> {
    workspace: Option<W>,
    idle: &'static Mutex<Vec<Vec<u64>>>,
}
impl<W: Region> Kept<W> {
    fn new(idle: &'static Mutex<Vec<Vec<u64>>>) -> StorageResult<Self> {
        let region = idle.lock().unwrap_or_else(PoisonError::into_inner).pop();
        let workspace = Some(match region {
            Some(region) => W::in_region(region)?,
            None => W::fresh()?,
        });
        Ok(Self { workspace, idle })
    }
}
impl<W: Region> Deref for Kept<W> {
    type Target = W;
    fn deref(&self) -> &W {
        self.workspace.as_ref().expect("workspace until drop")
    }
}
impl<W: Region> DerefMut for Kept<W> {
    fn deref_mut(&mut self) -> &mut W {
        self.workspace.as_mut().expect("workspace until drop")
    }
}
impl<W: Region> Drop for Kept<W> {
    fn drop(&mut self) {
        let Some(region) = self.workspace.take().and_then(W::into_region) else {
            return;
        };
        let bound = std::thread::available_parallelism().map_or(1, usize::from);
        let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
        if idle.len() < bound {
            idle.push(region);
        }
    }
}

/// Acknowledged admission and physical-selection counts; diagnostics, not timing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WriteOutcome {
    /// New object locators acknowledged by registration.
    pub inserted: u64,
    /// Exact canonical reuses, including first-wins races.
    pub reused: u64,
    /// New objects selected as FULL.
    pub full_records: u64,
    /// New objects selected as PREFIX.
    pub prefix_records: u64,
    /// Complete immutable packs acknowledged by registration.
    pub packs: u64,
    /// Canonical bytes of new acknowledged objects.
    pub canonical_bytes: u64,
    /// Complete newly published pack bodies, including first-wins race leftovers.
    pub stored_bytes: u64,
    /// Complete newly published metadata-domain pack bytes.
    pub metadata_bytes: u64,
    /// Actual pooled selection and value assignment counts, including race trials.
    pub pool: crate::save::PoolCounters,
    /// Actual payload delta selection counts and stored depths.
    pub delta: DeltaCounters,
}

pub(super) struct State<'a> {
    pub(super) storage: &'a Storage,
    pub(super) pending: PendingBatch,
    pub(super) candidates: RefMut<'a, Candidates>,
    pub(super) signatures: RefCell<BTreeMap<usize, SignatureRow>>,
    pub(super) packer: Packer,
    pub(super) compression: Kept<CompressionWorkspace>,
    pub(super) decode: Kept<DecompressionWorkspace>,
    pub(super) groups: GroupCache,
    pub(super) packs: crate::encoding::PackCache,
    pub(super) pool: PoolReader,
    /// Canonical bytes this Save resolved, within a byte bound.
    pub(super) resolved: super::resolved::Resolved,
    /// Values this Save assigned and has not published yet, by ordinal.
    pub(super) values: BTreeMap<u32, [u8; INODE_VALUE_BYTES]>,
    /// Fingerprint of each value this Save assigned and the ordinal it got.
    /// A candidate filter only: full value bytes decide equality.
    pub(super) value_memo: BTreeMap<u64, u32>,
    pub(super) pool_stats: crate::save::PoolCounters,
    pub(super) next_ordinal: u64,
    pub(super) ordinal_end: u64,
    pub(super) finishing: bool,
    pub(super) depths: DepthCache,
    pub(super) chain: ChainCounters,
    pub(super) chain_total: ChainCounters,
    pub(super) delta: DeltaCounters,
    pub(super) profile: SaveProfile,
    pub(super) outcome: WriteOutcome,
    pub(super) next_pack: i64,
    pub(super) pack_end: i64,
}
impl<'a> State<'a> {
    pub(super) fn new(storage: &'a Storage) -> StorageResult<Self> {
        // The candidate ring loads at its first use, not here (#352 record
        // 034, S1-b; `Candidates::ensure_loaded`).
        let mut candidates = storage
            .candidates
            .try_borrow_mut()
            .map_err(|_| crate::StorageError::Integrity("one save per storage handle"))?;
        candidates.begin_save();
        let mut state = Self {
            storage,
            pending: PendingBatch::new(storage.capacities()),
            candidates,
            signatures: RefCell::new(BTreeMap::new()),
            packer: Packer::new(),
            compression: Kept::new(&IDLE_ENCODE)?,
            decode: Kept::new(&IDLE_DECODE)?,
            groups: GroupCache::new(),
            packs: crate::encoding::PackCache::new(),
            pool: PoolReader::new(),
            resolved: Default::default(),
            values: BTreeMap::new(),
            value_memo: BTreeMap::new(),
            pool_stats: crate::save::PoolCounters::default(),
            next_ordinal: 0,
            ordinal_end: 0,
            finishing: false,
            depths: DepthCache::new(),
            chain: ChainCounters::default(),
            chain_total: ChainCounters::default(),
            delta: DeltaCounters::default(),
            profile: SaveProfile::default(),
            outcome: WriteOutcome::default(),
            next_pack: 0,
            pack_end: 0,
        };
        state.reserve_initial()?;
        Ok(state)
    }
}
