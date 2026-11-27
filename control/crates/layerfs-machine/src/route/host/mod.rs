#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod client;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod diagnostic;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod frame;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod hello;
pub(crate) mod stream;
