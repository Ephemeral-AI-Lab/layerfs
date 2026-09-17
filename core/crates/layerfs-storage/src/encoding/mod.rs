//! Physical encoding: pinned codec, supported records, delta selection, pooling.
//!
//! Entry module: declarations and re-exports only.
//!
//! `codec` is the crate's **only audited `unsafe` boundary** (see the module's
//! own documentation for the FFI inventory). Everywhere else in this crate,
//! `unsafe` is denied by `lib.rs` and rejected again by the product boundary
//! guard, so new FFI or unsafe code cannot appear outside the audited module
//! by accident.

/// The audited zstd FFI boundary; the only module where `unsafe` is allowed.
#[allow(unsafe_code)]
pub mod codec;
pub mod delta;
pub mod pool;

mod decode;
mod full;

pub use codec::{
    CodecProfile, CompressionWorkspace, DecompressionWorkspace, DECODE_WORKSPACE_BYTES,
    ENCODE_WORKSPACE_BYTES, GROUP_LIMIT,
};
pub use decode::{decode_canonical, framed_record, group_records};
pub use full::{encode_full, encode_prefix, lane_body_limit, raw_payload, EncodedRecord};
