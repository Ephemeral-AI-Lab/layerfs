use std::process::{Command, Output};

#[test]
fn standalone_human_and_json_carry_the_same_semantic_result_and_help_is_local() {
    let root = run_dir();
    let context = root.join("context");
    let layer = root.join("layer.sqlite");

    let help = Command::new(env!("CARGO_BIN_EXE_layerfs"))
        .env("LAYERFS_CONTEXT", root.join("unused"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage: layerfs"));

    success(run(
        &context,
        &["db", "create", "layer", layer.to_str().unwrap()],
    ));
    let human = success(run(&context, &["layer", "init", "--empty"]));
    let layer_id = human
        .lines()
        .find_map(|line| line.strip_prefix("FINISHED CREATED layer "))
        .unwrap();
    let json = success(run(&context, &["--json", "layer", "show", layer_id]));
    assert!(json
        .lines()
        .all(|line| line.starts_with('{') && line.ends_with('}')));
    assert!(json.contains("\"schema_version\":1"));
    assert!(json.contains("\"event\":\"snapshot\""));
    assert!(json.contains("\"event\":\"invocation_receipt\""));
    assert!(json.contains(layer_id));
    assert!(human.contains(layer_id));
    assert!(human.contains("INVOCATION operation="));
    let operation_ids = json
        .lines()
        .filter_map(|line| {
            line.split_once("\"operation_id\":\"")
                .and_then(|(_, tail)| tail.split_once('"').map(|(id, _)| id))
        })
        .collect::<Vec<_>>();
    assert!(operation_ids.len() >= 2);
    assert!(operation_ids
        .iter()
        .all(|operation_id| *operation_id == operation_ids[0]));

    let runtime = layerfs_cli::runtime_location(&context).unwrap();
    let pid = std::fs::read_to_string(runtime.join("host.pid")).unwrap();
    let pid = pid.trim();
    let _ = Command::new("kill").arg(pid).status();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while process_exists(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if runtime.exists() {
        std::fs::remove_dir_all(runtime).unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
}

fn process_exists(pid: &str) -> bool {
    Command::new("ps")
        .args(["-p", pid])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run(context: &std::path::Path, arguments: &[&str]) -> Output {
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

fn run_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "layerfs-cli-render-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
