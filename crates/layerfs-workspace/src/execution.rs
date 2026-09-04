use crate::output::{OutputFailure, OutputLog};
use crate::worker::WorkspaceWorker;
use crate::{
    DaemonTiming, ExecutionId, ExecutionReceipt, ExecutionSummary, ExecutionTransport, NonEmpty,
    OutputReader, OutputStream, WorkspaceError, WorkspaceExecution, WorkspaceId,
    WorkspacePlacement, WorkspaceResult, Workspaces,
};
use std::ffi::OsString;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

pub(crate) struct Execution {
    id: ExecutionId,
    session_id: WorkspaceId,
    process: Mutex<Option<ExecutionProcess>>,
    termination: Termination,
    output: Arc<OutputLog>,
    receipt: Mutex<Option<ExecutionReceipt>>,
    completed_at: Mutex<Option<std::time::SystemTime>>,
    stopped: AtomicBool,
    stdout_bytes: AtomicU64,
    stderr_bytes: AtomicU64,
}

enum ExecutionProcess {
    Child(Child),
    #[cfg(unix)]
    Docker(crate::docker_engine::DockerExec),
    #[cfg(unix)]
    Daemon(crate::daemon::DaemonExec),
}

enum Termination {
    #[cfg(unix)]
    Host(u32),
    Container {
        container: String,
        pid_file: String,
    },
    #[cfg(unix)]
    Direct,
    #[cfg(not(unix))]
    Foreground,
}

struct ExecutionTimingStart {
    total: std::time::Instant,
    spawn_finished: std::time::Instant,
    spawn_ns: u64,
    docker_engine_calls_before: u64,
}

impl Execution {
    fn summary(&self) -> WorkspaceResult<ExecutionSummary> {
        let receipt = self
            .receipt
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .clone();
        Ok(ExecutionSummary {
            id: self.id,
            running: !self.output.is_terminal(),
            receipt,
        })
    }

    pub(crate) fn session_id(&self) -> WorkspaceId {
        self.session_id
    }

    pub(crate) fn retention(&self) -> Option<(std::time::SystemTime, u64)> {
        self.completed_at
            .lock()
            .ok()
            .and_then(|completed_at| completed_at.map(|at| (at, self.output.retained_bytes())))
    }
}

impl Workspaces {
    /// Verification-only loss of the live daemon transport after a caller-observed barrier.
    #[cfg(feature = "test-instrumentation")]
    pub fn verification_disconnect_execution(
        &self,
        execution_id: ExecutionId,
    ) -> WorkspaceResult<()> {
        let execution = self.execution(execution_id)?;
        let process = execution
            .process
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        match process.as_ref() {
            #[cfg(unix)]
            Some(ExecutionProcess::Daemon(process)) => {
                process.disconnect().map_err(WorkspaceError::Io)
            }
            _ => Err(WorkspaceError::InvalidExecution),
        }
    }

    pub fn exec(
        &self,
        session_id: WorkspaceId,
        argv: NonEmpty<Vec<OsString>>,
    ) -> WorkspaceResult<WorkspaceExecution> {
        self.spawn(session_id, argv.as_slice(), false)
    }

    pub fn shell(&self, session_id: WorkspaceId) -> WorkspaceResult<WorkspaceExecution> {
        let worker = self.worker(session_id)?;
        let argv = match &worker.request.placement {
            WorkspacePlacement::Host { .. } => {
                vec![std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"))]
            }
            WorkspacePlacement::Container { .. } => vec![OsString::from("/bin/sh")],
        };
        self.spawn(session_id, &argv, true)
    }

    pub fn stop(&self, execution_id: ExecutionId) -> WorkspaceResult<()> {
        let execution = self.execution(execution_id)?;
        if execution
            .receipt
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .is_some()
        {
            return Ok(());
        }
        let mut process = execution
            .process
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let delivered = match process.as_mut() {
            Some(ExecutionProcess::Child(child)) => execution.termination.stop(child)?,
            #[cfg(unix)]
            Some(ExecutionProcess::Docker(process)) => process.stop()?,
            #[cfg(unix)]
            Some(ExecutionProcess::Daemon(process)) => {
                process.stop()?;
                true
            }
            None => false,
        };
        if delivered {
            execution.stopped.store(true, Ordering::Release);
        }
        Ok(())
    }

    pub fn output(&self, execution_id: ExecutionId) -> WorkspaceResult<OutputReader> {
        self.prune_retained()?;
        Ok(OutputReader::new(
            self.execution(execution_id)?.output.clone(),
        ))
    }

    fn spawn(
        &self,
        session_id: WorkspaceId,
        argv: &[OsString],
        interactive: bool,
    ) -> WorkspaceResult<WorkspaceExecution> {
        if argv.is_empty() {
            return Err(WorkspaceError::InvalidExecution);
        }
        self.prune_retained()?;
        let worker = self.worker(session_id)?;
        {
            let workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            workspace.ensure_active()?;
        }
        worker.note_execution(true)?;
        let id = new_execution_id();
        let output = match OutputLog::create(
            &self
                .runtime_root
                .join("output")
                .join(format!("{id}.frames")),
        ) {
            Ok(output) => output,
            Err(error) => {
                worker.note_execution(false)?;
                return Err(error);
            }
        };
        let total_started = std::time::Instant::now();
        let docker_engine_calls_before = docker_engine_calls();
        #[cfg(unix)]
        let daemon = !interactive
            && self.execution_route == crate::daemon::ExecutionRoute::Daemon
            && matches!(
                worker.request.placement,
                WorkspacePlacement::Container { .. }
            );
        #[cfg(not(unix))]
        let daemon = false;
        #[cfg(unix)]
        let direct = !interactive
            && !daemon
            && crate::docker_engine::DockerExec::available()
            && matches!(
                worker.request.placement,
                WorkspacePlacement::Container { .. }
            );
        #[cfg(not(unix))]
        let direct = false;
        let spawned = if daemon {
            #[cfg(unix)]
            {
                let WorkspacePlacement::Container { root, .. } = &worker.request.placement else {
                    unreachable!()
                };
                self.daemon
                    .as_ref()
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::NotConnected,
                            "daemon owner is unavailable",
                        )
                    })
                    .and_then(|daemon| daemon.start(session_id, id, root, argv))
                    .map(|process| {
                        (
                            ExecutionProcess::Daemon(process),
                            Termination::Direct,
                            None,
                            None,
                            ExecutionTransport::Daemon,
                        )
                    })
            }
            #[cfg(not(unix))]
            unreachable!()
        } else if direct {
            #[cfg(unix)]
            {
                let WorkspacePlacement::Container { container_id, root } =
                    &worker.request.placement
                else {
                    unreachable!()
                };
                let pid_file = format!("/tmp/layerfs-execution-{id}.pid");
                crate::docker_engine::DockerExec::start_wrapped(
                    &container_id.0,
                    root,
                    argv,
                    &pid_file,
                )
                .map(|process| {
                    (
                        ExecutionProcess::Docker(process),
                        Termination::Direct,
                        None,
                        None,
                        ExecutionTransport::DockerEngineFallback,
                    )
                })
            }
            #[cfg(not(unix))]
            unreachable!()
        } else {
            let container_control = match &worker.request.placement {
                WorkspacePlacement::Container { container_id, .. } => Some((
                    container_id.0.clone(),
                    format!("/tmp/layerfs-execution-{id}.pid"),
                )),
                WorkspacePlacement::Host { .. } => None,
            };
            let mut command = command(
                &worker.request.placement,
                argv,
                interactive,
                container_control
                    .as_ref()
                    .map(|(_, pid_file)| pid_file.as_str()),
            );
            #[cfg(unix)]
            if container_control.is_none() {
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
            if interactive {
                command
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit());
            } else {
                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
            }
            command.spawn().map(|mut child| {
                let termination = match container_control {
                    Some((container, pid_file)) => Termination::Container {
                        container,
                        pid_file,
                    },
                    None => {
                        #[cfg(unix)]
                        {
                            Termination::Host(child.id())
                        }
                        #[cfg(not(unix))]
                        {
                            Termination::Foreground
                        }
                    }
                };
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                let transport = match &worker.request.placement {
                    WorkspacePlacement::Host { .. } => ExecutionTransport::Host,
                    WorkspacePlacement::Container { .. } if interactive => {
                        ExecutionTransport::DockerCliInteractive
                    }
                    WorkspacePlacement::Container { .. } => ExecutionTransport::DockerCliFallback,
                };
                (
                    ExecutionProcess::Child(child),
                    termination,
                    stdout,
                    stderr,
                    transport,
                )
            })
        };
        let (process, termination, stdout, stderr, transport) = match spawned {
            Ok(spawned) => spawned,
            Err(error) => {
                worker.note_execution(false)?;
                return Err(WorkspaceError::Io(error));
            }
        };
        let spawn_ns = elapsed_ns(total_started);
        let spawn_finished = std::time::Instant::now();
        let execution = Arc::new(Execution {
            id,
            session_id,
            process: Mutex::new(Some(process)),
            termination,
            output,
            receipt: Mutex::new(None),
            completed_at: Mutex::new(None),
            stopped: AtomicBool::new(false),
            stdout_bytes: AtomicU64::new(0),
            stderr_bytes: AtomicU64::new(0),
        });
        self.executions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .insert(id, execution.clone());
        supervise(
            execution,
            Arc::downgrade(&worker),
            Arc::downgrade(&self.executions),
            ExecutionTimingStart {
                total: total_started,
                spawn_finished,
                spawn_ns,
                docker_engine_calls_before,
            },
            transport,
            stdout.map(|stdout| drain(stdout, OutputStream::Stdout)),
            stderr.map(|stderr| drain(stderr, OutputStream::Stderr)),
        );
        Ok(WorkspaceExecution { id, session_id })
    }

    fn execution(&self, id: ExecutionId) -> WorkspaceResult<Arc<Execution>> {
        self.executions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .get(&id)
            .cloned()
            .ok_or(WorkspaceError::NotFound)
    }

    pub(crate) fn execution_summaries(
        &self,
        session_id: WorkspaceId,
    ) -> WorkspaceResult<Vec<ExecutionSummary>> {
        self.prune_retained()?;
        self.executions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .values()
            .filter(|execution| execution.session_id == session_id)
            .map(|execution| execution.summary())
            .collect()
    }
}

impl Termination {
    fn stop(&self, child: &mut Child) -> WorkspaceResult<bool> {
        if child.try_wait()?.is_some() {
            return Ok(false);
        }
        match self {
            #[cfg(unix)]
            Self::Host(group) => signal_host_group(*group),
            Self::Container {
                container,
                pid_file,
            } => signal_container_group(container, pid_file),
            #[cfg(unix)]
            Self::Direct => Err(WorkspaceError::InvalidExecution),
            #[cfg(not(unix))]
            Self::Foreground => {
                child.kill()?;
                Ok(true)
            }
        }
    }
}

#[cfg(unix)]
fn signal_host_group(group: u32) -> WorkspaceResult<bool> {
    let status = Command::new("/bin/kill")
        .arg("-TERM")
        .arg(format!("-{group}"))
        .status()?;
    if status.success() {
        Ok(true)
    } else {
        let exists = Command::new("/bin/kill")
            .arg("-0")
            .arg(format!("-{group}"))
            .status()?;
        if exists.success() {
            Err(WorkspaceError::Io(std::io::Error::other(
                "execution process group signal",
            )))
        } else {
            Ok(false)
        }
    }
}

fn signal_container_group(container: &str, pid_file: &str) -> WorkspaceResult<bool> {
    let status = Command::new("docker")
        .args(["exec", container, "/bin/sh", "-c"])
        .arg(
            "attempts=0; while [ ! -s \"$1\" ]; do attempts=$((attempts + 1)); \
             [ \"$attempts\" -lt 100 ] || exit 2; sleep 0.01; done; \
             group=$(cat \"$1\") || exit 1; \
             if kill -TERM -\"$group\" 2>/dev/null; then exit 0; fi; \
             if kill -0 -\"$group\" 2>/dev/null; then exit 1; fi; exit 2",
        )
        .args(["layerfs-stop", pid_file])
        .status()?;
    match status.code() {
        Some(0) => Ok(true),
        Some(2) => Ok(false),
        _ => Err(WorkspaceError::Io(std::io::Error::other(
            "container execution process group signal",
        ))),
    }
}

fn command(
    placement: &WorkspacePlacement,
    argv: &[OsString],
    interactive: bool,
    container_control: Option<&str>,
) -> Command {
    match placement {
        WorkspacePlacement::Host { root } => {
            let mut command = Command::new(&argv[0]);
            command.args(&argv[1..]).current_dir(root);
            command
        }
        WorkspacePlacement::Container { container_id, root } => {
            let mut command = Command::new("docker");
            command.arg("exec");
            if interactive {
                command.arg("-it");
            }
            command
                .arg("-w")
                .arg(root)
                .arg(&container_id.0)
                .args(["/bin/sh", "-c"])
                .arg(
                    "pid_file=$1; shift; \
                     pgid=$(cut -d ' ' -f 5 \"/proc/$$/stat\") || exit 125; \
                     [ \"$pgid\" = \"$$\" ] || exit 125; \
                     (umask 077 && printf '%s\\n' \"$$\" > \"$pid_file\") || exit 125; \
                     trap 'rm -f \"$pid_file\"' EXIT; \"$@\"",
                )
                .arg("layerfs-exec")
                .arg(container_control.expect("container pid path"))
                .args(argv);
            command
        }
    }
}

fn drain<R: Read + Send + 'static>(
    mut reader: R,
    stream: OutputStream,
) -> impl FnOnce(Arc<Execution>) + Send + 'static {
    move |execution| {
        let mut bytes = [0; 16 * 1024];
        while let Ok(read) = reader.read(&mut bytes) {
            if read == 0 {
                break;
            }
            match stream {
                OutputStream::Stdout => {
                    execution
                        .stdout_bytes
                        .fetch_add(read as u64, Ordering::Relaxed);
                }
                OutputStream::Stderr => {
                    execution
                        .stderr_bytes
                        .fetch_add(read as u64, Ordering::Relaxed);
                }
            }
            let _ = execution.output.append(stream, &bytes[..read]);
        }
    }
}

fn supervise<F, G>(
    execution: Arc<Execution>,
    worker: Weak<WorkspaceWorker>,
    executions: Weak<Mutex<std::collections::BTreeMap<ExecutionId, Arc<Execution>>>>,
    timing: ExecutionTimingStart,
    transport: ExecutionTransport,
    stdout: Option<F>,
    stderr: Option<G>,
) where
    F: FnOnce(Arc<Execution>) + Send + 'static,
    G: FnOnce(Arc<Execution>) + Send + 'static,
{
    let stdout = stdout.map(|drain| {
        let execution = execution.clone();
        std::thread::spawn(move || drain(execution))
    });
    let stderr = stderr.map(|drain| {
        let execution = execution.clone();
        std::thread::spawn(move || drain(execution))
    });
    std::thread::spawn(move || {
        let supervisor_queue_ns = elapsed_ns(timing.spawn_finished);
        let runtime_started = std::time::Instant::now();
        let terminal = match transport {
            #[cfg(unix)]
            ExecutionTransport::Daemon => supervise_daemon(&execution),
            #[cfg(not(unix))]
            ExecutionTransport::Daemon => Err(OutputFailure::InfrastructureLost),
            #[cfg(unix)]
            ExecutionTransport::DockerEngineFallback => supervise_docker(&execution),
            #[cfg(not(unix))]
            ExecutionTransport::DockerEngineFallback => Err(OutputFailure::InfrastructureLost),
            _ => supervise_child(&execution),
        };
        let runtime_ns = elapsed_ns(runtime_started);
        let drain_started = std::time::Instant::now();
        if let Some(thread) = stdout {
            let _ = thread.join();
        }
        if let Some(thread) = stderr {
            let _ = thread.join();
        }
        if let Ok(mut process) = execution.process.lock() {
            process.take();
        }
        let drain_ns = elapsed_ns(drain_started);
        if let Some(worker) = worker.upgrade() {
            let _ = worker.note_execution(false);
        }
        match terminal {
            Ok(terminal) => {
                let receipt = ExecutionReceipt {
                    execution_id: execution.id,
                    exit_code: terminal.exit_code,
                    signal: terminal.signal,
                    elapsed_ns: 0,
                    total_wall_ns: 0,
                    spawn_ns: timing.spawn_ns,
                    supervisor_queue_ns,
                    runtime_ns,
                    drain_ns,
                    terminal_publication_ns: 0,
                    unattributed_ns: 0,
                    transport,
                    daemon_timing: terminal.daemon_timing,
                    docker_engine_calls: docker_engine_calls()
                        .saturating_sub(timing.docker_engine_calls_before),
                    stdout_bytes: execution.stdout_bytes.load(Ordering::Relaxed),
                    stderr_bytes: execution.stderr_bytes.load(Ordering::Relaxed),
                    stopped: terminal.stopped,
                };
                let _receipt =
                    execution
                        .output
                        .finish_timed(receipt, &execution.receipt, timing.total);
            }
            Err(failure) => execution.output.fail(failure),
        }
        if let Ok(mut completed_at) = execution.completed_at.lock() {
            *completed_at = Some(std::time::SystemTime::now());
        }
        if let Some(executions) = executions.upgrade() {
            crate::registry::prune_execution_registry(&executions);
        }
    });
}

struct Terminal {
    exit_code: Option<i32>,
    signal: Option<i32>,
    stopped: bool,
    daemon_timing: Option<DaemonTiming>,
}

#[cfg(unix)]
fn supervise_daemon(execution: &Execution) -> Result<Terminal, OutputFailure> {
    let mut stream = execution
        .process
        .lock()
        .map_err(|_| OutputFailure::InfrastructureLost)?
        .as_ref()
        .and_then(|process| match process {
            ExecutionProcess::Daemon(process) => process.reader().ok(),
            _ => None,
        })
        .ok_or(OutputFailure::InfrastructureLost)?;
    if std::env::var("LAYERFS_EXEC_INJECT_DISCONNECT").as_deref() == Ok("1") {
        if let Ok(process) = execution.process.lock() {
            if let Some(ExecutionProcess::Daemon(process)) = process.as_ref() {
                let _ = process.disconnect();
            }
        }
    }
    loop {
        match crate::daemon::DaemonExec::read(&mut stream) {
            Ok(crate::daemon::DaemonEvent::Stdout(bytes)) => {
                execution
                    .stdout_bytes
                    .fetch_add(bytes.len() as u64, Ordering::Relaxed);
                if execution
                    .output
                    .append(OutputStream::Stdout, &bytes)
                    .is_err()
                {
                    disconnect_daemon(execution);
                    return Err(OutputFailure::OutputFailed);
                }
            }
            Ok(crate::daemon::DaemonEvent::Stderr(bytes)) => {
                execution
                    .stderr_bytes
                    .fetch_add(bytes.len() as u64, Ordering::Relaxed);
                if execution
                    .output
                    .append(OutputStream::Stderr, &bytes)
                    .is_err()
                {
                    disconnect_daemon(execution);
                    return Err(OutputFailure::OutputFailed);
                }
            }
            Ok(crate::daemon::DaemonEvent::Exit(exit)) => {
                if exit.stdout_bytes != execution.stdout_bytes.load(Ordering::Relaxed)
                    || exit.stderr_bytes != execution.stderr_bytes.load(Ordering::Relaxed)
                {
                    return Err(OutputFailure::OutputFailed);
                }
                return Ok(Terminal {
                    exit_code: exit.code,
                    signal: exit.signal,
                    stopped: exit.stopped,
                    daemon_timing: Some(DaemonTiming {
                        accept_bind_ns: exit.timing.accept_bind_ns,
                        decode_ns: exit.timing.decode_ns,
                        spawn_ns: exit.timing.spawn_ns,
                        runtime_ns: exit.timing.runtime_ns,
                        drain_ns: exit.timing.drain_ns,
                    }),
                });
            }
            Ok(crate::daemon::DaemonEvent::Error(
                layerfs_daemon::protocol::RemoteError::OutputFailed,
            )) => return Err(OutputFailure::OutputFailed),
            Ok(crate::daemon::DaemonEvent::Error(_)) | Err(_) => {
                return Err(OutputFailure::InfrastructureLost)
            }
        }
    }
}

#[cfg(unix)]
fn disconnect_daemon(execution: &Execution) {
    if let Ok(process) = execution.process.lock() {
        if let Some(ExecutionProcess::Daemon(process)) = process.as_ref() {
            let _ = process.disconnect();
        }
    }
}

#[cfg(unix)]
fn supervise_docker(execution: &Execution) -> Result<Terminal, OutputFailure> {
    let mut stream = execution
        .process
        .lock()
        .map_err(|_| OutputFailure::InfrastructureLost)?
        .as_mut()
        .and_then(|process| match process {
            ExecutionProcess::Docker(process) => process.take_stream().ok(),
            _ => None,
        })
        .ok_or(OutputFailure::InfrastructureLost)?;
    let injected = std::env::var("LAYERFS_EXEC_INJECT_DISCONNECT").as_deref() == Ok("1");
    if injected {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
    if crate::docker_engine::drain_multiplexed(&mut stream, |stream, bytes| {
        let stream = match stream {
            1 => OutputStream::Stdout,
            2 => OutputStream::Stderr,
            _ => return Err(std::io::Error::other("Docker Exec output stream")),
        };
        match stream {
            OutputStream::Stdout => &execution.stdout_bytes,
            OutputStream::Stderr => &execution.stderr_bytes,
        }
        .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        execution
            .output
            .append(stream, bytes)
            .map_err(|error| match error {
                WorkspaceError::Io(error) => error,
                _ => std::io::Error::other("Docker Exec output"),
            })
    })
    .is_err()
    {
        if let Ok(mut process) = execution.process.lock() {
            if let Some(ExecutionProcess::Docker(process)) = process.as_mut() {
                let _ = process.stop();
            }
        }
        return Err(OutputFailure::InfrastructureLost);
    }
    if injected {
        if let Ok(mut process) = execution.process.lock() {
            if let Some(ExecutionProcess::Docker(process)) = process.as_mut() {
                let _ = process.stop();
            }
        }
        return Err(OutputFailure::InfrastructureLost);
    }
    let exit_code = execution
        .process
        .lock()
        .map_err(|_| OutputFailure::InfrastructureLost)?
        .as_ref()
        .and_then(|process| match process {
            ExecutionProcess::Docker(process) => process.exit_code().ok(),
            _ => None,
        })
        .ok_or(OutputFailure::InfrastructureLost)?;
    Ok(Terminal {
        exit_code,
        signal: None,
        stopped: execution.stopped.load(Ordering::Acquire),
        daemon_timing: None,
    })
}

fn supervise_child(execution: &Execution) -> Result<Terminal, OutputFailure> {
    let status = loop {
        let status = execution
            .process
            .lock()
            .map_err(|_| OutputFailure::InfrastructureLost)?
            .as_mut()
            .and_then(|process| match process {
                ExecutionProcess::Child(child) => child.try_wait().ok(),
                #[allow(unreachable_patterns)]
                _ => None,
            });
        match status {
            Some(Some(status)) => break status,
            None => return Err(OutputFailure::InfrastructureLost),
            Some(None) => std::thread::sleep(std::time::Duration::from_millis(2)),
        }
    };
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    };
    #[cfg(not(unix))]
    let signal = None;
    Ok(Terminal {
        exit_code: status.code(),
        signal,
        stopped: execution.stopped.load(Ordering::Acquire),
        daemon_timing: None,
    })
}

fn elapsed_ns(started: std::time::Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

fn docker_engine_calls() -> u64 {
    #[cfg(unix)]
    {
        crate::docker_engine::operation_count()
    }
    #[cfg(not(unix))]
    {
        0
    }
}

fn new_execution_id() -> ExecutionId {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&std::process::id().to_be_bytes());
    bytes.extend_from_slice(
        &std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_be_bytes(),
    );
    bytes.extend_from_slice(&SERIAL.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    let digest = layerfs_content::ObjectId::for_bytes(&bytes).to_bytes();
    ExecutionId(digest[..16].try_into().expect("fixed execution id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_execution_starts_without_a_ready_handshake() {
        let command = command(
            &WorkspacePlacement::Container {
                container_id: crate::ContainerId("target".to_owned()),
                root: "/workspace".into(),
            },
            &[OsString::from("/bin/true")],
            false,
            Some("/tmp/execution.pid"),
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments
            .iter()
            .any(|argument| argument.contains("pid_file")));
        assert!(arguments
            .iter()
            .all(|argument| !argument.contains("ready_file")));
        assert_eq!(
            arguments
                .iter()
                .filter(|argument| argument.as_str() == "/tmp/execution.pid")
                .count(),
            1
        );
    }
}
