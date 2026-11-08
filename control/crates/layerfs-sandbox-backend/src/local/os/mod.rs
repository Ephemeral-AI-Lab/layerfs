//! Platform facts behind the shared lifecycle: `linux` and `macos`.
#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
