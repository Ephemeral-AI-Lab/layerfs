#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
pub(crate) use linux::{sample, SOURCE};
#[cfg(target_os = "macos")]
pub(crate) use macos::{sample, SOURCE};
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("native telemetry requires a qualified macOS or Linux collector");
