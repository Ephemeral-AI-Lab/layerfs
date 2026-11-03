//! The home's conversions: opening a home (`open`), the one conversion of
//! a 1.1.0 home (`v110`) and its Store files from Docker volumes (`volume`).
mod open;
mod v110;
mod volume;

pub use open::{note_failure, open, open_from};
