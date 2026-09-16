//! Checked construction profile, capacities and the complete-file selector.
//!
//! The frozen profile of this slice accepts exactly one construction cutoff and
//! the two frozen depth defaults. Every other value is rejected before any work
//! starts, so a Store never records a policy that the implementation cannot
//! honor. Capacities are derived from that profile with checked arithmetic; they
//! are not independently configurable knobs.

use crate::error::{ContentError, ContentResult};

/// Frozen exclusive construction cutoff: files at or above this length are chunked.
pub const DEFAULT_SMALL_FILE_THRESHOLD_BYTES: u64 = 131_072;
/// Frozen whole-file dependency bound; DELTA selection is not implemented in this slice.
pub const DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH: u8 = 8;
/// Frozen chunk dependency bound; DELTA selection is not implemented in this slice.
pub const DEFAULT_CHUNK_DELTA_MAX_DEPTH: u8 = 4;

/// Smallest nonempty file that is stored as a whole-file object.
pub const MINIMUM_WHOLE_FILE_BYTES: u64 = 1;

/// Immutable construction policy: one cutoff and two role-specific depth bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructionPolicy {
    small_file_threshold_bytes: u64,
    whole_file_delta_max_depth: u8,
    chunk_delta_max_depth: u8,
}

impl ConstructionPolicy {
    /// The only accepted policy of this slice.
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

    /// Accepts the policy only when every field is implemented by this slice.
    ///
    /// The accepted cutoff range is the single frozen value: larger cutoffs need
    /// a capacity-aware pack/record contract and a matching reader, which this
    /// slice does not ship. Delta depths are recorded defaults; the range is
    /// validated so an unsupported profile is never persisted.
    pub const fn validated(self) -> ContentResult<Self> {
        if self.small_file_threshold_bytes != DEFAULT_SMALL_FILE_THRESHOLD_BYTES {
            return Err(ContentError::UnsupportedPolicy {
                field: "small_file_threshold_bytes",
            });
        }
        if self.whole_file_delta_max_depth != DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH {
            return Err(ContentError::UnsupportedPolicy {
                field: "whole_file_delta_max_depth",
            });
        }
        if self.chunk_delta_max_depth != DEFAULT_CHUNK_DELTA_MAX_DEPTH {
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

    /// Recorded whole-file dependency bound.
    pub const fn whole_file_delta_max_depth(self) -> u8 {
        self.whole_file_delta_max_depth
    }

    /// Recorded chunk dependency bound.
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
        ConstructionCapacities {
            whole_file_raw_limit: self.small_file_threshold_bytes as usize - 1,
            whole_file_frame_limit: 135_168,
            whole_file_canonical_limit: self.small_file_threshold_bytes as usize + 23,
            chunk_raw_limit: crate::file::cdc::MAXIMUM_CHUNK_BYTES,
            chunk_minimum_raw: crate::file::cdc::MINIMUM_CHUNK_BYTES,
            mapping_node_limit: 8_192,
            canonical_object_limit: MAX_CANONICAL_OBJECT_BYTES,
            stream_flush_entries: MAX_MAPPING_ENTRIES + STREAM_FLUSH_HEADROOM,
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
    /// Largest accepted whole-file codec frame.
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
}
