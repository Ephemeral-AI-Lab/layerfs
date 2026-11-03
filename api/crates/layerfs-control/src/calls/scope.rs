//! Folder scope (#521 P13, ported into the library by O32): a client option
//! of every front end, `{ project, folder, branch? }`. It is bound to one
//! project, as #521's scoped client was: every call it allows names that
//! project, or a Sandbox of it. It allows only the calls it covers (an
//! allow list): the `file` calls, `branch` list, get, fork, merge and
//! publish, and the `workspace` calls; every other call is refused, history
//! calls included. Paths must be canonical and beneath `folder`; Branch
//! names are relative to the `branch` namespace and composed as
//! `<branch>.<name>`. A scope guards calls; it is not filesystem isolation
//! (a command can still `cd` anywhere in the mounted tree). Every refusal
//! comes before dispatch.
use crate::common::home::with_field;
use crate::common::outcome::refused;
use crate::store::calls::NAMESPACE;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

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
            .filter(|p| crate::common::names::project(p).is_ok())
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

    /// The call's arguments under this scope, or its refusal.
    pub fn apply(&self, name: &str, args: SdkValue) -> Result<SdkValue, SdkRefusal> {
        let (family, call) = name.split_once('.').unwrap_or((name, ""));
        let mut args = args;
        let set = |args: &mut SdkValue, field: &str, value: String| {
            *args = with_field(std::mem::take(args), field, SdkValue::Text(value));
        };
        // the project of every `file` and `branch` call is the scope's
        let bound = |args: &SdkValue| match args.get_text("project") {
            Some(project) if project == self.project => Ok(()),
            _ => Err(out("project")),
        };
        match (family, call) {
            ("commit", _) | ("branch", "diff") => return Err(out("history")),
            ("file", _) => {
                bound(&args)?;
                let paths: &[&str] = match call {
                    "cp" | "mv" => &["from_path", "to_path"],
                    "diff" => &["left_path", "right_path"],
                    "apply" => &[],
                    _ => &["path"],
                };
                for field in paths {
                    if !self.inside(args.get_text(field).unwrap_or("")) {
                        return Err(out("path"));
                    }
                }
                if call == "apply" {
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
            ("branch", "list") => {
                bound(&args)?;
                self.compose("main")?;
                let namespace = self.branch.clone().unwrap_or_default();
                set(&mut args, NAMESPACE, namespace);
            }
            ("branch", "fork") => {
                bound(&args)?;
                for field in ["name", "from_branch"] {
                    let given = args.get_text(field).unwrap_or("").to_owned();
                    set(&mut args, field, self.compose(&given)?);
                }
            }
            ("branch", "merge") => {
                bound(&args)?;
                for field in ["name", "a", "b"] {
                    let given = args.get_text(field).unwrap_or("").to_owned();
                    set(&mut args, field, self.compose(&given)?);
                }
            }
            ("branch", "get" | "publish") => {
                bound(&args)?;
                let given = args.get_text("branch").unwrap_or("").to_owned();
                set(&mut args, "branch", self.compose(&given)?);
            }
            ("workspace", "mount") => {
                if args.get_text("branch").is_some_and(|b| b != "main") {
                    return Err(out("branch"));
                }
                set(&mut args, "branch", self.compose("main")?);
            }
            ("workspace", "exec") => {
                // the command starts in the scope's folder of the Workspace
                let command = args.get_text("command").unwrap_or("").to_owned();
                let folder = format!(".{}", self.folder).replace('\'', "'\\''");
                set(&mut args, "command", format!("cd '{folder}' && {command}"));
            }
            ("workspace", "commit" | "unmount" | "status") => {}
            // every call the scope does not cover: Stores, machines,
            // backups, projects, Sandboxes
            _ => return Err(out(name)),
        }
        Ok(args)
    }

    /// A `workspace` call's Sandbox must be one of the scope's project:
    /// its record names that project. Checked in `home` before dispatch;
    /// other calls pass.
    pub fn owns(&self, home: &Path, name: &str, args: &SdkValue) -> Result<(), SdkRefusal> {
        let sandbox = match name {
            "workspace.mount" => args.get_text("sandbox").unwrap_or(""),
            n if n.starts_with("workspace.") => args
                .get_text("ws")
                .and_then(|ws| ws.split_once(':'))
                .map_or("", |(sandbox, _)| sandbox),
            _ => return Ok(()),
        };
        if crate::common::names::name("sandbox", sandbox).is_err() {
            return Err(out("sandbox"));
        }
        // one read of the Sandbox's record (layout 3, #536 §5)
        match crate::inventory::sandbox(home, sandbox)? {
            Some(record) if record.project() == Some(self.project.as_str()) => Ok(()),
            _ => Err(out("sandbox")),
        }
    }
}
