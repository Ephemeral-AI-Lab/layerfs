//! FUSE kernel serving of a Workspace: Attach, dispatch and invalidation.
#[cfg(target_os = "linux")]
pub(crate) mod attach;
#[cfg(target_os = "linux")]
pub(crate) mod dispatcher;
pub(crate) mod invalidate;
