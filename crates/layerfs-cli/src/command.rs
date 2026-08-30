use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "layerfs", disable_help_subcommand = true)]
pub(crate) struct Invocation {
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum Command {
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
    Context {
        #[command(subcommand)]
        command: ContextCommand,
    },
    Layerstack {
        #[command(subcommand)]
        command: LayerStackCommand,
    },
    Branch {
        #[command(subcommand)]
        command: BranchCommand,
    },
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    Monitor {
        #[command(subcommand)]
        command: MonitorCommand,
    },
    Query {
        kind: QueryKind,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum DbCommand {
    Create {
        role: StoreRole,
        path: PathBuf,
        #[arg(long, required_if_eq("role", "branch"))]
        parent: Option<PathBuf>,
    },
    Connect {
        role: StoreRole,
        location: PathBuf,
        #[arg(long, required_if_eq("role", "branch"))]
        parent: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum StoreRole {
    Layerstack,
    Branch,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum ContextCommand {
    Use {
        #[arg(long)]
        layerstack: PathBuf,
        #[arg(long)]
        branch: PathBuf,
    },
    Show,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum LayerStackCommand {
    Init(LayerStackInit),
    Pull(RemoteLayer),
    Diff {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    Add {
        branch_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
#[command(group(ArgGroup::new("source").required(true).multiple(false).args(["directory", "empty"])))]
pub struct LayerStackInit {
    #[arg(long)]
    pub name: String,
    pub directory: Option<PathBuf>,
    #[arg(long)]
    pub empty: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
#[command(group(ArgGroup::new("placement").required(true).multiple(false).args(["reference", "replica"])))]
pub struct RemoteLayer {
    #[arg(long)]
    pub through: String,
    #[arg(long)]
    pub reference: bool,
    #[arg(long)]
    pub replica: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum BranchCommand {
    Pull(BranchPull),
    Fork(BranchFork),
    Diff(BranchDiff),
    Push { branch_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
#[command(group(ArgGroup::new("placement").required(true).multiple(false).args(["reference", "replica"])))]
pub struct BranchPull {
    pub branch_id: String,
    #[arg(long)]
    pub through: String,
    #[arg(long)]
    pub reference: bool,
    #[arg(long)]
    pub replica: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
#[command(group(ArgGroup::new("source").required(true).multiple(false).args(["layer", "branch"])))]
pub struct BranchFork {
    #[arg(long)]
    pub name: String,
    #[arg(long, conflicts_with = "commit")]
    pub layer: Option<String>,
    #[arg(long, requires = "commit")]
    pub branch: Option<String>,
    #[arg(long, requires = "branch")]
    pub commit: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
#[command(group(ArgGroup::new("comparison").required(true).multiple(false).args(["from", "layer"])))]
pub struct BranchDiff {
    #[arg(long)]
    pub branch: String,
    #[arg(long, requires = "to")]
    pub from: Option<String>,
    #[arg(long, requires = "from")]
    pub to: Option<String>,
    #[arg(long, conflicts_with_all = ["from", "to"])]
    pub layer: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum WorkspaceCommand {
    Create {
        branch_id: String,
        #[arg(long = "at")]
        root: PathBuf,
        #[arg(long)]
        container: Option<String>,
        #[arg(long)]
        projection: Option<Projection>,
    },
    Exec {
        workspace_id: String,
        #[arg(last = true, required = true, allow_hyphen_values = true)]
        argv: Vec<OsString>,
    },
    Shell {
        workspace_id: String,
    },
    Output {
        execution_id: String,
        #[arg(long)]
        follow: bool,
    },
    Stop {
        execution_id: String,
    },
    Conflicts {
        workspace_id: String,
        #[arg(long)]
        after: Option<String>,
    },
    Resolve(WorkspaceResolve),
    Commit {
        workspace_id: String,
    },
    End {
        workspace_id: String,
        #[arg(long)]
        discard: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
#[command(group(ArgGroup::new("choice").required(true).multiple(false).args(["branch", "layer", "working_tree"])))]
pub struct WorkspaceResolve {
    pub workspace_id: String,
    pub conflict_id: String,
    #[arg(long)]
    pub branch: bool,
    #[arg(long)]
    pub layer: bool,
    #[arg(long)]
    pub working_tree: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Projection {
    Fuse,
    Materialize,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum MonitorCommand {
    Snapshot,
    AnalyzeDedup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum QueryKind {
    Layerstacks,
    AuthorityLayerstacks,
    Layers,
    AuthorityLayers,
    AuthorityBranches,
    AuthorityCommits,
    Branches,
    Commits,
    Workspaces,
    Monitor,
}
