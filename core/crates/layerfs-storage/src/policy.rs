//! Checked storage policy, format profile and derived capacities.
//!
//! One persisted policy row describes the profile this Store was created with.
//! Open validates the row before any work and refuses conflicting overrides; a
//! Store is never silently migrated, and objects are never re-interpreted against
//! another construction cutoff. Threshold selection, maximum stored-object
//! validity, chain-work budgets and live-memory budgets are separate: raising the
//! cutoff or a depth never raises the others.

use layerfs_content::{ConstructionPolicy, ContentError};

use crate::error::{StorageError, StorageResult};

/// Persisted profile identifier of this slice.
pub const FORMAT_PROFILE: u8 = 1;

/// SQLite application id written by the candidate schema.
pub const APPLICATION_ID: i64 = 1_279_677_261;
/// SQLite `user_version` written by the candidate schema.
///
/// Version 2 added `store_policy.retained_pack_ceiling`, the publication
/// watermark that keeps an unfinished save's early-committed output invisible to
/// ordinary readers. Version 3 widened the persisted policy ranges to the
/// supported configurable profile. Version 4 adds
/// `store_policy.metadata_delta_max_depth`, the pooled-metadata dependency bound.
/// Version 5 removes `objects.base_object_id`: the direct base identity is a
/// property of the packed record, which already carries it, so the column was a
/// second copy of the same fact. A version-4 Store is rejected rather than
/// migrated, and the reader never guesses a base it cannot read from the record.
/// Version 6 adds `content_signatures`, the persisted cross-save content index
/// (`encoding/delta/candidates.rs`, the W2 section of `sql/schema.sql`).
/// Version 7 landed the save-owned multi-writer model: a `saves` row carries
/// either a private slot or a publication sequence, reads are
/// publication-scoped, and pack allocation advances a Store-wide watermark.
/// Version 8 replaces the fixed two-slot save model with the configured
/// per-Store write admission (#216): `store_policy.max_concurrent_writes` holds
/// the authoritative writer budget (1..=[`MAX_CONCURRENT_WRITES_LIMIT`]) and
/// `saves.active_slot` spans the whole supported slot space instead of 1..=2, so
/// a Store created at a higher setting keeps valid rows when the setting is
/// lowered. A version-7 Store is rejected rather than migrated, exactly as every
/// earlier version is, and no row is rewritten to fit the new column.
/// Version 9 changes the **pack framing** (#219): the group directory moves into
/// a region reserved at the lane's own width, so a body never moves when a group
/// is appended, and the pack declares its own assembled length in its control
/// area, so a pack row may be allocated with spare capacity and written in place
/// through incremental BLOB I/O. Framing versions 1, 2, 4, 6 and 7 are the
/// pre-v9 layout and are refused by the reader; a version-8 Store is refused at
/// open by the check below, before any pack is read, exactly as every earlier
/// version is. No row is rewritten and no pack is migrated.
/// Version 10 removes the `objects_save` secondary index on `objects` (#219): a
/// locator insert maintained two B-trees per row where the reference shape
/// maintains one, and the index's only consumer is one bounded cleanup page query
/// on the definite-failure path (`sqlite/cleanup.rs::abandon`). No column, no
/// constraint and no stored byte changes, so no row is rewritten and no pack is
/// migrated; a version-9 Store is refused at open by the check below, exactly as
/// every earlier version is.
/// Version 11 raises the ordinary, native, whole-file and pooled pack limit from
/// 256 KiB to 1 MiB (#219), so a Store written by this build may carry a pack four
/// times longer than the previous limit. The reader refuses a pack longer than its
/// lane's limit (`pack/layout.rs`), so a version-10 Store is refused at open by the
/// check below rather than failing late on the first long pack it meets, exactly as
/// every earlier version is. No column, no constraint and no stored byte changes,
/// so no row is rewritten and no pack is migrated.
/// Older Stores are rejected rather than migrated.
pub const SCHEMA_VERSION: i64 = 11;

/// Largest writer budget one Store may be configured with.
///
/// It is the width of the private save-slot space: `saves.active_slot` is
/// constrained to `1..=MAX_CONCURRENT_WRITES_LIMIT` by the shipped schema, while
/// the budget actually admitted at any moment is the persisted
/// `store_policy.max_concurrent_writes`, which is at most this value. The two are
/// deliberately different numbers: the slot space has to keep accepting rows a
/// higher earlier setting produced.
pub const MAX_CONCURRENT_WRITES_LIMIT: u8 = 64;
/// Writer budget of a Store that was created without an explicit setting.
///
/// Two is the v0.1.6 behaviour, so an existing operator sees no change until the
/// setting is raised on purpose. It is not a ceiling: any value up to
/// [`MAX_CONCURRENT_WRITES_LIMIT`] is supported.
pub const DEFAULT_MAX_CONCURRENT_WRITES: u8 = 2;
/// Locator rows one ObjectId may own.
///
/// Ownership is per save, and a save inserts its own locator only while no
/// eligible published row exists to reuse; the saves that were simultaneously
/// private at that moment are therefore the only ones that can hold a copy. The
/// supported slot space bounds that number, not the current setting, because
/// rows written under a higher setting remain valid after it is lowered.
pub const SAVE_SLOT_SPACE: usize = MAX_CONCURRENT_WRITES_LIMIT as usize;

/// Declared storage schema identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaIdentity {
    /// SQLite application id.
    pub application_id: i64,
    /// SQLite `user_version`.
    pub user_version: i64,
}

/// The schema identity this crate creates and accepts.
pub const SCHEMA_IDENTITY: SchemaIdentity = SchemaIdentity {
    application_id: APPLICATION_ID,
    user_version: SCHEMA_VERSION,
};

/// Declared bounds shared by placement, transactions and reads.
pub const GROUP_LIMIT: usize = 65_536;
/// Target framed size of one ordinary or native group.
///
/// The owner seals a group once the framed length the *next* record would produce
/// passes this target, so the target is compared against the group's own framing
/// identity (`pack::assemble::framed_group_length`) rather than against a sum of
/// per-record framed lengths, which would count the shared count and end offsets
/// once per record.
pub const GROUP_TARGET: usize = 48 * 1024;
/// Largest assembled pack BLOB for the ordinary, native, whole-file and pooled
/// lanes.
///
/// Every one of those lanes **allocates this much when the pack row is created**
/// and writes into it in place ([`crate::pack::layout::pack_capacity`]): a BLOB's
/// size cannot be changed in place, and growing it would be an `UPDATE` that
/// rewrites the whole row. The limit is therefore also the reserved capacity, and
/// the bytes a pack reserves but never fills are its *tail*, about half a group
/// per pack. Raising the limit is how that tail is paid for once instead of once
/// per pack: four times the pack holds four times the groups, so the same content
/// needs about a quarter of the packs and about a quarter of the dead tail. This
/// value is the first increment measured for that reason (#219 round 20,
/// 256 KiB -> 1 MiB, with `GROUP_LIMIT`, `GROUP_TARGET` and every framing byte
/// unchanged); it is not the largest pack the format admits, which is
/// [`GROUP_COUNT_LIMIT`] groups of [`GROUP_TARGET`] bytes.
pub const PACK_LIMIT: usize = 1024 * 1024;
/// Largest group count in one pack.
pub const GROUP_COUNT_LIMIT: usize = 256;
/// Largest record count in one group.
pub const RECORD_COUNT_LIMIT: usize = 8_191;
/// Largest canonical object accepted by the shared profile.
pub const CANONICAL_LIMIT: usize = 16 * 1024 * 1024;
/// Identifiers in one membership or locator page.
pub const LOOKUP_PAGE_IDS: usize = 128;
/// Objects held by one pending batch.
pub const BATCH_OBJECT_LIMIT: usize = 512;
/// Canonical bytes held by one pending batch, and therefore by one wave.
///
/// **The wave is bounded by the transaction it runs in.** A preparation wave
/// holds the Store's arbitration for its whole duration, every seal inside it
/// joins the one transaction the wave opened, and that transaction is
/// acknowledged at the wave's end - so the transaction's own declared capacity is
/// the bound that describes a wave, and this is that capacity rather than a
/// second, smaller figure with no stated relation to it.
///
/// The figure it replaces was 512 KiB, eight times below the transaction: the
/// row that motivated this bound wrote 302,406,480 canonical bytes in ~596 waves
/// of ~507 KiB, while every wave opened a transaction declared to accept 4 MiB.
/// The wave count is what the row paid for - one `BEGIN IMMEDIATE`, one locator
/// query, one presence seed, one collision check and one `COMMIT` per wave - and
/// none of those costs is proportional to the bytes the wave carries.
pub const WAVE_CANONICAL_BYTES_LIMIT: u64 = TRANSACTION_CANONICAL_BYTES_LIMIT;
/// Submitted rows in one open transaction.
pub const TRANSACTION_ROW_LIMIT: u64 = 8_191;
/// Canonical bytes submitted in one open transaction.
pub const TRANSACTION_CANONICAL_BYTES_LIMIT: u64 = 4 * 1024 * 1024 - 1;
/// Canonical bytes one group may hold before its lane seals it.
///
/// The group's own bound, and deliberately **not** the wave's. A group is a
/// framing unit inside one pack: its canonical bytes are what the owner projects
/// before deciding to seal (`cas::selection`), so the bound has to describe a
/// frame rather than a transaction. The two were the same constant, which is why
/// widening the wave's budget would otherwise have widened a frame's.
pub const GROUP_CANONICAL_BYTES_LIMIT: u64 = 512 * 1024;
/// Rows removed by one bounded cleanup page.
pub const CLEANUP_PAGE_ROWS: usize = 128;
/// Objects one authenticated read wave may demand.
///
/// One wave is one grouped SQL lookup plus one shared decode workspace, so the
/// reference's lookup page (128) is the wrong figure to copy here: a tree walk
/// demands a page's children at once. This is the declared ceiling for that
/// demand, and the C1 provider inherits it.
pub const READ_OBJECT_LIMIT: usize = 4_096;
/// Total returned canonical bytes in one read wave, counting repeated demands.
pub const READ_CANONICAL_BYTES_LIMIT: usize = 32 * 1024 * 1024;
/// Canonical bytes a whole-file object adds over its raw payload: the 13-byte
/// bytes-role envelope and the 10-byte whole-file value header. The chunk lane's
/// equivalent is 21 bytes, because a chunk value carries only its eight-byte
/// magic, and the frozen codec profile holds both limits.
pub const WHOLE_FILE_CANONICAL_OVERHEAD: usize = 23;

/// Framing bytes a singleton pack adds around its single record.
pub const SINGLETON_FRAMING_SLACK: usize = 4_096;
/// Largest assembled singleton pack: one maximum canonical record plus framing.
pub const SINGLETON_PACK_LIMIT: usize = CANONICAL_LIMIT + SINGLETON_FRAMING_SLACK;

/// Values in one pooled physical metadata group.
pub const VALUES_PER_GROUP: usize = 165;
/// Largest accepted pooled metadata group body.
pub const METADATA_GROUP_LIMIT: usize = 16 * 1024;
/// Rows in one pooled metadata leaf.
pub const POOLED_LEAF_ROWS_LIMIT: usize = 100;

/// Chain-work budgets. These are separate from the cutoff and the depths: a
/// deeper chain is still bounded by the bytes it may decode and encode.
/// Canonical bytes one dependency chain may reconstruct.
pub const CHAIN_CANONICAL_LIMIT: u64 = 512 * 1024;
/// Encoded bytes one dependency chain may read.
pub const CHAIN_ENCODED_LIMIT: u64 = 256 * 1024;
/// Pack bodies one operation's dependency cache may retain.
///
/// Owner: the save operation that fills it. Bound: this many bytes of pack bodies.
/// Live multiplicity: one cache per save, one copy per distinct pack. Lifetime:
/// the operation. Release: dropped with the operation, and released wholesale when
/// the next body would cross the bound - exactly the discipline the pooled value
/// cache and the index window use. A released body is read again if a later
/// dependency needs it, so the bound costs reads and never correctness.
pub const DEPENDENCY_PACK_CACHE_BYTES: usize = 4 * 1024 * 1024;
/// Decoded values one pooled reader may retain across one wave.
///
/// Owner: one pooled reader. Bound: this many canonical decoded value bytes. Live
/// multiplicity: one cache per reader, one copy per distinct value group. Lifetime:
/// the wave. Release: the whole cache is dropped when the next group would cross
/// the bound, or with the reader. Dropping it costs reads and the index window
/// still finds the group, so the bound costs work and never correctness.
pub const POOLED_VALUE_CACHE_BYTES: usize = 512 * 1024;
/// Decoded ordinary-lane group bodies one read wave may retain.
///
/// Owner: the read wave that decodes through it - one cache per wave, created by
/// the caller that owns the wave's pack cache and carried by every resolver in that
/// wave. Bound: this many decoded body bytes. Live multiplicity: one copy per
/// distinct `(pack, group)`. Lifetime: the wave; dropped with it. Release: the
/// whole cache is released when the next body would cross the bound, the
/// discipline the pooled value cache and the dependency pack cache already use. A
/// released body is decompressed again if a later record needs it, so the bound
/// costs work and never correctness.
///
/// A group body is immutable once published - a later append reaches a pack's
/// *new* groups and never rewrites an existing group's bytes - so a retained body
/// cannot go stale within a wave. Visibility is checked before the cache is
/// consulted, never after (see `encoding::delta::read::Resolver::decode_at`).
pub const DECODED_GROUP_CACHE_BYTES: usize = 512 * 1024;
/// Decoded value-group work one pooled metadata chain may spend.
pub const METADATA_DECODED_WORK_LIMIT: u64 = 32 * 1024 * 1024;
/// Default pooled-metadata dependency depth.
pub const DEFAULT_METADATA_DELTA_MAX_DEPTH: u8 = 8;
/// Canonical bytes one pooled-metadata chain may reconstruct.
pub const METADATA_CHAIN_CANONICAL_LIMIT: u64 = 8 * 8_192;
/// Encoded bytes one pooled-metadata chain may read.
pub const METADATA_CHAIN_ENCODED_LIMIT: u64 = 17 * 8_193;
/// Largest physical pooled leaf record.
pub const METADATA_RECORD_LIMIT: usize = 8_192;
/// Match-comparison budget of one pooled delta trial, in bytes.
pub const METADATA_MATCH_BUDGET_BYTES: usize = 128 * 1024;
/// Largest canonical inode leaf object.
pub const INODE_LEAF_LIMIT: usize = 8_192;
/// Retained entries of the bounded metadata value index.
///
/// The index's live-byte bound is this entry count times the per-entry charge that
/// `PoolIndex::live_bytes` reports through `Store::pool_index_bytes`; there is no
/// separate byte ceiling. A 32 MiB constant with no reader used to sit beside it
/// and overstated the real bound by an order of magnitude.
pub const METADATA_INDEX_VALUES: usize = 131_072;
/// Smallest block of pooled ordinals one reservation takes.
///
/// A reservation is a step: it is acknowledged before any value uses it, so an
/// aborted save never hands the same ordinal out again (`cas::pool_lane`). That
/// guarantee is about *when* the statement is acknowledged, not about how many
/// ordinals it covers, so this is the granularity of the step and not its
/// existence. One reservation per leaf closes the wave's transaction once per
/// leaf - measured at **~207 reservations and ~210 of #219's 285 COMMITs** - and
/// every closed transaction rewrites the pages dirtied in it again, because the
/// connection's journal is in memory and a committed page stays in the pager
/// cache. The block is the *smallest* number of ordinals a reservation takes, so
/// a leaf that needs more still gets exactly what it needs; the cost is that up
/// to `ORDINAL_RESERVE_BLOCK - 1` ordinals are burnt when a save aborts, out of a
/// 32-bit space, and that the retained window's value count over-claims by the
/// same bound.
pub const ORDINAL_RESERVE_BLOCK: usize = 1024;
/// Leaves one blocked reservation covers, as a multiple of the demand observed.
///
/// The block is `fresh_values_of_this_leaf * ORDINAL_BLOCK_LEAVES`, not a fixed
/// count: a workload whose leaves carry a steady number of fresh values consumes
/// its block exactly, so the ordinals the catalogue hands out stay as dense as
/// they were, and one whose leaves vary wastes at most `ORDINAL_BLOCK_LEAVES - 1`
/// leaves' worth per block. A fixed block was measured instead and rejected: on a
/// 1,312-leaf fixture with 100 values per leaf it handed out 134,645 ordinals for
/// 131,200 values and pulled the retained window past its bound three leaves
/// early - both of which the block's own size caused and this one does not.
pub const ORDINAL_BLOCK_LEAVES: usize = 16;
/// Exact reservations a save makes before it starts taking blocks.
///
/// The ordinals are part of a pooled leaf body, so a save's *first* reservations
/// are exact and the ordinal sequence a small workload sees is unchanged. A save
/// that has reserved this many times is one that will reserve many more, which is
/// where the block pays: `cas::pool_lane` measured a one-value leaf's delta
/// program at 50 bytes with contiguous ordinals and 57 - a tie with its FULL
/// alternative, and a tie stores FULL - with the next block's first ordinal.
pub const ORDINAL_RESERVE_AFTER: usize = 4;
/// Persisted storage policy: one profile plus the configurable construction values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoragePolicy {
    format_profile: u8,
    small_file_threshold_bytes: u64,
    whole_file_delta_max_depth: u8,
    chunk_delta_max_depth: u8,
    metadata_delta_max_depth: u8,
}

impl StoragePolicy {
    /// The default policy this slice creates and opens.
    pub const fn frozen_default() -> Self {
        let construction = ConstructionPolicy::frozen_default();
        Self {
            format_profile: FORMAT_PROFILE,
            small_file_threshold_bytes: construction.small_file_threshold_bytes(),
            whole_file_delta_max_depth: construction.whole_file_delta_max_depth(),
            chunk_delta_max_depth: construction.chunk_delta_max_depth(),
            metadata_delta_max_depth: DEFAULT_METADATA_DELTA_MAX_DEPTH,
        }
    }

    /// Builds a candidate; call [`Self::validated`] before persisting it.
    pub const fn new(
        format_profile: u8,
        small_file_threshold_bytes: u64,
        whole_file_delta_max_depth: u8,
        chunk_delta_max_depth: u8,
    ) -> Self {
        Self {
            format_profile,
            small_file_threshold_bytes,
            whole_file_delta_max_depth,
            chunk_delta_max_depth,
            metadata_delta_max_depth: DEFAULT_METADATA_DELTA_MAX_DEPTH,
        }
    }

    /// Builds a candidate with an explicit pooled-metadata depth.
    pub const fn with_metadata_depth(mut self, metadata_delta_max_depth: u8) -> Self {
        self.metadata_delta_max_depth = metadata_delta_max_depth;
        self
    }

    /// Rejects any profile or value this slice does not implement.
    pub fn validated(self) -> StorageResult<Self> {
        if self.format_profile != FORMAT_PROFILE {
            return Err(StorageError::UnsupportedPolicy {
                field: "format_profile",
            });
        }
        ConstructionPolicy::new(
            self.small_file_threshold_bytes,
            self.whole_file_delta_max_depth,
            self.chunk_delta_max_depth,
        )
        .validated()
        .map_err(|error| match error {
            ContentError::UnsupportedPolicy { field } => StorageError::UnsupportedPolicy { field },
            other => StorageError::Content(other),
        })?;
        if self.metadata_delta_max_depth > layerfs_content::MAXIMUM_DELTA_MAX_DEPTH {
            return Err(StorageError::UnsupportedPolicy {
                field: "metadata_delta_max_depth",
            });
        }
        Ok(self)
    }

    /// Persisted profile identifier.
    pub const fn format_profile(self) -> u8 {
        self.format_profile
    }

    /// Persisted construction cutoff.
    pub const fn small_file_threshold_bytes(self) -> u64 {
        self.small_file_threshold_bytes
    }

    /// Persisted whole-file dependency bound.
    pub const fn whole_file_delta_max_depth(self) -> u8 {
        self.whole_file_delta_max_depth
    }

    /// Persisted chunk dependency bound.
    pub const fn chunk_delta_max_depth(self) -> u8 {
        self.chunk_delta_max_depth
    }

    /// Persisted pooled-metadata dependency bound.
    pub const fn metadata_delta_max_depth(self) -> u8 {
        self.metadata_delta_max_depth
    }

    /// Construction policy C1 must use with this Store.
    pub const fn construction(self) -> ConstructionPolicy {
        ConstructionPolicy::new(
            self.small_file_threshold_bytes,
            self.whole_file_delta_max_depth,
            self.chunk_delta_max_depth,
        )
    }
}

impl Default for StoragePolicy {
    fn default() -> Self {
        Self::frozen_default()
    }
}

/// Capacities derived from the policy by checked arithmetic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCapacities {
    /// Construction cutoff the profile selects representations with.
    pub small_file_threshold_bytes: u64,
    /// Largest canonical whole-file object under this cutoff.
    pub whole_file_canonical_limit: usize,
    /// Largest accepted whole-file codec frame under this cutoff.
    pub whole_file_frame_limit: usize,
    /// Whole-file codec window log.
    pub whole_file_window_log: i32,
    /// Largest assembled ordinary, native, compact or pooled pack.
    pub pack_limit: usize,
    /// Largest assembled singleton pack.
    pub singleton_pack_limit: usize,
    /// Largest group payload.
    pub group_limit: usize,
    /// Largest pooled metadata group body.
    pub metadata_group_limit: usize,
    /// Whole-file dependency bound.
    pub whole_file_delta_max_depth: u8,
    /// Chunk dependency bound.
    pub chunk_delta_max_depth: u8,
    /// Pooled-metadata dependency bound.
    pub metadata_delta_max_depth: u8,
    /// Canonical bytes one pooled-metadata chain may reconstruct.
    pub metadata_chain_canonical_limit: u64,
    /// Encoded bytes one pooled-metadata chain may read.
    pub metadata_chain_encoded_limit: u64,
    /// Canonical bytes one dependency chain may reconstruct.
    pub chain_canonical_limit: u64,
    /// Encoded bytes one dependency chain may read.
    pub chain_encoded_limit: u64,
    /// Objects held by one pending batch.
    pub batch_objects: usize,
    /// Canonical bytes held by one pending batch.
    pub batch_bytes: u64,
    /// Submitted rows per open transaction.
    pub transaction_rows: u64,
    /// Canonical bytes per open transaction.
    pub transaction_bytes: u64,
    /// Objects one authenticated read wave may demand.
    ///
    /// A read wave is one grouped query with one decode workspace, so its size is
    /// a declared resource rather than an unbounded caller choice. The same
    /// figure bounds a save's own in-transaction read.
    pub read_objects: usize,
}

impl StorageCapacities {
    /// Derives the capacities of an accepted policy.
    pub fn from_policy(policy: StoragePolicy) -> StorageResult<Self> {
        let policy = policy.validated()?;
        let construction = policy.construction().capacities();
        Ok(Self {
            small_file_threshold_bytes: policy.small_file_threshold_bytes(),
            whole_file_canonical_limit: construction.whole_file_canonical_limit,
            whole_file_frame_limit: construction.whole_file_frame_limit,
            whole_file_window_log: policy.construction().whole_file_window_log(),
            pack_limit: PACK_LIMIT,
            singleton_pack_limit: SINGLETON_PACK_LIMIT,
            group_limit: GROUP_LIMIT,
            metadata_group_limit: METADATA_GROUP_LIMIT,
            whole_file_delta_max_depth: policy.whole_file_delta_max_depth(),
            chunk_delta_max_depth: policy.chunk_delta_max_depth(),
            metadata_delta_max_depth: policy.metadata_delta_max_depth(),
            metadata_chain_canonical_limit: METADATA_CHAIN_CANONICAL_LIMIT,
            metadata_chain_encoded_limit: METADATA_CHAIN_ENCODED_LIMIT,
            chain_canonical_limit: CHAIN_CANONICAL_LIMIT,
            chain_encoded_limit: CHAIN_ENCODED_LIMIT,
            batch_objects: BATCH_OBJECT_LIMIT,
            batch_bytes: WAVE_CANONICAL_BYTES_LIMIT,
            transaction_rows: TRANSACTION_ROW_LIMIT,
            transaction_bytes: TRANSACTION_CANONICAL_BYTES_LIMIT,
            read_objects: READ_OBJECT_LIMIT,
        })
    }

    /// Dependency bound for one role's prospective delta selection.
    ///
    /// Every payload role uses its own configured bound, and a pooled metadata
    /// leaf uses the pooled bound it is persisted with: mapping a role to another
    /// role's depth would silently select against the wrong policy.
    pub const fn delta_depth_for_role(self, role: layerfs_content::ObjectRole) -> u8 {
        match role {
            layerfs_content::ObjectRole::Chunk => self.chunk_delta_max_depth,
            layerfs_content::ObjectRole::InodeLeaf => self.metadata_delta_max_depth,
            _ => self.whole_file_delta_max_depth,
        }
    }
}
