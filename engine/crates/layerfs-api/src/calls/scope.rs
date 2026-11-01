//! Folder scope (#521 P13, ported into the library by O32): a client option
//! of every front end, `{ folder, branch? }`. Paths must be canonical and
//! beneath `folder`; Branch names are relative to the `branch` namespace and
//! composed as `<branch>.<name>`; history calls are refused. A scope guards
//! calls; it is not filesystem isolation (a command can still `cd` anywhere
//! in the mounted tree). Every refusal comes before dispatch.
use crate::home::with_field;
use crate::host::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// A checked scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
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
    /// Checks `{ folder, branch? }`.
    pub fn parse(value: &SdkValue) -> Result<Self, SdkRefusal> {
        let SdkValue::Map(fields) = value else {
            return Err(refused("InvalidInput", "scope must be { folder, branch? }"));
        };
        if let Some((name, _)) = fields.iter().find(|(n, _)| n != "folder" && n != "branch") {
            return Err(refused(
                "InvalidInput",
                format!("unknown scope option `{name}`"),
            ));
        }
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
        match (family, call) {
            ("commit", _) | ("branch", "diff") => return Err(out("history")),
            ("file", _) => {
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
                self.compose("main")?;
            }
            ("branch", "fork") => {
                for field in ["name", "from_branch"] {
                    let given = args.get_text(field).unwrap_or("").to_owned();
                    set(&mut args, field, self.compose(&given)?);
                }
            }
            ("branch", "merge") => {
                for field in ["name", "a", "b"] {
                    let given = args.get_text(field).unwrap_or("").to_owned();
                    set(&mut args, field, self.compose(&given)?);
                }
            }
            ("branch", "get" | "publish") => {
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
            _ => {}
        }
        Ok(args)
    }

    /// A scoped `branch.list` result: only Branches in the namespace, named
    /// relative to it.
    pub fn branches(&self, value: SdkValue) -> SdkValue {
        let prefix = format!("{}.", self.branch.as_deref().unwrap_or(""));
        let Some(SdkValue::List(items)) = value.get("items").cloned() else {
            return value;
        };
        let kept = items
            .into_iter()
            .filter_map(|item| {
                let name = item.get_text("name")?.strip_prefix(&prefix)?.to_owned();
                Some(with_field(item, "name", SdkValue::Text(name)))
            })
            .collect();
        with_field(value, "items", SdkValue::List(kept))
    }
}
