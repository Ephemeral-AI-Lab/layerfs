//! Checked storage policy, format profile and derived capacities.
//!
//! One persisted policy row describes the profile this Store was created with.
//! Open validates the row before any work and refuses conflicting overrides; a
//! Store is never silently migrated, and objects are never re-interpreted against
//! another construction cutoff.

use layerfs_content::{ConstructionPolicy, ContentError, ContentResult};

use crate::error::{StorageError, StorageResult};

/// Persisted profile identifier of this slice.
pub const FORMAT_PROFILE: u8 = 1;

/// SQLite application id written by the candidate schema.
pub const APPLICATION_ID: i64 = 1_279_677_261;
/// SQLite `user_version` written by the candidate schema.
pub const SCHEMA_VERSION: i64 = 1;

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
/// Largest whole-file payload accepted by the profile.
pub const WHOLE_FILE_RAW_LIMIT: usize = 131_071;
/// Largest accepted whole-file codec frame.
pub const WHOLE_FILE_FRAME_LIMIT: usize = 135_168;
/// Largest chunk payload accepted by the frozen CDC profile.
pub const CHUNK_RAW_LIMIT: usize = 32_768;
/// Largest accepted chunk codec frame.
pub const CHUNK_FRAME_LIMIT: usize = 33_024;
/// Canonical envelope overhead of a whole-file object over its raw payload.
pub const OBJECT_ENVELOPE_OVERHEAD: usize = 23;

/// Persisted storage policy: one profile plus the frozen construction values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoragePolicy {
    format_profile: u8,
    small_file_threshold_bytes: u64,
    whole_file_delta_max_depth: u8,
    chunk_delta_max_depth: u8,
}

impl StoragePolicy {
    /// The only policy this slice creates and opens.
    pub const fn frozen_default() -> Self {
        let construction = ConstructionPolicy::frozen_default();
        Self {
            format_profile: FORMAT_PROFILE,
            small_file_threshold_bytes: construction.small_file_threshold_bytes(),
            whole_file_delta_max_depth: construction.whole_file_delta_max_depth(),
            chunk_delta_max_depth: construction.chunk_delta_max_depth(),
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
        }
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
    /// Largest canonical whole-file object.
    pub whole_file_canonical_limit: usize,
    /// Largest accepted whole-file codec frame.
    pub whole_file_frame_limit: usize,
    /// Largest canonical chunk object.
    pub chunk_canonical_limit: usize,
    /// Largest accepted chunk codec frame.
    pub chunk_frame_limit: usize,
    /// Largest assembled ordinary or native pack.
    pub pack_limit: usize,
    /// Largest group payload.
    pub group_limit: usize,
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
        let whole_file_raw = policy.small_file_threshold_bytes() as usize - 1;
        let chunk_raw = CHUNK_RAW_LIMIT;
        Ok(Self {
            whole_file_canonical_limit: whole_file_raw
                .checked_add(23)
                .ok_or(StorageError::Integrity("whole-file capacity"))?,
            whole_file_frame_limit: WHOLE_FILE_FRAME_LIMIT,
            chunk_canonical_limit: chunk_raw
                .checked_add(21)
                .ok_or(StorageError::Integrity("chunk capacity"))?,
            chunk_frame_limit: CHUNK_FRAME_LIMIT,
            pack_limit: PACK_LIMIT,
            group_limit: GROUP_LIMIT,
            batch_objects: BATCH_OBJECT_LIMIT,
            batch_bytes: BATCH_CANONICAL_BYTES_LIMIT,
            transaction_rows: TRANSACTION_ROW_LIMIT,
            transaction_bytes: TRANSACTION_CANONICAL_BYTES_LIMIT,
        })
    }
}

/// Rejects a canonical object larger than the profile allows.
pub fn check_canonical_limit(length: usize) -> ContentResult<()> {
    if length > CANONICAL_LIMIT {
        return Err(ContentError::ObjectLimitExceeded {
            limit: CANONICAL_LIMIT,
            actual: length,
        });
    }
    Ok(())
}
