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
    CommitOutcomeWire, ExecResult, HistoryMode, Project, ProjectApi, SandboxApi, Server,
    ServerConfig, WorkspaceApi, WorkspaceError,
};
use layerfs_telemetry::{
    output::{Identity, OutputConfig},
    runtime::{Configuration, MonitorConfig, Runtime},
};
use std::{collections::BTreeMap, fmt::Write as _, time::Instant};

/// One caller root with an `edit` and a `commit` child, and nothing else.
const OPERATION_KEY: u64 = 232_000;

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct DarwinRusageInfoV2 {
    uuid: [u8; 16],
    user_time: u64,
    system_time: u64,
    package_idle_wakeups: u64,
    interrupt_wakeups: u64,
    pageins: u64,
    wired_size: u64,
    resident_size: u64,
    physical_footprint: u64,
    process_start_time: u64,
    process_exit_time: u64,
    child_user_time: u64,
    child_system_time: u64,
    child_package_idle_wakeups: u64,
    child_interrupt_wakeups: u64,
    child_pageins: u64,
    child_elapsed_time: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
}

#[cfg(target_os = "macos")]
#[link(name = "proc")]
unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
}

#[cfg(target_os = "macos")]
fn process_disk_read_bytes() -> Option<u64> {
    let mut usage = DarwinRusageInfoV2::default();
    let pid = i32::try_from(std::process::id()).ok()?;
    // SAFETY: RUSAGE_INFO_V2 writes its fixed C-layout buffer for this process.
    (unsafe { proc_pid_rusage(pid, 2, std::ptr::from_mut(&mut usage).cast()) } == 0)
        .then_some(usage.disk_read_bytes)
}

#[cfg(not(target_os = "macos"))]
fn process_disk_read_bytes() -> Option<u64> {
    None
}

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

fn confirmed_exec(exec: &ExecResult, expected_splice_output: Option<&str>) -> bool {
    exec.exit_status == Some(0)
        && !exec.stdout_truncated
        && !exec.stderr_truncated
        && expected_splice_output.is_none_or(|expected| exec.stdout == expected.as_bytes())
}

fn confirmed_splice(
    exec: &ExecResult,
    final_bytes: u64,
    operation: &str,
    count: Option<u64>,
) -> Option<(i64, u32)> {
    if exec.exit_status != Some(0) || exec.stdout_truncated || exec.stderr_truncated {
        return None;
    }
    let output = std::str::from_utf8(&exec.stdout).ok()?;
    let prefix = format!(
        "{{\"status\":\"PASS\",\"operation\":\"{operation}\",\"direction\":\"-\",\"final_bytes\":{final_bytes},\"shifted_bytes\":0,\"mtime_seconds\":"
    );
    let (seconds, tail) = output
        .strip_prefix(&prefix)?
        .split_once(",\"mtime_nanoseconds\":")?;
    let expected_tail = count.map_or("}\n".to_string(), |count| {
        format!(",\"edit_count\":{count},\"accepted_bytes\":4096}}\n")
    });
    let nanoseconds = tail.strip_suffix(&expected_tail)?;
    let seconds = seconds.parse::<i64>().ok()?;
    let nanoseconds = nanoseconds.parse::<u32>().ok()?;
    if nanoseconds >= 1_000_000_000
        || output != format!("{prefix}{seconds},\"mtime_nanoseconds\":{nanoseconds}{expected_tail}")
    {
        return None;
    }
    Some((seconds, nanoseconds))
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
    let v3 = case.get("operation_contract_id")? == "workspace-exec-fuse-range-splice-commit-v3";
    let v4 = case.get("operation_contract_id")? == "workspace-exec-fuse-range-splice-commit-v4";
    let v5 =
        case.get("operation_contract_id")? == "workspace-exec-fuse-range-splice-batch-commit-v1";
    let complexity_single = case.get("operation_contract_id")?
        == "workspace-exec-fuse-range-splice-complexity-commit-v1";

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
            let cleanup = error.sandbox.map(|id| sandboxes.delete(id));
            println!(
                "RECEIPT\t{{\"status\":\"FAIL\",\"stage\":\"sandbox_create\",\"sandbox\":\"{}\",\"retained\":{},\"cause\":\"{:?}\",\"sandbox_delete_ok\":{},\"sandbox_delete_result\":\"{}\",\"preparation_ns\":{}}}",
                error.sandbox.map(|id| id.to_string()).unwrap_or_default(),
                error.retained,
                error.cause,
                cleanup.as_ref().is_some_and(Result::is_ok),
                format!("{cleanup:?}").escape_debug(),
                prepared.elapsed().as_nanos()
            );
            drop(owner);
            server.shutdown();
            return Err("sandbox create failed".into());
        }
    };
    let mount = match workspaces.mount(sandbox, &project, branch.id, None) {
        Ok(mount) => mount,
        Err(error) => {
            let mount_id = match &error {
                WorkspaceError::UncertainMount { id, .. } | WorkspaceError::Retained { id, .. } => {
                    Some(id)
                }
                _ => None,
            };
            let preparation_ns = prepared.elapsed().as_nanos();
            let cleanup_started = Instant::now();
            let unmount = mount_id.map(|id| workspaces.unmount(id));
            let delete = sandboxes.delete(sandbox);
            let cleanup_ns = cleanup_started.elapsed().as_nanos();
            println!(
                "RECEIPT\t{{\"status\":\"FAIL\",\"stage\":\"workspace_mount\",\
\"scenario_id\":\"{}\",\"sandbox\":\"{sandbox}\",\"mount_id\":\"{}\",\
\"mount_error\":\"{}\",\"unmount_attempted\":{},\"unmount_ok\":{},\
\"unmount_result\":\"{}\",\"sandbox_delete_ok\":{},\"sandbox_delete_result\":\"{}\",\
\"preparation_ns\":{},\"cleanup_ns\":{cleanup_ns}}}",
                case.get("scenario_id")?,
                mount_id.map_or("", |id| id.0.as_str()),
                format!("{error:?}").escape_debug(),
                unmount.is_some(),
                unmount.as_ref().is_some_and(Result::is_ok),
                format!("{unmount:?}").escape_debug(),
                delete.is_ok(),
                format!("{delete:?}").escape_debug(),
                preparation_ns,
            );
            drop(owner);
            server.shutdown();
            return Err("Workspace Mount failed; cleanup outcomes retained".into());
        }
    };
    let preparation_ns = prepared.elapsed().as_nanos();

    // The one measured caller operation: Exec, its output check, then Commit.
    let command = case.get("command")?.to_string();
    let expected_splice_output = v3
        .then(|| {
            Ok::<_, Box<dyn std::error::Error>>(format!(
                "{{\"status\":\"PASS\",\"operation\":\"splice\",\"direction\":\"-\",\"final_bytes\":{},\"shifted_bytes\":0}}\n",
                case.number("final_bytes")?
            ))
        })
        .transpose()?;
    let final_bytes = case.number("final_bytes")?;
    let expected_batch_count = if v5 {
        Some(case.number("edit_count")?)
    } else {
        None
    };
    let mut edit_ns = 0u128;
    let mut commit_ns = 0u128;
    let mut observed_mtime = None;
    let mut status = "FAIL";
    let mut detail;
    let read_before =
        std::env::var_os("LAYERFS_FINISH_DIAGNOSTIC").and_then(|_| process_disk_read_bytes());
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
                    Ok(exec) if v4 || v5 || complexity_single => {
                        observed_mtime = confirmed_splice(
                            &exec,
                            final_bytes,
                            if v5 { "splice-batch" } else { "splice" },
                            expected_batch_count,
                        );
                        if observed_mtime.is_none() {
                            return Err(format!(
                                "exec splice output not exact: status={:?} truncated stdout={} stderr={} output={}",
                                exec.exit_status,
                                exec.stdout_truncated,
                                exec.stderr_truncated,
                                String::from_utf8_lossy(&exec.stdout).escape_debug()
                            ));
                        }
                    }
                    Ok(exec) if confirmed_exec(&exec, expected_splice_output.as_deref()) => {}
                    Ok(exec) => {
                        return Err(format!(
                            "exec status {:?} truncated stdout={} stderr={} expected-splice-output={} stderr={}",
                            exec.exit_status,
                            exec.stdout_truncated,
                            exec.stderr_truncated,
                            expected_splice_output.is_some(),
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
    let read_after =
        std::env::var_os("LAYERFS_FINISH_DIAGNOSTIC").and_then(|_| process_disk_read_bytes());
    let read_delta = read_before
        .zip(read_after)
        .and_then(|(before, after)| after.checked_sub(before));
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
    let (delete, log_capture) = if v4 || v5 || complexity_single {
        let (delete, capture) = sandboxes.delete_with_logs(sandbox, &mut std::io::stderr());
        (delete, Some(capture))
    } else {
        (sandboxes.delete(sandbox), None)
    };
    let cleanup_ns = cleanup_started.elapsed().as_nanos();
    if unmount.is_err() || delete.is_err() {
        status = "FAIL";
        detail = format!("{detail}; cleanup failed: unmount={unmount:?} delete={delete:?}");
    }
    let projection = post_status
        .as_ref()
        .map(|value| {
            value
                .projection
                .iter()
                .map(|(label, count)| format!("{label}={count}"))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let upstream = post_status
        .as_ref()
        .map(|value| value.upstream_calls)
        .unwrap_or(0);
    let range_accepted_payload_bytes = post_status
        .as_ref()
        .map(|value| value.range_accepted_payload_bytes)
        .unwrap_or(0);
    let range_shifted_suffix_bytes = post_status
        .as_ref()
        .map(|value| value.range_shifted_suffix_bytes)
        .unwrap_or(0);
    let v4_metadata = if v4 || v5 || complexity_single {
        let capture = log_capture.as_ref().expect("v4 captured logs");
        match observed_mtime {
            Some((seconds, nanoseconds)) => format!(
                ",\"observed_mtime_seconds\":{seconds},\"observed_mtime_nanoseconds\":{nanoseconds},\"sandbox\":\"{sandbox}\",\"caller_pid\":{},\"telemetry_run\":\"{run:032x}\",\"daemon_log_attempted\":{},\"daemon_log_bytes\":{},\"daemon_log_truncated\":{},\"daemon_log_error\":\"{}\"",
                std::process::id(), capture.attempted, capture.bytes, capture.truncated,
                format!("{:?}", capture.error).escape_debug()
            ),
            None => format!(
                ",\"observed_mtime_seconds\":null,\"observed_mtime_nanoseconds\":null,\"sandbox\":\"{sandbox}\",\"caller_pid\":{},\"telemetry_run\":\"{run:032x}\",\"daemon_log_attempted\":{},\"daemon_log_bytes\":{},\"daemon_log_truncated\":{},\"daemon_log_error\":\"{}\"",
                std::process::id(), capture.attempted, capture.bytes, capture.truncated,
                format!("{:?}", capture.error).escape_debug()
            ),
        }
    } else {
        String::new()
    };
    let physical_reads = format!(
        ",\"process_disk_read_bytes_before\":{},\"process_disk_read_bytes_after\":{},\"process_disk_read_bytes_delta\":{},\"process_disk_read_source\":\"proc_pid_rusage_v2_process_wide\"",
        read_before.map_or("null".to_string(), |value| value.to_string()),
        read_after.map_or("null".to_string(), |value| value.to_string()),
        read_delta.map_or("null".to_string(), |value| value.to_string()),
    );
    let receipt =
        format!(
        "{{\"schema\":\"core-fs-bench-pro-exec-fuse-edit-performance-v1\",\"status\":\"{status}\",\
\"detail\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"route\":\"{}\",\
\"operation_contract_id\":\"{}\",\
\"operation_surface\":\"workspace-posix-fuse\",\"operation_entrypoint\":\"WorkspaceApi::exec\",\
\"acknowledgement_boundary\":\"WorkspaceApi::commit\",\"fixture_bytes\":{},\"edit_start\":{},\
\"delete_len\":{},\"replacement_len\":{},\"replacement_sha256\":\"{}\",\"final_bytes\":{},\
\"g2_target_ms\":{},\"edit_ns\":{edit_ns},\"commit_ns\":{commit_ns},\
\"edit_commit_ns\":{edit_commit_ns},\"preparation_ns\":{preparation_ns},\
\"cleanup_ns\":{cleanup_ns},\"branch_id\":\"{}\",\"head_commit\":\"{head_commit}\",\
\"projection_counts\":\"{projection}\",\"upstream_calls\":{upstream},\
\"range_accepted_payload_bytes\":{range_accepted_payload_bytes},\
\"range_shifted_suffix_bytes\":{range_shifted_suffix_bytes},\
\"unmount_ok\":{},\"sandbox_delete_ok\":{},\"sandbox_delete_container_removed\":{},\
\"sandbox_delete_volume_removed\":{},\"store\":\"{}\",\"history\":\"{}\",\
\"image\":\"{}\",\"service_endpoint_port\":{},\"replay\":false{v4_metadata}{physical_reads}}}",
        detail.escape_debug(),
        case.get("family_id")?,
        case.get("scenario_id")?,
        case.get("route")?,
        case.get("operation_contract_id")?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_tool_output_requires_exact_verified_mtime() {
        let mut exec = ExecResult {
            exit_status: Some(0),
            stdout: b"{\"status\":\"PASS\",\"operation\":\"splice\",\"direction\":\"-\",\"final_bytes\":12288,\"shifted_bytes\":0,\"mtime_seconds\":1700000000,\"mtime_nanoseconds\":123}\n".to_vec(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        };
        assert_eq!(
            confirmed_splice(&exec, 12288, "splice", None),
            Some((1_700_000_000, 123))
        );
        exec.stdout.extend_from_slice(b"extra");
        assert_eq!(confirmed_splice(&exec, 12288, "splice", None), None);
        exec.stdout.truncate(exec.stdout.len() - 5);
        exec.stdout_truncated = true;
        assert_eq!(confirmed_splice(&exec, 12288, "splice", None), None);
        exec.stdout_truncated = false;
        exec.stdout = b"{\"status\":\"PASS\",\"operation\":\"splice-batch\",\"direction\":\"-\",\"final_bytes\":12288,\"shifted_bytes\":0,\"mtime_seconds\":1700000000,\"mtime_nanoseconds\":123,\"edit_count\":32,\"accepted_bytes\":4096}\n".to_vec();
        assert_eq!(
            confirmed_splice(&exec, 12288, "splice-batch", Some(32)),
            Some((1_700_000_000, 123))
        );
        assert_eq!(
            confirmed_splice(&exec, 12288, "splice-batch", Some(128)),
            None
        );
    }

    #[test]
    fn splice_requires_exact_caller_proof_before_commit() {
        let expected = "{\"status\":\"PASS\",\"operation\":\"splice\",\"direction\":\"-\",\"final_bytes\":12288,\"shifted_bytes\":0}\n";
        let mut exec = ExecResult {
            exit_status: Some(0),
            stdout: expected.as_bytes().to_vec(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        };
        assert!(confirmed_exec(&exec, Some(expected)));
        exec.stdout = b"{\"status\":\"PASS\"}\n".to_vec();
        assert!(!confirmed_exec(&exec, Some(expected)));
        assert!(confirmed_exec(&exec, None));
        exec.stdout_truncated = true;
        assert!(!confirmed_exec(&exec, None));
    }
}
