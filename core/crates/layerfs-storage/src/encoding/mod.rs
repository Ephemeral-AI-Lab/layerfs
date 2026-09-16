//! Physical encoding: pinned codec, supported records, delta selection, pooling.
//!
//! Entry module: declarations and re-exports only.

pub mod codec;
pub mod delta;
pub mod pool;

mod decode;
mod full;

pub use codec::{
    CodecProfile, CompressionWorkspace, DecompressionWorkspace, DECODE_WORKSPACE_BYTES,
    ENCODE_WORKSPACE_BYTES, GROUP_LIMIT,
};
pub use decode::{decode_canonical, framed_record};
pub use full::{encode_full, encode_prefix, lane_body_limit, raw_payload, EncodedRecord};
