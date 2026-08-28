//! Bounded streaming content-defined chunking.

mod chunker;
mod gear;

pub use chunker::{
    profile_id, CdcCounters, FastCdc, MAXIMUM_CHUNK_BYTES, MINIMUM_CHUNK_BYTES,
    NORMALIZATION_SHIFT, PROFILE_SEED, TARGET_CHUNK_BYTES,
};
