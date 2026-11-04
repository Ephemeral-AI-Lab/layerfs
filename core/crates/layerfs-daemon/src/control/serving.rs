//! One shared dispatcher for every mount, and the exact owners an entry keeps.
use super::kernel::{bounded, KernelConfig};
use layerfs_bridge::control::{
    ForcedFacts, MountPhase, MountReceipt, MountWork, ReadyMount, TeardownCustody, TeardownStage,
    WorkspaceToken,
};
use layerfs_fuse::{
    session::{DrainStage, KernelSession, SessionFacts, SessionObserver, Undrained},
    DispatchError, MountRequests,
};
use layerfs_overlay::KernelMount;
use std::{fmt, path::PathBuf};

/// Process-wide kernel serving registry: each mount has its own receive workers.
pub struct KernelServing {
    /// Short registration and observation sections only; never held across
    /// a mount, a kernel request or an engine job.
    pub(super) config: KernelConfig,
}
impl KernelServing {
    /// Starts the fixed worker pool once. No mount, device or kernel effect.
    pub fn start(config: KernelConfig) -> Result<Self, std::io::Error> {
        Ok(Self { config })
    }
    pub(super) fn register(&self, mount: KernelMount) -> Result<MountRequests, DispatchError> {
        Ok(MountRequests::new(mount))
    }
    pub fn config(&self) -> &KernelConfig {
        &self.config
    }
    /// A namespace is never reused by this daemon, so neither is its directory.
    pub(super) fn directory(&self, token: WorkspaceToken) -> PathBuf {
        self.config.mounts.join(token.namespace.to_string())
    }
}
impl fmt::Debug for KernelServing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KernelServing")
            .field("config", &self.config)
            .finish()
    }
}
pub(super) struct Attached {
    pub session: KernelSession,
    pub ready: ReadyMount,
}
pub(super) struct Leaving {
    pub ready: ReadyMount,
    pub observer: SessionObserver,
    pub phase: MountPhase,
}
/// Custody that a terminal or attach operation could not finish disposing.
/// Nothing here is dropped, retried or settled by a later observation.
pub(super) struct Kept {
    pub ready: Option<ReadyMount>,
    pub stage: TeardownStage,
    pub detail: String,
    pub observer: Option<SessionObserver>,
    /// The original owner, exactly as the stopping boundary left it.
    pub evidence: Evidence,
    /// Effects of a forced teardown as recorded where it stopped; absent for
    /// a normal unmount or an Attach.
    pub forced: Option<ForcedFacts>,
}
impl fmt::Debug for Kept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Kept")
            .field("ready", &self.ready)
            .field("stage", &self.stage)
            .field("detail", &self.detail)
            .field("observer", &self.observer)
            .field("evidence", &self.evidence)
            .field("forced", &self.forced)
            .finish()
    }
}
/// What a retained entry owns. Only `Stalled` can still be ended, by one
/// forced unmount; every other owner is kept as it is.
pub(super) enum Evidence {
    /// A normal unmount's drain that stopped at `Join` after its plain detach.
    Stalled(Box<Undrained>),
    /// An undrained session, a drained receipt or a refused engine completion.
    Other(Box<dyn fmt::Debug + Send>),
}
/// Shows the owner itself, as the retained-owner observation always has.
impl fmt::Debug for Evidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stalled(undrained) => undrained.fmt(f),
            Self::Other(owner) => owner.fmt(f),
        }
    }
}
impl Kept {
    /// A stalled normal unmount whose abort control is bound and unused.
    pub(super) fn forceable(&self) -> bool {
        let facts = self.observer.as_ref().map(SessionObserver::facts);
        matches!(self.evidence, Evidence::Stalled(_))
            && self.forced.is_none()
            && self.ready.is_some()
            && facts
                .is_some_and(|facts| facts.detached && facts.abort_bound && facts.aborted.is_none())
    }
    pub(super) fn custody(&self, token: WorkspaceToken) -> TeardownCustody {
        let facts = self.observer.as_ref().map(SessionObserver::facts);
        TeardownCustody {
            token,
            stage: self.stage,
            detached: facts.as_ref().is_some_and(|facts| facts.detached),
            work: facts.as_ref().map(work),
            detail: bounded(&self.detail),
            forced: self.forced.clone(),
        }
    }
}
pub(super) fn stage(stage: DrainStage) -> TeardownStage {
    match stage {
        DrainStage::Detach => TeardownStage::Detach,
        DrainStage::Join => TeardownStage::Join,
        DrainStage::Owner => TeardownStage::Owner,
        DrainStage::Requests => TeardownStage::Requests,
        DrainStage::Lane => TeardownStage::Lane,
    }
}
fn narrow(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
pub(super) fn work(facts: &SessionFacts) -> MountWork {
    let lane = facts.work.unwrap_or_default();
    let count = |value: usize| u16::try_from(value).unwrap_or(u16::MAX);
    let (configured, entered, exited, joined) = facts.loops.map_or((0, 0, 0, 0), |loops| {
        (loops.configured, loops.entered, loops.exited, loops.joined)
    });
    MountWork {
        loops_configured: count(configured),
        loops_entered: count(entered),
        loops_exited: count(exited),
        loops_joined: count(joined),
        active: narrow(lane.active),
        retained: narrow(lane.retained),
        completed: lane.completed,
        served: facts.opcodes.served,
        inline: facts.opcodes.inline,
        refused: facts.opcodes.refused,
        terminal: facts.opcodes.terminal,
        forget_units: facts.opcodes.forget_units,
    }
}
/// Built only from a serving session: negotiation and mount entry are present.
pub(super) fn ready(
    token: WorkspaceToken,
    directory: String,
    session: &KernelSession,
) -> Option<ReadyMount> {
    let negotiation = session.negotiation()?;
    let entry = session.entry()?;
    let facts = session.facts();
    let (abi_major, abi_minor) = negotiation.abi_version();
    Some(ReadyMount {
        token,
        directory,
        receipt: MountReceipt {
            mount: facts.mount.owner_id(),
            root: facts.mount.root_serial(),
            mount_id: entry.id,
            device_major: entry.device.0,
            device_minor: entry.device.1,
            abi_major,
            abi_minor,
            offered: negotiation.offered_bits(),
            selected: negotiation.selected_bits(),
            max_write: negotiation.max_write,
            max_readahead: negotiation.max_readahead,
            max_background: negotiation.max_background,
            congestion_threshold: negotiation.congestion_threshold,
            page_size: negotiation.page_size,
            loops: u16::try_from(facts.loops.map_or(0, |loops| loops.entered)).unwrap_or(u16::MAX),
            abort_bound: facts.abort_bound,
        },
    })
}
