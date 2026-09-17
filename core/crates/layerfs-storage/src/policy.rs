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
/// Older Stores are rejected rather than migrated.
pub const SCHEMA_VERSION: i64 = 4;

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
/// Largest whole-file payload accepted at the default cutoff.
pub const WHOLE_FILE_RAW_LIMIT: usize = 131_071;
/// Largest accepted whole-file codec frame at the default cutoff.
pub const WHOLE_FILE_FRAME_LIMIT: usize = 135_168;
/// Largest chunk payload accepted by the frozen CDC profile.
pub const CHUNK_RAW_LIMIT: usize = 32_768;
/// Largest accepted chunk codec frame.
pub const CHUNK_FRAME_LIMIT: usize = 33_024;
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
/// Canonical prefix of a pooled metadata leaf: envelope plus node header.
pub const POOLED_LEAF_PREFIX: usize = 44;
/// Canonical bytes of one pooled value row.
pub const POOLED_VALUE_BYTES: usize = 81;
/// Physical bytes of one pooled value row.
pub const POOLED_PHYSICAL_ROW: usize = 12;

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
