//! Seal, headroom admission and page reclamation of the Store file.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
pub(crate) mod file_control;
pub(crate) mod headroom;
pub(crate) mod reclamation;
pub(crate) mod seal;
#[cfg(target_os = "macos")]
pub(crate) mod seal_allocation;
