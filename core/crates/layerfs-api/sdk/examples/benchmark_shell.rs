//! Public SDK driver for the #243 ordinary Workspace shell scenario.
use layerfs_sdk::{
    CommitOutcomeWire, HistoryMode, Project, ProjectApi, SandboxApi, Server, ServerConfig,
    WorkspaceApi, WorkspaceError,
};
use layerfs_telemetry::{
    output::{Identity, OutputConfig},
    runtime::{Configuration, MonitorConfig, Runtime},
};
use std::{collections::BTreeMap, fmt::Write as _, time::Instant};

struct Case(BTreeMap<String, String>);
impl Case {
    fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut values = BTreeMap::new();
        for line in std::fs::read_to_string(path)?.lines() {
            let (key, value) = line.split_once('=').ok_or("invalid case line")?;
            if values.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err("duplicate case key".into());
            }
        }
        Ok(Self(values))
    }
    fn get(&self, key: &str) -> Result<&str, Box<dyn std::error::Error>> {
        self.0
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {key}").into())
    }
    fn bytes(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let value = self.get(key)?;
        if value.len() % 2 != 0 {
            return Err("odd hex width".into());
        }
        (0..value.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(Into::into))
            .collect()
    }
}
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}
fn key(text: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let value: Vec<u8> = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16))
        .collect::<Result<_, _>>()?;
    Ok(value.try_into().map_err(|_| "cursor key width")?)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("mode case store history image required".into());
    }
    let seed = args[1] == "seed";
    if !seed && args[1] != "run" {
        return Err("unknown mode".into());
    }
    let case = Case::load(&args[2])?;
    let expected_failure = !seed && case.get("expected_failure")? == "1";
    let command = String::from_utf8(case.bytes("command_hex")?)?;
    let run: u128 = case.get("telemetry_run")?.parse()?;
    let runtime = Runtime::start(Configuration {
        enabled: !seed,
        timing: !seed,
        monitor: MonitorConfig {
            cpu: true,
            memory: true,
            interval_ms: 10,
            history: 600,
            windows: 32,
        },
        output: OutputConfig::forward(),
        identity: Identity {
            run,
            pid: std::process::id(),
            role: 1,
            namespace: 7,
        },
    })?;
    let server = Server::open(ServerConfig {
        store_path: args[3].clone().into(),
        history_path: args[4].clone().into(),
        binding_key: b"layerfs-bench-pro".to_vec(),
        incarnation: 1,
        cursor_key: key(&std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?)?,
        history: HistoryMode::OpenWritable,
        service_host: "host.docker.internal".into(),
        runtime: runtime.clone(),
        telemetry_run: Some(run),
    })?;
    server.listen()?;
    let owner = server.owner()?;
    let workspaces = WorkspaceApi::new(&owner);
    let sandboxes = SandboxApi::new(&owner);
    let project = Project {
        id: case
            .bytes("project_id")?
            .try_into()
            .map_err(|_| "project id width")?,
        genesis_layer: case
            .bytes("genesis_layer")?
            .try_into()
            .map_err(|_| "layer id width")?,
        root: case
            .bytes("genesis_root")?
            .try_into()
            .map_err(|_| "root id width")?,
        root_serial: case.get("genesis_root_serial")?.parse()?,
    };
    let branch = if seed {
        ProjectApi::new(&server)
            .fork(
                &project,
                case.bytes("branch_body")?
                    .try_into()
                    .map_err(|_| "branch body width")?,
                "main",
            )?
            .id
    } else {
        case.bytes("branch_id")?
            .try_into()
            .map_err(|_| "branch id width")?
    };
    let sandbox = match sandboxes.create(
        &args[5],
        &format!(
            "shell243-{}-{}",
            case.get("scenario_id")?,
            std::process::id()
        ),
    ) {
        Ok(value) => value,
        Err(error) => {
            let cleanup = error.sandbox.map(|id| sandboxes.delete(id));
            println!("RECEIPT\t{{\"status\":\"FAIL\",\"stage\":\"sandbox_create\",\"detail\":{:?},\"sandbox_delete_ok\":{}}}", format!("{error:?}"), cleanup.as_ref().is_some_and(Result::is_ok));
            drop(owner);
            server.shutdown();
            return Err("sandbox create".into());
        }
    };
    let mount = match workspaces.mount(sandbox, &project, branch, None) {
        Ok(value) => value,
        Err(error) => {
            let retained = match &error {
                WorkspaceError::UncertainMount { id, .. } | WorkspaceError::Retained { id, .. } => {
                    Some(id)
                }
                _ => None,
            };
            let unmount = retained.map(|id| workspaces.unmount(id));
            let delete = sandboxes.delete(sandbox);
            println!("RECEIPT\t{{\"status\":\"FAIL\",\"stage\":\"workspace_mount\",\"detail\":{:?},\"unmount_ok\":{},\"sandbox_delete_ok\":{}}}", format!("{error:?}"), unmount.as_ref().is_some_and(Result::is_ok), delete.is_ok());
            drop(owner);
            server.shutdown();
            return Err("workspace mount".into());
        }
    };
    let mut exec_ns = 0;
    let mut commit_ns = 0;
    let mut commit_called = false;
    let start = Instant::now();
    let (outcome, diagnostic) = runtime
        .recorder()
        .run(243_000, "sdk.shell_package", |scope| {
            let exec = scope
                .child("exec")
                .run(|_| {
                    let began = Instant::now();
                    let result = workspaces.exec(&mount.id, &command);
                    exec_ns = began.elapsed().as_nanos();
                    result
                })
                .map_err(|error| format!("exec: {error:?}"))?;
            if exec.stdout_truncated || exec.stderr_truncated {
                return Err("truncated Exec output".to_string());
            }
            if expected_failure {
                return match exec.exit_status {
                    Some(code) if code != 0 => Ok(None),
                    other => Err(format!("failure case returned {other:?}")),
                };
            }
            if exec.exit_status != Some(0) {
                return Err(format!(
                    "Exec status {:?}: {}",
                    exec.exit_status,
                    String::from_utf8_lossy(&exec.stderr)
                ));
            }
            commit_called = true;
            scope
                .child("commit")
                .run(|_| {
                    let began = Instant::now();
                    let result = workspaces.commit(&mount.id);
                    commit_ns = began.elapsed().as_nanos();
                    result
                })
                .map(Some)
                .map_err(|error| format!("commit: {error:?}"))
        });
    let operation_ns = start.elapsed().as_nanos();
    runtime.publish(diagnostic);
    let (mut status, mut detail, mut head_commit) = ("COMPLETE", String::new(), String::new());
    match outcome {
        Ok(Some(report)) => match report.outcome {
            CommitOutcomeWire::Committed(record) => head_commit = hex(&record.commit),
            other => {
                status = "FAIL";
                detail = format!("commit outcome {other:?}");
            }
        },
        Ok(None) if expected_failure => (),
        Ok(None) => {
            status = "FAIL";
            detail = "missing commit".into();
        }
        Err(error) => {
            status = "FAIL";
            detail = error;
        }
    }
    let cleanup_start = Instant::now();
    let post_status = workspaces.status(&mount.id);
    let unmount = workspaces.unmount(&mount.id);
    let (delete, capture) = sandboxes.delete_with_logs(sandbox, &mut std::io::stderr());
    let cleanup_ns = cleanup_start.elapsed().as_nanos();
    if unmount.is_err() || delete.is_err() || post_status.is_err() {
        status = "FAIL";
        detail = format!("{detail}; status={post_status:?} unmount={unmount:?} delete={delete:?}");
    }
    let counts = post_status
        .as_ref()
        .map(|value| {
            value
                .projection
                .iter()
                .map(|(name, count)| format!("{name}={count}"))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let range_bytes = post_status
        .as_ref()
        .map(|value| value.range_accepted_payload_bytes)
        .map_or("null".to_string(), |value| value.to_string());
    println!("RECEIPT\t{{\"schema\":\"issue243-shell-driver-v1\",\"status\":\"{status}\",\"detail\":{:?},\"mode\":{:?},\"scenario_id\":{:?},\"branch_id\":{:?},\"head_commit\":{:?},\"commit_called\":{commit_called},\"exec_ns\":{exec_ns},\"commit_ns\":{commit_ns},\"operation_ns\":{operation_ns},\"cleanup_ns\":{cleanup_ns},\"projection_counts\":{:?},\"range_accepted_payload_bytes\":{range_bytes},\"unmount_ok\":{},\"sandbox_delete_ok\":{},\"daemon_log_attempted\":{},\"daemon_log_bytes\":{},\"daemon_log_truncated\":{},\"daemon_log_error\":{:?}}}",
        detail, args[1], case.get("scenario_id")?, hex(&branch), head_commit, counts,
        unmount.is_ok(), delete.is_ok(), capture.attempted, capture.bytes, capture.truncated,
        format!("{:?}", capture.error));
    drop(owner);
    server.shutdown();
    if status != "COMPLETE" {
        return Err(detail.into());
    }
    Ok(())
}
