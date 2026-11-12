//! A host operation's typed request (layerfs-project#108): which operation
//! it is, from the one table of them (R1), with its own fields ([`Call`]),
//! and what both sides give every operation ([`Host`]: the home or worker
//! root, the project, its Store and how the Sandbox is deployed,
//! [`Deploy`]). Fields a request may lack stay optional and are refused
//! where each is used, with the text the untyped request was refused with.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers::required;
use layerfs_store::open_args::PgSelection;
use layerfs_store::record::StoreRecord;
use std::path::PathBuf;

/// One host operation (R1: this table is the only list of them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `sandbox.create`: starts the Sandbox.
    SandboxCreate,
    /// `sandbox.close`: stops it.
    SandboxClose,
    /// `sandbox.destroy`: deletes only its data plane, for an invisible
    /// holder that exec records cannot see.
    SandboxDestroy,
    /// `sandbox.list_workspaces`: its daemon's Workspace list.
    ListWorkspaces,
    /// `backend.daemon.connection_failures`: its daemon's control-capacity
    /// read model (#540), a bounded operator read.
    Capacity,
    /// `workspace.mount`.
    Mount,
    /// `workspace.exec`.
    Exec,
    /// `workspace.exec_cancel` (D-11: it rides `sandbox.host` too).
    ExecCancel,
    /// `workspace.commit`.
    Commit,
    /// `workspace.unmount`.
    Unmount,
    /// `backend.daemon.workspace.status`: a Workspace's status.
    Status,
}

/// Every host operation and the name a request carries (`op`). A
/// machine's worker runs exactly these.
pub const OPS: [(Op, &str); 11] = [
    (Op::SandboxCreate, "sandbox.create"),
    (Op::SandboxClose, "sandbox.close"),
    (Op::SandboxDestroy, "sandbox.destroy"),
    (Op::ListWorkspaces, "sandbox.list_workspaces"),
    (Op::Capacity, layerfs_bridge::control::OPERATION),
    (Op::Mount, "workspace.mount"),
    (Op::Exec, "workspace.exec"),
    (Op::ExecCancel, "workspace.exec_cancel"),
    (Op::Commit, "workspace.commit"),
    (Op::Unmount, "workspace.unmount"),
    (Op::Status, "backend.daemon.workspace.status"),
];

impl Op {
    /// The operation named `name`, if it is one.
    pub fn parse(name: &str) -> Option<Self> {
        OPS.iter().find(|(_, n)| *n == name).map(|(op, _)| *op)
    }

    /// Its name.
    pub fn name(self) -> &'static str {
        OPS.iter()
            .find(|(op, _)| *op == self)
            .map_or("", |(_, name)| name)
    }
}

/// A Commit `message` as a request carries it. The untyped request checked
/// it for every operation (`dispatch::checked`), so every request has one.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Message {
    /// Not sent.
    #[default]
    Absent,
    /// Sent as null.
    Null,
    /// Sent as text.
    Text(String),
    /// Sent as another value: refused before any effect.
    NotText,
}

impl Message {
    /// The message as text, when it is.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// One host operation with its own fields; the operation is its variant.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// `sandbox.create`; the image travels in [`Deploy`]. `platform` is a
    /// caller's named platform (#529), checked against the deployment.
    Create { platform: Option<String> },
    /// `sandbox.close(force)`; `held` names Workspaces the caller records
    /// (a Sandbox it reaches directly, #535 G4).
    Close {
        force: bool,
        held: Option<Vec<String>>,
    },
    /// `sandbox.destroy`: delete only the Sandbox's data plane.
    Destroy,
    /// `sandbox.list_workspaces`, with the Workspace `path` it may name.
    ListWorkspaces { path: Option<String> },
    /// `backend.daemon.connection_failures`: its daemon's control-capacity
    /// read model (#540).
    Capacity,
    /// `workspace.mount(branch, { name })`.
    Mount {
        branch: Option<String>,
        name: Option<String>,
    },
    /// `workspace.exec(path, command, { exec_id })`; `directory` and the
    /// Workspace's id (`workspace`) from a caller that keeps the
    /// Workspace's record and the command's (#535 G4); `spool` sends every
    /// non-empty stream to its file.
    Exec {
        path: Option<String>,
        directory: Option<String>,
        command: Option<String>,
        spool: bool,
        exec_id: Option<String>,
        workspace: Option<String>,
    },
    /// `workspace.exec_cancel(path, exec_id, { grace_ms })`; the
    /// Workspace's id (`workspace`) from a caller that keeps the command's
    /// record (#535 G4), so this side keeps none.
    ExecCancel {
        path: Option<String>,
        exec_id: Option<String>,
        grace_ms: Option<i64>,
        workspace: Option<String>,
    },
    /// `workspace.commit(path, { force })`; a flag a caller did not send
    /// is `None`.
    Commit {
        path: Option<String>,
        force: Option<bool>,
    },
    /// `workspace.unmount(path, { discard, force })`.
    Unmount {
        path: Option<String>,
        discard: Option<bool>,
        force: Option<bool>,
    },
    /// A Workspace's status.
    Status { path: Option<String> },
}

impl Call {
    /// Its operation.
    pub fn op(&self) -> Op {
        match self {
            Self::Create { .. } => Op::SandboxCreate,
            Self::Close { .. } => Op::SandboxClose,
            Self::Destroy => Op::SandboxDestroy,
            Self::ListWorkspaces { .. } => Op::ListWorkspaces,
            Self::Capacity => Op::Capacity,
            Self::Mount { .. } => Op::Mount,
            Self::Exec { .. } => Op::Exec,
            Self::ExecCancel { .. } => Op::ExecCancel,
            Self::Commit { .. } => Op::Commit,
            Self::Unmount { .. } => Op::Unmount,
            Self::Status { .. } => Op::Status,
        }
    }

    /// The Workspace path (or name) it names, if any.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::ListWorkspaces { path }
            | Self::Exec { path, .. }
            | Self::ExecCancel { path, .. }
            | Self::Commit { path, .. }
            | Self::Unmount { path, .. }
            | Self::Status { path } => path.as_deref(),
            _ => None,
        }
    }

    /// The call with its Workspace `path` (a name resolved to its path).
    pub fn at(mut self, resolved: Option<String>) -> Self {
        match &mut self {
            Self::ListWorkspaces { path }
            | Self::Exec { path, .. }
            | Self::ExecCancel { path, .. }
            | Self::Commit { path, .. }
            | Self::Unmount { path, .. }
            | Self::Status { path } => *path = resolved,
            _ => {}
        }
        self
    }
}

/// How the side that runs a host operation deploys its Sandbox (`deploy`):
/// its backend, local root and name prefix, and what [`crate::deploy::resolve`]
/// resolved there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Deploy {
    /// `native` (macOS) or `namespaces` (Linux).
    pub sandbox: Option<String>,
    /// The local runtime's root (`machine/native/run`).
    pub local_dir: Option<String>,
    /// The Sandbox directory prefix, `lfs-<home id prefix>-`.
    pub container_prefix: Option<String>,
    /// The daemon executable.
    pub daemon: Option<String>,
    /// A namespaces Sandbox's image archive.
    pub image_archive: Option<String>,
    /// A namespaces Sandbox's image digest.
    pub image: Option<String>,
    /// The platform the machine implies for its Sandboxes (`docker`, D-8).
    pub platform: Option<String>,
}

/// What every host operation runs on: the home (or a worker's root), the
/// project and its Store, the Sandbox it names and how it is deployed. A
/// call on a Sandbox the home reaches directly (#535 G4) has no Store
/// record, path or deployment. No `Debug`: it carries the PostgreSQL
/// selection, password included.
#[derive(Clone)]
pub struct Host {
    /// The home, or a worker's root.
    pub home: PathBuf,
    /// The Store's name.
    pub store: String,
    /// The project's name in its Store.
    pub project: String,
    /// The Sandbox it names (`sandbox`).
    pub sandbox: Option<String>,
    /// The Store's record, as the side that runs the call keeps it.
    pub store_record: Option<StoreRecord>,
    /// The path the open-Store cache keys the Store by (`storePath`).
    pub store_path: Option<String>,
    /// How the Sandbox is deployed.
    pub deploy: Option<Deploy>,
    /// A PostgreSQL Store's resolved server, never written (layerfs-store's
    /// `postgresql` selection, H §7.4).
    pub postgresql: Option<PgSelection>,
    /// The request's Commit message.
    pub message: Message,
    /// Where a forwarded command's exec tells, while it runs, that the
    /// command started (`{ exec_started: { stop } }`): the caller that keeps
    /// its record records the stop handle then (layerfs-project#113).
    pub started: Option<std::sync::mpsc::Sender<SdkValue>>,
}

impl Host {
    /// The Sandbox it names, refused as a missing argument.
    pub fn sandbox(&self) -> Result<&str, SdkRefusal> {
        required(self.sandbox.as_deref(), "sandbox")
    }

    /// Its deployment, when it has one.
    pub fn deploy(&self) -> Option<&Deploy> {
        self.deploy.as_ref()
    }
}

/// What `sandbox.create` answers once its Sandbox started (`started`): its
/// id on its machine (`sandbox`), name, image, platform, runtime, kernel
/// route and runtime id, and, from a machine's worker, its daemon's
/// endpoint and control keys (`control`), which only the caller reads. A
/// field it did not give is `None`, recorded as null. No `Debug`: `control`
/// holds the daemon's secret.
#[derive(Clone, Default)]
pub struct Started {
    /// Its id on its machine.
    pub sandbox: Option<String>,
    /// Its name.
    pub name: Option<String>,
    /// The image it runs.
    pub image: Option<String>,
    /// Its platform (#529).
    pub platform: Option<String>,
    /// Its runtime, `native` or `namespaces`.
    pub runtime: Option<String>,
    /// Its kernel route.
    pub route: Option<String>,
    /// Its runtime's identity.
    pub runtime_id: Option<String>,
    /// Its daemon's endpoint in the machine and control keys (a worker's).
    pub control: Option<SdkValue>,
}
