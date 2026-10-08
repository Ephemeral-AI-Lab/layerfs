//! One shared dispatcher for every mount, and the exact owners an entry keeps.
use super::native::{bounded, NativeConfig};
use layerfs_bridge::control::{
    NativePhase, NativeReceipt, NativeWork, ReadyMount, TeardownCustody, TeardownStage,
    WorkspaceToken,
};
use layerfs_fuse::{
    session::{DrainStage, NativeSession, SessionFacts, SessionObserver},
    Dispatch, DispatchConfig, DispatchError, MountQueue, StartFailure,
};
use layerfs_overlay::NativeMount;
use std::{fmt, path::PathBuf, sync::Mutex};

/// Process-wide native serving: fixed workers shared by all mount lanes.
pub struct NativeServing {
    /// Short registration and observation sections only; never held across
    /// a mount, a kernel request or an engine job.
    dispatch: Mutex<Dispatch>,
    pub(super) config: NativeConfig,
}
impl NativeServing {
    /// Starts the fixed worker pool once. No mount, device or kernel effect.
    pub fn start(config: NativeConfig) -> Result<Self, Box<StartFailure>> {
        let dispatch = Dispatch::start(DispatchConfig {
            read_handles: config.read_handles,
            namespaces: config.namespaces,
        })
        .map_err(Box::new)?;
        Ok(Self {
            dispatch: Mutex::new(dispatch),
            config,
        })
    }
    /// Claims one fixed lane for an engine mount incarnation.
    pub(super) fn register(&self, mount: NativeMount) -> Result<MountQueue, DispatchError> {
        self.dispatch
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .register(mount)
    }
    pub fn config(&self) -> &NativeConfig {
        &self.config
    }
    /// Maintained daemon-wide worker and lane counters.
    pub fn work(&self) -> Option<layerfs_fuse::DispatchWork> {
        self.dispatch.lock().ok().map(|dispatch| dispatch.work())
    }
    /// A namespace is never reused by this daemon, so neither is its directory.
    pub(super) fn directory(&self, token: WorkspaceToken) -> PathBuf {
        self.config.mounts.join(token.namespace.to_string())
    }
}
impl fmt::Debug for NativeServing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeServing")
            .field("config", &self.config)
            .field("work", &self.work())
            .finish()
    }
}
pub(super) struct Attached {
    pub session: NativeSession,
    pub ready: ReadyMount,
}
pub(super) struct Leaving {
    pub ready: ReadyMount,
    pub observer: SessionObserver,
    pub phase: NativePhase,
}
/// Custody that a terminal or attach operation could not finish disposing.
/// Nothing here is dropped, retried or settled by a later observation.
pub(super) struct Kept {
    pub ready: Option<ReadyMount>,
    pub stage: TeardownStage,
    pub detail: String,
    pub observer: Option<SessionObserver>,
    /// The original owner: an undrained session, a drained receipt or a
    /// refused engine completion, exactly as the stopping boundary left it.
    pub evidence: Box<dyn fmt::Debug + Send>,
}
impl fmt::Debug for Kept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Kept")
            .field("ready", &self.ready)
            .field("stage", &self.stage)
            .field("detail", &self.detail)
            .field("observer", &self.observer)
            .field("evidence", &self.evidence)
            .finish()
    }
}
impl Kept {
    pub(super) fn custody(&self, token: WorkspaceToken) -> TeardownCustody {
        let facts = self.observer.as_ref().map(SessionObserver::facts);
        TeardownCustody {
            token,
            stage: self.stage,
            detached: facts.as_ref().is_some_and(|facts| facts.detached),
            work: facts.as_ref().map(work),
            detail: bounded(&self.detail),
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
pub(super) fn work(facts: &SessionFacts) -> NativeWork {
    let lane = facts.work.unwrap_or_default();
    let count = |value: usize| u16::try_from(value).unwrap_or(u16::MAX);
    let (configured, entered, exited, joined) = facts.loops.map_or((0, 0, 0, 0), |loops| {
        (loops.configured, loops.entered, loops.exited, loops.joined)
    });
    NativeWork {
        loops_configured: count(configured),
        loops_entered: count(entered),
        loops_exited: count(exited),
        loops_joined: count(joined),
        received: narrow(lane.received),
        admitted: narrow(lane.admitted),
        queued: narrow(lane.queued),
        running: narrow(lane.running),
        parked: narrow(lane.parked),
        retained: narrow(lane.retained),
        completed: lane.completed,
        handoffs: facts.opcodes.handoffs,
        inline: facts.opcodes.inline,
        refused: facts.opcodes.refused,
        terminal: facts.opcodes.terminal,
        unadmitted: facts.opcodes.unadmitted,
        forget_units: facts.opcodes.forget_units,
    }
}
/// Built only from a serving session: negotiation and mount entry are present.
pub(super) fn ready(
    token: WorkspaceToken,
    directory: String,
    session: &NativeSession,
) -> Option<ReadyMount> {
    let negotiation = session.negotiation()?;
    let entry = session.entry()?;
    let facts = session.facts();
    let (abi_major, abi_minor) = negotiation.abi_version();
    Some(ReadyMount {
        token,
        directory,
        receipt: NativeReceipt {
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
