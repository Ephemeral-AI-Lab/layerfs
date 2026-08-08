//! Private backend-neutral LayerFS storage engine.
//!
//! The implementation will own frozen canonical formats, BLAKE3 identities,
//! FastCDC, immutable CAS admission, dense packs and indexes, copy-on-write
//! trees, workspaces, structural diff, and generic publication primitives.

#![forbid(unsafe_code)]
