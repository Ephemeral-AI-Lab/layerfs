use layerfs_cli::{CliEvent, CliSession};
use layerfs_sdk::{BranchStore, EntityName, LayerStackStore, LocalForkSource, RemotePlacement};
use std::sync::Arc;

#[test]
fn process_status_is_nonzero_for_an_operational_failure() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-v2-cli-status-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut output = Vec::new();
    let code = layerfs_cli::invoke(
        root.join("context"),
        ["query", "branches"].map(Into::into).to_vec(),
        false,
        &mut output,
    )
    .unwrap();
    assert_ne!(code, 0, "{}", String::from_utf8_lossy(&output));
    assert!(String::from_utf8_lossy(&output).contains("FAILED"));
}

#[test]
fn exact_v2_grammar_parses_and_deleted_aliases_do_not() {
    let branch = "1100000000007000800000000000000000";
    let commit = "120000000000000000000000000000000000000000000000000000000000000000";
    let layer = "320000000000000000000000000000000000000000000000000000000000000000";
    for line in [
        "db create layerstack /tmp/layerstack.db",
        "db create branch /tmp/branch.db --parent /tmp/layerstack.db",
        "db connect layerstack /tmp/layerstack.db",
        "db connect branch /tmp/branch.db --parent /tmp/layerstack.db",
        "context use --layerstack /tmp/layerstack.db --branch /tmp/branch.db",
        "context show",
        "layerstack init --name project --empty",
        "layerstack init --name imported /tmp/root",
        &format!("layerstack pull --through {layer} --reference"),
        &format!("layerstack pull --through {layer} --replica"),
        &format!("layerstack diff --from {layer} --to {layer}"),
        &format!("layerstack add {branch}"),
        &format!("branch pull {branch} --through {commit} --reference"),
        &format!("branch pull {branch} --through {commit} --replica"),
        &format!("branch fork --name main --layer {layer}"),
        &format!("branch fork --name rollout --branch {branch} --commit {commit}"),
        &format!("branch diff --branch {branch} --from {commit} --to {commit}"),
        &format!("branch diff --branch {branch} --layer {layer}"),
        &format!("branch push {branch}"),
        &format!("workspace create {branch} --at /tmp/work"),
        "workspace exec w:00000000000000000000000000000000 -- /bin/echo ok",
        "workspace shell w:00000000000000000000000000000000",
        "workspace output x:00000000000000000000000000000000 --follow",
        "workspace stop x:00000000000000000000000000000000",
        "workspace conflicts w:00000000000000000000000000000000 --after cursor",
        "workspace resolve w:00000000000000000000000000000000 conflict --branch",
        "workspace commit w:00000000000000000000000000000000",
        "workspace end w:00000000000000000000000000000000 --discard",
        "monitor snapshot",
        "monitor analyze-dedup",
    ] {
        CliSession::parse_line(line).unwrap_or_else(|error| panic!("{line}: {error}"));
    }
    for deleted in [
        "layer init --empty",
        "layerstack init --empty",
        "stack create --from id",
        "branch create --from id",
        "branch merge a --into b",
        &format!("branch fork --layer {layer}"),
        "branch pull id",
        "branch pull-commits id",
        &format!("layerstack pull {layer} --reference"),
        &format!("branch fork --name invalid --branch {branch} --commit {commit} --reference"),
        "db use /tmp/db",
        "db create layerstack /tmp/layerstack.db --parent /tmp/parent.db",
        "db connect layerstack /tmp/layerstack.db --parent /tmp/parent.db",
    ] {
        assert!(CliSession::parse_line(deleted).is_err(), "{deleted}");
    }
}

#[test]
fn frontend_session_executes_one_pair_lifecycle_without_a_tui() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-v2-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let context = root.join("context");
    let layerstack = root.join("layerstack.sqlite");
    let branch = root.join("branch.sqlite");
    let session = CliSession::open(&context).unwrap();
    for command in [
        format!("db create layerstack {}", layerstack.display()),
        format!(
            "db create branch {} --parent {}",
            branch.display(),
            layerstack.display()
        ),
        format!(
            "context use --layerstack {} --branch {}",
            layerstack.display(),
            branch.display()
        ),
    ] {
        let handle = session
            .execute(CliSession::parse_line(&command).unwrap())
            .unwrap();
        assert!(matches!(
            handle.next_event().unwrap(),
            Some(CliEvent::Started(_))
        ));
        assert!(matches!(
            handle.next_event().unwrap(),
            Some(CliEvent::Finished(Ok(_)))
        ));
    }
    let authority = Arc::new(LayerStackStore::connect(&layerstack).unwrap());
    let genesis = authority
        .initialize_layerstack(
            EntityName::new("project").unwrap(),
            layerfs_sdk::LayerStackInitialization::Empty,
        )
        .unwrap()
        .genesis_layer_id;
    let branches = BranchStore::connect(&branch, authority.store_id()).unwrap();
    branches
        .pull_layer(authority.clone(), genesis, RemotePlacement::Reference)
        .unwrap();
    let branch_id = branches
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer { layer_id: genesis },
        )
        .unwrap();
    drop(branches);
    drop(authority);
    let completions = session.complete("branch push ", 12).unwrap();
    assert!(completions.iter().any(|completion| {
        completion.value == branch_id.to_string()
            && completion.display == format!("project/main ({branch_id})")
    }));
    let workspace = finished_text(
        &session,
        &format!(
            "workspace create {branch_id} --at {} --projection materialize",
            root.join("workspace").display()
        ),
    );
    let execution = finished_text(
        &session,
        &format!(
            "workspace exec {workspace} -- /bin/sh -c 'printf first; printf data > created; sleep 0.2; printf second'"
        ),
    );
    let handle = session
        .execute(CliSession::parse_line(&format!("workspace output {execution} --follow")).unwrap())
        .unwrap();
    let mut output = Vec::new();
    while let Some(event) = handle.next_event().unwrap() {
        if let CliEvent::Output(bytes) = event {
            output.extend(bytes);
        }
    }
    assert_eq!(output, b"firstsecond");
    finished_text(&session, &format!("workspace commit {workspace}"));
    finish(&session, &format!("workspace end {workspace}"));
    finished_text(&session, &format!("branch push {branch_id}"));
    drop(session);
    let session = CliSession::open(&context).unwrap();

    let second_branch = root.join("branch-second.sqlite");
    finished_text(
        &session,
        &format!(
            "db create branch {} --parent {}",
            second_branch.display(),
            layerstack.display()
        ),
    );
    finished_text(
        &session,
        &format!(
            "context use --layerstack {} --branch {}",
            layerstack.display(),
            second_branch.display()
        ),
    );
    assert!(session
        .complete("branch pull ", 12)
        .unwrap()
        .iter()
        .any(|completion| completion.value == branch_id.to_string()));
    assert!(session.complete("branch push ", 12).unwrap().is_empty());
    let plan = session
        .plan(
            &CliSession::parse_line(&format!(
                "branch pull {branch_id} --through 120000000000000000000000000000000000000000000000000000000000000000 --reference"
            ))
            .unwrap(),
        )
        .unwrap();
    assert!(plan
        .summary
        .contains(&format!("project/main ({branch_id})")));
    drop(session);
    let mut json = Vec::new();
    assert_eq!(
        layerfs_cli::invoke(
            &context,
            ["--json", "monitor", "snapshot"].map(Into::into).to_vec(),
            false,
            &mut json,
        )
        .unwrap(),
        0
    );
    let json = String::from_utf8(json).unwrap();
    assert!(json.contains("\"kind\":\"monitor\""));
    assert!(json.contains("\"operations\":["));
    assert!(json.contains("\"databases\":["));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn query_and_diff_events_stream_every_bounded_page() {
    use layerfs_sdk::{
        Client, ConnectionContext, CreateWorkspaceSession, EndWorkspaceMode, LayerStackEndpoint,
        LayerStackInitialization, WorkspacePlacement, WorkspaceProjection,
    };

    let root = std::env::temp_dir().join(format!(
        "layerfs-v2-cli-stream-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let context = root.join("context");
    let layerstack_path = root.join("layerstack.sqlite");
    let branch_path = root.join("branch.sqlite");
    let authority = LayerStackStore::create(&layerstack_path).unwrap();
    let branches = BranchStore::create(&branch_path, authority.store_id()).unwrap();
    drop(branches);
    drop(authority);
    let session = CliSession::open(&context).unwrap();
    finish(
        &session,
        &format!(
            "context use --layerstack {} --branch {}",
            layerstack_path.display(),
            branch_path.display()
        ),
    );

    let authority = Arc::new(LayerStackStore::connect(&layerstack_path).unwrap());
    let branches = BranchStore::connect(&branch_path, authority.store_id()).unwrap();
    let client = Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(authority.clone()),
        branches,
    })
    .unwrap();
    let initialized = client
        .initialize_layerstack(
            EntityName::new("project-000").unwrap(),
            LayerStackInitialization::Empty,
        )
        .unwrap();
    for index in 1..513 {
        authority
            .initialize_layerstack(
                EntityName::new(format!("project-{index:03}")).unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap();
    }
    client
        .pull_layer(initialized.genesis_layer_id, RemotePlacement::Reference)
        .unwrap();
    let branch = client
        .fork_branch(
            EntityName::new("main").unwrap(),
            LocalForkSource::Layer {
                layer_id: initialized.genesis_layer_id,
            },
        )
        .unwrap();
    let mount = root.join("workspace");
    let workspace = client
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Host {
                root: mount.clone(),
            },
            projection: Some(WorkspaceProjection::Materialize),
        })
        .unwrap();
    for index in 0..140 {
        std::fs::write(mount.join(format!("file-{index:03}")), index.to_string()).unwrap();
    }
    client.commit_workspace_session(workspace).unwrap();
    client
        .end_workspace_session(workspace, EndWorkspaceMode::Clean)
        .unwrap();
    client.push_branch(branch).unwrap();
    let layer = match client.add_layer(branch).unwrap() {
        layerfs_sdk::AddLayerResult::Added { layer_id } => layer_id,
        result => panic!("unexpected Add result: {result:?}"),
    };
    client
        .pull_layer(layer, RemotePlacement::Reference)
        .unwrap();
    drop(client);
    drop(authority);

    let handle = session
        .execute(
            CliSession::parse_line(&format!(
                "layerstack diff --from {} --to {layer}",
                initialized.genesis_layer_id
            ))
            .unwrap(),
        )
        .unwrap();
    let mut entries = 0;
    let mut pages = 0;
    while let Some(event) = handle.next_event().unwrap() {
        if let CliEvent::Diff(page) = event {
            assert!(page.entries.len() <= 128);
            entries += page.entries.len();
            pages += 1;
        }
    }
    assert!(entries > 128);
    assert!(pages > 1);

    let handle = session
        .execute(CliSession::parse_line("query authority-layerstacks").unwrap())
        .unwrap();
    let mut records = 0;
    let mut pages = 0;
    while let Some(event) = handle.next_event().unwrap() {
        if let CliEvent::Snapshot(page) = event {
            assert!(page.items.len() <= 512);
            records += page.items.len();
            pages += 1;
        }
    }
    assert_eq!(records, 513);
    assert_eq!(pages, 2);

    std::fs::remove_dir_all(root).unwrap();
}

fn finish(session: &CliSession, command: &str) {
    let handle = session
        .execute(CliSession::parse_line(command).unwrap())
        .unwrap();
    while let Some(event) = handle.next_event().unwrap() {
        if let CliEvent::Finished(result) = event {
            result.unwrap();
        }
    }
}

fn finished_text(session: &CliSession, command: &str) -> String {
    let handle = session
        .execute(CliSession::parse_line(command).unwrap())
        .unwrap();
    while let Some(event) = handle.next_event().unwrap() {
        if let CliEvent::Finished(result) = event {
            return match result.unwrap_or_else(|error| panic!("{command}: {error}")) {
                layerfs_cli::CommandResult::Text(value) => value,
                result => panic!("unexpected command result: {result:?}"),
            };
        }
    }
    panic!("missing command result")
}
