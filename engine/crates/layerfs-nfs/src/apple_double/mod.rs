//! macOS AppleDouble (`._*`) companions kept by the export alone: their
//! index, codec, edits, spill files, reclamation and trace.
pub mod codec;
pub mod companions;
pub(crate) mod edit;
pub(crate) mod index;
pub(crate) mod reclaim;
pub(crate) mod spill;
pub(crate) mod trace;

// #828 compat: old public paths, removed in wave 3.
#[doc(hidden)]
pub use companions::{companion, DECODED_ENTRY_BYTES, FILE_BYTES, HELD, RESIDENT_BYTES, SERIALS}; // #828 compat
