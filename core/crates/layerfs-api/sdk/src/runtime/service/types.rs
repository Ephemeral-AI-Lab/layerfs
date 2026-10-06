//! Admission windows, typed local requests and original result routing.
use crate::runtime::SaveId;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::StoragePolicy;

/// Configured live admission, independent of total file/Save/Workspace duration.
#[derive(Clone, Copy, Debug)]
pub struct ServiceConfig {
    /// Simultaneously attached authenticated connections.
    pub connections: usize,
    /// Queued/result entries inside this service.
    pub jobs: usize,
    /// Queued/executing/retained jobs for one Workspace, across its connections.
    pub jobs_per_workspace: usize,
    /// Aggregate input/result/transient-copy credit until last completion release.
    pub bytes: usize,
    /// Capacity ordinary Save accepts cannot consume, preserving demand service.
    pub read_reserve: usize,
    /// Capacity accepts/demands cannot consume, preserving finish/history/control.
    pub control_reserve: usize,
}
impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            connections: 32,
            jobs: 128,
            jobs_per_workspace: 16,
            bytes: 128 << 20,
            read_reserve: 66 << 20,
            control_reserve: 128 << 10,
        }
    }
}
/// Round-robin classes; demand cannot starve publication and vice versa.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ServiceClass {
    /// Canonical objects and file-length facts.
    Demand,
    /// Small persisted policy reads.
    Policy,
    /// Owning serial range reservations.
    Serial,
    /// Save object accepts.
    Accept,
    /// Begin/finish/abort and explicit terminal receipt release.
    Finish,
    /// Stage/conditional transition/explicit discard and retained history reads.
    History,
}
/// Authenticated attachment capability; untrusted fields cannot construct it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionId {
    pub(super) owner: u64,
    pub(super) slot: usize,
    pub(super) serial: u64,
}
/// One admitted request, never reused or automatically replayed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ticket {
    pub(super) owner: u64,
    pub(super) slot: usize,
    pub(super) serial: u64,
}
/// Owned input for one bounded host adapter invocation.
#[derive(Debug)]
pub enum Request {
    /// Inspect the original authenticated binding, without a Branch refresh.
    Binding,
    /// Persisted policy under the attached binding.
    Policy,
    /// One authority-owned inode range.
    ReserveInodes {
        /// Requested count.
        count: u64,
    },
    /// Admit one Save capability.
    Begin,
    /// One canonical object; semantic/closure admission remains in Sessions.
    Accept {
        /// Exact owned Save.
        save: SaveId,
        /// Claimed canonical identity.
        id: ObjectId,
        /// Expected canonical role.
        role: ObjectRole,
        /// Borrow-free bounded canonical input.
        canonical: Vec<u8>,
    },
    /// One saved or same-Save canonical demand window.
    Objects {
        /// Owned Save for pending visibility, when selected.
        save: Option<SaveId>,
        /// Exact object demand order.
        ids: Vec<ObjectId>,
    },
    /// One cheap saved-file length window.
    Lengths {
        /// Exact demand order.
        ids: Vec<ObjectId>,
    },
    /// One consuming SaveFinish attempt.
    Finish {
        /// Owned Save.
        save: SaveId,
    },
    /// Explicit producer abort; prior published waves stay published.
    Abort {
        /// Owned Save.
        save: SaveId,
    },
    /// Inspect a retained Save receipt without retrying its operation.
    Completion {
        /// Owned Save.
        save: SaveId,
    },
    /// Explicitly acknowledge a known terminal Save/history disposition.
    Release {
        /// Owned Save.
        save: SaveId,
    },
    /// Stage only after successful SaveFinish, with bound expectations.
    Stage {
        /// Owned finished Save.
        save: SaveId,
        /// Saved candidate root.
        root: ObjectId,
        /// Captured generation.
        generation: u64,
    },
    /// Attempt the acknowledged exact stage/token once.
    Commit {
        /// Owned staged Save.
        save: SaveId,
    },
    /// Explicitly discard a known exact stage/token once.
    Discard {
        /// Owned staged Save.
        save: SaveId,
    },
    /// Read the original stage/transition/discard knowledge.
    History {
        /// Owned Save.
        save: SaveId,
    },
}
/// One owned canonical delivery window; transport may fragment its bytes.
#[derive(Debug)]
pub struct ObjectValue {
    /// Original demanded identity.
    pub id: ObjectId,
    /// Authenticated canonical bytes, kept under completion credit.
    pub canonical: Vec<u8>,
}
/// Result data or a capability for an exact registry-retained receipt.
#[derive(Debug)]
pub enum Response {
    /// Exact original authenticated binding and coherent captured snapshot.
    Binding(Box<crate::Binding>),
    /// Persisted policy.
    Policy(StoragePolicy),
    /// Half-open authority-owned range.
    Serials {
        /// First serial.
        start: u64,
        /// Reserved count.
        count: u64,
    },
    /// Fresh Save capability.
    Begun(SaveId),
    /// Acknowledged canonical object identity.
    Accepted(ObjectId),
    /// Exact ordered canonical results.
    Objects(Vec<ObjectValue>),
    /// Exact ordered trusted length facts.
    Lengths(Vec<(ObjectId, u64)>),
    /// Registry-retained Save result; inspect through Service::save_completion.
    /// This does not itself assert successful SaveFinish.
    Completion(SaveId),
    /// Registry-retained history results; inspect through Service::history_receipts.
    /// This does not itself assert successful history publication.
    History(SaveId),
    /// Explicit known terminal acknowledgement released its registry slot.
    Released,
}
/// Fixed cumulative observations and actual first-party admission ownership.
#[derive(Clone, Copy, Debug, Default)]
pub struct ServiceWork {
    /// Live Demand/Accept/Control job owners, including caller-held completions.
    /// First demand/control slots cannot be consumed by other groups.
    pub live_class_jobs: [usize; 3],
    /// Every submission by original request class, including pre-credit refusals.
    pub submission_attempts: [u64; 6],
    /// Submission refusals by class; original typed error/body are returned.
    /// Queued adapter errors are counted separately as dispatched jobs.
    pub submission_refusals: [u64; 6],
    /// Successfully admitted jobs.
    pub admitted: u64,
    /// Byte/job credit-window refusals, with no provider work.
    /// Authority/format/stale-capability errors are returned separately.
    pub refused: u64,
    /// Invoked adapter jobs per class, including original refusals/errors.
    pub dispatched: [u64; 6],
    /// Queued jobs cancelled before any adapter invocation.
    pub cancelled: u64,
    /// Current queued/executing/completion-owned bytes.
    pub credited_bytes: usize,
    /// Largest observed aggregate credit, not process/OS residency.
    pub peak_credited_bytes: usize,
    /// Jobs still owning credit, including caller-retained completions.
    pub outstanding: usize,
    /// Actual canonical bytes copied into delivery windows.
    pub copied_bytes: u64,
    /// Admission-to-dispatch wall, diagnostic and inclusive of parking.
    pub queue_wait_ns: [u64; 6],
    /// Adapter invocation wall, overlapping underlying library work.
    pub service_ns: [u64; 6],
    /// Fixed queue/connection vector capacities, excluding opaque allocator state.
    pub registry_capacity_bytes: usize,
}
/// Local fence after synchronous in-progress work has returned and queued work
/// is cancelled. It does not decide an unknown Store/history publication.
#[derive(Clone, Copy, Debug)]
pub struct DisconnectFence {
    /// Exact revoked attachment.
    pub connection: ConnectionId,
    /// Queued requests cancelled without invocation.
    pub cancelled: usize,
    /// Already-dispatched results retained unchanged inside the service.
    pub completed: usize,
}
