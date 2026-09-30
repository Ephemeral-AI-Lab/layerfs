//! Public SDK driver for the #243 ordinary Workspace shell scenario.
use layerfs_sdk::{
    CommitOutcomeWire, HistoryMode, Project, ProjectApi, SandboxApi, Server, ServerConfig,
    WorkspaceApi, WorkspaceError, WorkspaceViewLease, WorkspaceViewRelease,
};
use layerfs_telemetry::{
    output::{Identity, OutputConfig},
    runtime::{Configuration, MonitorConfig, Runtime},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write as _, time::Instant};

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct RusageV2 {
    uuid: [u8; 16],
    user_time: u64,
    system_time: u64,
    package_idle_wakeups: u64,
    interrupt_wakeups: u64,
    pageins: u64,
    wired_size: u64,
    resident_size: u64,
    physical_footprint: u64,
    process_start_abstime: u64,
    process_exit_abstime: u64,
    child_user_time: u64,
    child_system_time: u64,
    child_package_idle_wakeups: u64,
    child_interrupt_wakeups: u64,
    child_pageins: u64,
    child_elapsed_abstime: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
}
#[cfg(target_os = "macos")]
fn disk_read_bytes() -> Option<u64> {
    let mut usage = RusageV2::default();
    // SAFETY: libproc writes the C-layout V2 structure supplied here.
    (unsafe {
        proc_pid_rusage(
            std::process::id() as i32,
            2,
            std::ptr::from_mut(&mut usage).cast(),
        )
    } == 0)
        .then_some(usage.disk_read_bytes)
}
#[cfg(not(target_os = "macos"))]
fn disk_read_bytes() -> Option<u64> {
    None
}

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

fn pinned_digest(
    api: &WorkspaceApi<'_>,
    lease: &WorkspaceViewLease,
    request_bytes: usize,
    path: &[u8],
) -> Result<(u64, String), String> {
    let mut file = lease.root().clone();
    for name in path.split(|byte| *byte == b'/') {
        file = api
            .view_lookup(lease, &file, name)
            .map_err(|e| format!("pin lookup: {e:?}"))?;
    }
    let mut offset = 0;
    let mut hash = Sha256::new();
    while offset < file.size {
        let length = (file.size - offset).min(request_bytes as u64) as usize;
        let read = api
            .view_read(lease, &file, offset, length)
            .map_err(|e| format!("pin read: {e:?}"))?;
        if read.bytes.len() != length || read.size != file.size {
            return Err("short or changed pinned file".into());
        }
        hash.update(&read.bytes);
        offset += read.bytes.len() as u64;
    }
    Ok((offset, hex(&hash.finalize())))
}
fn stopping_control(
    api: &WorkspaceApi<'_>,
    id: &layerfs_sdk::WorkspaceId,
    sandbox: &layerfs_sdk::SandboxId,
) -> Result<(), String> {
    let setup = api.exec(id, "mkfifo /tmp/f4-ready /tmp/f4-release /tmp/f4-closed; (exec 3<data.bin && printf 'ready\\n' > /tmp/f4-ready && read token < /tmp/f4-release && exec 3<&- && printf 'closed\\n' > /tmp/f4-closed) > /tmp/f4-holder.log 2>&1 & read ready < /tmp/f4-ready")
        .map_err(|error| format!("holder setup: {error:?}"))?;
    if setup.exit_status != Some(0) {
        return Err(format!("holder setup: {setup:?}"));
    }
    let before = api
        .status(id)
        .map_err(|error| format!("before: {error:?}"))?;
    let unmount = api.unmount(id);
    let retained = matches!(
        &unmount,
        Err(WorkspaceError::Retained {
            cause: layerfs_bridge::contract::Code::Deadline,
            ..
        })
    );
    let stopped = api.status(id);
    let stopping = stopped
        .as_ref()
        .is_ok_and(|status| status.mounted && status.stopping && status.handles > 0);
    let pin = api.pin_view(id);
    let pin_refused = matches!(&pin, Err(WorkspaceError::Failure(failure)) if failure.code == layerfs_bridge::contract::Code::Busy && !failure.unknown);
    let read = api.exec(id, "head -c 1 data.bin");
    let read_refused = matches!(&read, Err(WorkspaceError::Failure(failure))
        if failure.code == layerfs_bridge::contract::Code::Io && !failure.unknown);
    // Test-controller release touches only outside-Workspace synchronization
    // FIFOs. Product unmount/deletion still go through their checked SDK APIs.
    let release = std::process::Command::new("docker")
        .args([
            "exec",
            &format!("layerfs-{sandbox}"),
            "/bin/sh",
            "-c",
            "printf 'release\\n' > /tmp/f4-release; read closed < /tmp/f4-closed",
        ])
        .output();
    let released = release.as_ref().is_ok_and(|result| result.status.success());
    println!("CONTROL\t{{\"stopping_observed\":{stopping},\"retained_unmount_known\":{retained},\"new_pin_refused_known_busy\":{pin_refused},\"new_exec_refused_known_io\":{read_refused},\"holder_released\":{released},\"pin_release_ok\":{}}}", pin.is_err());
    println!("STOPPING_DETAIL\tbefore={before:?} unmount={unmount:?} stopped={stopped:?} pin={pin:?} read={read:?} release={release:?}");
    if let Ok(lease) = pin {
        api.release_view(&lease)
            .map_err(|error| format!("unexpected pin custody: {error:?}"))?;
    }
    if retained && stopping && pin_refused && read_refused && released {
        Ok(())
    } else {
        Err("stopping/refusal or holder-release condition failed".into())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let disk_read_before = disk_read_bytes();
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("mode case store history image required".into());
    }
    let seed = args[1] == "seed" || args[1] == "seed-existing";
    if !seed && args[1] != "run" {
        return Err("unknown mode".into());
    }
    let case = Case::load(&args[2])?;
    let expected_failure = !seed && case.get("expected_failure")? == "1";
    let expected_clean = case.0.get("clean_commit").is_some_and(|value| value == "1");
    let prelude = case
        .0
        .get("prelude_command_hex")
        .map(|_| {
            case.bytes("prelude_command_hex")
                .and_then(|bytes| String::from_utf8(bytes).map_err(Into::into))
        })
        .transpose()?;
    let pin_read_bytes = case
        .0
        .get("pin_read_bytes")
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(16_384);
    if prelude.is_some() && pin_read_bytes > 31_744 {
        return Err("this benchmark profile permits 0 through 31 KiB pinned reads".into());
    }
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
        binding_key: if case.0.contains_key("binding_key_hex") {
            case.bytes("binding_key_hex")?
        } else {
            b"layerfs-bench-pro".to_vec()
        },
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
    let branch = if args[1] == "seed" {
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
            "bench-{}-{}",
            hex(&Sha256::digest(case.get("scenario_id")?.as_bytes())[..8]),
            std::process::id()
        ),
    ) {
        Ok(value) => value,
        Err(error) => {
            let resources_owned = error.sandbox.is_some();
            let cleanup = error.sandbox.map(|id| sandboxes.delete(id));
            println!("RECEIPT\t{{\"status\":\"FAIL\",\"stage\":\"sandbox_create\",\"detail\":{:?},\"resources_owned\":{resources_owned},\"sandbox_delete_attempted\":{resources_owned},\"sandbox_delete_ok\":{}}}", format!("{error:?}"), cleanup.as_ref().is_none_or(Result::is_ok));
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
    if case
        .0
        .get("stopping_control")
        .is_some_and(|value| value == "1")
    {
        let start = Instant::now();
        let result = stopping_control(&workspaces, &mount.id, &sandbox);
        let operation_ns = start.elapsed().as_nanos();
        let unmount = workspaces.unmount(&mount.id);
        let final_status = workspaces.status(&mount.id);
        let (delete, _) = sandboxes.delete_with_logs(sandbox, &mut std::io::stderr());
        let complete = result.is_ok()
            && unmount.is_ok()
            && delete.is_ok()
            && final_status
                .as_ref()
                .is_ok_and(|status| !status.mounted && !status.stopping);
        println!("RECEIPT\t{{\"status\":{:?},\"scenario_id\":{:?},\"detail\":{:?},\"head_commit\":{:?},\"commit_called\":false,\"exec_ns\":null,\"commit_ns\":null,\"operation_ns\":{operation_ns},\"unmount_ok\":{},\"sandbox_delete_ok\":{}}}",
            if complete { "COMPLETE" } else { "FAIL" }, case.get("scenario_id")?, format!("{result:?}; final={final_status:?}"), case.get("old_commit")?, unmount.is_ok(), delete.is_ok());
        drop(owner);
        server.shutdown();
        return if complete {
            Ok(())
        } else {
            Err("stopping control incomplete".into())
        };
    }
    let mut exec_ns = 0;
    let mut exec_exit_status = None;
    let mut commit_ns = 0;
    let mut commit_called = false;
    let mut prelude_exec_ns = 0;
    let mut prelude_commit_ns = 0;
    let mut prelude_head = String::new();
    let mut lease = None;
    let mut early_unmount_ok = false;
    let mut recovery_complete = false;
    let start = Instant::now();
    let (outcome, diagnostic) = runtime
        .recorder()
        .run(243_000, "sdk.shell_package", |scope| {
            if let Some(command) = &prelude {
                let began = Instant::now();
                let exec = workspaces
                    .exec(&mount.id, command)
                    .map_err(|e| format!("prelude exec: {e:?}"))?;
                prelude_exec_ns = began.elapsed().as_nanos();
                if exec.exit_status != Some(0) || exec.stdout_truncated || exec.stderr_truncated {
                    return Err(format!("prelude Exec: {exec:?}"));
                }
                lease = Some(
                    workspaces
                        .pin_view(&mount.id)
                        .map_err(|e| format!("pin: {e:?}"))?,
                );
                let began = Instant::now();
                let committed = workspaces
                    .commit(&mount.id)
                    .map_err(|e| format!("prelude commit: {e:?}"))?;
                prelude_commit_ns = began.elapsed().as_nanos();
                match committed.outcome {
                    CommitOutcomeWire::Committed(record) => prelude_head = hex(&record.commit),
                    other => return Err(format!("prelude outcome: {other:?}")),
                }
            }
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
            exec_exit_status = exec.exit_status;
            if expected_failure {
                let wanted = case
                    .0
                    .get("expected_exit_status")
                    .map(|value| value.parse::<i32>())
                    .transpose()
                    .map_err(|error| format!("expected exit: {error}"))?;
                return match exec.exit_status {
                    Some(code) if code != 0 && wanted.is_none_or(|wanted| code == wanted) => {
                        Ok(None)
                    }
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
    let mut operation_ns = start.elapsed().as_nanos();
    runtime.publish(diagnostic);
    let (mut status, mut detail, mut head_commit) = ("COMPLETE", String::new(), String::new());
    let mut up_to_date = false;
    match outcome {
        Ok(Some(report)) => match report.outcome {
            CommitOutcomeWire::Committed(record) => head_commit = hex(&record.commit),
            CommitOutcomeWire::UpToDate {
                head: Some(head), ..
            } if expected_clean => {
                head_commit = hex(&head);
                up_to_date = true;
            }
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
    if status == "COMPLETE"
        && case
            .0
            .get("recovery_commit")
            .is_some_and(|value| value == "1")
    {
        let recovery = (|| -> Result<(), String> {
            use layerfs_bridge::contract::{
                HistoryQuery, HistoryResult, Operation, Request, Response, HISTORY_PROFILE,
            };
            if !expected_failure || commit_called || exec_exit_status != Some(7) {
                return Err("recovery requires known exit7 before any Commit".into());
            }
            let response = server
                .service()
                .handle(
                    &server.peer().map_err(|e| format!("peer: {e:?}"))?,
                    &Request {
                        id: 244_001,
                        generation: 1,
                        store: server.store(),
                        profile: HISTORY_PROFILE,
                        deadline_ms: 10000,
                        response_bytes: 16384,
                        operation: Operation::HistoryQuery(HistoryQuery::GetBranch { branch }),
                    },
                    &mut std::io::empty(),
                    &mut std::io::sink(),
                )
                .0
                .map_err(|e| format!("head: {e:?}"))?;
            let Response::History(record) = response else {
                return Err("history reply".into());
            };
            let HistoryResult::BranchSnapshot(snapshot) = *record else {
                return Err("branch snapshot".into());
            };
            if snapshot.branch.head_commit.map(|id| hex(&id)).as_deref()
                != Some(case.get("old_commit").map_err(|e| e.to_string())?)
            {
                return Err("failed Exec implicitly changed canonical head".into());
            }
            lease = Some(
                workspaces
                    .pin_view(&mount.id)
                    .map_err(|e| format!("dirty pin: {e:?}"))?,
            );
            let path = case.bytes("pin_path_hex").map_err(|e| e.to_string())?;
            let (bytes, digest) = pinned_digest(
                &workspaces,
                lease.as_ref().ok_or("dirty lease")?,
                16384,
                &path,
            )?;
            if bytes != 7 || digest != hex(&Sha256::digest(b"private")) {
                return Err("accepted private bytes unavailable before recovery".into());
            }
            workspaces
                .unmount(&mount.id)
                .map_err(|e| format!("early unmount: {e:?}"))?;
            early_unmount_ok = true;
            let observed = workspaces
                .status(&mount.id)
                .map_err(|e| format!("retained status: {e:?}"))?;
            if observed.mounted || observed.closed || observed.stopping {
                return Err(format!("unmounted dirty owner not retained: {observed:?}"));
            }
            // Explicit benchmark-owner recovery after observing the known error.
            // The product never automatically commits a failed shell command.
            commit_called = true;
            let began = Instant::now();
            let result = workspaces
                .commit(&mount.id)
                .map_err(|e| format!("explicit recovery Commit: {e:?}"))?;
            commit_ns = began.elapsed().as_nanos();
            let CommitOutcomeWire::Committed(record) = result.outcome else {
                return Err("known new recovery Commit".into());
            };
            if record.parent.map(|id| hex(&id)).as_deref()
                != Some(case.get("old_commit").map_err(|e| e.to_string())?)
            {
                return Err("recovery parent mismatch".into());
            }
            head_commit = hex(&record.commit);
            Ok(())
        })();
        recovery_complete = recovery.is_ok();
        if let Err(error) = recovery {
            status = "FAIL";
            detail = error;
        }
        operation_ns = start.elapsed().as_nanos();
    }
    let mut pinned_bytes = 0;
    let mut pinned_sha256 = String::new();
    let mut pin_observation_ok = lease.is_none();
    let mut pin_release_ok = lease.is_none();
    let pin_generation = lease.as_ref().map_or(0, WorkspaceViewLease::generation);
    if let Some(held) = &lease {
        match workspaces.view_status(held) {
            Ok(observed) => {
                pin_observation_ok =
                    observed.held_leases == 1 && observed.generation == held.generation()
            }
            Err(error) => {
                status = "FAIL";
                detail = format!("{detail}; pin status: {error:?}");
            }
        }
        if !pin_observation_ok {
            status = "FAIL";
            detail = format!("{detail}; held pin identity/status changed");
        }
        if pin_read_bytes > 0 {
            let pin_path = if case.0.contains_key("pin_path_hex") {
                case.bytes("pin_path_hex")?
            } else {
                b"data.bin".to_vec()
            };
            match pinned_digest(&workspaces, held, pin_read_bytes, &pin_path) {
                Ok((bytes, digest)) => {
                    pinned_bytes = bytes;
                    pinned_sha256 = digest;
                }
                Err(error) => {
                    status = "FAIL";
                    detail = format!("{detail}; {error}");
                }
            }
        }
        pin_release_ok = matches!(
            workspaces.release_view(held),
            Ok(WorkspaceViewRelease::Completed)
        );
        if !pin_release_ok {
            status = "FAIL";
            detail = format!("{detail}; pin release failed");
        }
    }
    eprintln!("BENCH_PHASE_OUTCOME status={status} exec_ns={exec_ns} commit_ns={commit_ns} head={head_commit} detail={detail}");
    let cleanup_start = Instant::now();
    let post_status = workspaces.status(&mount.id);
    eprintln!("BENCH_POST_STATUS {post_status:?}");
    let unmount = if early_unmount_ok {
        Ok(())
    } else {
        workspaces.unmount(&mount.id)
    };
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
    let disk_read_delta =
        disk_read_before.and_then(|before| disk_read_bytes()?.checked_sub(before));
    println!("CONTROL\t{{\"prelude_exec_ns\":{prelude_exec_ns},\"prelude_commit_ns\":{prelude_commit_ns},\"prelude_head_commit\":{prelude_head:?},\"up_to_date\":{up_to_date},\"pin_generation\":{pin_generation},\"pin_observation_ok\":{pin_observation_ok},\"pin_read_bytes\":{pin_read_bytes},\"pinned_bytes\":{pinned_bytes},\"pinned_sha256\":{pinned_sha256:?},\"pin_release_ok\":{pin_release_ok},\"recovery_complete\":{recovery_complete},\"unmounted_before_recovery\":{early_unmount_ok}}}");
    println!("RECEIPT\t{{\"schema\":\"issue243-shell-driver-v1\",\"status\":\"{status}\",\"detail\":{:?},\"mode\":{:?},\"scenario_id\":{:?},\"branch_id\":{:?},\"head_commit\":{:?},\"commit_called\":{commit_called},\"exec_exit_status\":{},\"exec_ns\":{exec_ns},\"commit_ns\":{commit_ns},\"operation_ns\":{operation_ns},\"cleanup_ns\":{cleanup_ns},\"projection_counts\":{:?},\"unmount_ok\":{},\"sandbox_delete_ok\":{},\"daemon_log_attempted\":{},\"daemon_log_bytes\":{},\"daemon_log_truncated\":{},\"daemon_log_error\":{:?},\"host_disk_read_bytes\":{}}}",
        detail, args[1], case.get("scenario_id")?, hex(&branch), head_commit, exec_exit_status.map_or("null".into(), |code| code.to_string()), counts,
        unmount.is_ok(), delete.is_ok(), capture.attempted, capture.bytes, capture.truncated,
        format!("{:?}", capture.error), disk_read_delta.map_or("null".to_owned(), |bytes| bytes.to_string()));
    drop(owner);
    server.shutdown();
    if status != "COMPLETE" {
        return Err(detail.into());
    }
    Ok(())
}
