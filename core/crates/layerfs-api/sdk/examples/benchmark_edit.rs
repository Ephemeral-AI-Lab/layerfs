//! Release SDK driver for one registered #232 Workspace Exec/FUSE edit case.
//!
//! Every product operation goes through the public SDK: `Server::open` over the
//! case's own byte copy, `ProjectApi::fork`, `SandboxApi::create`,
//! `WorkspaceApi::{mount,exec,commit,status,unmount}` and
//! `SandboxApi::delete`. The measured boundary is one caller operation with an
//! `edit` child around `WorkspaceApi::exec` and a `commit` child around
//! `WorkspaceApi::commit`; mount, status, unmount, sandbox create/delete and
//! preparation are outside it and reported separately.
use layerfs_sdk::{
    CommitOutcomeWire, HistoryMode, Project, ProjectApi, SandboxApi, Server, ServerConfig,
    WorkspaceApi,
};
use layerfs_telemetry::{
    output::{Identity, OutputConfig},
    runtime::{Configuration, MonitorConfig, Runtime},
};
use std::{collections::BTreeMap, fmt::Write as _, time::Instant};

/// One caller root with an `edit` and a `commit` child, and nothing else.
const OPERATION_KEY: u64 = 232_000;

struct Case {
    fields: BTreeMap<String, String>,
}

impl Case {
    /// The case arrives as one argument, so the driver opens no host path of its
    /// own: every byte it sees is either declared input or the mounted Workspace.
    fn load(spec: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut fields = BTreeMap::new();
        for entry in spec.split(';') {
            if entry.is_empty() {
                continue;
            }
            let (key, value) = entry.split_once('=').ok_or("case row without '='")?;
            fields.insert(key.to_string(), value.to_string());
        }
        Ok(Self { fields })
    }
    fn get(&self, key: &str) -> Result<&str, Box<dyn std::error::Error>> {
        self.fields
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("case field {key}").into())
    }
    fn number(&self, key: &str) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self.get(key)?.parse()?)
    }
    fn hex(&self, key: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let text = self.get(key)?;
        if text.len() % 2 != 0 {
            return Err("odd hex width".into());
        }
        (0..text.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16).map_err(Into::into))
            .collect()
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("string write");
    }
    text
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("case-spec store history image sandbox-name required".into());
    }
    let case = Case::load(&args[1])?;
    let (store_path, history_path) = (args[2].clone(), args[3].clone());
    let image = args[4].clone();
    let sandbox_name = args[5].clone();
    let run: u128 = case
        .get("telemetry_run")
        .unwrap_or("1")
        .parse()
        .unwrap_or(1);
    let runtime = Runtime::start(Configuration {
        enabled: true,
        timing: true,
        monitor: MonitorConfig {
            cpu: true,
            memory: true,
            // The crate's documented minimum sampling interval. A short Edit or
            // Commit may therefore have no valid resource window; that is
            // reported as UNAVAILABLE rather than extended with sleep.
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
        store_path: store_path.clone().into(),
        history_path: history_path.clone().into(),
        binding_key: b"layerfs-bench-pro".to_vec(),
        incarnation: 1,
        cursor_key: {
            let text = std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?;
            case_bytes(&text)?
        },
        history: HistoryMode::OpenWritable,
        service_host: "host.docker.internal".into(),
        runtime: runtime.clone(),
        telemetry_run: Some(run),
    })?;
    let endpoint = server.listen()?;
    let owner = server.owner()?;
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let workspaces = WorkspaceApi::new(&owner);
    let project = Project {
        id: {
            let raw = case.hex("project_id")?;
            raw.try_into().map_err(|_| "project id width")?
        },
        genesis_layer: {
            let raw = case.hex("genesis_layer")?;
            raw.try_into().map_err(|_| "genesis layer width")?
        },
        root: {
            let raw = case.hex("genesis_root")?;
            raw.try_into().map_err(|_| "genesis root width")?
        },
        root_serial: case.number("genesis_root_serial")?,
    };

    // Per-case preparation, outside the operation timer but reported.
    let prepared = Instant::now();
    let branch = projects.fork(
        &project,
        {
            let raw = case.hex("branch_body")?;
            raw.try_into().map_err(|_| "branch body width")?
        },
        "main",
    )?;
    let sandbox = match sandboxes.create(&image, &sandbox_name) {
        Ok(id) => id,
        Err(error) => {
            println!(
                "RECEIPT\t{{\"status\":\"FAIL\",\"stage\":\"sandbox_create\",\"sandbox\":\"{}\",\"retained\":{},\"cause\":\"{:?}\",\"preparation_ns\":{}}}",
                error.sandbox.map(|id| id.to_string()).unwrap_or_default(),
                error.retained,
                error.cause,
                prepared.elapsed().as_nanos()
            );
            return Err("sandbox create failed".into());
        }
    };
    let mount = workspaces.mount(sandbox, &project, branch.id, None)?;
    let preparation_ns = prepared.elapsed().as_nanos();

    // The one measured caller operation: Exec, its output check, then Commit.
    let command = case.get("command")?.to_string();
    let mut edit_ns = 0u128;
    let mut commit_ns = 0u128;
    let mut status = "FAIL";
    let detail;
    let started = Instant::now();
    let (result, diagnostic) =
        runtime
            .recorder()
            .run(OPERATION_KEY, "sdk.edit_commit.fuse", |scope| {
                let edit = scope.child("edit").run(|_| {
                    let child = Instant::now();
                    let result = workspaces.exec(&mount.id, &command);
                    edit_ns = child.elapsed().as_nanos();
                    result
                });
                match edit {
                    Ok(exec)
                        if exec.exit_status == Some(0)
                            && !exec.stdout_truncated
                            && !exec.stderr_truncated => {}
                    Ok(exec) => {
                        return Err(format!(
                            "exec status {:?} truncated stdout={} stderr={} stderr={}",
                            exec.exit_status,
                            exec.stdout_truncated,
                            exec.stderr_truncated,
                            String::from_utf8_lossy(&exec.stderr).escape_debug()
                        ))
                    }
                    Err(error) => return Err(format!("exec failed: {error:?}")),
                }
                let commit = scope.child("commit").run(|_| {
                    let child = Instant::now();
                    let result = workspaces.commit(&mount.id);
                    commit_ns = child.elapsed().as_nanos();
                    result
                });
                match commit {
                    Ok(report) => Ok(report),
                    Err(error) => Err(format!("commit failed: {error:?}")),
                }
            });
    let edit_commit_ns = started.elapsed().as_nanos();
    runtime.publish(diagnostic);
    let mut head_commit = String::new();
    match result {
        Ok(report) => {
            let outcome = match report.outcome {
                CommitOutcomeWire::Committed(record) => {
                    head_commit = hex(&record.commit);
                    "COMMITTED".to_string()
                }
                other => format!("{other:?}"),
            };
            status = "COMPLETE";
            detail = format!("commit_outcome={outcome} revision={}", report.revision);
        }
        Err(error) => detail = error,
    }
    // Untimed cleanup and post-timer observation, each separately reported.
    let cleanup_started = Instant::now();
    let post_status = workspaces.status(&mount.id);
    let unmount = workspaces.unmount(&mount.id);
    let delete = sandboxes.delete(sandbox);
    let cleanup_ns = cleanup_started.elapsed().as_nanos();
    let projection = post_status
        .as_ref()
        .map(|value| joined(&value.projection))
        .unwrap_or_default();
    let byte_totals = post_status
        .as_ref()
        .map(|value| joined(&value.projection_bytes))
        .unwrap_or_default();
    let size_histogram = post_status
        .as_ref()
        .map(|value| joined(&value.projection_histogram))
        .unwrap_or_default();
    let upstream = post_status
        .as_ref()
        .map(|value| value.upstream_calls)
        .unwrap_or(0);
    let receipt =
        format!(
        "{{\"schema\":\"core-fs-bench-pro-exec-fuse-edit-performance-v1\",\"status\":\"{status}\",\
\"detail\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"route\":\"{}\",\
\"operation_contract_id\":\"workspace-exec-fuse-edit-commit-v1\",\
\"operation_surface\":\"workspace-posix-fuse\",\"operation_entrypoint\":\"WorkspaceApi::exec\",\
\"acknowledgement_boundary\":\"WorkspaceApi::commit\",\"fixture_bytes\":{},\"edit_start\":{},\
\"delete_len\":{},\"replacement_len\":{},\"replacement_sha256\":\"{}\",\"final_bytes\":{},\
\"g2_target_ms\":{},\"edit_ns\":{edit_ns},\"commit_ns\":{commit_ns},\
\"edit_commit_ns\":{edit_commit_ns},\"preparation_ns\":{preparation_ns},\
\"cleanup_ns\":{cleanup_ns},\"branch_id\":\"{}\",\"head_commit\":\"{head_commit}\",\
\"projection_counts\":\"{projection}\",\"projection_bytes\":\"{byte_totals}\",\"projection_size_histogram\":\"{size_histogram}\",\"upstream_calls\":{upstream},\
\"unmount_ok\":{},\"sandbox_delete_ok\":{},\"sandbox_delete_container_removed\":{},\
\"sandbox_delete_volume_removed\":{},\"store\":\"{}\",\"history\":\"{}\",\
\"image\":\"{}\",\"service_endpoint_port\":{},\"replay\":false}}",
        detail.escape_debug(),
        case.get("family_id")?,
        case.get("scenario_id")?,
        case.get("route")?,
        case.number("fixture_bytes")?,
        case.number("edit_start")?,
        case.number("delete_len")?,
        case.number("replacement_len")?,
        case.get("replacement_sha256")?,
        case.number("final_bytes")?,
        case.get("g2_target_ms")?,
        hex(&branch.id),
        unmount.is_ok(),
        delete.is_ok(),
        delete.as_ref().err().map(|error| error.container_removed).unwrap_or(false),
        delete.as_ref().err().map(|error| error.volume_removed).unwrap_or(false),
        store_path,
        history_path,
        image,
        endpoint.port(),
    );
    println!("RECEIPT\t{receipt}");
    drop(owner);
    server.shutdown();
    if status != "COMPLETE" {
        return Err(format!("measured attempt failed: {detail}").into());
    }
    Ok(())
}

/// Renders labeled counts as the receipt's stable `label=value` list.
fn joined(rows: &[(String, u64)]) -> String {
    rows.iter()
        .map(|(label, value)| format!("{label}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn case_bytes(text: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if text.len() != 64 {
        return Err("key width".into());
    }
    let mut value = [0u8; 32];
    for (index, byte) in value.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)?;
    }
    Ok(value)
}
