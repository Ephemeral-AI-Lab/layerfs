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
/// Older Stores are rejected rather than migrated.
pub const SCHEMA_VERSION: i64 = 9;

/// Page size of a Store this build creates.
///
/// **The engine's page is the unit a write is charged in.** With the MEMORY
/// journal, rollback atomicity costs one image copy per modified page, and the
/// commit hands every dirty page to the operating system one `pwrite` at a time,
/// so a Store's write cost is bounded below by the pages it dirties rather than
/// by the bytes it stores. The row that motivated this constant wrote
/// 302,406,480 bytes of pack bodies and left 82,129 dirty pages behind at the
/// engine's 4096-byte default; the same bytes at 65536 are about 5,200 pages.
///
/// Measured with no product code at all on the row's own write shape (800
/// transactions, 16,800 blob appends of 18,000 bytes through `blobopen`, 24,800
/// `WITHOUT ROWID` row inserts, the product's pragma profile), same bytes at four
/// page sizes — pages, then the summed `COMMIT`, then microseconds per page:
/// 4096 gives 77,420 pages, 482.48 ms, 6.23 us; 16384 gives 19,470 pages,
/// 167.31 ms, 8.59 us; 65536 gives 5,008 pages, 122.05 ms, 24.37 us. 65536 is
/// chosen because the cost stops being per-page there: 24.37 us for 64 KiB is
/// `memcpy` speed, the floor, while the two narrower widths pay 3.95x and 2.88x
/// more for the same bytes.
///
/// **This is a Store-creation decision and it is not a format version.** The
/// value lives in the database file's own header, so a reader takes it from the
/// file it opened: a Store created at any other page size — including every Store
/// created before this constant existed — opens, validates, writes and reads
/// unchanged, and no product path compares an opened Store's page size to this
/// number. See `crate::sqlite::schema::create`, the only writer of the pragma.
pub const STORE_PAGE_SIZE_BYTES: usize = 65_536;

/// Pages the connection's page cache is declared to hold.
///
/// **The page cache is a count of pages, and SQLite's default is a count of
/// bytes.** The engine's own default is `cache_size = -2000` KiB, which is 512
/// pages only because the engine also assumes a 4096-byte page; a Store created
/// with a wider page silently kept 2 MiB and therefore a sixteenth of the pages.
/// Measured on this row's own shape (800 transactions, 24,800 random-key row
/// inserts, 16,800 blob appends) with no product code: at 65536-byte pages a
/// 2 MiB cache costs 6.63 us per row insert and 66.38 ms of signature writes,
/// while 8 MiB or 32 MiB costs 4.48-4.54 us and 45.5 ms, against 4.20 us and
/// 16.62 ms for the 4096-byte default. The statement seeks a random leaf, so an
/// uncached page is read, journaled and modified whole - sixteen times the bytes
/// for the same row.
///
/// 512 is not a tuned value: it is SQLite's own default restated in the unit the
/// engine charges in, so a 4096-byte Store keeps the 2 MiB it always had and a
/// wider Store keeps the pages it needs. `connection::apply_cache_size` derives
/// the pragma from the page size the *file* reports, so no Store is starved and
/// no format is implied: a page cache is per connection and is never stored.
pub const STORE_CACHE_PAGES: i64 = 512;

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
/// Largest assembled pack BLOB for the ordinary and native lanes.
pub const PACK_LIMIT: usize = 256 * 1024;
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
/// Canonical bytes held by one pending batch.
pub const BATCH_CANONICAL_BYTES_LIMIT: u64 = 512 * 1024;
/// Submitted rows in one open transaction.
pub const TRANSACTION_ROW_LIMIT: u64 = 8_191;
/// Canonical bytes submitted in one open transaction.
pub const TRANSACTION_CANONICAL_BYTES_LIMIT: u64 = 4 * 1024 * 1024 - 1;
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
            batch_bytes: BATCH_CANONICAL_BYTES_LIMIT,
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
