//! Frozen content-defined chunking.
//!
//! Entry module: declarations and re-exports only.

mod gear;
mod zeros;

pub(crate) use gear::Scanner;

pub use gear::{
    profile_id, CdcCounters, FastCdc, MAXIMUM_CHUNK_BYTES, MINIMUM_CHUNK_BYTES,
    NORMALIZATION_SHIFT, PROFILE_SEED, TARGET_CHUNK_BYTES,
};
