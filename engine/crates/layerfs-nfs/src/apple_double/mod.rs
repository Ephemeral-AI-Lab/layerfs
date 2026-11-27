//! macOS AppleDouble (`._*`) companions kept by the export alone: their
//! index, codec, edits, spill files, reclamation and trace.
pub mod codec;
pub mod companions;
pub(crate) mod edit;
pub(crate) mod index;
pub(crate) mod reclaim;
pub(crate) mod spill;
pub(crate) mod trace;
