//! One correlated bounded control request, never an immutable-content upload.
use crate::{
    daemon::history::*,
    provision::install::ControlError,
    wire::{Reader, Writer},
};
use layerfs_history::{
    BranchId, CommitHistoryRequest, ForkRequest, ForkSource, HistoryName, LayerId, LayerStackId,
    Page, WorkspaceId,
};
pub(crate) const MAGIC: &[u8] = b"LFSC\x01";
/// A named mount's project and Branch names travel as given, invalid ones
/// too (the daemon refuses them as its lookup does), up to the field limit.
const NAME_FIELD: usize = u16::MAX as usize;
impl Call {
    /// Encodes one checked request. Failure before send has no remote effect.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        if self.id == 0 {
            return Err(ControlError("control correlation"));
        }
        let request = self.request.without_observation()?;
        let mut out = Writer::new(MAGIC);
        out.put(&self.id.to_be_bytes())?;
        if let Request::Observed { scope, traced, .. } = &self.request {
            out.byte(if *traced { 21 } else { 12 })?;
            out.put(scope)?;
        }
        match request {
            Request::Observed { .. } => return Err(ControlError("invalid observed operation")),
            Request::Cleanup(token) => {
                out.byte(13)?;
                put_token(&mut out, *token)?;
            }
            Request::EndSession => out.byte(8)?,
            Request::Hello(value) => {
                out.byte(7)?;
                crate::daemon::hello::put_request(&mut out, value)?;
            }
            Request::Mount { workspace, branch } => {
                out.byte(1)?;
                out.put(&workspace.to_bytes())?;
                out.put(&branch.to_bytes())?;
            }
            Request::MountNamed {
                workspace,
                project,
                branch,
            } => {
                out.byte(15)?;
                out.put(&workspace.to_bytes())?;
                out.blob(project.as_bytes())?;
                out.blob(branch.as_bytes())?;
            }
            Request::MountPlaced {
                workspace,
                branch,
                place,
            } => {
                out.byte(16)?;
                out.put(&workspace.to_bytes())?;
                out.put(&branch.to_bytes())?;
                put_place(&mut out, place)?;
            }
            Request::MountNamedPlaced {
                workspace,
                project,
                branch,
                place,
            } => {
                out.byte(17)?;
                out.put(&workspace.to_bytes())?;
                out.blob(project.as_bytes())?;
                out.blob(branch.as_bytes())?;
                put_place(&mut out, place)?;
            }
            Request::Sdk(value) => {
                out.byte(20)?;
                crate::sdk::put_call(&mut out, value)?;
            }
            Request::Commit(value) => {
                out.byte(2)?;
                crate::daemon::commit::put_commit_control(&mut out, value)?;
            }
            Request::Status(token) => {
                out.byte(3)?;
                put_token(&mut out, *token)?;
            }
            Request::Unmount(value) => {
                out.byte(4)?;
                crate::daemon::unmount::put_unmount(&mut out, value)?;
            }
            Request::Fork(value) => {
                out.byte(5)?;
                out.put(&value.stack.to_bytes())?;
                out.put(&value.branch.to_bytes())?;
                out.blob(value.name.as_str().as_bytes())?;
                match value.source {
                    ForkSource::Layer(id) => {
                        out.byte(1)?;
                        out.put(&id.to_bytes())?;
                    }
                    ForkSource::Commit { branch, commit } => {
                        out.byte(2)?;
                        out.put(&branch.to_bytes())?;
                        out.put(&commit.to_bytes())?;
                    }
                }
            }
            Request::Attach(token) => {
                out.byte(9)?;
                put_token(&mut out, *token)?;
            }
            Request::Locate(workspace) => {
                out.byte(10)?;
                out.put(&workspace.to_bytes())?;
            }
            Request::ForceUnmount {
                token,
                relinquish_unknown,
            } => {
                out.byte(11)?;
                put_token(&mut out, *token)?;
                out.byte(u8::from(*relinquish_unknown))?;
            }
            Request::Branches { stack, page } => {
                window(page.limit)?;
                out.byte(14)?;
                out.put(&stack.to_bytes())?;
                put_cursor(&mut out, &page.cursor)?;
                out.put(&page.limit.to_be_bytes())?;
            }
            Request::History(value) => {
                window(value.limit)?;
                out.byte(6)?;
                out.put(&value.branch.to_bytes())?;
                put_head(&mut out, value.start)?;
                put_cursor(&mut out, &value.cursor)?;
                out.put(&value.limit.to_be_bytes())?;
            }
        }
        Ok(out.0)
    }
    /// Decodes exactly one original request. Unknown tags/versions are refused.
    pub fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        let mut input = Reader::new(bytes, MAGIC)?;
        let id = u64::from_be_bytes(input.array()?);
        if id == 0 {
            return Err(ControlError("control correlation"));
        }
        let tag = input.byte()?;
        let traced = tag == 21;
        let observed = tag == 12 || traced;
        let scope = if observed {
            let scope = input.array::<32>()?;
            if scope == [0; 32] {
                return Err(ControlError("zero observation scope"));
            }
            Some(scope)
        } else {
            None
        };
        let tag = if observed {
            let inner = input.byte()?;
            if matches!(inner, 7 | 8 | 12 | 21) {
                return Err(ControlError("invalid observed operation"));
            }
            inner
        } else {
            tag
        };
        let request = match tag {
            14 => {
                let stack = history(LayerStackId::from_bytes(input.array()?))?;
                let cursor = cursor(&mut input)?;
                let limit = u16::from_be_bytes(input.array()?);
                window(limit)?;
                Request::Branches {
                    stack,
                    page: Page { cursor, limit },
                }
            }
            13 => Request::Cleanup(token(&mut input)?),
            8 => Request::EndSession,
            7 => Request::Hello(crate::daemon::hello::request(&mut input)?),
            1 => Request::Mount {
                workspace: history(WorkspaceId::from_authority(input.array()?))?,
                branch: history(BranchId::from_bytes(input.array()?))?,
            },
            15 => Request::MountNamed {
                workspace: history(WorkspaceId::from_authority(input.array()?))?,
                project: input.text(NAME_FIELD)?,
                branch: input.text(NAME_FIELD)?,
            },
            16 => Request::MountPlaced {
                workspace: history(WorkspaceId::from_authority(input.array()?))?,
                branch: history(BranchId::from_bytes(input.array()?))?,
                place: place(&mut input)?,
            },
            17 => Request::MountNamedPlaced {
                workspace: history(WorkspaceId::from_authority(input.array()?))?,
                project: input.text(NAME_FIELD)?,
                branch: input.text(NAME_FIELD)?,
                place: place(&mut input)?,
            },
            2 => Request::Commit(crate::daemon::commit::commit_control(&mut input)?),
            20 => Request::Sdk(crate::sdk::call(&mut input)?),
            3 => Request::Status(token(&mut input)?),
            4 => Request::Unmount(crate::daemon::unmount::unmount(&mut input)?),
            9 => Request::Attach(token(&mut input)?),
            10 => Request::Locate(history(WorkspaceId::from_authority(input.array()?))?),
            11 => Request::ForceUnmount {
                token: token(&mut input)?,
                relinquish_unknown: boolean(&mut input)?,
            },
            5 => {
                let stack = history(LayerStackId::from_bytes(input.array()?))?;
                let branch = history(BranchId::from_bytes(input.array()?))?;
                let name = history(HistoryName::new(&input.text(63)?))?;
                let source = match input.byte()? {
                    1 => ForkSource::Layer(history(LayerId::from_bytes(input.array()?))?),
                    2 => ForkSource::Commit {
                        branch: history(BranchId::from_bytes(input.array()?))?,
                        commit: history(layerfs_history::CommitId::from_bytes(input.array()?))?,
                    },
                    _ => return Err(ControlError("fork source")),
                };
                Request::Fork(ForkRequest {
                    stack,
                    branch,
                    name,
                    source,
                })
            }
            6 => {
                let branch = history(BranchId::from_bytes(input.array()?))?;
                let start = head(&mut input)?;
                let cursor = cursor(&mut input)?;
                let limit = u16::from_be_bytes(input.array()?);
                window(limit)?;
                Request::History(CommitHistoryRequest {
                    branch,
                    start,
                    cursor,
                    limit,
                })
            }
            _ => return Err(ControlError("control operation")),
        };
        input.finish()?;
        let request = match scope {
            Some(scope) => Request::Observed {
                scope,
                request: Box::new(request),
                traced,
            },
            None => request,
        };
        Ok(Self { id, request })
    }
}
/// Writes the known names and place values of one placed mount.
fn put_place(out: &mut Writer, place: &MountPlace) -> Result<(), ControlError> {
    let flags = u8::from(place.sandbox.is_some())
        | (u8::from(place.machine.is_some()) << 1)
        | (u8::from(place.generation.is_some()) << 2)
        | (u8::from(place.machine_incarnation.is_some()) << 3);
    out.byte(flags)?;
    if let Some(sandbox) = &place.sandbox {
        out.blob(sandbox.as_bytes())?;
    }
    if let Some(machine) = &place.machine {
        out.blob(machine.as_bytes())?;
    }
    if let Some(generation) = &place.generation {
        out.blob(generation.as_bytes())?;
    }
    if let Some(incarnation) = &place.machine_incarnation {
        out.blob(incarnation.as_bytes())?;
    }
    Ok(())
}

/// Reads the known names and place values of one placed mount.
fn place(input: &mut Reader<'_>) -> Result<MountPlace, ControlError> {
    let flags = input.byte()?;
    if flags & !0b1111 != 0 {
        return Err(ControlError("mount place"));
    }
    let sandbox = if flags & 1 != 0 {
        Some(input.text(NAME_FIELD)?)
    } else {
        None
    };
    let machine = if flags & 2 != 0 {
        Some(input.text(NAME_FIELD)?)
    } else {
        None
    };
    let generation = if flags & 4 != 0 {
        Some(input.text(NAME_FIELD)?)
    } else {
        None
    };
    let machine_incarnation = if flags & 8 != 0 {
        Some(input.text(NAME_FIELD)?)
    } else {
        None
    };
    Ok(MountPlace {
        sandbox,
        machine,
        generation,
        machine_incarnation,
    })
}

fn window(limit: u16) -> Result<(), ControlError> {
    if limit == 0 || limit > HISTORY_WINDOW {
        Err(ControlError("history processing window"))
    } else {
        Ok(())
    }
}

// Public mount-control vocabulary; no immutable-content data service.
use layerfs_history::{
    error::MovedState, BranchRecord, BranchSnapshot, CommitOutcome, CommitRecord, PageResult,
};
/// History processing window; authenticated continuation covers arbitrarily many pages.
pub const HISTORY_WINDOW: u16 = 32;
/// Exact daemon-local namespace plus authority incarnation. Never redirect a stale token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceToken {
    /// Authority-selected Workspace incarnation.
    pub workspace: WorkspaceId,
    /// Original local namespace returned by the owner; never reused by this daemon.
    pub namespace: i64,
}
/// The Sandbox and machine a mount belongs to, when the caller knows them.
/// The holder report of a live mount shows them beside its Workspace; a
/// caller that knows neither leaves both absent and the report keeps the
/// Workspace alone.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MountPlace {
    /// The Sandbox the mounting daemon serves, when known.
    pub sandbox: Option<String>,
    /// The machine that Sandbox runs on, when known.
    pub machine: Option<String>,
    /// The generation the Sandbox record named when the caller read it, when
    /// known. A mount intent records it, so a later tombstone retiring that
    /// generation proves the mount dead without an operator.
    pub generation: Option<String>,
    /// The machine record's current ordered incarnation when the caller read
    /// it, when known. A mount intent records it, so a later machine
    /// incarnation proves the mount dead even though the daemon process is
    /// not the worker that recorded the value.
    pub machine_incarnation: Option<String>,
}

impl MountPlace {
    /// The place a caller names, by the names it knows (`None` when not).
    /// An empty name is no name: only a real one is reported.
    #[must_use]
    pub fn new(sandbox: Option<&str>, machine: Option<&str>) -> Self {
        let known = |name: Option<&str>| name.filter(|name| !name.is_empty()).map(str::to_owned);
        Self {
            sandbox: known(sandbox),
            machine: known(machine),
            generation: None,
            machine_incarnation: None,
        }
    }

    /// The same place with the Sandbox `generation` its record named, when the
    /// caller read one. An empty generation is no generation.
    #[must_use]
    pub fn with_generation(self, generation: Option<&str>) -> Self {
        Self {
            generation: generation
                .filter(|generation| !generation.is_empty())
                .map(str::to_owned),
            ..self
        }
    }

    /// The same place with the machine record's current `incarnation`, when
    /// the caller read one. An empty incarnation is no incarnation.
    #[must_use]
    pub fn with_machine_incarnation(self, incarnation: Option<&str>) -> Self {
        Self {
            machine_incarnation: incarnation
                .filter(|incarnation| !incarnation.is_empty())
                .map(str::to_owned),
            ..self
        }
    }

    /// True when either name is known: a mount then reports what it knows.
    #[must_use]
    pub fn known(&self) -> bool {
        self.sandbox.is_some() || self.machine.is_some()
    }
}

/// One explicitly requested control operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    /// Execute one ordinary control once and report its existing numeric work
    /// to the daemon's diagnostic sink. No observation preference is retained.
    /// Nested observations, Hello and EndSession are invalid.
    Observed {
        /// Fresh caller-selected public diagnostic scope for this channel/run.
        /// Zero is invalid. The daemon keeps no registry of previously used scopes.
        scope: [u8; 32],
        /// One original ordinary operation, never another observation wrapper.
        request: Box<Request>,
        /// The SDK trace route: the daemon also records phase timing and
        /// answers [`Reply::Observed`] with that telemetry, and a failed
        /// diagnostic write is reported there instead of ending the
        /// connection. False keeps the original records-only observation.
        traced: bool,
    },
    /// Observe physical cleanup of one allocated namespace without repeating
    /// its terminal operation or retaining its deleted control binding.
    Cleanup(WorkspaceToken),
    /// End this authenticated control session; no Workspace is closed.
    EndSession,
    /// Observe/wait for actual daemon startup, separate from a mount mount.
    Hello(crate::daemon::hello::HelloRequest),
    /// Prepare the Store/engine binding. Kernel attachment is the separate Attach.
    Mount {
        /// New authority incarnation.
        workspace: WorkspaceId,
        /// Selected Branch.
        branch: BranchId,
    },
    /// [`Request::Mount`] of the Branch a project and Branch name select
    /// (#64): the daemon finds it on the new Workspace's own Store reader, so
    /// the caller reads no Store. A missing project or Branch is answered
    /// [`crate::control::Reply::SdkRefused`] `NoSuchProject` or
    /// `NoSuchBranch`, as the Store service's lookup refuses it.
    MountNamed {
        /// New authority incarnation.
        workspace: WorkspaceId,
        /// The project's name, as the caller gave it.
        project: String,
        /// The Branch's name in that project, as the caller gave it.
        branch: String,
    },
    /// [`Request::Mount`] whose caller knows the Sandbox and machine its
    /// daemon serves: a live mount's holders then show all three.
    MountPlaced {
        /// New authority incarnation.
        workspace: WorkspaceId,
        /// Selected Branch.
        branch: BranchId,
        /// The Sandbox and machine the mount belongs to, as known.
        place: MountPlace,
    },
    /// [`Request::MountNamed`] whose caller knows the Sandbox and machine
    /// its daemon serves.
    MountNamedPlaced {
        /// New authority incarnation.
        workspace: WorkspaceId,
        /// The project's name, as the caller gave it.
        project: String,
        /// The Branch's name in that project, as the caller gave it.
        branch: String,
        /// The Sandbox and machine the mount belongs to, as known.
        place: MountPlace,
    },
    /// One SDK call answered by a value or a typed SDK refusal.
    Sdk(crate::sdk::SdkCall),
    /// Capture the published frontier, construct it in the daemon, merge it
    /// with a moved head unless forced, Save and publish.
    Commit(crate::daemon::commit::CommitControl),
    /// Observe maintained local state without reading or refreshing the Store.
    Status(WorkspaceToken),
    /// Terminally close a quiescent local binding and transfer cleanup to the
    /// owner; uncommitted changes refuse it unless discarded.
    Unmount(crate::daemon::unmount::UnmountControl),
    /// Create a Branch through the daemon's direct History port.
    Fork(ForkRequest),
    /// Read one bounded anchored ancestry page through the daemon.
    History(CommitHistoryRequest),
    /// Read one bounded Branch page in stack-local name order through the daemon.
    Branches {
        /// Explicit selected stack, without a hidden catalog scan.
        stack: LayerStackId,
        /// Bounded page and authenticated continuation from the caller.
        page: Page,
    },
    /// Mount the bound Workspace mountly; acknowledged only when it is Ready.
    Attach(WorkspaceToken),
    /// Observe one incarnation by identity alone; settles no original unknown.
    Locate(WorkspaceId),
    /// Abort the owned connection and close terminally; no caller process is signalled.
    ForceUnmount {
        /// Exact incarnation to tear down.
        token: WorkspaceToken,
        /// Permit local disposal of stopped unknown Commit custody; it stays unknown.
        relinquish_unknown: bool,
    },
}
impl Request {
    /// Returns the original operation after validating at most one observation
    /// layer. This validation never allocates, traverses or executes a request.
    pub fn without_observation(&self) -> Result<&Self, crate::provision::install::ControlError> {
        match self {
            Self::Observed { scope, request, .. } => {
                if *scope == [0; 32] {
                    return Err(crate::provision::install::ControlError(
                        "zero observation scope",
                    ));
                }
                match request.as_ref() {
                    Self::Observed { .. } | Self::Hello(_) | Self::EndSession => Err(
                        crate::provision::install::ControlError("invalid observed operation"),
                    ),
                    operation => Ok(operation),
                }
            }
            operation => Ok(operation),
        }
    }
}
/// Correlation for one attempted request on an authenticated connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    /// Nonzero original correlation.
    pub id: u64,
    /// Exact original command.
    pub request: Request,
}
/// Last acknowledged control-owner disposition, copied with its installed binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Activity {
    /// No admitted control Commit or terminal transition.
    Idle,
    /// One admitted Commit is running; this does not predict its result.
    Committing,
    /// Terminal close is admitted; no new Commit may enter.
    Closing,
    /// An original unknown must retain custody; normal Commit/unmount are refused.
    Uncertain,
    /// Publication is known but local installation did not complete normally.
    LocalFailure,
    /// One mount Attach is admitted; its outcome is not yet known.
    Attaching,
}
/// One indexed engine state-row observation, with its own revision scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalObservation {
    /// Original acknowledged local mutation revision.
    pub revision: i64,
    /// Active engine generation.
    pub active: i64,
    /// Retained capture generation, if any.
    pub captured: Option<i64>,
    /// Frozen capture revision, if any.
    pub captured_revision: Option<i64>,
    /// Engine-installed immutable root at this row observation.
    pub base_root: [u8; 32],
    /// Maintained dirty inode count, without scanning.
    pub dirty_inodes: u64,
    /// Maintained dirty binding count, without scanning.
    pub dirty_directory_entries: u64,
    /// Original logical terminal state.
    pub closed: bool,
    /// Maintained base-source ownership count.
    pub base_readers: u64,
}
/// Scoped bounded observations; registry metadata and engine row have distinct epochs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceStatus {
    /// Exact selected incarnation/namespace.
    pub token: WorkspaceToken,
    /// Last binding acknowledged by the control owner, never a Branch refresh.
    pub binding: BranchSnapshot,
    /// Disposition copied coherently with that binding.
    pub activity: Activity,
    /// Control-owner state epoch; saturation does not reject operations.
    pub epoch: u64,
    /// True when the diagnostic epoch can no longer increase.
    pub epoch_saturated: bool,
    /// Known publication retained after a local failure, when available.
    pub published: Option<CommitOutcome>,
    /// Separately scoped indexed engine observation taken after the control copy.
    pub local: Option<LocalObservation>,
    /// Original engine observation refusal when local fields are unavailable.
    pub local_failure: Option<ControlRefusal>,
    /// Kernel attachment block; absent from a daemon without kernel serving.
    pub kernel: Option<crate::daemon::mount::MountStatus>,
}
/// Typed refusal knowledge; the daemon retains its original detailed cause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlCode {
    /// Original write/admission contention; earlier acknowledged Save waves may remain.
    Busy,
    /// Exact conditional Branch conflict, carried in moved.
    HeadMoved,
    /// Selected route or history record is absent.
    Missing,
    /// Declared control or owner capacity refused admission.
    Capacity,
    /// Invalid command or stale namespace token.
    Invalid,
    /// Definite failure; retained context is described by the original phase.
    Failed,
    /// An original outcome is uncertain and cannot be replayed or guessed.
    Unknown,
}
/// Original refused control operation, independent of message text parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlRefusal {
    /// Deciding typed category.
    pub code: ControlCode,
    /// Original operation phase, bounded control metadata.
    pub phase: String,
    /// Exact expected/actual Branch state for HeadMoved.
    pub moved: Option<MovedState>,
    /// Original known publication, including when local install failed.
    pub published: Option<CommitOutcome>,
    /// Original bounded cause description; never parsed to infer the category.
    pub detail: String,
}
/// Original command result. Bound is engine/Store preparation; Ready is kernel readiness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reply {
    /// One exact namespace cleanup observation; it drives no maintenance work.
    Cleanup {
        /// Original requested namespace/incarnation, returned without redirection.
        token: WorkspaceToken,
        /// Existing indexed row/terminal-queue state at this observation.
        state: CleanupObservation,
    },
    /// Original control-session end acknowledgment; filesystem ownership is unchanged.
    SessionEnded,
    /// Original application startup observation; never mount Workspace Ready.
    Hello(crate::daemon::hello::DaemonStatus),
    /// Newly bound engine namespace and exact selected history snapshot.
    Bound {
        /// Original namespace token.
        token: WorkspaceToken,
        /// Exact selected binding.
        binding: BranchSnapshot,
    },
    /// Original known history outcome after paired local installation.
    Committed(CommitOutcome),
    /// A Commit published after merging with a moved head.
    MergeCommitted(Box<crate::daemon::commit::MergedCommit>),
    /// The value an SDK call returned.
    Sdk(crate::sdk::SdkValue),
    /// A refused SDK call.
    SdkRefused(crate::sdk::SdkRefusal),
    /// A Commit refused for conflicts: nothing was published.
    Conflict(Box<crate::daemon::commit::ConflictReport>),
    /// Bounded local observation with explicit owner scopes.
    Status(Box<WorkspaceStatus>),
    /// Logical terminal close acknowledged; physical cleanup remains owner-driven.
    Unmounted(WorkspaceToken),
    /// Unmount refused for uncommitted changes: nothing was closed.
    Dirty(Box<crate::daemon::unmount::DirtyReport>),
    /// Original acknowledged new Branch.
    Forked(BranchSnapshot),
    /// Bounded immutable ancestry page and authenticated continuation.
    History(PageResult<CommitRecord>),
    /// Bounded stack-local Branch page and authenticated continuation.
    Branches(PageResult<BranchRecord>),
    /// Original typed refusal.
    Refused(ControlRefusal),
    /// Mounted, handshake complete and every receive loop serving.
    Ready(Box<crate::daemon::mount::ReadyMount>),
    /// Registry observation for Locate; never a replayed Mount or Attach.
    Located(Box<WorkspaceStatus>),
    /// A terminal operation stopped after effects with its custody retained.
    Retained(Box<crate::daemon::mount::TeardownCustody>),
    /// Forced terminal close acknowledged with its dispositions and cleanup state.
    ForceUnmounted(Box<crate::daemon::forced::ForceUnmounted>),
    /// The answer to a traced observation: the operation's own reply,
    /// unchanged, and diagnostic telemetry beside it. Never nested.
    Observed {
        /// The original operation's reply.
        reply: Box<Reply>,
        /// Phase timing and diagnostics; never part of the operation result.
        telemetry: crate::sdk::SdkValue,
    },
}
/// Maintained cleanup state in the current daemon's allocated namespace domain.
/// A live row must match the token incarnation; missing allocated rows are Gone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupObservation {
    /// The exact existing namespace row is live rather than logically closed.
    Live,
    /// The exact row is closed but terminal reclamation is held by existing custody.
    Held,
    /// The exact closed row has terminal reclamation queued.
    Queued,
    /// No row exists within this daemon's allocated namespace domain. Absence
    /// does not attest which historical incarnation previously occupied it.
    Gone,
}
/// Correlated original reply; transport failure does not reverse its effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Answer {
    /// Exact original request correlation.
    pub id: u64,
    /// Original result.
    pub reply: Reply,
}
