//! Native half of one registry entry and its bounded status block.
use layerfs_bridge::control::{NativePhase, NativeStatus};
use std::{path::PathBuf, time::Duration};

/// Declared native serving inputs, assembled once by the application.
#[derive(Clone, Debug)]
pub struct NativeConfig {
    /// Existing daemon-owned directory; each mount is a new child of it.
    pub mounts: PathBuf,
    /// Daemon identity recorded as every mount's owner.
    pub owner_uid: u32,
    pub owner_gid: u32,
    /// Runtime command identity presented as every inode's owner.
    pub command_uid: u32,
    pub command_gid: u32,
    /// Store reader population; fixes the shared worker count.
    pub read_handles: usize,
    /// Fixed mount lanes, equal to the engine namespace capacity.
    pub namespaces: usize,
    /// Observation deadline for all-loop serving during Attach.
    pub ready_wait: Duration,
    /// Observation deadline for loop joins and request drain.
    pub drain_wait: Duration,
}
/// Native serving is a Linux capability; elsewhere it cannot be constructed.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub enum NativeServing {}
#[cfg(target_os = "linux")]
pub use super::serving::NativeServing;

/// The registry is the sole authority for which connection an entry owns.
pub(super) enum Native {
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
impl Native {
    /// Maintained observations only: no syscall, SQL job or kernel request.
    pub(super) fn block(&self) -> NativeStatus {
        let plain = |phase| NativeStatus {
            phase,
            ready: None,
            detached: false,
            work: None,
        };
        match self {
            Self::Unattached => plain(NativePhase::Unattached),
            #[cfg(target_os = "linux")]
            Self::Attaching => plain(NativePhase::Attaching),
            #[cfg(target_os = "linux")]
            Self::Ready(attached) => {
                let facts = attached.session.facts();
                NativeStatus {
                    phase: NativePhase::Ready,
                    ready: Some(attached.ready.clone()),
                    detached: facts.detached,
                    work: Some(super::serving::work(&facts)),
                }
            }
            #[cfg(target_os = "linux")]
            Self::Leaving(leaving) => {
                let facts = leaving.observer.facts();
                NativeStatus {
                    phase: leaving.phase,
                    ready: Some(leaving.ready.clone()),
                    detached: facts.detached,
                    work: Some(super::serving::work(&facts)),
                }
            }
            #[cfg(target_os = "linux")]
            Self::Retained(kept) => {
                let facts = kept.observer.as_ref().map(|observer| observer.facts());
                NativeStatus {
                    phase: NativePhase::Retained,
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
