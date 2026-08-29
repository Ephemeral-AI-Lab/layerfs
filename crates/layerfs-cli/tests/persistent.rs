use std::process::{Command, Output};

use layerfs_cli::{CliEvent, CliSession, CommandResult};

#[test]
fn separate_processes_reconnect_to_one_ready_host_and_workspace_worker() {
    let root = run_dir();
    let context = root.join("context");
    let layer = root.join("layer.sqlite");
    let branch = root.join("branch.sqlite");
    let workspace = root.join("workspace");

    success(run(&context, ["db", "create", "layer", text(&layer)]));
    let runtime = layerfs_cli::runtime_location(&context).unwrap();
    let pid = std::fs::read_to_string(runtime.join("host.pid"))
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    success(run(&context, ["db", "list"]));
    assert_eq!(
        std::fs::read_to_string(runtime.join("host.pid"))
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap(),
        pid
    );

    let initialized = success(run(&context, ["layer", "init", "--empty"]));
    let layer_id = created_id(&initialized);
    success(run(&context, ["db", "create", "branch", text(&branch)]));
    let created = success(run(&context, ["branch", "create", "--from", &layer_id]));
    let branch_id = created_id(&created);
    let created = success(run(
        &context,
        [
            "workspace",
            "create",
            &branch_id,
            "--at",
            text(&workspace),
            "--projection",
            "materialize",
        ],
    ));
    let workspace_id = created_id(&created);
    let executed = success(run(
        &context,
        [
            "workspace",
            "exec",
            &workspace_id,
            "--",
            "/bin/sh",
            "-c",
            "printf persistent > persisted",
        ],
    ));
    assert!(created_id(&executed).starts_with("x:"));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let committed = run(&context, ["workspace", "commit", &workspace_id]);
        if committed.status.success() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "commit remained busy");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    success(run(&context, ["workspace", "end", &workspace_id]));

    stop_host(&runtime);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_session_reuses_the_process_host() {
    let root = run_dir();
    let context = root.join("context");
    let layer = root.join("layer.sqlite");
    let branch = root.join("branch.sqlite");

    success(run(&context, ["db", "create", "layer", text(&layer)]));
    let runtime = layerfs_cli::runtime_location(&context).unwrap();
    let pid = std::fs::read_to_string(runtime.join("host.pid")).unwrap();

    let session = CliSession::open(&context).unwrap();
    execute(&session, "layer init --empty");
    execute(&session, &format!("db create branch {}", branch.display()));

    let listed = success(run(&context, ["db", "list"]));
    assert!(listed.contains(text(&branch)), "{listed}");
    assert_eq!(
        std::fs::read_to_string(runtime.join("host.pid")).unwrap(),
        pid
    );

    stop_host(&runtime);
    std::fs::remove_dir_all(root).unwrap();
}

fn execute(session: &CliSession, line: &str) -> CommandResult {
    let handle = session
        .execute(CliSession::parse_line(line).unwrap())
        .unwrap();
    loop {
        match handle.next_event().unwrap() {
            Some(CliEvent::Finished { result, .. }) => return result.unwrap(),
            Some(_) => {}
            None => panic!("missing Finished"),
        }
    }
}

fn run<const N: usize>(context: &std::path::Path, arguments: [&str; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_layerfs"))
        .env("LAYERFS_CONTEXT", context)
        .args(arguments)
        .output()
        .unwrap()
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn created_id(output: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix("FINISHED CREATED "))
        .and_then(|line| line.split_whitespace().last())
        .unwrap()
        .to_owned()
}

fn text(path: &std::path::Path) -> &str {
    path.to_str().unwrap()
}

fn run_dir() -> std::path::PathBuf {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "layerfs-cli-persistent-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn stop_host(runtime: &std::path::Path) {
    let pid = std::fs::read_to_string(runtime.join("host.pid"))
        .unwrap()
        .trim()
        .to_owned();
    let _ = Command::new("kill").arg(&pid).status();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while process_exists(&pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if runtime.exists() {
        std::fs::remove_dir_all(runtime).unwrap();
    }
}

fn process_exists(pid: &str) -> bool {
    Command::new("ps")
        .args(["-p", pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
