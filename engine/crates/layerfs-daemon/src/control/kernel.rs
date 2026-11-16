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

/// The Windows Ready state: one mounted WinFsp file system (A2, #454).
/// The point names the directory it is mounted at; dropping the inner
/// mount unmounts. No detach facts: the in-process mount has no
/// kernel-session teardown states.
#[cfg(windows)]
pub(super) struct WinReady {
    pub mounted: layerfs_winfsp::Mounted,
    pub point: std::path::PathBuf,
}
/// The registry is the sole authority for which connection an entry owns.
pub(super) enum Kernel {
    Unattached,
    /// Windows (A2, 454): the in-process WinFsp mount; attach routes
    /// through layerfs-winfsp mount_workspace instead of a kernel session.
    #[cfg(windows)]
    Ready(Box<WinReady>),
    /// One Attach is admitted, or its engine outcome is unknown.
    Attaching,
    #[cfg(target_os = "linux")]
    Ready(Box<super::serving::Attached>),
    /// A terminal operation owns the session; status still observes it.
    #[cfg(target_os = "linux")]
    Leaving(Box<super::serving::Leaving>),
    #[cfg(target_os = "linux")]
    Retained(Box<super::serving::Kept>),
    /// NFS route: a serving export the host client has mounted.
    #[cfg(unix)]
    Exported(Box<super::export::Exported>),
    /// NFS route: a terminal operation owns the export.
    #[cfg(unix)]
    ExportLeaving(Box<super::export::ExportLeaving>),
    /// NFS route: export custody a teardown could not finish disposing.
    #[cfg(unix)]
    ExportKept(Box<super::export::ExportKept>),
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
            #[cfg(windows)]
            Self::Ready(_) => plain(MountPhase::Ready),
            Self::Attaching => plain(MountPhase::Attaching),
            #[cfg(unix)]
            Self::Exported(_) | Self::ExportLeaving(_) | Self::ExportKept(_) => {
                super::export::block(self).unwrap_or_else(|| plain(MountPhase::Unattached))
            }
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
