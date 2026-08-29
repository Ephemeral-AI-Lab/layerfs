use clap::{Args, Parser, Subcommand, ValueEnum};
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
    Layer {
        #[command(subcommand)]
        command: LayerCommand,
    },
    Stack {
        #[command(subcommand)]
        command: StackCommand,
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
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum DbCommand {
    Create { role: StoreRole, location: PathBuf },
    Connect { role: StoreRole, location: PathBuf },
    Use { location: PathBuf },
    Disconnect { location: PathBuf },
    List,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum StoreRole {
    Layer,
    Stack,
    Branch,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum LayerCommand {
    Init(LayerInit),
    Pull {
        layer_id: String,
    },
    Add {
        #[arg(long = "from")]
        source: String,
    },
    List,
    Show {
        id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Args)]
#[group(required = true, multiple = false)]
pub struct LayerInit {
    pub directory: Option<PathBuf>,
    #[arg(long)]
    pub empty: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum StackCommand {
    Create {
        #[arg(long = "from")]
        layer_id: String,
    },
    Pull {
        stack_id: String,
    },
    Add {
        #[arg(long = "from")]
        source: String,
    },
    Push {
        stack_id: String,
    },
    List,
    Show {
        id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum BranchCommand {
    Create {
        #[arg(long = "from")]
        source: String,
    },
    Merge {
        source_branch_id: String,
        #[arg(long = "into")]
        target_branch_id: String,
    },
    Pull {
        branch_id: String,
    },
    Push {
        branch_id: String,
    },
    PullCommits {
        branch_id: String,
    },
    List,
    Show {
        id: String,
    },
    Diff {
        left: String,
        right: String,
    },
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
    Shell {
        workspace_id: String,
    },
    Exec {
        workspace_id: String,
        #[arg(last = true, required = true, allow_hyphen_values = true)]
        argv: Vec<OsString>,
    },
    Output {
        execution_id: String,
        #[arg(long)]
        follow: bool,
    },
    Stop {
        execution_id: String,
    },
    Commit {
        workspace_id: String,
    },
    End {
        workspace_id: String,
        #[arg(long)]
        discard: bool,
    },
    List,
    Show {
        workspace_id: String,
    },
    Diff {
        workspace_id: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Projection {
    Fuse,
    Materialize,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum MonitorCommand {
    Db,
    Dedup {
        #[arg(long)]
        route: Option<String>,
        #[arg(long)]
        analyze: bool,
    },
    Workspace {
        workspace_id: Option<String>,
    },
    Branch {
        branch_id: String,
    },
    Operation {
        operation_id: Option<String>,
    },
    Process,
}

impl Command {
    pub(crate) fn arguments(&self) -> Vec<OsString> {
        let mut values = Vec::new();
        match self {
            Self::Db { command } => {
                values.push("db".into());
                match command {
                    DbCommand::Create { role, location } => {
                        values.extend(["create".into(), role_name(*role).into()]);
                        values.push(location.as_os_str().to_owned());
                    }
                    DbCommand::Connect { role, location } => {
                        values.extend(["connect".into(), role_name(*role).into()]);
                        values.push(location.as_os_str().to_owned());
                    }
                    DbCommand::Use { location } => {
                        values.push("use".into());
                        values.push(location.as_os_str().to_owned());
                    }
                    DbCommand::Disconnect { location } => {
                        values.push("disconnect".into());
                        values.push(location.as_os_str().to_owned());
                    }
                    DbCommand::List => values.push("list".into()),
                }
            }
            Self::Layer { command } => {
                values.push("layer".into());
                match command {
                    LayerCommand::Init(request) => {
                        values.push("init".into());
                        if request.empty {
                            values.push("--empty".into());
                        } else if let Some(directory) = &request.directory {
                            values.push(directory.as_os_str().to_owned());
                        }
                    }
                    LayerCommand::Pull { layer_id } => {
                        values.extend(["pull".into(), layer_id.into()]);
                    }
                    LayerCommand::Add { source } => {
                        values.extend(["add".into(), "--from".into(), source.into()]);
                    }
                    LayerCommand::List => values.push("list".into()),
                    LayerCommand::Show { id } => values.extend(["show".into(), id.into()]),
                }
            }
            Self::Stack { command } => {
                values.push("stack".into());
                match command {
                    StackCommand::Create { layer_id } => {
                        values.extend(["create".into(), "--from".into(), layer_id.into()]);
                    }
                    StackCommand::Pull { stack_id } => {
                        values.extend(["pull".into(), stack_id.into()]);
                    }
                    StackCommand::Add { source } => {
                        values.extend(["add".into(), "--from".into(), source.into()]);
                    }
                    StackCommand::Push { stack_id } => {
                        values.extend(["push".into(), stack_id.into()]);
                    }
                    StackCommand::List => values.push("list".into()),
                    StackCommand::Show { id } => values.extend(["show".into(), id.into()]),
                }
            }
            Self::Branch { command } => {
                values.push("branch".into());
                match command {
                    BranchCommand::Create { source } => {
                        values.extend(["create".into(), "--from".into(), source.into()]);
                    }
                    BranchCommand::Merge {
                        source_branch_id,
                        target_branch_id,
                    } => values.extend([
                        "merge".into(),
                        source_branch_id.into(),
                        "--into".into(),
                        target_branch_id.into(),
                    ]),
                    BranchCommand::Pull { branch_id } => {
                        values.extend(["pull".into(), branch_id.into()]);
                    }
                    BranchCommand::Push { branch_id } => {
                        values.extend(["push".into(), branch_id.into()]);
                    }
                    BranchCommand::PullCommits { branch_id } => {
                        values.extend(["pull-commits".into(), branch_id.into()]);
                    }
                    BranchCommand::List => values.push("list".into()),
                    BranchCommand::Show { id } => values.extend(["show".into(), id.into()]),
                    BranchCommand::Diff { left, right } => {
                        values.extend(["diff".into(), left.into(), right.into()]);
                    }
                }
            }
            Self::Workspace { command } => {
                values.push("workspace".into());
                match command {
                    WorkspaceCommand::Create {
                        branch_id,
                        root,
                        container,
                        projection,
                    } => {
                        values.extend(["create".into(), branch_id.into(), "--at".into()]);
                        values.push(root.as_os_str().to_owned());
                        if let Some(container) = container {
                            values.extend(["--container".into(), container.into()]);
                        }
                        if let Some(projection) = projection {
                            values.extend([
                                "--projection".into(),
                                match projection {
                                    Projection::Fuse => "fuse",
                                    Projection::Materialize => "materialize",
                                }
                                .into(),
                            ]);
                        }
                    }
                    WorkspaceCommand::Shell { workspace_id } => {
                        values.extend(["shell".into(), workspace_id.into()]);
                    }
                    WorkspaceCommand::Exec { workspace_id, argv } => {
                        values.extend(["exec".into(), workspace_id.into(), "--".into()]);
                        values.extend(argv.iter().cloned());
                    }
                    WorkspaceCommand::Output {
                        execution_id,
                        follow,
                    } => {
                        values.extend(["output".into(), execution_id.into()]);
                        if *follow {
                            values.push("--follow".into());
                        }
                    }
                    WorkspaceCommand::Stop { execution_id } => {
                        values.extend(["stop".into(), execution_id.into()]);
                    }
                    WorkspaceCommand::Commit { workspace_id } => {
                        values.extend(["commit".into(), workspace_id.into()]);
                    }
                    WorkspaceCommand::End {
                        workspace_id,
                        discard,
                    } => {
                        values.extend(["end".into(), workspace_id.into()]);
                        if *discard {
                            values.push("--discard".into());
                        }
                    }
                    WorkspaceCommand::List => values.push("list".into()),
                    WorkspaceCommand::Show { workspace_id } => {
                        values.extend(["show".into(), workspace_id.into()]);
                    }
                    WorkspaceCommand::Diff { workspace_id } => {
                        values.extend(["diff".into(), workspace_id.into()]);
                    }
                }
            }
            Self::Monitor { command } => {
                values.push("monitor".into());
                match command {
                    MonitorCommand::Db => values.push("db".into()),
                    MonitorCommand::Dedup { route, analyze } => {
                        values.push("dedup".into());
                        if let Some(route) = route {
                            values.extend(["--route".into(), route.into()]);
                        }
                        if *analyze {
                            values.push("--analyze".into());
                        }
                    }
                    MonitorCommand::Workspace { workspace_id } => {
                        values.push("workspace".into());
                        values.extend(workspace_id.iter().map(OsString::from));
                    }
                    MonitorCommand::Branch { branch_id } => {
                        values.extend(["branch".into(), branch_id.into()]);
                    }
                    MonitorCommand::Operation { operation_id } => {
                        values.push("operation".into());
                        values.extend(operation_id.iter().map(OsString::from));
                    }
                    MonitorCommand::Process => values.push("process".into()),
                }
            }
        }
        values
    }
}

fn role_name(role: StoreRole) -> &'static str {
    match role {
        StoreRole::Layer => "layer",
        StoreRole::Stack => "stack",
        StoreRole::Branch => "branch",
    }
}
