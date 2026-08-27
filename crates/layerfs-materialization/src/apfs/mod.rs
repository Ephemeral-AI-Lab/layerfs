#[allow(clippy::module_inception)]
mod apfs;
mod ffi;
mod metadata;
mod workspace;

#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub use workspace::inject_clone_unsupported_for_test;
pub use workspace::AppleDriver;
