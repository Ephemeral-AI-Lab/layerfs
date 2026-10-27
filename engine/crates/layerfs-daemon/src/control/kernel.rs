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
    /// Control tree each connection's abort control is bound from.
    pub control: PathBuf,
}
/// Kernel serving is a Linux capability; elsewhere it cannot be constructed.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub enum KernelServing {}
#[cfg(target_os = "linux")]
pub use super::serving::KernelServing;

/// The registry is the sole authority for which connection an entry owns.
pub(super) enum Kernel {
    Unattached,
    /// One Attach is admitted, or its engine outcome is unknown.
    #[cfg(target_os = "linux")]
    Attaching,
    #[cfg(target_os = "linux")]
    Ready(Box<super::serving::Attached>),
    /// A terminal operation owns the session; status still observes it.
    #[cfg(target_os = "linux")]
    Leaving(Box<super::serving::Leaving>),
    #[cfg(target_os = "linux")]
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
            #[cfg(target_os = "linux")]
            Self::Attaching => plain(MountPhase::Attaching),
            #[cfg(target_os = "linux")]
            Self::Ready(attached) => {
                let facts = attached.session.facts();
                MountStatus {
                    phase: MountPhase::Ready,
                    ready: Some(attached.ready.clone()),
                    detached: facts.detached,
                    work: Some(super::serving::work(&facts)),
                }
            }
            #[cfg(target_os = "linux")]
            Self::Leaving(leaving) => {
                let facts = leaving.observer.facts();
                MountStatus {
                    phase: leaving.phase,
                    ready: Some(leaving.ready.clone()),
                    detached: facts.detached,
                    work: Some(super::serving::work(&facts)),
                }
            }
            #[cfg(target_os = "linux")]
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
