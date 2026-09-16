//! Checked construction profile, capacities and the complete-file selector.
//!
//! The profile is genuinely configurable inside explicitly supported ranges: the
//! construction cutoff accepts any power of two from 128 KiB to 1 MiB, and each
//! role's dependency depth accepts `0` (prospective delta disabled) up to the
//! documented maximum. Everything else is rejected before any work starts, so a
//! Store never records a policy the implementation cannot honor. Capacities are
//! derived from the accepted profile with checked arithmetic; raising one field
//! never raises another, and the chain-work and live-memory budgets stay fixed.

use crate::error::{ContentError, ContentResult};

/// Default exclusive construction cutoff: files at or above this length are chunked.
pub const DEFAULT_SMALL_FILE_THRESHOLD_BYTES: u64 = 131_072;
/// Smallest accepted construction cutoff.
pub const MINIMUM_SMALL_FILE_THRESHOLD_BYTES: u64 = 131_072;
/// Largest accepted construction cutoff.
pub const MAXIMUM_SMALL_FILE_THRESHOLD_BYTES: u64 = 1_048_576;
/// Default whole-file dependency bound.
pub const DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH: u8 = 8;
/// Default chunk dependency bound.
pub const DEFAULT_CHUNK_DELTA_MAX_DEPTH: u8 = 4;
/// Largest accepted dependency depth for either role.
pub const MAXIMUM_DELTA_MAX_DEPTH: u8 = 50;

/// Smallest nonempty file that is stored as a whole-file object.
pub const MINIMUM_WHOLE_FILE_BYTES: u64 = 1;

/// Frozen whole-file codec window log below the 256 KiB cutoff.
const WHOLE_FILE_WINDOW_LOG_SMALL: i32 = 18;
/// Whole-file codec window log from the 256 KiB cutoff upwards.
const WHOLE_FILE_WINDOW_LOG_LARGE: i32 = 20;
/// Largest accepted whole-file codec frame at the default cutoff.
pub const DEFAULT_WHOLE_FILE_FRAME_LIMIT: usize = 135_168;

/// Conservative upper bound of one uncompressed-size Zstandard frame.
///
/// The bound is the codec's own `compressBound` shape (`raw + raw / 128 + 1024`)
/// rounded up; the default cutoff keeps its frozen, tighter constant.
pub const fn conservative_frame_bound(raw: usize) -> usize {
    raw + raw / 128 + 1024
}

/// Immutable construction policy: one cutoff and two role-specific depth bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructionPolicy {
    small_file_threshold_bytes: u64,
    whole_file_delta_max_depth: u8,
    chunk_delta_max_depth: u8,
}

impl ConstructionPolicy {
    /// The default accepted policy.
    pub const fn frozen_default() -> Self {
        Self {
            small_file_threshold_bytes: DEFAULT_SMALL_FILE_THRESHOLD_BYTES,
            whole_file_delta_max_depth: DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH,
            chunk_delta_max_depth: DEFAULT_CHUNK_DELTA_MAX_DEPTH,
        }
    }

    /// Builds a policy candidate; call [`Self::validated`] before use.
    pub const fn new(
        small_file_threshold_bytes: u64,
        whole_file_delta_max_depth: u8,
        chunk_delta_max_depth: u8,
    ) -> Self {
        Self {
            small_file_threshold_bytes,
            whole_file_delta_max_depth,
            chunk_delta_max_depth,
        }
    }

    /// Accepts the policy only when every field is inside a supported range.
    ///
    /// The cutoff must be a power of two inside
    /// [`MINIMUM_SMALL_FILE_THRESHOLD_BYTES`]..=[`MAXIMUM_SMALL_FILE_THRESHOLD_BYTES`]:
    /// a larger whole-file object needs a capacity-aware pack grammar and a
    /// matching reader, and a non-power-of-two cutoff would make the transition
    /// probe ambiguous. Each depth accepts `0` (that role's prospective delta is
    /// disabled) up to [`MAXIMUM_DELTA_MAX_DEPTH`]. A depth never widens the
    /// chain-work or live-memory budgets, which are separate fixed capacities.
    pub const fn validated(self) -> ContentResult<Self> {
        let threshold = self.small_file_threshold_bytes;
        if !threshold.is_power_of_two()
            || threshold < MINIMUM_SMALL_FILE_THRESHOLD_BYTES
            || threshold > MAXIMUM_SMALL_FILE_THRESHOLD_BYTES
        {
            return Err(ContentError::UnsupportedPolicy {
                field: "small_file_threshold_bytes",
            });
        }
        if self.whole_file_delta_max_depth > MAXIMUM_DELTA_MAX_DEPTH {
            return Err(ContentError::UnsupportedPolicy {
                field: "whole_file_delta_max_depth",
            });
        }
        if self.chunk_delta_max_depth > MAXIMUM_DELTA_MAX_DEPTH {
            return Err(ContentError::UnsupportedPolicy {
                field: "chunk_delta_max_depth",
            });
        }
        Ok(self)
    }

    /// Exclusive construction cutoff in bytes.
    pub const fn small_file_threshold_bytes(self) -> u64 {
        self.small_file_threshold_bytes
    }

    /// Whole-file dependency bound; `0` disables whole-file delta selection.
    pub const fn whole_file_delta_max_depth(self) -> u8 {
        self.whole_file_delta_max_depth
    }

    /// Chunk dependency bound; `0` disables chunk delta selection.
    pub const fn chunk_delta_max_depth(self) -> u8 {
        self.chunk_delta_max_depth
    }

    /// Complete-file representation this policy selects for `logical_len`.
    pub const fn representation(self, logical_len: u64) -> Representation {
        if logical_len == 0 {
            Representation::Empty
        } else if logical_len < self.small_file_threshold_bytes {
            Representation::WholeFile
        } else {
            Representation::Chunked
        }
    }

    /// Capacities derived from this policy by checked arithmetic.
    pub const fn capacities(self) -> ConstructionCapacities {
        let raw = self.small_file_threshold_bytes as usize - 1;
        let bound = conservative_frame_bound(raw);
        ConstructionCapacities {
            whole_file_raw_limit: raw,
            whole_file_frame_limit: if bound > DEFAULT_WHOLE_FILE_FRAME_LIMIT {
                bound
            } else {
                DEFAULT_WHOLE_FILE_FRAME_LIMIT
            },
            whole_file_canonical_limit: raw + OBJECT_ENVELOPE_BYTES,
            chunk_raw_limit: crate::file::cdc::MAXIMUM_CHUNK_BYTES,
            chunk_minimum_raw: crate::file::cdc::MINIMUM_CHUNK_BYTES,
            mapping_node_limit: 8_192,
            canonical_object_limit: MAX_CANONICAL_OBJECT_BYTES,
            stream_flush_entries: MAX_MAPPING_ENTRIES + STREAM_FLUSH_HEADROOM,
            whole_file_delta_max_depth: self.whole_file_delta_max_depth,
            chunk_delta_max_depth: self.chunk_delta_max_depth,
        }
    }

    /// Whole-file codec window log this cutoff needs.
    pub const fn whole_file_window_log(self) -> i32 {
        if self.small_file_threshold_bytes <= 2 * DEFAULT_SMALL_FILE_THRESHOLD_BYTES {
            WHOLE_FILE_WINDOW_LOG_SMALL
        } else {
            WHOLE_FILE_WINDOW_LOG_LARGE
        }
    }
}

impl Default for ConstructionPolicy {
    fn default() -> Self {
        Self::frozen_default()
    }
}

/// Complete-file representation selected from the logical length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Representation {
    /// Zero-length file: the defined empty file-state form.
    Empty,
    /// `0 < length < cutoff`: one whole-file canonical object.
    WholeFile,
    /// `length >= cutoff`: file state over a chunked extent tree.
    Chunked,
}

/// Canonical object envelope overhead of a bytes-role value.
pub const OBJECT_ENVELOPE_BYTES: usize = 23;

/// Largest canonical object accepted by this profile (16 MiB).
///
/// This is the role-independent envelope ceiling. The envelope decoder cannot
/// know an object's role - the role tag lives inside the value - so it needs one
/// bound that no role could legitimately need to exceed.
pub const MAX_CANONICAL_OBJECT_BYTES: usize = 16 * 1024 * 1024;

/// Largest single value a canonical bytes-role object may carry (8 MiB).
///
/// The frozen format bounds a field *and* the envelope separately; the envelope
/// ceiling sits above this one so a maximum-size field still fits its framing.
pub const MAX_OBJECT_FIELD_BYTES: usize = 8 * 1024 * 1024;

/// Largest number of entries a non-root mapping page may hold.
pub const MAX_MAPPING_ENTRIES: usize = 128;

const STREAM_FLUSH_HEADROOM: usize = 64;

/// Derived, immutable capacities charged before allocations happen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructionCapacities {
    /// Largest whole-file payload: cutoff minus one.
    pub whole_file_raw_limit: usize,
    /// Largest accepted whole-file codec frame for this cutoff.
    pub whole_file_frame_limit: usize,
    /// Largest canonical whole-file object including its envelope.
    pub whole_file_canonical_limit: usize,
    /// Largest chunk payload accepted by the frozen CDC profile.
    pub chunk_raw_limit: usize,
    /// Smallest chunk payload accepted by the frozen CDC profile.
    pub chunk_minimum_raw: usize,
    /// Largest canonical mapping node object.
    pub mapping_node_limit: usize,
    /// Largest canonical object of any role.
    pub canonical_object_limit: usize,
    /// Entry count that triggers a streaming flush of a full mapping page.
    pub stream_flush_entries: usize,
    /// Recorded whole-file dependency bound.
    pub whole_file_delta_max_depth: u8,
    /// Recorded chunk dependency bound.
    pub chunk_delta_max_depth: u8,
}
