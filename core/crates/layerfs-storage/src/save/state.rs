//! One producer's bounded pending, framing and reconstruction state.
use super::seal::Packer;
use crate::{
    cas::{PendingBatch, SaveProfile},
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
    storage::Storage,
};
use std::{
    cell::{RefCell, RefMut},
    collections::BTreeMap,
};

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
}

pub(super) struct State<'a> {
    pub(super) storage: &'a Storage,
    pub(super) pending: PendingBatch,
    pub(super) candidates: RefMut<'a, Candidates>,
    pub(super) signatures: RefCell<BTreeMap<usize, SignatureRow>>,
    pub(super) packer: Packer,
    pub(super) compression: CompressionWorkspace,
    pub(super) decode: DecompressionWorkspace,
    pub(super) groups: GroupCache,
    pub(super) packs: BTreeMap<i64, Vec<u8>>,
    pub(super) pool: PoolReader,
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
        let mut candidates = storage
            .candidates
            .try_borrow_mut()
            .map_err(|_| crate::StorageError::Integrity("one save per storage handle"))?;
        if candidates.needs_load() {
            candidates.reload(&storage.source)?;
        }
        Ok(Self {
            storage,
            pending: PendingBatch::new(storage.capacities()),
            candidates,
            signatures: RefCell::new(BTreeMap::new()),
            packer: Packer::new(),
            compression: CompressionWorkspace::new()?,
            decode: DecompressionWorkspace::new()?,
            groups: GroupCache::new(),
            packs: BTreeMap::new(),
            pool: PoolReader::new(),
            depths: DepthCache::new(),
            chain: ChainCounters::default(),
            chain_total: ChainCounters::default(),
            delta: DeltaCounters::default(),
            profile: SaveProfile::default(),
            outcome: WriteOutcome::default(),
            next_pack: 0,
            pack_end: 0,
        })
    }
}
