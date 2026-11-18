//! Folder scope (#521 P13, ported into the library by O32): a client option
//! of every front end, `{ project, folder, branch? }`. It is bound to one
//! project, as #521's scoped client was: every call it allows names that
//! project, or a Sandbox of it. It allows only the calls it covers (an
//! allow list): the `file` calls, `branch` list, get, fork, merge and
//! publish, and the `workspace` calls; every other call is refused, history
//! calls included. Each table entry says which [`Rule`] it falls under, so
//! no call name is written here. Paths must be canonical and beneath `folder`; Branch
//! names are relative to the `branch` namespace and composed as
//! `<branch>.<name>`. A scope guards calls; it is not filesystem isolation
//! (a command can still `cd` anywhere in the mounted tree). Every refusal
//! comes before dispatch.
use super::table::Call;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::with_field;
use layerfs_home::outcome::refused;
use std::path::Path;

/// The argument a scoped `branch.list` carries to its family: the Branch
/// namespace, which only a scope sets (the table has no such argument).
pub const NAMESPACE: &str = "namespace";

/// What a folder scope does with one call; its table entry says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Not covered: refused, naming the call. Stores, machines, backups,
    /// projects, Sandboxes, and the GC calls, which retire or compact a
    /// whole entity and never one folder (§10).
    Outside,
    /// A history call: refused as `history`.
    History,
    /// A `file` call: its project is the scope's, each of these path
    /// arguments is beneath the folder, and its Branch and Commit
    /// references are composed.
    File(&'static [&'static str]),
    /// `file.apply`: a `file` call whose paths are in its `changes`.
    FileChanges,
    /// The Branches of the scope's namespace.
    BranchList,
    /// A fork inside the namespace.
    BranchFork,
    /// A merge inside the namespace.
    BranchMerge,
    /// One Branch of the namespace, answered by its name relative to it.
    BranchGet,
    /// One Branch of the namespace, published.
    BranchPublish,
    /// A mount of the namespace's `main`, in a Sandbox of the project.
    WorkspaceMount,
    /// A command, started in the scope's folder of its Workspace.
    WorkspaceExec,
    /// Any other call on a Workspace of a Sandbox of the project.
    Workspace,
}

/// A checked scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    /// The one project, `<store>/<project>`, the scope is bound to.
    pub project: String,
    /// A canonical project-root absolute path.
    pub folder: String,
    /// A Branch namespace prefix, when given.
    pub branch: Option<String>,
}

fn out(kind: &str) -> SdkRefusal {
    refused("InvalidInput", format!("out of scope: {kind}"))
}

fn history_name(name: &str) -> bool {
    let b = name.as_bytes();
    let edge = |c: &u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    (1..=63).contains(&b.len())
        && edge(&b[0])
        && edge(&b[b.len() - 1])
        && b.iter().all(|c| edge(c) || b"_.-".contains(c))
}

fn canonical(path: &str) -> bool {
    path == "/"
        || (path.starts_with('/')
            && !path[1..]
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".."))
}

impl Scope {
    /// Checks `{ project, folder, branch? }`.
    pub fn parse(value: &SdkValue) -> Result<Self, SdkRefusal> {
        let SdkValue::Map(fields) = value else {
            return Err(refused(
                "InvalidInput",
                "scope must be { project, folder, branch? }",
            ));
        };
        if let Some((name, _)) = fields
            .iter()
            .find(|(n, _)| !matches!(n.as_str(), "project" | "folder" | "branch"))
        {
            return Err(refused(
                "InvalidInput",
                format!("unknown scope option `{name}`"),
            ));
        }
        let project = value
            .get_text("project")
            .filter(|p| layerfs_home::names::project(p).is_ok())
            .ok_or_else(|| {
                refused(
                    "InvalidInput",
                    "scope.project must name its project, <store>/<project>",
                )
            })?;
        let folder = value
            .get_text("folder")
            .filter(|f| canonical(f))
            .ok_or_else(|| {
                refused(
                    "InvalidInput",
                    "scope.folder must be a canonical project-root absolute path",
                )
            })?;
        let branch = match value.get("branch") {
            None | Some(SdkValue::Null) => None,
            Some(SdkValue::Text(b)) if history_name(b) => Some(b.clone()),
            Some(_) => {
                return Err(refused(
                    "InvalidInput",
                    "scope.branch must be a slash-free Branch namespace",
                ))
            }
        };
        Ok(Self {
            project: project.to_owned(),
            folder: folder.to_owned(),
            branch,
        })
    }

    fn inside(&self, path: &str) -> bool {
        canonical(path)
            && (self.folder == "/"
                || path == self.folder
                || path.starts_with(&format!("{}/", self.folder)))
    }

    fn compose(&self, name: &str) -> Result<String, SdkRefusal> {
        let prefix = self.branch.as_deref().ok_or_else(|| out("branch"))?;
        let composed = format!("{prefix}.{name}");
        if history_name(name) && history_name(&composed) {
            Ok(composed)
        } else {
            Err(out("branch"))
        }
    }

    /// A Commit id stays as it is; a Branch name is composed.
    fn reference(&self, name: &str) -> Result<String, SdkRefusal> {
        let commit = name.len() == 66
            && name
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if commit {
            Ok(name.to_owned())
        } else {
            self.compose(name)
        }
    }

    /// The arguments of `call` under this scope, or its refusal.
    pub fn apply(&self, call: &Call, args: SdkValue) -> Result<SdkValue, SdkRefusal> {
        let mut args = args;
        let set = |args: &mut SdkValue, field: &str, value: String| {
            *args = with_field(std::mem::take(args), field, SdkValue::Text(value));
        };
        // the project of every `file` and `branch` call is the scope's
        let bound = |args: &SdkValue| match args.get_text("project") {
            Some(project) if project == self.project => Ok(()),
            _ => Err(out("project")),
        };
        match call.scope {
            Rule::History => return Err(out("history")),
            Rule::Outside => return Err(out(&call.name())),
            Rule::File(_) | Rule::FileChanges => {
                bound(&args)?;
                let paths = match call.scope {
                    Rule::File(paths) => paths,
                    _ => &[],
                };
                for field in paths {
                    if !self.inside(args.get_text(field).unwrap_or("")) {
                        return Err(out("path"));
                    }
                }
                if call.scope == Rule::FileChanges {
                    let Some(SdkValue::List(changes)) = args.get("changes") else {
                        return Err(out("path"));
                    };
                    for change in changes {
                        for field in ["path", "from_path", "to_path"] {
                            if let Some(path) = change.get_text(field) {
                                if !self.inside(path) {
                                    return Err(out("path"));
                                }
                            }
                        }
                    }
                }
                for field in [
                    "branch",
                    "commit",
                    "left_branch",
                    "right_branch",
                    "left_commit",
                    "right_commit",
                ] {
                    if let Some(given) = args.get_text(field).map(str::to_owned) {
                        set(&mut args, field, self.reference(&given)?);
                    }
                }
            }
            Rule::BranchList => {
                bound(&args)?;
                self.compose("main")?;
                let namespace = self.branch.clone().unwrap_or_default();
                set(&mut args, NAMESPACE, namespace);
            }
            Rule::BranchFork => {
                bound(&args)?;
                // `from` is the namespace's `main` when left off; `at` names
                // a Commit and is never composed
                let from = args.get_text("from").unwrap_or("main").to_owned();
                set(&mut args, "from", self.reference(&from)?);
                let given = args.get_text("name").unwrap_or("").to_owned();
                set(&mut args, "name", self.compose(&given)?);
            }
            Rule::BranchMerge => {
                bound(&args)?;
                for field in ["branch", "from"] {
                    let given = args.get_text(field).unwrap_or("").to_owned();
                    set(&mut args, field, self.compose(&given)?);
                }
                if let Some(given) = args.get_text("into").map(str::to_owned) {
                    set(&mut args, "into", self.compose(&given)?);
                }
            }
            Rule::BranchGet | Rule::BranchPublish => {
                // `at` and `expected` name Commits and are never composed
                bound(&args)?;
                let given = args.get_text("branch").unwrap_or("").to_owned();
                set(&mut args, "branch", self.compose(&given)?);
            }
            Rule::WorkspaceMount => {
                if args.get_text("branch").is_some_and(|b| b != "main") {
                    return Err(out("branch"));
                }
                set(&mut args, "branch", self.compose("main")?);
            }
            Rule::WorkspaceExec => {
                // the command starts in the scope's folder of the Workspace
                let command = args.get_text("command").unwrap_or("").to_owned();
                let folder = format!(".{}", self.folder).replace('\'', "'\\''");
                set(&mut args, "command", format!("cd '{folder}' && {command}"));
            }
            Rule::Workspace => {}
        }
        Ok(args)
    }

    /// What `call` answered, under this scope: a Branch read by its scoped
    /// name is answered by that name, relative to the namespace.
    pub fn answer(&self, call: &Call, value: SdkValue) -> SdkValue {
        if call.scope != Rule::BranchGet {
            return value;
        }
        let prefix = format!("{}.", self.branch.as_deref().unwrap_or(""));
        match value
            .get_text("branch")
            .and_then(|n| n.strip_prefix(&prefix))
            .map(str::to_owned)
        {
            Some(name) => with_field(value, "branch", SdkValue::Text(name)),
            None => value,
        }
    }

    /// A `workspace` call's Sandbox must be one of the scope's project:
    /// its record names that project. Checked in `home` before dispatch;
    /// other calls pass.
    pub fn owns(&self, home: &Path, call: &Call, args: &SdkValue) -> Result<(), SdkRefusal> {
        let sandbox = match call.scope {
            Rule::WorkspaceMount => args.get_text("sandbox").unwrap_or(""),
            Rule::WorkspaceExec | Rule::Workspace => args
                .get_text("ws")
                .and_then(|ws| ws.split_once(':'))
                .map_or("", |(sandbox, _)| sandbox),
            _ => return Ok(()),
        };
        if layerfs_home::names::name("sandbox", sandbox).is_err() {
            return Err(out("sandbox"));
        }
        // one read of the Sandbox's record (layout 3, #536 §5)
        match layerfs_control::inventory::sandbox(home, sandbox)? {
            Some(record) if record.project() == Some(self.project.as_str()) => Ok(()),
            _ => Err(out("sandbox")),
        }
    }
}
