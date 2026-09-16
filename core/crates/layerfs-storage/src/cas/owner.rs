//! One save mutation owner: cursors, placement state and the write transaction.
//!
//! Ownership is acquired once with a single `BEGIN IMMEDIATE` attempt; a lost
//! write lock fails immediately instead of queueing. The owner tracks the pack and
//! group cursors it established at acquisition, keeps at most one open tail per
//! framing lane, and writes packs and locators only inside its open transaction.

use rusqlite::Connection;

use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};

use crate::cas::dependencies::Availability;
use crate::encoding::codec::{CompressionWorkspace, DecompressionWorkspace};
use crate::encoding::delta::candidates::Candidates;
use crate::encoding::delta::read::ChainCounters;
use crate::encoding::delta::select::{select, DeltaCounters, DepthCache, SelectInput};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::PackLane;
use crate::pack::placement::LanePlacement;
use crate::pack::{build_group, SelectedWrite};
use crate::policy::{StorageCapacities, StoragePolicy};
use crate::sqlite::lookup;
use crate::sqlite::write::{self, ObjectRow, TransactionState};
use std::collections::BTreeMap;

/// Soft group target: a group is sealed once the next record would pass it.
const GROUP_TARGET: usize = 48 * 1024;

/// One record waiting for its group to be framed and placed.
///
/// Only identifiers and lengths are retained, never the payload: a group is bounded
/// by its framed bytes, so retaining canonical bytes could hold far more than the
/// group target whenever records compress well. An identity that is still waiting
/// here has no row yet, which is why the owner can seal its group on demand and let
/// the ordinary verified storage path answer for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingMember {
    object_id: ObjectId,
    role: ObjectRole,
    canonical_length: usize,
    base_object_id: Option<ObjectId>,
}

#[derive(Default)]
struct PendingGroup {
    records: Vec<Vec<u8>>,
    members: Vec<PendingMember>,
    framed_len: usize,
}

impl PendingGroup {
    fn clear(&mut self) {
        self.records.clear();
        self.members.clear();
        self.framed_len = 0;
    }
}

/// Counters describing what one save operation actually did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutcomeCounters {
    /// Occurrences served by an exact existing row.
    pub reused: u64,
    /// Objects newly written.
    pub inserted: u64,
    /// Packs created.
    pub packs_created: u64,
    /// Appends to a pack this save created.
    pub pack_appends: u64,
    /// Write transactions started.
    pub transactions: u64,
    /// Write transactions acknowledged with `COMMIT`.
    pub commits: u64,
    /// Record-level objects newly written as a FULL representation.
    pub full_records: u64,
    /// Record-level objects newly written as a PREFIX representation.
    pub prefix_records: u64,
    /// Representation selection outcomes.
    pub delta: DeltaCounters,
    /// Work spent acquiring delta bases.
    pub chain: ChainCounters,
    /// Pooled metadata lane outcomes.
    pub pool: PoolCounters,
}

/// Exclusive writer state for one save operation.
pub struct MutationOwner {
    connection: Connection,
    capacities: StorageCapacities,
    baseline_pack_id: i64,
    next_pack_id: i64,
    /// Highest pack id this save created; becomes the publication watermark on
    /// acknowledgement and is left untouched on any failure.
    ceiling: i64,
    placement: [LanePlacement; 5],
    groups: [PendingGroup; 5],
    transaction: TransactionState,
    transaction_open: bool,
    compression: CompressionWorkspace,
    decompression: DecompressionWorkspace,
    terminal: bool,
    cleanup_attempted: bool,
    quarantined: bool,
    counters: OutcomeCounters,
    /// Admitted-FULL winner cache: owned by this operation, bounded and dropped
    /// with it, so a failed save can never leave a partly advanced cache usable.
    candidates: Candidates,
    /// Bounded per-save dependency-depth cache.
    depths: DepthCache,
    /// Pack bodies already read while acquiring delta bases in this operation.
    pack_cache: BTreeMap<i64, Vec<u8>>,
    /// Work spent acquiring and reading delta bases.
    chain: ChainCounters,
    /// Representation selection outcomes.
    delta: DeltaCounters,
    /// Store-owned bounded ordered set of pooled value candidates.
    pool_index: std::sync::Arc<std::sync::Mutex<crate::encoding::pool::PoolIndex>>,
    /// Bounded pooled-value reader used while synchronizing the index.
    pool_reader: crate::encoding::pool::PoolReader,
    /// Ordinals assigned by this save but not yet visible to the index.
    pending_values: BTreeMap<[u8; 73], u32>,
    /// Next ordinal this save may assign.
    next_ordinal: Option<u32>,
    /// True once the index was synchronized inside this save.
    pool_synced: bool,
    /// Pooled representation outcomes of this save.
    pool: PoolCounters,
}

/// What the pooled metadata lane actually did for one save operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoolCounters {
    /// Canonical inode leaves admitted.
    pub leaves: u64,
    /// Values that reused an existing ordinal.
    pub reused_values: u64,
    /// Values that received a new ordinal.
    pub new_values: u64,
    /// Value groups written.
    pub groups: u64,
    /// Pooled leaves stored against a direct base.
    pub delta_leaves: u64,
    /// Pooled leaves stored in full.
    pub full_leaves: u64,
    /// Delta trials attempted.
    pub trials: u64,
}

impl MutationOwner {
    /// Acquires ownership once and establishes the operation's cursors.
    pub fn acquire(
        connection: Connection,
        policy: StoragePolicy,
        capacities: StorageCapacities,
        pool_index: std::sync::Arc<std::sync::Mutex<crate::encoding::pool::PoolIndex>>,
    ) -> StorageResult<Self> {
        // Ownership first: the baseline and cursor are only meaningful when no
        // other writer can publish a pack between reading them and using them.
        write::begin_immediate(&connection)?;
        let baseline_pack_id = lookup::highest_pack_id(&connection)?;
        let published = crate::sqlite::schema::retained_pack_ceiling(&connection)?;
        if published != baseline_pack_id {
            // Undeleted packs from a save whose cleanup did not complete. Writing
            // now would have to guess their ownership, so the save is refused.
            return Err(StorageError::UninspectedState {
                ceiling: published,
                highest_pack_id: baseline_pack_id,
            });
        }
        let next_pack_id = baseline_pack_id
            .checked_add(1)
            .ok_or(StorageError::Integrity("pack identifier overflow"))?;
        let compression = CompressionWorkspace::new()?;
        let decompression = DecompressionWorkspace::new()?;
        let _ = policy;
        Ok(Self {
            connection,
            capacities,
            baseline_pack_id,
            next_pack_id,
            ceiling: baseline_pack_id,
            placement: [
                LanePlacement::new(),
                LanePlacement::new(),
                LanePlacement::new(),
                LanePlacement::new(),
                LanePlacement::new(),
            ],
            groups: [
                PendingGroup::default(),
                PendingGroup::default(),
                PendingGroup::default(),
                PendingGroup::default(),
                PendingGroup::default(),
            ],
            transaction: TransactionState { rows: 1, bytes: 0 },
            transaction_open: true,
            compression,
            decompression,
            terminal: false,
            cleanup_attempted: false,
            quarantined: false,
            counters: OutcomeCounters {
                transactions: 1,
                ..OutcomeCounters::default()
            },
            candidates: Candidates::new()?,
            depths: DepthCache::new(),
            pack_cache: BTreeMap::new(),
            chain: ChainCounters::default(),
            delta: DeltaCounters::default(),
            pool_index,
            pool_reader: crate::encoding::pool::PoolReader::new(),
            pending_values: BTreeMap::new(),
            next_ordinal: None,
            pool_synced: false,
            pool: PoolCounters::default(),
        })
    }

    /// Highest retained pack identifier before this operation.
    pub fn baseline_pack_id(&self) -> i64 {
        self.baseline_pack_id
    }

    /// Connection held by this owner; same-save reads use it directly.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// True when this owner accepted `id` into an unfinished group.
    ///
    /// Such an identity has no row yet but is available to the same operation, so
    /// a later dependent must resolve it from this bounded state instead of
    /// querying storage for bytes that only exist in memory.
    pub fn pending_member(&self, id: ObjectId) -> bool {
        self.groups
            .iter()
            .any(|group| group.members.iter().any(|member| member.object_id == id))
    }

    /// Records one exact reuse occurrence.
    pub fn note_reuse(&mut self) {
        self.counters.reused += 1;
    }

    /// Bytes retained by the open pack tails of every lane.
    pub fn retained_tail_bytes(&self) -> StorageResult<usize> {
        PackLane::ALL.iter().try_fold(0usize, |total, lane| {
            total
                .checked_add(self.placement[lane.index()].retained_bytes(*lane)?)
                .ok_or(StorageError::Integrity("retained tail accounting"))
        })
    }

    /// Frames and places the open group holding any of `ids`.
    ///
    /// A member of an unfinished group has no row yet: its group is framed but not
    /// placed, so no locator exists for it. Rather than retain a second copy of every
    /// waiting payload, the owner seals that group on demand and lets the ordinary
    /// verified storage path answer for the identity. Retaining the canonical bytes
    /// instead would be unbounded in practice: a group is bounded by its *framed*
    /// bytes, so highly compressible records could hold far more canonical data than
    /// the group target. The cost here is packing granularity for the caller that
    /// needs the identity, never correctness, and each identity is sealed at most
    /// once: afterwards its row exists and the membership lookup finds it.
    pub fn seal_pending(&mut self, ids: &[ObjectId]) -> StorageResult<()> {
        let mut lanes: Vec<PackLane> = Vec::new();
        for id in ids {
            for lane in PackLane::ALL {
                let holds = self.groups[lane.index()]
                    .members
                    .iter()
                    .any(|member| member.object_id == *id);
                if holds && !lanes.contains(&lane) {
                    lanes.push(lane);
                }
            }
        }
        if lanes.is_empty() {
            return Ok(());
        }
        let mut availability = Availability::default();
        for lane in lanes {
            self.seal_group(lane, &mut availability)?;
        }
        Ok(())
    }

    /// Reads objects inside this owner's transaction, with no ceiling.
    pub fn read_batch(&mut self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        let (values, _) = crate::cas::read::read_objects(
            &self.connection,
            ids,
            i64::MAX,
            &self.capacities,
            &mut self.decompression,
        )?;
        Ok(values)
    }

    /// Reconstructs one stored object, following and authenticating its chain.
    pub fn resolve_location(&mut self, location: lookup::ObjectLocation) -> StorageResult<Vec<u8>> {
        let mut resolver = crate::encoding::delta::read::Resolver::new(
            &self.connection,
            i64::MAX,
            &self.capacities,
            &mut self.pack_cache,
            &mut self.decompression,
            &mut self.chain,
        );
        resolver.resolve_at(location)
    }

    /// Prepares and places one missing object.
    ///
    /// `advisory` is the caller's bounded, explicitly declared candidate list in
    /// preference order. Selection may store FULL by policy when no candidate is
    /// acquired, but a failed acquisition, codec call or allocation fails the
    /// operation: a stored FULL alternative is a policy outcome, never error
    /// recovery.
    pub fn offer(
        &mut self,
        object: FinalizedObject,
        advisory: &[ObjectId],
        availability: &mut Availability,
    ) -> StorageResult<()> {
        if self.terminal {
            return Err(StorageError::Aborted);
        }
        availability.validate(&self.connection, &object, i64::MAX, |id| {
            self.pending_member(id)
        })?;
        let record = self.select_record(&object, advisory)?;
        let lane = record.lane;
        let index = lane.index();
        let body = crate::pack::assemble::framed_length(std::slice::from_ref(&record.record))?;
        let body_limit = crate::encoding::lane_body_limit(lane, &self.capacities);
        if body > body_limit && lane != PackLane::Singleton {
            return Err(StorageError::CapacityExceeded {
                what: "owner.record_body",
                limit: body_limit as u64,
                actual: body as u64,
            });
        }
        let occupied = !self.groups[index].records.is_empty();
        let must_seal = match lane {
            PackLane::WholeFile | PackLane::PooledMetadata | PackLane::Singleton => occupied,
            PackLane::Ordinary | PackLane::Native => {
                occupied && self.groups[index].framed_len.saturating_add(body) > GROUP_TARGET
            }
        };
        if must_seal {
            self.seal_group(lane, availability)?;
        }
        let group = &mut self.groups[index];
        group.framed_len = group
            .framed_len
            .checked_add(body)
            .ok_or(StorageError::Integrity("group framing"))?;
        group.records.push(record.record);
        group.members.push(PendingMember {
            object_id: object.id(),
            role: object.role(),
            canonical_length: object.canonical_len(),
            base_object_id: record.base,
        });
        availability.inserted(object.id());
        if matches!(
            lane,
            PackLane::WholeFile | PackLane::PooledMetadata | PackLane::Singleton
        ) {
            self.seal_group(lane, availability)?;
        }
        Ok(())
    }

    fn select_record(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
    ) -> StorageResult<crate::encoding::EncodedRecord> {
        if object.role() == ObjectRole::InodeLeaf {
            return self.select_pooled(object, advisory);
        }
        let mut input = SelectInput {
            connection: &self.connection,
            capacities: &self.capacities,
            candidates: &mut self.candidates,
            depths: &mut self.depths,
            packs: &mut self.pack_cache,
            decode: &mut self.decompression,
            chain: &mut self.chain,
            counters: &mut self.delta,
        };
        select(
            &mut input,
            object.canonical(),
            object.role(),
            advisory,
            &mut self.compression,
        )
    }

    /// Representation selection outcomes of this operation.
    pub fn delta_counters(&self) -> DeltaCounters {
        self.delta
    }

    /// Work spent acquiring delta bases in this operation.
    pub fn chain_counters(&self) -> ChainCounters {
        self.chain
    }

    /// Live bytes held by the bounded admitted-FULL winner cache.
    pub fn candidate_index_bytes(&self) -> usize {
        self.candidates.live_bytes()
    }

    /// Entries currently retained by the dependency-depth cache.
    pub fn depth_cache_entries(&self) -> usize {
        self.depths.len()
    }

    /// Admits one canonical inode leaf through the pooled metadata lane.
    ///
    /// Ordinals are assigned in first-encounter order, new values are grouped and
    /// written as their own packs, and the leaf body is stored in full or as a
    /// COPY/INSERT delta against one acquired base. The catalogue row and the leaf
    /// row share the save's transaction, so a group is visible exactly when the
    /// leaf that refers to it is.
    fn select_pooled(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
    ) -> StorageResult<crate::encoding::EncodedRecord> {
        use layerfs_content::inode_leaf::{pooled_body, INODE_VALUE_BYTES};
        self.sync_pool_index()?;
        let leaf = layerfs_content::inode_leaf::InodeLeaf::decode(object.canonical())?;
        let mut ordinals = Vec::with_capacity(leaf.rows.len());
        let mut fresh: Vec<[u8; INODE_VALUE_BYTES]> = Vec::new();
        for row in &leaf.rows {
            let ordinal = match self.pending_values.get(&row.value) {
                Some(ordinal) => {
                    self.pool.reused_values += 1;
                    *ordinal
                }
                None => {
                    let known = {
                        let mut index = self
                            .pool_index
                            .lock()
                            .map_err(|_| StorageError::Integrity("pool index lock"))?;
                        index.find(
                            &self.connection,
                            &self.capacities,
                            i64::MAX,
                            &mut self.pool_reader,
                            &mut self.decompression,
                            std::slice::from_ref(&row.value),
                        )?
                    };
                    match known.get(&row.value).copied() {
                        Some(ordinal) => {
                            self.pool.reused_values += 1;
                            ordinal
                        }
                        None => {
                            let ordinal = self.assign_ordinal()?;
                            self.pending_values.insert(row.value, ordinal);
                            fresh.push(row.value);
                            self.pool.new_values += 1;
                            ordinal
                        }
                    }
                }
            };
            ordinals.push(ordinal);
        }
        self.write_value_groups(&fresh)?;
        let body = pooled_body(object.canonical(), &ordinals)?;
        self.pool.leaves += 1;
        let full = crate::encoding::pool::leaf::encode_full(&body)?;
        // One base acquisition, then one instruction trial. A missing or
        // ineligible base, or a losing comparison, stores the leaf in full.
        let base = self.pool_base(advisory)?;
        let Some((base_id, base_body)) = base else {
            self.pool.full_leaves += 1;
            return Ok(crate::encoding::EncodedRecord {
                lane: PackLane::Ordinary,
                record: full,
                canonical_length: object.canonical_len(),
                raw_length: body.len(),
                base: None,
            });
        };
        self.pool.trials += 1;
        let mut budget = crate::policy::METADATA_MATCH_BUDGET_BYTES;
        let program = crate::encoding::pool::delta::build(base_id, &base_body, &body, &mut budget)?;
        let Some(program) = program else {
            self.pool.full_leaves += 1;
            return Ok(crate::encoding::EncodedRecord {
                lane: PackLane::Ordinary,
                record: full,
                canonical_length: object.canonical_len(),
                raw_length: body.len(),
                base: None,
            });
        };
        if program.len() >= full.len() {
            self.pool.full_leaves += 1;
            return Ok(crate::encoding::EncodedRecord {
                lane: PackLane::Ordinary,
                record: full,
                canonical_length: object.canonical_len(),
                raw_length: body.len(),
                base: None,
            });
        }
        self.pool.delta_leaves += 1;
        Ok(crate::encoding::EncodedRecord {
            lane: PackLane::Ordinary,
            record: program,
            canonical_length: object.canonical_len(),
            raw_length: body.len(),
            base: Some(base_id),
        })
    }

    /// Synchronizes the Store-owned index with the catalogue once per save.
    fn sync_pool_index(&mut self) -> StorageResult<()> {
        if self.pool_synced {
            return Ok(());
        }
        let mut index = self
            .pool_index
            .lock()
            .map_err(|_| StorageError::Integrity("pool index lock"))?;
        index.sync(
            &self.connection,
            &self.capacities,
            i64::MAX,
            &mut self.pool_reader,
            &mut self.decompression,
        )?;
        self.pool_synced = true;
        Ok(())
    }

    /// Next ordinal this save may assign, read from the catalogue once.
    fn assign_ordinal(&mut self) -> StorageResult<u32> {
        let next = match self.next_ordinal {
            Some(next) => next,
            None => crate::sqlite::pool::next_ordinal(&self.connection)?,
        };
        let assigned = next
            .checked_add(1)
            .ok_or(StorageError::Integrity("metadata ordinal maximum"))?;
        self.next_ordinal = Some(assigned);
        Ok(next)
    }

    /// Builds and places the value groups of every new value of one leaf.
    fn write_value_groups(&mut self, fresh: &[[u8; 73]]) -> StorageResult<()> {
        if fresh.is_empty() {
            return Ok(());
        }
        let first = self
            .next_ordinal
            .ok_or(StorageError::Integrity("metadata ordinal cursor"))?
            .checked_sub(fresh.len() as u32)
            .ok_or(StorageError::Integrity("metadata ordinal cursor"))?;
        let mut built = Vec::new();
        for (number, chunk) in fresh.chunks(crate::policy::VALUES_PER_GROUP).enumerate() {
            let canonical = chunk
                .iter()
                .map(crate::encoding::pool::value_group::canonical_value)
                .collect::<StorageResult<Vec<_>>>()?;
            let group =
                crate::encoding::pool::value_group::build(&canonical, &mut self.compression)?;
            built.push((
                first + (number * crate::policy::VALUES_PER_GROUP) as u32,
                group,
            ));
        }
        let lane = PackLane::PooledMetadata;
        let encoded: Vec<crate::pack::layout::EncodedGroup> =
            built.iter().map(|(_, group)| group.group.clone()).collect();
        let writes =
            self.placement[lane.index()].select_many(lane, encoded, &mut self.next_pack_id)?;
        let mut next_group = 0_usize;
        for write in &writes {
            self.write_pack(write)?;
            for placed in &write.placed {
                let (first_ordinal, group) = built
                    .get(next_group)
                    .ok_or(StorageError::Integrity("placed value group count"))?;
                next_group += 1;
                crate::sqlite::pool::insert_group(
                    &self.connection,
                    &crate::sqlite::pool::ValueGroupRow {
                        first_ordinal: *first_ordinal,
                        count: group.count,
                        pack_id: write.pack_id,
                        group_number: placed.group_number,
                        digest: group.digest,
                    },
                )?;
                self.transaction.rows += 1;
                self.transaction.bytes += group.body.len() as u64;
                self.pool.groups += 1;
            }
        }
        if next_group != built.len() {
            return Err(StorageError::Integrity("value group placement count"));
        }
        // The index learns about ordinals this save assigned without re-reading
        // the catalogue it just wrote to.
        {
            let mut index = self
                .pool_index
                .lock()
                .map_err(|_| StorageError::Integrity("pool index lock"))?;
            let mut cursor = first;
            for (_, group) in &built {
                let values = fresh
                    .iter()
                    .skip((cursor - first) as usize)
                    .take(group.count)
                    .copied()
                    .collect::<Vec<_>>();
                index.note_group(cursor, &values)?;
                cursor += group.count as u32;
            }
        }
        self.maybe_commit()
    }

    /// First acquired pooled-leaf base, with its physical body.
    ///
    /// The base must already be stored: a leaf this same save accepted but has not
    /// sealed is not read for a trial, and an absent or too-deep base selects a
    /// full leaf by policy.
    fn pool_base(&mut self, advisory: &[ObjectId]) -> StorageResult<Option<(ObjectId, Vec<u8>)>> {
        let depth_cap = self.capacities.metadata_delta_max_depth;
        if depth_cap == 0 {
            return Ok(None);
        }
        for id in advisory {
            let Some(location) = lookup::location(&self.connection, *id, i64::MAX)? else {
                continue;
            };
            if location.role != ObjectRole::InodeLeaf {
                continue;
            }
            let Some(depth) = self.depths.depth_of(&self.connection, *id)? else {
                continue;
            };
            if depth >= depth_cap {
                continue;
            }
            let mut reader = crate::encoding::pool::PoolReader::new();
            let body = reader.leaf_body(
                &self.connection,
                &self.capacities,
                i64::MAX,
                &mut self.decompression,
                location,
            )?;
            let depth = self
                .depths
                .depth_of(&self.connection, *id)?
                .ok_or(StorageError::Integrity("pooled base depth"))?;
            let _ = depth;
            return Ok(Some((*id, body)));
        }
        Ok(None)
    }

    /// Pooled lane outcomes of this operation.
    pub fn pool_counters(&self) -> PoolCounters {
        self.pool
    }

    /// Frames and places the lane's pending group, then records its locators.
    pub fn seal_group(
        &mut self,
        lane: PackLane,
        availability: &mut Availability,
    ) -> StorageResult<()> {
        let index = lane.index();
        if self.groups[index].records.is_empty() {
            return Ok(());
        }
        let mut pending = std::mem::take(&mut self.groups[index]);
        let group = match build_group(lane, &pending.records, Some(&mut self.compression)) {
            Ok(group) => group,
            Err(error) => {
                pending.clear();
                return Err(error);
            }
        };
        let writes =
            self.placement[index].select_many(lane, vec![group], &mut self.next_pack_id)?;
        let write = writes
            .first()
            .ok_or(StorageError::Integrity("placement produced no write"))?;
        let placed = write
            .placed
            .first()
            .copied()
            .ok_or(StorageError::Integrity("placement produced no group"))?;
        if placed.records != pending.members.len() {
            return Err(StorageError::Integrity("placed record count"));
        }
        self.write_pack(write)?;
        for (record_number, member) in pending.members.iter().enumerate() {
            if member.base_object_id.is_some() {
                self.counters.prefix_records += 1;
            } else {
                self.counters.full_records += 1;
            }
            write::insert_object(
                &self.connection,
                &ObjectRow {
                    object_id: member.object_id,
                    role: member.role.code(),
                    canonical_length: member.canonical_length,
                    base_object_id: member.base_object_id,
                    pack_id: write.pack_id,
                    group_number: placed.group_number,
                    record_number,
                },
            )?;
            availability.inserted(member.object_id);
            self.counters.inserted += 1;
            self.transaction.rows += 1;
            self.transaction.bytes += member.canonical_length as u64;
        }
        pending.clear();
        self.maybe_commit()
    }

    fn write_pack(&mut self, write: &SelectedWrite) -> StorageResult<()> {
        if write.created {
            write::insert_pack(&self.connection, write.pack_id, &write.bytes)?;
            self.counters.packs_created += 1;
        } else {
            write::append_pack(&self.connection, write.pack_id, &write.bytes)?;
            self.counters.pack_appends += 1;
        }
        self.ceiling = self.ceiling.max(write.pack_id);
        self.transaction.rows += 1;
        self.transaction.bytes += write.bytes.len() as u64;
        Ok(())
    }

    /// Commits and lazily restarts the shared write transaction when it is full.
    pub fn maybe_commit(&mut self) -> StorageResult<()> {
        if self.transaction_open
            && (self.transaction.rows >= self.capacities.transaction_rows
                || self.transaction.bytes >= self.capacities.transaction_bytes)
        {
            write::commit(&self.connection)?;
            // Clear the flag before the re-acquire: if the lock is lost here, no
            // transaction is open and cleanup must proceed to the deletion pass
            // instead of trying to roll back a transaction that does not exist.
            self.transaction_open = false;
            self.counters.commits += 1;
            write::begin_immediate(&self.connection)?;
            self.transaction_open = true;
            self.transaction = TransactionState { rows: 1, bytes: 0 };
            self.counters.transactions += 1;
        }
        Ok(())
    }

    /// Completes every remaining group and acknowledges the final transaction.
    ///
    /// An operation whose last transaction holds no write at all is released with
    /// `ROLLBACK`: no `COMMIT` is issued for an empty write.
    pub fn finish(&mut self) -> StorageResult<OutcomeCounters> {
        if self.terminal {
            return Err(StorageError::Aborted);
        }
        match self.finish_inner() {
            Ok(mut counters) => {
                // Selection and chain counters live beside the resolver while the
                // operation runs; the outcome reports the totals once.
                counters.delta = self.delta;
                counters.chain = self.chain;
                counters.pool = self.pool;
                Ok(counters)
            }
            Err(error) => {
                self.terminal = true;
                Err(error)
            }
        }
    }

    fn finish_inner(&mut self) -> StorageResult<OutcomeCounters> {
        let mut availability = Availability::default();
        for lane in PackLane::ALL {
            self.seal_group(lane, &mut availability)?;
        }
        let pending_rows = self.transaction.rows.saturating_sub(1);
        let has_pending = pending_rows > 0 || self.transaction.bytes > 0;
        // A save that created no pack has nothing to publish. It releases its
        // acquisition with ROLLBACK: no COMMIT is issued for an empty write.
        let publishes = self.ceiling > self.baseline_pack_id;
        if !self.transaction_open {
            return Ok(self.counters);
        }
        if !has_pending && !publishes {
            write::rollback(&self.connection)?;
            self.transaction_open = false;
            return Ok(self.counters);
        }
        if !has_pending {
            // An earlier bounded commit already published this save's last packs.
            // The watermark still needs its own acknowledgement, so the empty
            // acquisition is released and a dedicated final transaction carries
            // the publication. It is the last thing this save does; if it is lost,
            // the packs stay unpublished and the Store is uninspected rather than
            // silently exposed.
            write::rollback(&self.connection)?;
            write::begin_immediate(&self.connection)?;
            self.counters.transactions += 1;
        }
        // The watermark names the packs this save created. It advances only here,
        // so either the save's output and its watermark both become visible, or
        // neither does.
        crate::sqlite::schema::advance_retained_pack_ceiling(&self.connection, self.ceiling)?;
        write::commit(&self.connection)?;
        self.counters.commits += 1;
        self.transaction_open = false;
        Ok(self.counters)
    }

    /// Ends a failed operation with exactly one cleanup attempt.
    pub fn abandon(&mut self) -> StorageResult<()> {
        if self.cleanup_attempted || self.quarantined {
            return Ok(());
        }
        self.cleanup_attempted = true;
        self.terminal = true;
        if self.transaction_open {
            write::rollback(&self.connection)?;
            self.transaction_open = false;
        }
        crate::sqlite::cleanup::abandon(&self.connection, self.baseline_pack_id)?;
        Ok(())
    }

    /// Marks the operation terminal without touching storage.
    ///
    /// A failed save also invalidates the Store-owned pooled index: ordinals it
    /// assigned may never have been committed, so the disposable derivation is
    /// reset and rebuilt from the catalogue by the next save.
    pub fn mark_terminal(&mut self) {
        self.terminal = true;
        if let Ok(mut index) = self.pool_index.lock() {
            index.invalidate();
        }
    }

    /// Marks an unproven outcome: affected writes stop and nothing is deleted.
    pub fn quarantine(&mut self) {
        self.terminal = true;
        self.quarantined = true;
        if let Ok(mut index) = self.pool_index.lock() {
            index.invalidate();
        }
    }
}
