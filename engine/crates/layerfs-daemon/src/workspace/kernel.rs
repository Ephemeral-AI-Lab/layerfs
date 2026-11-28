//! Kernel half of one registry entry and its bounded status block.
use layerfs_protocol::daemon::{MountPhase, MountStatus, WorkspaceToken};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

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
pub use super::fuse::dispatcher::KernelServing;

/// A Workspace's mount point under the mount home `mounts`: the folder of
/// its name when its attach carries one (#846), so one name mounted again
/// lands on one path; else its namespace's, which this daemon never
/// reuses. Each is created exclusively by its attach: a folder an earlier
/// mount of that name has not released yet refuses it.
pub(crate) fn mount_directory(mounts: &Path, token: WorkspaceToken, name: Option<&str>) -> PathBuf {
    match name {
        Some(name) => mounts.join(name),
        None => mounts.join(token.namespace.to_string()),
    }
}

/// A named mount point that is still there refuses the attach `Busy`
/// before any effect (#846): an earlier mount of that name has not been
/// unmounted, or its folder not removed. Its exclusive create would refuse
/// it too; this says why. A numbered point is new by construction.
pub(crate) fn named_point_free(directory: &Path, name: Option<&str>) -> Result<(), super::Failure> {
    if name.is_none() || std::fs::symlink_metadata(directory).is_err() {
        return Ok(());
    }
    Err(super::Failure::Kernel(Box::new(
        super::receipts::KernelFailure {
            code: layerfs_protocol::daemon::ControlCode::Busy,
            phase: "attach:directory",
            detail: format!(
                "Workspace mount point {} is still there: an earlier mount of this name \
                 has not been released; unmount it, or mount under another name",
                directory.display()
            ),
            evidence: None,
        },
    )))
}

/// The registry is the sole authority for which connection an entry owns.
pub(crate) enum Kernel {
    Unattached,
    /// One Attach is admitted, or its engine outcome is unknown.
    Attaching,
    #[cfg(target_os = "linux")]
    Ready(Box<super::fuse::dispatcher::Attached>),
    /// A terminal operation owns the session; status still observes it.
    #[cfg(target_os = "linux")]
    Leaving(Box<super::fuse::dispatcher::Leaving>),
    #[cfg(target_os = "linux")]
    Retained(Box<super::fuse::dispatcher::Kept>),
    /// NFS route: a serving export the host client has mounted.
    Exported(Box<super::nfs::export::Exported>),
    /// NFS route: a terminal operation owns the export.
    ExportLeaving(Box<super::nfs::export::ExportLeaving>),
    /// NFS route: export custody a teardown could not finish disposing.
    ExportKept(Box<super::nfs::export::ExportKept>),
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
            Self::Attaching => plain(MountPhase::Attaching),
            Self::Exported(_) | Self::ExportLeaving(_) | Self::ExportKept(_) => {
                super::nfs::export::block(self).unwrap_or_else(|| plain(MountPhase::Unattached))
            }
            #[cfg(target_os = "linux")]
            Self::Ready(attached) => {
                let facts = attached.session.facts();
                MountStatus {
                    phase: MountPhase::Ready,
                    ready: Some(attached.ready.clone()),
                    detached: facts.detached,
                    work: Some(super::fuse::dispatcher::work(&facts)),
                }
            }
            #[cfg(target_os = "linux")]
            Self::Leaving(leaving) => {
                let facts = leaving.observer.facts();
                MountStatus {
                    phase: leaving.phase,
                    ready: Some(leaving.ready.clone()),
                    detached: facts.detached,
                    work: Some(super::fuse::dispatcher::work(&facts)),
                }
            }
            #[cfg(target_os = "linux")]
            Self::Retained(kept) => {
                let facts = kept.observer.as_ref().map(|observer| observer.facts());
                MountStatus {
                    phase: MountPhase::Retained,
                    ready: kept.ready.clone(),
                    detached: facts.as_ref().is_some_and(|facts| facts.detached),
                    work: facts.as_ref().map(super::fuse::dispatcher::work),
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
