//! Reference pieces: rows of one layer that name committed content.
pub(crate) mod captured;
pub(crate) mod moves;
pub(crate) mod rows;
pub(crate) use rows::check_claims;
