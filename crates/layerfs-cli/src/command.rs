use crate::{CliError, CliResult, EntityName, RemotePlacement};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    pub raw: String,
    pub kind: CommandKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceAnchor {
    Commit { branch: String, commit: String },
    InitialLayer { branch: String, layer: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffRequest {
    Layers {
        from: String,
        to: String,
    },
    BranchCommits {
        branch: String,
        from: String,
        to: String,
    },
    BranchLayer {
        branch: String,
        layer: String,
    },
}

impl DiffRequest {
    pub(crate) fn labels(&self) -> (String, String, String) {
        match self {
            Self::Layers { from, to } => ("Layer to Layer".into(), from.clone(), to.clone()),
            Self::BranchCommits { branch, from, to } => (
                format!("Branch {branch} Commit Diff"),
                from.clone(),
                to.clone(),
            ),
            Self::BranchLayer { branch, layer } => (
                format!("Branch {branch} versus Layer"),
                layer.clone(),
                branch.clone(),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreRole {
    LayerStack,
    Branch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandKind {
    DbCreate {
        role: StoreRole,
        location: String,
        parent: Option<String>,
    },
    DbConnect {
        role: StoreRole,
        location: String,
        parent: Option<String>,
    },
    ContextUse {
        layerstack: String,
        branch: String,
    },
    ContextShow,
    ReadOnly {
        family: String,
        action: String,
    },
    LayerStackInit {
        name: EntityName,
        source: String,
    },
    LayerStackPull {
        through: String,
        placement: RemotePlacement,
    },
    BranchPull {
        branch: String,
        through: String,
        placement: RemotePlacement,
    },
    BranchForkLayer {
        name: EntityName,
        layer: String,
    },
    BranchForkCommit {
        name: EntityName,
        branch: String,
        commit: String,
    },
    BranchPush {
        branch: String,
    },
    LayerStackAdd {
        branch: String,
    },
    Diff(DiffRequest),
    WorkspaceCreate {
        anchor: WorkspaceAnchor,
        path: String,
        container: Option<String>,
        projection: String,
    },
    WorkspaceAction {
        action: String,
        target: String,
        arguments: Vec<String>,
    },
    Monitor {
        action: String,
    },
}

impl Command {
    pub fn parse(input: &str) -> CliResult<Self> {
        let tokens = tokenize(input)?;
        if tokens.is_empty() {
            return Err(CliError::Parse("command required".into()));
        }
        let kind = parse_tokens(&tokens)?;
        Ok(Self {
            raw: input.trim().to_owned(),
            kind,
        })
    }
}

fn parse_tokens(tokens: &[String]) -> CliResult<CommandKind> {
    match tokens.first().map(String::as_str) {
        Some("db") => parse_db(tokens),
        Some("context") => parse_context(tokens),
        Some("query") => parse_read_only(tokens),
        Some("layerstack") => parse_layerstack(tokens),
        Some("branch") => parse_branch(tokens),
        Some("workspace") => parse_workspace(tokens),
        Some("monitor") => {
            if tokens.len() != 2 {
                return Err(CliError::Parse("monitor accepts one action".into()));
            }
            let action = tokens
                .get(1)
                .ok_or_else(|| CliError::Parse("monitor action".into()))?;
            if !matches!(action.as_str(), "snapshot" | "analyze-dedup") {
                return Err(CliError::Parse("monitor snapshot|analyze-dedup".into()));
            }
            Ok(CommandKind::Monitor {
                action: action.clone(),
            })
        }
        Some("stack" | "merge") => Err(CliError::Parse("deleted V2 command".into())),
        Some(value) => Err(CliError::Parse(format!("unknown family {value}"))),
        None => Err(CliError::Parse("command required".into())),
    }
}

fn parse_db(tokens: &[String]) -> CliResult<CommandKind> {
    let role = match tokens.get(2).map(String::as_str) {
        Some("layerstack") => StoreRole::LayerStack,
        Some("branch") => StoreRole::Branch,
        _ => return Err(CliError::Parse("db role must be layerstack|branch".into())),
    };
    let location = positional(tokens, 3, "Store location")?;
    let parent = optional_flag(tokens, "--parent");
    let valid = match role {
        StoreRole::LayerStack => tokens.len() == 4 && parent.is_none(),
        StoreRole::Branch => tokens.len() == 6 && parent.is_some(),
    };
    if !valid {
        return Err(CliError::Parse(
            "BranchStore requires exactly one --parent LayerStackStore".into(),
        ));
    }
    match tokens.get(1).map(String::as_str) {
        Some("create") => Ok(CommandKind::DbCreate {
            role,
            location,
            parent,
        }),
        Some("connect") => Ok(CommandKind::DbConnect {
            role,
            location,
            parent,
        }),
        _ => Err(CliError::Parse("db create|connect required".into())),
    }
}

fn parse_context(tokens: &[String]) -> CliResult<CommandKind> {
    match tokens.get(1).map(String::as_str) {
        Some("show") if tokens.len() == 2 => Ok(CommandKind::ContextShow),
        Some("use") if tokens.len() == 6 => Ok(CommandKind::ContextUse {
            layerstack: flag(tokens, "--layerstack")?,
            branch: flag(tokens, "--branch")?,
        }),
        _ => Err(CliError::Parse(
            "context show|use --layerstack <location> --branch <path>".into(),
        )),
    }
}

fn parse_read_only(tokens: &[String]) -> CliResult<CommandKind> {
    let valid = match tokens.first().map(String::as_str) {
        Some("query") => {
            matches!(
                tokens.get(1).map(String::as_str),
                Some(
                    "projects"
                        | "project"
                        | "layers"
                        | "branches"
                        | "commits"
                        | "workspaces"
                        | "executions"
                        | "operations"
                )
            ) && matches!(tokens.len(), 2 | 3)
        }
        _ => false,
    };
    if !valid {
        return Err(CliError::Parse("invalid read-only command".into()));
    }
    Ok(CommandKind::ReadOnly {
        family: tokens[0].clone(),
        action: tokens[1..].join(" "),
    })
}

fn parse_layerstack(tokens: &[String]) -> CliResult<CommandKind> {
    match tokens.get(1).map(String::as_str) {
        Some("init") => {
            exact_len(tokens, 5)?;
            only_flags(tokens, &["--name", "--empty"])?;
            let value = flag(tokens, "--name")?;
            let name = EntityName::parse(value).map_err(|error| CliError::Parse(error.into()))?;
            let empty = tokens.iter().any(|token| token == "--empty");
            let directory = tokens
                .iter()
                .enumerate()
                .skip(2)
                .find(|(index, token)| {
                    !token.starts_with('-')
                        && tokens.get(index.saturating_sub(1)).map(String::as_str) != Some("--name")
                })
                .map(|(_, token)| token.clone());
            let source = match (empty, directory) {
                (true, None) => "empty".into(),
                (false, Some(directory)) => directory,
                _ => {
                    return Err(CliError::Parse(
                        "init requires exactly --empty or one directory".into(),
                    ))
                }
            };
            Ok(CommandKind::LayerStackInit { name, source })
        }
        Some("pull") => {
            exact_len(tokens, 5)?;
            only_flags(tokens, &["--through", "--reference", "--replica"])?;
            Ok(CommandKind::LayerStackPull {
                through: flag(tokens, "--through")?,
                placement: placement(tokens)?,
            })
        }
        Some("diff") => {
            exact_len(tokens, 6)?;
            only_flags(tokens, &["--from", "--to"])?;
            Ok(CommandKind::Diff(DiffRequest::Layers {
                from: flag(tokens, "--from")?,
                to: flag(tokens, "--to")?,
            }))
        }
        Some("add") => {
            exact_len(tokens, 3)?;
            Ok(CommandKind::LayerStackAdd {
                branch: positional(tokens, 2, "Branch")?,
            })
        }
        Some("push") => Err(CliError::Parse("Layer Push does not exist".into())),
        _ => Err(CliError::Parse(
            "layerstack init|pull|diff|add required".into(),
        )),
    }
}

fn parse_branch(tokens: &[String]) -> CliResult<CommandKind> {
    match tokens.get(1).map(String::as_str) {
        Some("pull") => {
            exact_len(tokens, 6)?;
            only_flags(tokens, &["--through", "--reference", "--replica"])?;
            Ok(CommandKind::BranchPull {
                branch: positional(tokens, 2, "Branch")?,
                through: flag(tokens, "--through")?,
                placement: placement(tokens)?,
            })
        }
        Some("fork") => {
            only_flags(tokens, &["--name", "--layer", "--branch", "--commit"])?;
            if tokens
                .iter()
                .any(|token| matches!(token.as_str(), "--reference" | "--replica"))
            {
                return Err(CliError::Parse("Fork accepts no placement".into()));
            }
            let name = EntityName::parse(flag(tokens, "--name")?)
                .map_err(|error| CliError::Parse(error.into()))?;
            match (
                optional_flag(tokens, "--layer"),
                optional_flag(tokens, "--branch"),
            ) {
                (Some(layer), None) if optional_flag(tokens, "--commit").is_none() => {
                    exact_len(tokens, 6)?;
                    Ok(CommandKind::BranchForkLayer { name, layer })
                }
                (None, Some(branch)) => {
                    exact_len(tokens, 8)?;
                    Ok(CommandKind::BranchForkCommit {
                        name,
                        branch,
                        commit: flag(tokens, "--commit")?,
                    })
                }
                _ => Err(CliError::Parse(
                    "Fork requires exactly --layer or --branch + --commit".into(),
                )),
            }
        }
        Some("push") => {
            exact_len(tokens, 3)?;
            Ok(CommandKind::BranchPush {
                branch: positional(tokens, 2, "Branch")?,
            })
        }
        Some("diff") => {
            only_flags(tokens, &["--branch", "--layer", "--from", "--to"])?;
            let branch = flag(tokens, "--branch")?;
            match (
                optional_flag(tokens, "--layer"),
                optional_flag(tokens, "--from"),
                optional_flag(tokens, "--to"),
            ) {
                (Some(layer), None, None) => {
                    exact_len(tokens, 6)?;
                    Ok(CommandKind::Diff(DiffRequest::BranchLayer {
                        branch,
                        layer,
                    }))
                }
                (None, Some(from), Some(to)) => {
                    exact_len(tokens, 8)?;
                    Ok(CommandKind::Diff(DiffRequest::BranchCommits {
                        branch,
                        from,
                        to,
                    }))
                }
                _ => Err(CliError::Parse(
                    "Branch Diff requires --layer or --from + --to".into(),
                )),
            }
        }
        Some("advance" | "merge") => Err(CliError::Parse("deleted V2 command".into())),
        _ => Err(CliError::Parse(
            "branch pull|fork|push|diff required".into(),
        )),
    }
}

fn parse_workspace(tokens: &[String]) -> CliResult<CommandKind> {
    match tokens.get(1).map(String::as_str) {
        Some("create") => {
            only_flags(
                tokens,
                &[
                    "--branch",
                    "--commit",
                    "--initial-layer",
                    "--at",
                    "--container",
                    "--projection",
                ],
            )?;
            if !matches!(tokens.len(), 8 | 10 | 12) {
                return Err(CliError::Parse("unexpected create arguments".into()));
            }
            let branch = flag(tokens, "--branch")?;
            let anchor = match (
                optional_flag(tokens, "--commit"),
                optional_flag(tokens, "--initial-layer"),
            ) {
                (Some(commit), None) => WorkspaceAnchor::Commit { branch, commit },
                (None, Some(layer)) => WorkspaceAnchor::InitialLayer { branch, layer },
                _ => {
                    return Err(CliError::Parse(
                        "Workspace requires exactly --commit or --initial-layer".into(),
                    ))
                }
            };
            Ok(CommandKind::WorkspaceCreate {
                anchor,
                path: flag(tokens, "--at")?,
                container: optional_flag(tokens, "--container"),
                projection: match optional_flag(tokens, "--projection").as_deref() {
                    None | Some("fuse") => "fuse".into(),
                    Some("materialize") => "materialize".into(),
                    Some(_) => {
                        return Err(CliError::Parse(
                            "projection must be fuse or materialize".into(),
                        ))
                    }
                },
            })
        }
        Some(
            action @ ("exec" | "output" | "stop" | "conflicts" | "resolve" | "commit" | "end"),
        ) => {
            validate_workspace_action(action, tokens)?;
            Ok(CommandKind::WorkspaceAction {
                action: action.into(),
                target: positional(tokens, 2, "Workspace or execution")?,
                arguments: tokens.iter().skip(3).cloned().collect(),
            })
        }
        _ => Err(CliError::Parse("workspace action required".into())),
    }
}

fn validate_workspace_action(action: &str, tokens: &[String]) -> CliResult<()> {
    let arguments = tokens.get(3..).unwrap_or_default();
    let valid = match action {
        "exec" => {
            matches!(
                arguments,
                [separator, executable, login, _]
                    if separator == "--" && executable == "/bin/bash" && login == "-lc"
            )
        }
        "stop" | "commit" => arguments.is_empty(),
        "output" => arguments.is_empty() || arguments == ["--follow"],
        "conflicts" => {
            arguments.is_empty()
                || (arguments.len() == 2
                    && arguments.first().is_some_and(|value| value == "--after"))
        }
        "resolve" => {
            arguments.len() == 2
                && matches!(
                    arguments.get(1).map(String::as_str),
                    Some("--branch" | "--layer" | "--working-tree")
                )
        }
        "end" => arguments.is_empty() || arguments == ["--discard"],
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(CliError::Parse(format!(
            "invalid workspace {action} arguments"
        )))
    }
}

fn placement(tokens: &[String]) -> CliResult<RemotePlacement> {
    match (
        tokens.iter().any(|token| token == "--reference"),
        tokens.iter().any(|token| token == "--replica"),
    ) {
        (true, false) => Ok(RemotePlacement::Reference),
        (false, true) => Ok(RemotePlacement::Replica),
        _ => Err(CliError::Parse(
            "exactly one of --reference or --replica required".into(),
        )),
    }
}

fn exact_len(tokens: &[String], expected: usize) -> CliResult<()> {
    if tokens.len() == expected {
        Ok(())
    } else {
        Err(CliError::Parse("unexpected arguments".into()))
    }
}

fn only_flags(tokens: &[String], allowed: &[&str]) -> CliResult<()> {
    if let Some(flag) = tokens
        .iter()
        .filter(|token| token.starts_with("--"))
        .find(|token| !allowed.contains(&token.as_str()))
    {
        Err(CliError::Parse(format!("unexpected flag {flag}")))
    } else {
        Ok(())
    }
}

fn flag(tokens: &[String], name: &str) -> CliResult<String> {
    optional_flag(tokens, name).ok_or_else(|| CliError::Parse(format!("{name} required")))
}

fn optional_flag(tokens: &[String], name: &str) -> Option<String> {
    tokens
        .iter()
        .position(|token| token == name)
        .and_then(|index| tokens.get(index + 1))
        .filter(|value| !value.starts_with('-'))
        .cloned()
}

fn positional(tokens: &[String], index: usize, label: &str) -> CliResult<String> {
    tokens
        .get(index)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| CliError::Parse(format!("{label} required")))
}

fn tokenize(input: &str) -> CliResult<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in input.trim().chars() {
        if escaped {
            token.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            } else {
                token.push(character);
            }
            continue;
        }
        if character.is_whitespace() && quote.is_none() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(character);
        }
    }
    if quote.is_some() || escaped {
        return Err(CliError::Parse("unterminated quote or escape".into()));
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{Command, CommandKind, DiffRequest, WorkspaceAnchor};

    #[test]
    fn accepts_refined_grammar() {
        let commands = [
            "layerstack init --name api-server --empty",
            "layerstack pull --through L-A-19 --replica",
            "branch pull B-main --through C-B-main-42 --reference",
            "branch fork --name search-a --branch B-main --commit C-B-main-35",
            "branch fork --name scratch --layer L-A-18",
            "branch push B-search-a",
            "layerstack add B-search-a",
            "workspace create --branch B-search-a --commit C-B-search-a-08 --at /tmp/w",
            "workspace create --branch B-scratch --initial-layer L-A-18 --at /tmp/w",
            "monitor analyze-dedup",
            "workspace exec W9 -- /bin/bash -lc 'printf ok'",
            "workspace output E-W14 --follow",
            "workspace resolve W31 conflict-1 --working-tree",
        ];
        for command in commands {
            Command::parse(command).unwrap_or_else(|error| panic!("{command}: {error}"));
        }
    }

    #[test]
    fn rejects_deleted_or_ambiguous_grammar() {
        for command in [
            "stack pull S1",
            "branch merge B1 B2",
            "branch fork --name x --branch B --commit C --replica",
            "layerstack pull --through L1 --reference --replica",
            "workspace create --branch B --at /tmp/w",
            "workspace create --branch B --commit C --at /tmp/w --projection overlay",
            "workspace exec W9 cargo test",
            "workspace resolve W31 conflict-1 --branch --layer",
            "layerstack init --name mixed --empty /tmp/source",
            "db garbage",
            "db create branch /tmp/branch",
            "db create layerstack /tmp/layer --parent /tmp/other",
            "context show extra",
            "query projects extra extra",
            "layerstack pull --through L1 --replica --bogus x",
            "branch push B1 extra",
            "branch fork --name x --layer L1 --bogus value",
        ] {
            assert!(Command::parse(command).is_err(), "{command}");
        }
    }

    #[test]
    fn parses_diff_and_anchor_shapes() {
        let command = Command::parse("branch diff --branch B --from C1 --to C2").unwrap();
        assert!(matches!(
            command.kind,
            CommandKind::Diff(DiffRequest::BranchCommits { .. })
        ));
        let command =
            Command::parse("workspace create --branch B --commit C2 --at '/tmp/mock workspace'")
                .unwrap();
        assert!(matches!(
            command.kind,
            CommandKind::WorkspaceCreate {
                anchor: WorkspaceAnchor::Commit { .. },
                ..
            }
        ));
    }
}
