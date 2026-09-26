//! The two private page-tree algorithms of one Workspace backing.
//!
//! Both specializations are ordered COW page trees over the same arena, page
//! header and ownership ledger; they differ in what selects a child. The
//! length-indexed extent tree orders by logical byte position and carries
//! subtree lengths, so an insertion moves a suffix without rewriting it. The
//! keyed tree orders by cell key and splits variable-size cells by maximum key
//! and occupied bytes. Their descent, balancing and cursor positions are
//! deliberately not shared.
//!
//! Slice 3.0 of the phase-3 plan realized this component by relocating the two
//! existing algorithms here. It changed no page format, no algorithm and no
//! public path: the previous module paths remain reexports of these.
pub mod extent;
pub mod keyed;
