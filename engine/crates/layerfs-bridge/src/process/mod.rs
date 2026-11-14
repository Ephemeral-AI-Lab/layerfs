//! A local process as records and Sandboxes name it (moved from the Sandbox
//! driver, layerfs-project#112): its id, kernel start time and boot, read
//! from `/proc` on Linux (`linux.rs`) and `proc_pidinfo`/`sysctl` on macOS
//! (`darwin.rs`, this crate's one audited `unsafe` file). It sits below the
//! control layer so the home and the drivers share it without an edge
//! between them.
#[cfg(target_os = "macos")]
mod darwin;
mod identity;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
pub use darwin::{boot, processes, stat, sync_volume_wait};
pub use identity::{pid, LocalProcess, Stat};
#[cfg(target_os = "linux")]
pub use linux::{boot, processes, stat};
