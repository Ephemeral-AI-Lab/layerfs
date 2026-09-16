//! Physical encoding: pinned codec, supported FULL records and reconstruction.
//!
//! Entry module: declarations and re-exports only.

pub mod codec;

mod decode;
mod full;

pub use codec::{
    CodecProfile, CompressionWorkspace, DecompressionWorkspace, DECODE_WORKSPACE_BYTES,
    ENCODE_WORKSPACE_BYTES,
};
pub use decode::decode_canonical;
pub use full::{encode_full, FullRecord};
