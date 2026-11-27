//! One coherent snapshot and bounded root demand before local namespace creation.
use super::{
    BindError, BindIntent, BindPhase, BindRefusal, BindRequest, BindSuccess, BoundWorkspace,
    NamedBindRequest, Store,
};
use crate::OwnerClient;
use layerfs_content::filesystem::{
    attributes::{read_portable, AttributeReadWork},
    directory::decode_directory_page,
    inode::read::{lookup, InodeReadWork, InodeTable},
    FilesystemRootId, InodeScope,
};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult};
use layerfs_history::{BranchSnapshot, HistoryCatalog, HistoryName, MountMarkers, NamedSnapshot};
use layerfs_protocol::daemon::MountPlace;
use layerfs_vfs::{BaseView, CanonicalClient, Workspace};
use std::cell::Cell;
use std::sync::Arc;

impl Store {
    /// Performs one bind attempt. Root checks never enumerate the namespace.
    /// A lost Open outcome returns original intent; it permits no guessed close.
    #[allow(clippy::result_large_err)]
    pub fn bind(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: BindRequest,
    ) -> Result<BindSuccess, BindRefusal> {
        self.bind_with(owner, BindIntent::Id(request), MountPlace::default(), false)
    }
    /// [`Self::bind`] of a mount whose caller knows the Sandbox and machine
    /// its daemon serves: the registered holder reports them beside the
    /// Workspace. An unknown place is the plain [`Self::bind`].
    #[allow(clippy::result_large_err)]
    pub fn bind_placed(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: BindRequest,
        place: MountPlace,
    ) -> Result<BindSuccess, BindRefusal> {
        self.bind_with(owner, BindIntent::Id(request), place, false)
    }
    /// [`Self::bind`] of the Branch a project and Branch name select (#64),
    /// found on the Workspace's own reader with its snapshot in one read.
    /// A missing project or Branch is [`BindError::NoSuchProject`] or
    /// [`BindError::NoSuchBranch`] in phase Snapshot, as the Store service's
    /// lookup refuses it: an invalid project name is never read, and an
    /// invalid Branch name or `main` is no Branch once the project is found.
    #[allow(clippy::result_large_err)]
    pub fn bind_named(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: NamedBindRequest,
    ) -> Result<BindSuccess, BindRefusal> {
        self.bind_named_with(owner, request, false, &Cell::new(false))
    }
    /// [`Self::bind_named`] for an SDK call's private Workspace, whose
    /// reader comes from and returns to the Store's idle readers, as
    /// [`Self::bind_private`] (#818 W2). `looked_up` is set when the bind
    /// read the project and Branch, so a refusal tells the lookup's own
    /// answer from a failure before it (a reader that did not open).
    #[allow(clippy::result_large_err)]
    pub fn bind_named_private(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: NamedBindRequest,
        looked_up: &Cell<bool>,
    ) -> Result<BindSuccess, BindRefusal> {
        self.bind_named_with(owner, request, true, looked_up)
    }
    #[allow(clippy::result_large_err)]
    fn bind_named_with(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: NamedBindRequest,
        pooled: bool,
        looked_up: &Cell<bool>,
    ) -> Result<BindSuccess, BindRefusal> {
        if HistoryName::new(&request.project).is_err() {
            looked_up.set(true);
            return Err(BindRefusal {
                phase: BindPhase::Snapshot,
                snapshot: None,
                error: BindError::NoSuchProject(request.project.clone()),
                request: BindIntent::Named(request),
            });
        }
        let intent = BindIntent::Named(request);
        self.bind_looked(owner, intent, MountPlace::default(), pooled, looked_up)
    }
    /// [`Self::bind_named`] of a mount whose caller knows the Sandbox and
    /// machine its daemon serves.
    #[allow(clippy::result_large_err)]
    pub fn bind_named_placed(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: NamedBindRequest,
        place: MountPlace,
    ) -> Result<BindSuccess, BindRefusal> {
        self.bind_with(owner, BindIntent::Named(request), place, false)
    }
    /// [`Self::bind`] for an SDK call's private Workspace: its reader comes
    /// from, and returns to, the Store's idle readers (#352 F-025 fix 4).
    #[allow(clippy::result_large_err)]
    pub fn bind_private(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: BindRequest,
    ) -> Result<BindSuccess, BindRefusal> {
        self.bind_with(owner, BindIntent::Id(request), MountPlace::default(), true)
    }
    #[allow(clippy::result_large_err)]
    fn bind_with(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: BindIntent,
        place: MountPlace,
        pooled: bool,
    ) -> Result<BindSuccess, BindRefusal> {
        self.bind_looked(owner, request, place, pooled, &Cell::new(false))
    }
    #[allow(clippy::result_large_err)]
    fn bind_looked(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: BindIntent,
        place: MountPlace,
        pooled: bool,
        looked_up: &Cell<bool>,
    ) -> Result<BindSuccess, BindRefusal> {
        // One call: a quarantined PostgreSQL session reopens at most once.
        layerfs_persistence::one_call(|| {
            looked_up.set(false);
            self.bind_once(owner, request, place, pooled, looked_up)
        })
    }
    #[allow(clippy::result_large_err)]
    fn bind_once(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: BindIntent,
        place: MountPlace,
        pooled: bool,
        looked_up: &Cell<bool>,
    ) -> Result<BindSuccess, BindRefusal> {
        let mut phase = BindPhase::Snapshot;
        let mut snapshot = None;
        let mut gc_op: Option<[u8; 16]> = None;
        let workspace = request.workspace();
        crate::telemetry::timed("bind.reader", |_| self.readers.mount(workspace, pooled)).map_err(
            |error| BindRefusal {
                phase,
                request: request.clone(),
                snapshot: None,
                error: BindError::Read(error),
            },
        )?;
        let result = (|| {
            // The row names its project, so the operator audit finds the
            // released row by project and Branch name: a named bind knows
            // it, a bind by Branch id reads it with the snapshot.
            // A Store that mounts on its writer (PostgreSQL, #768 step B)
            // reads the snapshot there, in one autocommit statement, and
            // its Workspace reader serves no round trip at mount.
            let (captured, project) = crate::telemetry::timed("bind.snapshot", |_| {
                let source = match self.mount_on_writer {
                    true => Source::Writer(self.history()),
                    false => Source::Reader(self.reader(Some(workspace)).map_err(|error| {
                        BindError::Read(Arc::new(super::PortError::ReadAdmission(error)))
                    })?),
                };
                match &request {
                    BindIntent::Id(request) => source
                        .snapshot(request.branch)
                        .map_err(BindError::Read)?
                        .map(|(snapshot, project)| (snapshot, project.as_str().to_owned()))
                        .ok_or(BindError::MissingBranch(request.branch)),
                    BindIntent::Named(named_request) => {
                        looked_up.set(true);
                        named(&source, named_request).map(|s| (s, named_request.project.clone()))
                    }
                }
            })?;
            snapshot = Some(captured.clone());
            // Admission before any root demand: a live removal is `Busy`, an
            // uncertain owner `Unresolved`, and the intent lives until this
            // Workspace's unmount deletes it.
            // A placed mount records the place values its control caller
            // read from the home records, so admission proves it dead later:
            // the machine record's current incarnation and the Sandbox
            // record's generation. A private or place-unknown bind keeps
            // today's behavior: the intent records only the owner tokens, and
            // admission falls back to this process's namespace and boot. The
            // daemon holds no lease or fence of its own, so neither is
            // invented here.
            let mut authority = layerfs_history::PlaceAuthority::default();
            if place.machine.is_some() {
                authority.machine_incarnation = place.machine_incarnation.clone();
            }
            if place.machine.is_some() && place.sandbox.is_some() {
                authority.sandbox_generation = place.generation.clone();
            }
            let intent = crate::history::admission::mount_intent(
                workspace,
                &captured,
                Some(&project),
                &place,
                authority,
            );
            // The bound node's markers come with the registration where the
            // provider reads them after the intent committed (PostgreSQL,
            // #768 step B); otherwise they are read after it, below.
            let mut markers = MountMarkers::NotRead;
            let node = crate::removal::node_of(&captured);
            match (self.history()).gc_register_mount(&intent, captured.branch.stack, node) {
                Ok(read) => {
                    gc_op = Some(intent.op_id);
                    markers = read;
                }
                // A catalog with no GC admission at all (a test double) cannot
                // exclude a removal. The real provider's own `Unsupported`,
                // such as a transaction-pooled PostgreSQL connection that
                // cannot keep a session key, refuses the mount (#776 C1).
                Err(error) if error.no_admission() => {}
                Err(error) => return Err(BindError::History(error)),
            }
            phase = BindPhase::Root;
            let ports = self.ports_for(captured.scope, workspace);
            let client = ports.client();
            let base = crate::telemetry::timed("bind.root", |_| checked_base(client, &captured))
                .map_err(|error| BindError::Content {
                    error,
                    provider: ports
                        .failure()
                        .unwrap_or_else(|error| Some(Arc::new(error))),
                })?;
            phase = BindPhase::Open;
            let (route, owner) = crate::telemetry::timed("bind.overlay", |_| {
                let route = owner.open(
                    None,
                    workspace.to_bytes(),
                    captured.effective_root.to_bytes(),
                )?;
                Ok((route, owner.workspace_client(route)?))
            })
            .map_err(BindError::Owner)?;
            let bound = BoundWorkspace {
                store: self.clone(),
                owner,
                snapshot: std::sync::Mutex::new(captured),
                identity: workspace,
                workspace: Arc::new(Workspace::bind(route, base)),
                gc_op,
                admitted: (gc_op.is_some() && self.batched_chain_reads())
                    .then(|| Arc::new(std::sync::atomic::AtomicBool::new(true))),
                committing: std::sync::atomic::AtomicBool::new(false),
                producer: Default::default(),
                pending: std::sync::Mutex::new(None),
                retained: std::sync::Mutex::new(None),
                logical: std::sync::Mutex::new(None),
                removals: std::sync::Mutex::new(None),
            };
            // §4.3: a mount of a node at or below a marker frontier applies
            // the whiteout, so its view hides the path and its next Commit
            // records a real deletion. The markers are read after the intent
            // committed: a removal that committed before it is seen here, and
            // one after it meets the intent and is refused.
            let known = match markers {
                MountMarkers::Read(found) => Some(found),
                MountMarkers::NotRead => None,
            };
            if bound.materialize_removals_with(known).is_err() {
                let _ = bound.owner.close(route);
                if let Some(op) = bound.gc_op {
                    let _ = crate::history::admission::release_or_mark(self.history(), &op);
                }
                return Err(BindError::Content {
                    error: ContentError::ProviderFailure {
                        what: "path removal whiteout",
                    },
                    provider: None,
                });
            }
            Ok(BindSuccess { workspace: bound })
        })();
        result.map_err(|error| {
            let error = match gc_op
                .map(|op| crate::history::admission::release_or_mark(self.history(), &op))
            {
                Some(Err(cause)) => BindError::Cleanup {
                    original: Box::new(error),
                    cause: Arc::new(super::PortError::History(cause)),
                },
                _ => error,
            };
            let error = match self.close_reader(workspace) {
                Ok(()) => error,
                Err(cause) => BindError::Cleanup {
                    original: Box::new(error),
                    cause,
                },
            };
            BindRefusal {
                phase,
                request,
                snapshot,
                error,
            }
        })
    }
}
/// Where a bind reads its snapshot: its Workspace's reader, or the writer
/// of a Store that mounts on it (#768 step B).
enum Source<'a> {
    Reader(Arc<super::read_handle::ReadSession>),
    Writer(&'a dyn HistoryCatalog),
}
impl Source<'_> {
    fn published<T>(
        &self,
        lookup: impl FnOnce(&dyn HistoryCatalog) -> layerfs_history::HistoryResult<T>,
    ) -> Result<T, Arc<super::PortError>> {
        match self {
            Self::Reader(reader) => reader.published(lookup),
            Self::Writer(history) => {
                lookup(*history).map_err(|error| Arc::new(super::PortError::History(error)))
            }
        }
    }
    fn snapshot(
        &self,
        branch: layerfs_history::BranchId,
    ) -> Result<Option<(BranchSnapshot, HistoryName)>, Arc<super::PortError>> {
        match self {
            Self::Reader(reader) => reader.snapshot(branch),
            Self::Writer(_) => {
                self.published(|history| history.branch_snapshot_with_stack_name(branch))
            }
        }
    }
    fn snapshot_named(
        &self,
        project: &HistoryName,
        branch: &HistoryName,
    ) -> Result<NamedSnapshot, Arc<super::PortError>> {
        match self {
            Self::Reader(reader) => reader.snapshot_named(project, branch),
            Self::Writer(_) => {
                self.published(|history| history.branch_snapshot_named(project, branch))
            }
        }
    }
}

/// The snapshot a named bind selects: the project and Branch by name with
/// the snapshot in one statement. A Branch name that cannot name one
/// (invalid, or `main`) is looked for no further than its project, as the
/// Store service's lookup does.
fn named(source: &Source<'_>, request: &NamedBindRequest) -> Result<BranchSnapshot, BindError> {
    let project = HistoryName::new(&request.project)
        .map_err(|_| BindError::NoSuchProject(request.project.clone()))?;
    let branch = match HistoryName::new(&request.branch) {
        Ok(name) if request.branch != crate::history::MAIN => name,
        _ => {
            let found = source
                .published(|history| history.layer_stack_named(&project))
                .map_err(BindError::Read)?;
            return Err(match found {
                Some(_) => BindError::NoSuchBranch(request.branch.clone()),
                None => BindError::NoSuchProject(request.project.clone()),
            });
        }
    };
    match source
        .snapshot_named(&project, &branch)
        .map_err(BindError::Read)?
    {
        NamedSnapshot::Found(snapshot) => Ok(*snapshot),
        NamedSnapshot::NoStack => Err(BindError::NoSuchProject(request.project.clone())),
        NamedSnapshot::NoBranch => Err(BindError::NoSuchBranch(request.branch.clone())),
    }
}

pub fn checked_base(
    client: Arc<CanonicalClient>,
    snapshot: &BranchSnapshot,
) -> ContentResult<BaseView> {
    let base = BaseView::open(
        client.clone(),
        FilesystemRootId(snapshot.effective_root),
        InodeScope::from_object(snapshot.scope),
    )?;
    let root = base.root();
    if root.profile() != snapshot.profile {
        return Err(ContentError::ScopeMismatch {
            what: "Branch root profile",
        });
    }
    let serial = root.root_inode().serial();
    let value = lookup(
        client.as_ref(),
        InodeTable {
            root: root.inode_table(),
            root_serial: serial,
        },
        serial,
        &mut InodeReadWork::default(),
    )?
    .ok_or(ContentError::InvalidRecord("missing filesystem root inode"))?;
    value.validate(true)?;
    decode_directory_page(&client.read_canonical(value.content_root)?)?;
    read_portable(
        client.as_ref(),
        value.metadata_root,
        value.kind,
        &mut AttributeReadWork::default(),
    )?;
    Ok(base)
}
