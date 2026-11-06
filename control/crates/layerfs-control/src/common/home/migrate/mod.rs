//! The home's conversions: opening a home (`open`), the one conversion of
//! a 1.1.0 home (`v110`) with its Store files from Docker volumes
//! (`volume`), and of a layout 2 home into layout 3 (`v2`).
mod open;
mod v110;
mod v2;
mod volume;

pub use open::{note_failure, open, open_from};
pub(crate) use v2::blockers as layout_2_blockers;
