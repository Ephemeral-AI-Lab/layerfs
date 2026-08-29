use layerfs_cli::{
    CliError, CliEvent, CliSession, CommandResult, StoreQuery, StoreScope, StoreSnapshot,
    ViewQuery, ViewSnapshot,
};
use layerfs_sdk::{FactKind, MonitorScope};

#[test]
fn non_ratatui_client_consumes_plan_completion_events_paging_and_interruption() {
    let root = run_dir();
    let context = root.join("context");
    let runtime = layerfs_cli::runtime_location(&context).unwrap();
    let session = CliSession::open(&context).unwrap();
    let layer_path = root.join("layer.sqlite");
    let branch_path = root.join("branch.sqlite");
    let output = root.join("workspace");

    let ViewSnapshot::Topology(topology) = session.snapshot(ViewQuery::Topology).unwrap() else {
        panic!("empty topology")
    };
    assert!(topology.is_empty());

    run(
        &session,
        &format!("db create layer {}", layer_path.display()),
    )
    .1
    .unwrap();
    let (_, layer) = id(run(&session, "layer init --empty").1);
    let ViewSnapshot::Store(StoreSnapshot::Page {
        facts: first_facts,
        next,
    }) = session
        .snapshot(ViewQuery::Store(StoreQuery::Page {
            scope: StoreScope::Layer,
            kind: FactKind::Layer,
            after: None,
            limit: 1,
        }))
        .unwrap()
    else {
        panic!("Layer page")
    };
    assert_eq!(first_facts.len(), 1);
    let ViewSnapshot::Store(StoreSnapshot::Page {
        facts: second_facts,
        ..
    }) = session
        .snapshot(ViewQuery::Store(StoreQuery::Page {
            scope: StoreScope::Layer,
            kind: FactKind::Layer,
            after: next,
            limit: 1,
        }))
        .unwrap()
    else {
        panic!("Layer continuation")
    };
    assert!(second_facts.is_empty());
    let (list_events, list_result) = run(&session, "layer list");
    list_result.unwrap();
    assert!(list_events
        .iter()
        .any(|event| matches!(event, CliEvent::Snapshot { .. })));
    run(&session, &format!("layer show {layer}")).1.unwrap();
    run(
        &session,
        &format!("db create branch {}", branch_path.display()),
    )
    .1
    .unwrap();
    let (_, branch) = id(run(&session, &format!("branch create --from {layer}")).1);

    let command = CliSession::parse_line(&format!(
        "workspace create {branch} --at {} --projection materialize",
        output.display()
    ))
    .unwrap();
    let plan = session.plan(&command).unwrap();
    assert!(!plan.route.is_empty());
    let (_, result) = events(session.execute(command).unwrap());
    let CommandResult::Workspace(workspace) = result.unwrap() else {
        panic!("Workspace result")
    };
    assert!(matches!(
        run(
            &session,
            &format!("db disconnect {}", branch_path.display())
        )
        .1,
        Err(CliError::WorkspaceBusy)
    ));

    let completions = session.complete("work", 4).unwrap();
    assert_eq!(completions[0].value, "workspace");
    let ViewSnapshot::Topology(topology) = session.snapshot(ViewQuery::Topology).unwrap() else {
        panic!("topology")
    };
    assert_eq!(topology.len(), 2);

    let (_, execution) = id(run(
        &session,
        &format!(
            "workspace exec {} -- /bin/sh -c 'printf frontend; printf final > from-cli'",
            workspace.id
        ),
    )
    .1);
    let execution = execution.parse().unwrap();
    let output_page = loop {
        let ViewSnapshot::Output(page) = session
            .snapshot(ViewQuery::Output {
                execution_id: execution,
                after: 0,
                follow: true,
            })
            .unwrap()
        else {
            panic!("output")
        };
        if page.exited {
            break page;
        }
    };
    assert!(output_page
        .chunks
        .iter()
        .any(|chunk| chunk.bytes == b"frontend"));
    let (follow_events, follow_result) =
        run(&session, &format!("workspace output {execution} --follow"));
    follow_result.unwrap();
    assert!(follow_events.iter().any(|event| matches!(
        event,
        CliEvent::Output { bytes, .. } if bytes == b"frontend"
    )));

    let (_, running) = id(run(
        &session,
        &format!(
            "workspace exec {} -- /bin/sh -c 'printf live; sleep 30'",
            workspace.id
        ),
    )
    .1);
    let interrupted = session
        .execute(CliSession::parse_line(&format!("workspace output {running} --follow")).unwrap())
        .unwrap();
    assert!(matches!(
        interrupted.next_event().unwrap(),
        Some(CliEvent::Started { .. })
    ));
    interrupted.interrupt().unwrap();
    let (_, result) = events(interrupted);
    assert!(matches!(result, Err(CliError::Interrupted)));
    run(&session, &format!("workspace stop {running}"))
        .1
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let ViewSnapshot::Output(page) = session
            .snapshot(ViewQuery::Output {
                execution_id: running.parse().unwrap(),
                after: 0,
                follow: true,
            })
            .unwrap()
        else {
            panic!("stopped output")
        };
        if page.exited {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "stop did not drain");
    }

    run(&session, &format!("workspace commit {}", workspace.id))
        .1
        .unwrap();
    let ViewSnapshot::Workspace(detail) = session
        .snapshot(ViewQuery::Workspace(workspace.id))
        .unwrap()
    else {
        panic!("Workspace detail")
    };
    assert_eq!(detail.executions.len(), 2);
    let ViewSnapshot::WorkspaceDiff(diff) = session
        .snapshot(ViewQuery::WorkspaceDiff(workspace.id))
        .unwrap()
    else {
        panic!("Workspace diff")
    };
    assert!(diff.dirty);
    let ViewSnapshot::Monitor(_) = session
        .snapshot(ViewQuery::Monitor(MonitorScope::Process))
        .unwrap()
    else {
        panic!("Monitor")
    };

    let list = session
        .execute(CliSession::parse_line("workspace list").unwrap())
        .unwrap();
    let (list_events, _) = events(list);
    assert!(list_events
        .iter()
        .any(|event| matches!(event, CliEvent::Snapshot { .. })));

    run(&session, &format!("workspace end {}", workspace.id))
        .1
        .unwrap();
    run(
        &session,
        &format!("db disconnect {}", branch_path.display()),
    )
    .1
    .unwrap();
    run(&session, &format!("db disconnect {}", layer_path.display()))
        .1
        .unwrap();
    drop(session);
    stop_host(&runtime);
    std::fs::remove_dir_all(root).unwrap();
}

fn run(session: &CliSession, line: &str) -> (Vec<CliEvent>, Result<CommandResult, CliError>) {
    events(
        session
            .execute(CliSession::parse_line(line).unwrap())
            .unwrap(),
    )
}

fn events(
    handle: layerfs_cli::OperationHandle,
) -> (Vec<CliEvent>, Result<CommandResult, CliError>) {
    let mut events = Vec::new();
    loop {
        let Some(event) = handle.next_event().unwrap() else {
            panic!("missing Finished")
        };
        if let CliEvent::Finished { result, .. } = &event {
            let result = result.clone();
            events.push(event);
            return (events, result);
        }
        events.push(event);
    }
}

fn id(result: Result<CommandResult, CliError>) -> (String, String) {
    match result.unwrap() {
        CommandResult::Id { kind, id } => (kind, id),
        CommandResult::InitializedLayer { layer_id, .. } => ("layer".to_owned(), layer_id),
        result => panic!("ID result: {result:?}"),
    }
}

fn run_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-cli-frontend-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn stop_host(runtime: &std::path::Path) {
    let pid = std::fs::read_to_string(runtime.join("host.pid"))
        .unwrap()
        .trim()
        .to_owned();
    let _ = std::process::Command::new("kill").arg(&pid).status();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while process_exists(&pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if runtime.exists() {
        std::fs::remove_dir_all(runtime).unwrap();
    }
}

fn process_exists(pid: &str) -> bool {
    std::process::Command::new("ps")
        .args(["-p", pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
