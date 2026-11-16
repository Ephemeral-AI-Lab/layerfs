//! Writer admission and the publication lane.
#[allow(clippy::module_inception)]
pub(crate) mod lane;
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
pub(crate) mod writer_admission;
