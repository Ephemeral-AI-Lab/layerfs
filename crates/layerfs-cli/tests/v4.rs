use layerfs_cli::{CliEvent, CliSession};
use std::ffi::OsStr;
use std::process::Command;

#[test]
fn exact_replacement_grammar_parses_and_deleted_topology_is_rejected() {
    let branch = "1100000000007000800000000000000000";
    let commit = "120000000000000000000000000000000000000000000000000000000000000000";
    let layer = "320000000000000000000000000000000000000000000000000000000000000000";
    let stack = "3100000000007000800000000000000000";
    for line in [
        "db create /tmp/store.db",
        "db connect /tmp/store.db",
        "context use --store /tmp/store.db",
        "context show",
        "container create --name runtime --image layerfs-runtime:dev",
        "container create --name runtime --image layerfs-runtime:dev --memory-mib 512 --cpus 2 --pids-limit 512",
        "container start runtime",
        "container status runtime",
        "container stop runtime",
        "container remove runtime",
        "layerstack init --name project --empty",
        "layerstack init --name imported /tmp/root",
        &format!("layerstack diff --from {layer} --to {layer}"),
        &format!("layerstack add {branch}"),
        &format!("branch fork --name main --layer {layer}"),
        &format!("branch fork --name rollout --branch {branch} --commit {commit}"),
        &format!("branch diff --branch {branch} --from {commit} --to {commit}"),
        &format!("branch diff --branch {branch} --layer {layer}"),
        &format!("workspace create {branch} --at /tmp/work --projection materialize"),
        "workspace exec w:00000000000000000000000000000000 -- /bin/echo ok",
        "workspace shell w:00000000000000000000000000000000",
        "workspace output x:00000000000000000000000000000000 --follow",
        "workspace stop x:00000000000000000000000000000000",
        "workspace conflicts w:00000000000000000000000000000000 --after 1",
        "workspace resolve w:00000000000000000000000000000000 c:00000000000000000000000000000000 --working-tree",
        "workspace commit w:00000000000000000000000000000000",
        "workspace end w:00000000000000000000000000000000 --discard",
        "monitor snapshot",
        "monitor analyze-dedup",
        "query layerstacks",
        "query layers",
        &format!("query branches --layerstack {stack}"),
        "query commits",
        "query workspaces",
        "query monitor",
    ] {
        CliSession::parse_line(line).unwrap_or_else(|error| panic!("{line}: {error}"));
    }

    for deleted in [
        "db create layerstack /tmp/store.db",
        "db create branch /tmp/branch.db --parent /tmp/store.db",
        "db connect layerstack /tmp/store.db",
        "context use --layerstack /tmp/store.db --branch /tmp/branch.db",
        &format!("layerstack pull --through {layer} --reference"),
        &format!("layerstack pull --through {layer} --replica"),
        &format!("branch pull {branch} --through {commit} --reference"),
        &format!("branch push {branch}"),
        "query authority-layerstacks",
        "query authority-branches",
    ] {
        assert!(CliSession::parse_line(deleted).is_err(), "{deleted}");
    }
}

#[cfg(unix)]
#[test]
fn standalone_cli_keeps_one_sdk_client_through_workspace_end() {
    let root = temp();
    let context = root.join("context");
    let store = root.join("store.sqlite");
    let view = root.join("view");
    let binary = env!("CARGO_BIN_EXE_layerfs");

    assert_success(binary, &context, ["db", "create", path(&store)]);
    assert_success(
        binary,
        &context,
        ["context", "use", "--store", path(&store)],
    );
    let initialized = assert_success(
        binary,
        &context,
        ["layerstack", "init", "--name", "project", "--empty"],
    );
    let layer = between(&initialized, "genesis_layer_id: LayerId(", ")");
    let branch = assert_success(
        binary,
        &context,
        ["branch", "fork", "--name", "main", "--layer", layer],
    );
    let workspace = assert_success(
        binary,
        &context,
        [
            "workspace",
            "create",
            branch.trim(),
            "--at",
            path(&view),
            "--projection",
            "materialize",
        ],
    );
    assert!(workspace.trim().starts_with("w:"));
    let execution = assert_success(
        binary,
        &context,
        [
            "workspace",
            "exec",
            workspace.trim(),
            "--",
            "/bin/sh",
            "-c",
            "printf world > hello.txt; printf done",
        ],
    );
    assert!(execution.trim().starts_with("x:"));
    let output = assert_success(
        binary,
        &context,
        ["workspace", "output", execution.trim(), "--follow"],
    );
    assert!(output.contains("exited: true"));
    assert!(output.contains("exit_code: Some(0)"));
    assert!(
        assert_success(binary, &context, ["workspace", "commit", workspace.trim()],)
            .contains("Created")
    );
    assert!(assert_success(binary, &context, ["query", "commits"]).contains("CommitRecord"));
    assert_success(binary, &context, ["workspace", "end", workspace.trim()]);

    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!view.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn one_store_context_and_query_execute_without_a_tui() {
    let root = temp();
    let context = root.join("context");
    let store = root.join("store.sqlite");
    let session = CliSession::open(&context).unwrap();
    for line in [
        format!("db create {}", store.display()),
        format!("context use --store {}", store.display()),
        "layerstack init --name project --empty".to_owned(),
        "query layerstacks".to_owned(),
        "monitor snapshot".to_owned(),
    ] {
        let handle = session
            .execute(CliSession::parse_line(&line).unwrap())
            .unwrap();
        assert_eq!(handle.next_event().unwrap(), Some(CliEvent::Started));
        assert!(matches!(
            handle.next_event().unwrap(),
            Some(CliEvent::Completed(_))
        ));
    }
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}

fn temp() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-cli-v4-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[cfg(unix)]
fn assert_success<I, S>(binary: &str, context: &std::path::Path, arguments: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(binary)
        .args(arguments)
        .env("LAYERFS_CONTEXT", context)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[cfg(unix)]
fn between<'a>(value: &'a str, prefix: &str, suffix: &str) -> &'a str {
    value
        .split_once(prefix)
        .and_then(|(_, value)| value.split_once(suffix).map(|(value, _)| value))
        .unwrap()
}

#[cfg(unix)]
fn path(path: &std::path::Path) -> &str {
    path.to_str().unwrap()
}
