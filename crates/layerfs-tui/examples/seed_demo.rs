use layerfs_cli::{CliEvent, CliSession, CommandResult, FinishedStatus};
use std::{error::Error, io, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: seed_demo <new-directory>")?;
    if root.exists() {
        return Err(
            io::Error::new(io::ErrorKind::AlreadyExists, root.display().to_string()).into(),
        );
    }
    std::fs::create_dir_all(root.join("workspaces"))?;
    let context = root.join("context");
    let layerstack = root.join("layerstack.sqlite");
    let branch_store = root.join("branch.sqlite");
    let session = CliSession::open(&context)?;

    run(
        &session,
        &format!("db create layerstack {}", layerstack.display()),
    )?;
    run(
        &session,
        &format!(
            "db create branch {} --parent {}",
            branch_store.display(),
            layerstack.display()
        ),
    )?;
    run(
        &session,
        &format!(
            "context use --layerstack {} --branch {}",
            layerstack.display(),
            branch_store.display()
        ),
    )?;
    run(&session, "layerstack init --name npm-demo --empty")?;
    run(
        &session,
        "layerstack pull --through L-S-MOCK-1-01 --replica",
    )?;
    let mut layer = "L-S-MOCK-1-01".to_owned();
    let mut workspace_number = 90;

    for branch_number in 1..=5 {
        let branch_name = if branch_number == 1 {
            "main".to_owned()
        } else {
            format!("rollout-{branch_number}")
        };
        let branch = fork(run(
            &session,
            &format!("branch fork --name {branch_name} --layer {layer}"),
        )?)?;
        let mut anchor = None;
        for commit_number in 1..=6 {
            let workspace = format!("W{workspace_number}");
            let path = root.join("workspaces").join(&workspace);
            let create = match &anchor {
                Some(commit) => format!(
                    "workspace create --branch {branch} --commit {commit} --at {} --projection materialize",
                    path.display()
                ),
                None => format!(
                    "workspace create --branch {branch} --initial-layer {layer} --at {} --projection materialize",
                    path.display()
                ),
            };
            let created = workspace_result(run(&session, &create)?, "Created ")?;
            if created != workspace {
                return Err(io::Error::other("unexpected Workspace ID").into());
            }
            run(
                &session,
                &format!(
                    "workspace exec {workspace} -- /bin/bash -lc 'mkdir -p packages/{branch_name}; printf {branch_name}-{commit_number} > packages/{branch_name}/revision.txt'"
                ),
            )?;
            anchor = Some(workspace_result(
                run(&session, &format!("workspace commit {workspace}"))?,
                "Created ",
            )?);
            run(&session, &format!("workspace end {workspace}"))?;
            workspace_number += 1;
        }
        run(&session, &format!("branch push {branch}"))?;
        layer = added_layer(run(&session, &format!("layerstack add {branch}"))?)?;
        if branch_number < 5 {
            run(
                &session,
                &format!("layerstack pull --through {layer} --replica"),
            )?;
        }
    }

    println!("context={}", context.display());
    println!("branches=5 commits=30 accepted_layers=5 workspaces=0");
    Ok(())
}

fn run(session: &CliSession, line: &str) -> Result<CommandResult, Box<dyn Error>> {
    let command = CliSession::parse_line(line)?;
    session.plan(&command)?;
    let mut handle = session.execute(command)?;
    while let Some(event) = handle.next_event()? {
        if let CliEvent::Finished { status, result, .. } = event {
            if status != FinishedStatus::Succeeded {
                return Err(io::Error::other(format!("failed: {line}")).into());
            }
            return result.map_err(|error| error.into());
        }
    }
    Err(io::Error::other(format!("missing result: {line}")).into())
}

fn fork(result: CommandResult) -> Result<String, Box<dyn Error>> {
    match result {
        CommandResult::Fork { branch_id, .. } => Ok(branch_id.to_string()),
        _ => Err(io::Error::other("expected Fork result").into()),
    }
}

fn workspace_result(result: CommandResult, prefix: &str) -> Result<String, Box<dyn Error>> {
    match result {
        CommandResult::Workspace(value) => value
            .strip_prefix(prefix)
            .map(str::to_owned)
            .ok_or_else(|| io::Error::other("unexpected Workspace result").into()),
        _ => Err(io::Error::other("expected Workspace result").into()),
    }
}

fn added_layer(result: CommandResult) -> Result<String, Box<dyn Error>> {
    match result {
        CommandResult::Add(value) => value
            .strip_prefix("Added ")
            .and_then(|value| value.split_whitespace().next())
            .map(str::to_owned)
            .ok_or_else(|| io::Error::other("unexpected Add result").into()),
        _ => Err(io::Error::other("expected Add result").into()),
    }
}
