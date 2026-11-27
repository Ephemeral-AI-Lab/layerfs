//! Writer admission and the publication lane.
pub(crate) mod exclusion;
#[allow(clippy::module_inception)]
pub(crate) mod publication;
pub(crate) mod writer_admission;
