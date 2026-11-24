//! The backend-neutral surface: the runtime dispatch over the backends, one
//! command's cancel, a traced command's step times and the shared types.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod cancel;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod dispatch;
pub mod trace;
pub(crate) mod types;
