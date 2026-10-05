//! Explicit native monitor and application lifetime assembly.
#[cfg(feature = "native")]
mod monitor;
#[cfg(feature = "native")]
pub use monitor::{Monitor, MonitorConfig};
#[cfg(feature = "native")]
mod session;
#[cfg(feature = "native")]
pub use session::{Configuration, Runtime};

pub mod observation;
pub mod operation;
