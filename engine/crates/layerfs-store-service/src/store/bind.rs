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
use layerfs_history::{BranchSnapshot, HistoryName, NamedSnapshot};
use layerfs_workspace::{BaseView, CanonicalClient, Workspace};
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
        self.bind_with(owner, BindIntent::Id(request), false)
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
        if HistoryName::new(&request.project).is_err() {
            return Err(BindRefusal {
                phase: BindPhase::Snapshot,
                snapshot: None,
                error: BindError::NoSuchProject(request.project.clone()),
                request: BindIntent::Named(request),
            });
        }
        self.bind_with(owner, BindIntent::Named(request), false)
    }
    /// [`Self::bind`] for an SDK call's private Workspace: its reader comes
    /// from, and returns to, the Store's idle readers (#352 F-025 fix 4).
    #[allow(clippy::result_large_err)]
    pub fn bind_private(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: BindRequest,
    ) -> Result<BindSuccess, BindRefusal> {
        self.bind_with(owner, BindIntent::Id(request), true)
    }
    #[allow(clippy::result_large_err)]
    fn bind_with(
        self: &Arc<Self>,
        owner: OwnerClient,
        request: BindIntent,
        pooled: bool,
    ) -> Result<BindSuccess, BindRefusal> {
        let mut phase = BindPhase::Snapshot;
        let mut snapshot = None;
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
            let captured = crate::telemetry::timed("bind.snapshot", |_| {
                let reader = self.reader(Some(workspace)).map_err(|error| {
                    BindError::Read(Arc::new(super::PortError::ReadAdmission(error)))
                })?;
                match &request {
                    BindIntent::Id(request) => reader
                        .snapshot(request.branch)
                        .map_err(BindError::Read)?
                        .ok_or(BindError::MissingBranch(request.branch)),
                    BindIntent::Named(request) => named(&reader, request),
                }
            })?;
            snapshot = Some(captured.clone());
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
            Ok(BindSuccess {
                workspace: BoundWorkspace {
                    store: self.clone(),
                    owner,
                    snapshot: std::sync::Mutex::new(captured),
                    identity: workspace,
                    workspace: Arc::new(Workspace::bind(route, base)),
                    committing: std::sync::atomic::AtomicBool::new(false),
                    producer: Default::default(),
                    pending: std::sync::Mutex::new(None),
                },
            })
        })();
        result.map_err(|error| {
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
/// The snapshot a named bind selects, read on its Workspace's reader: the
/// project and Branch by name with the snapshot in one statement. A Branch
/// name that cannot name one (invalid, or `main`) is looked for no further
/// than its project, as the Store service's lookup does.
fn named(
    reader: &super::read_handle::ReadSession,
    request: &NamedBindRequest,
) -> Result<BranchSnapshot, BindError> {
    let project = HistoryName::new(&request.project)
        .map_err(|_| BindError::NoSuchProject(request.project.clone()))?;
    let branch = match HistoryName::new(&request.branch) {
        Ok(name) if request.branch != crate::history::MAIN => name,
        _ => {
            let found = reader
                .published(|history| history.layer_stack_named(&project))
                .map_err(BindError::Read)?;
            return Err(match found {
                Some(_) => BindError::NoSuchBranch(request.branch.clone()),
                None => BindError::NoSuchProject(request.project.clone()),
            });
        }
    };
    match reader
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
