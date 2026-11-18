//! A local process as records and Sandboxes name it (moved from the Sandbox
//! driver, layerfs-project#112): its id, kernel start time and boot, read
//! from `/proc` on Linux (`linux.rs`) and `proc_pidinfo`/`sysctl` on macOS
//! (`darwin.rs`, this crate's one audited `unsafe` file), and the retained
//! child custody both the daemon's NFS helpers and the macOS driver's
//! `umount -f` use after a KILL (`retained.rs`), and the macOS mount table
//! under a deadline (`mount_table.rs`). It sits below the
//! control layer so the home and the drivers share it without an edge
//! between them.
#[cfg(target_os = "macos")]
mod darwin;
mod identity;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod mount_table;
mod retained;
#[cfg(target_os = "macos")]
pub use darwin::{boot, processes, stat, sync_volume_wait};
pub use identity::{pid, LocalProcess, Stat};
#[cfg(target_os = "linux")]
pub use linux::{boot, processes, stat};
#[cfg(target_os = "macos")]
pub use mount_table::mount_table;
pub use retained::Retained;
