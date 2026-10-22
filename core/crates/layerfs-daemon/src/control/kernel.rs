//! Kernel half of one registry entry and its bounded status block.
use layerfs_bridge::control::{MountPhase, MountStatus};
use std::{path::PathBuf, time::Duration};

/// Declared kernel serving inputs, assembled once by the application.
#[derive(Clone, Debug)]
pub struct KernelConfig {
    /// Existing daemon-owned directory; each mount is a new child of it.
    pub mounts: PathBuf,
    /// Daemon identity recorded as every mount's owner.
    pub owner_uid: u32,
    pub owner_gid: u32,
    /// Runtime command identity presented as every inode's owner.
    pub command_uid: u32,
    pub command_gid: u32,
    /// Observation deadline for all-loop serving during Attach.
    pub ready_wait: Duration,
    /// Observation deadline for loop joins and request drain.
    pub drain_wait: Duration,
}
/// Kernel serving is a Linux capability; elsewhere it cannot be constructed.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[derive(Debug)]
pub enum KernelServing {}
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use super::serving::KernelServing;

/// The registry is the sole authority for which connection an entry owns.
pub(super) enum Kernel {
    Unattached,
    /// One Attach is admitted, or its engine outcome is unknown.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Attaching,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Ready(Box<super::serving::Attached>),
    /// A terminal operation owns the session; status still observes it.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Leaving(Box<super::serving::Leaving>),
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Retained(Box<super::serving::Kept>),
}
impl Kernel {
    /// Maintained observations only: no syscall, SQL job or kernel request.
    pub(super) fn block(&self) -> MountStatus {
        let plain = |phase| MountStatus {
            phase,
            ready: None,
            detached: false,
            work: None,
        };
        match self {
            Self::Unattached => plain(MountPhase::Unattached),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Attaching => plain(MountPhase::Attaching),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Ready(attached) => {
                let facts = attached.session.facts();
                MountStatus {
                    phase: MountPhase::Ready,
                    ready: Some(attached.ready.clone()),
                    detached: facts.detached,
                    work: Some(super::serving::work(&facts)),
                }
            }
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Leaving(leaving) => {
                let facts = leaving.observer.facts();
                MountStatus {
                    phase: leaving.phase,
                    ready: Some(leaving.ready.clone()),
                    detached: facts.detached,
                    work: Some(super::serving::work(&facts)),
                }
            }
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Retained(kept) => {
                let facts = kept.observer.as_ref().map(|observer| observer.facts());
                MountStatus {
                    phase: MountPhase::Retained,
                    ready: kept.ready.clone(),
                    detached: facts.as_ref().is_some_and(|facts| facts.detached),
                    work: facts.as_ref().map(super::serving::work),
                }
            }
        }
    }
}
/// Control detail fields are bounded; the original stays in daemon custody.
pub(super) fn bounded(detail: &str) -> String {
    let mut end = detail.len().min(2048);
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    detail[..end].to_owned()
}
