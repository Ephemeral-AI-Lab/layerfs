//! Explicit platform monitor and application lifetime assembly.
#[cfg(feature = "platform")]
mod monitor;
#[cfg(feature = "platform")]
pub use monitor::{Monitor, MonitorConfig};
#[cfg(feature = "platform")]
#[cfg(target_os = "linux")]
mod linux;
#[cfg(feature = "platform")]
#[cfg(target_os = "macos")]
mod macos;
#[cfg(feature = "platform")]
mod session;
#[cfg(feature = "platform")]
pub use session::{Configuration, Runtime};

#[cfg(target_os = "linux")]
pub(crate) use linux::{sample, SOURCE};
#[cfg(target_os = "macos")]
pub(crate) use macos::{sample, SOURCE};
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("platform telemetry requires a qualified macOS or Linux collector");
