use crate::output::OutputLog;
use crate::worker::WorkspaceWorker;
use crate::{
    ExecutionId, ExecutionReceipt, ExecutionSummary, NonEmpty, OutputReader, OutputStream,
    WorkspaceError, WorkspaceExecution, WorkspacePlacement, WorkspaceResult, WorkspaceSessionId,
    Workspaces,
};
use std::ffi::OsString;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

pub(crate) struct Execution {
    id: ExecutionId,
    session_id: WorkspaceSessionId,
    child: Mutex<Option<Child>>,
    termination: Termination,
    output: Arc<OutputLog>,
    receipt: Mutex<Option<ExecutionReceipt>>,
    completed_at: Mutex<Option<std::time::SystemTime>>,
    stopped: AtomicBool,
    stdout_bytes: AtomicU64,
    stderr_bytes: AtomicU64,
}

enum Termination {
    #[cfg(unix)]
    Host(u32),
    Container {
        container: String,
        group: u32,
    },
    #[cfg(not(unix))]
    Direct,
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
            running: receipt.is_none(),
            receipt,
        })
    }

    pub(crate) fn session_id(&self) -> WorkspaceSessionId {
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
    pub fn exec(
        &self,
        session_id: WorkspaceSessionId,
        argv: NonEmpty<Vec<OsString>>,
    ) -> WorkspaceResult<WorkspaceExecution> {
        self.spawn(session_id, argv.as_slice(), false)
    }

    pub fn shell(&self, session_id: WorkspaceSessionId) -> WorkspaceResult<WorkspaceExecution> {
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
        let mut child = execution
            .child
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let delivered = match child.as_mut() {
            Some(child) => execution.termination.stop(child)?,
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
        session_id: WorkspaceSessionId,
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
        let container_control = match &worker.request.placement {
            WorkspacePlacement::Container { container_id, .. } => Some((
                container_id.0.clone(),
                format!("/tmp/layerfs-execution-{id}.pid"),
                format!("/tmp/layerfs-execution-{id}.ready"),
            )),
            WorkspacePlacement::Host { .. } => None,
        };
        let mut command = command(
            &worker.request.placement,
            argv,
            interactive,
            container_control
                .as_ref()
                .map(|(_, pid, ready)| (pid.as_str(), ready.as_str())),
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
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                worker.note_execution(false)?;
                return Err(WorkspaceError::Io(error));
            }
        };
        let termination = match container_control {
            Some((container, pid, ready)) => match container_process_group(&container, &pid) {
                Ok(group) => {
                    let _ = release_container_execution(&container, &ready);
                    Termination::Container { container, group }
                }
                Err(error) => {
                    let _ = child.wait();
                    worker.note_execution(false)?;
                    return Err(error);
                }
            },
            None => {
                #[cfg(unix)]
                {
                    Termination::Host(child.id())
                }
                #[cfg(not(unix))]
                {
                    Termination::Direct
                }
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let execution = Arc::new(Execution {
            id,
            session_id,
            child: Mutex::new(Some(child)),
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
        session_id: WorkspaceSessionId,
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
    fn stop(&self, _child: &mut Child) -> WorkspaceResult<bool> {
        match self {
            #[cfg(unix)]
            Self::Host(group) => signal_host_group(*group),
            Self::Container { container, group } => signal_container_group(container, *group),
            #[cfg(not(unix))]
            Self::Direct => {
                if _child.try_wait()?.is_some() {
                    Ok(false)
                } else {
                    _child.kill()?;
                    Ok(true)
                }
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

fn signal_container_group(container: &str, group: u32) -> WorkspaceResult<bool> {
    let status = Command::new("docker")
        .args(["exec", container, "/bin/sh", "-c"])
        .arg(
            "if kill -TERM -\"$1\" 2>/dev/null; then exit 0; fi; \
             if kill -0 -\"$1\" 2>/dev/null; then exit 1; fi; exit 2",
        )
        .args(["layerfs-stop", &group.to_string()])
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
    container_control: Option<(&str, &str)>,
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
                    "umask 077; pid_file=$1; ready_file=$2; shift 2; \
                     pgid=$(cut -d ' ' -f 5 \"/proc/$$/stat\") || exit 125; \
                     [ \"$pgid\" = \"$$\" ] || exit 125; \
                     printf '%s\\n' \"$$\" > \"$pid_file\" || exit 125; attempts=0; \
                     while [ ! -e \"$ready_file\" ]; do attempts=$((attempts + 1)); \
                     if [ \"$attempts\" -ge 500 ]; then rm -f \"$pid_file\" \"$ready_file\"; \
                     exit 125; fi; sleep 0.01; done; rm -f \"$pid_file\" \"$ready_file\"; \
                     exec \"$@\"",
                )
                .arg("layerfs-exec")
                .arg(container_control.expect("container control paths").0)
                .arg(container_control.expect("container control paths").1)
                .args(argv);
            command
        }
    }
}

fn container_process_group(container: &str, path: &str) -> WorkspaceResult<u32> {
    let output = Command::new("docker")
        .args(["exec", container, "/bin/sh", "-c"])
        .arg(
            "attempts=0; while [ ! -s \"$1\" ]; do attempts=$((attempts + 1)); \
             [ \"$attempts\" -lt 500 ] || exit 1; sleep 0.01; done; \
             cat \"$1\"",
        )
        .args(["layerfs-pid", path])
        .output()?;
    if !output.status.success() {
        return Err(WorkspaceError::Io(std::io::Error::other(
            "container execution pid handshake",
        )));
    }
    let group = std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|group| *group > 0)
        .ok_or(WorkspaceError::InvalidExecution)?;
    Ok(group)
}

fn release_container_execution(container: &str, path: &str) -> WorkspaceResult<()> {
    let status = Command::new("docker")
        .args(["exec", container, "/bin/sh", "-c", ": > \"$1\""])
        .args(["layerfs-ready", path])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(WorkspaceError::Io(std::io::Error::other(
            "container execution ready handshake",
        )))
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
    stdout: Option<F>,
    stderr: Option<G>,
) where
    F: FnOnce(Arc<Execution>) + Send + 'static,
    G: FnOnce(Arc<Execution>) + Send + 'static,
{
    let started = std::time::Instant::now();
    let stdout = stdout.map(|drain| {
        let execution = execution.clone();
        std::thread::spawn(move || drain(execution))
    });
    let stderr = stderr.map(|drain| {
        let execution = execution.clone();
        std::thread::spawn(move || drain(execution))
    });
    std::thread::spawn(move || {
        let status = loop {
            let status = execution
                .child
                .lock()
                .map_err(|_| ())
                .and_then(|mut child| child.as_mut().ok_or(())?.try_wait().map_err(|_| ()));
            match status {
                Ok(Some(status)) => break Some(status),
                Err(()) => break None,
                Ok(None) => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        if let Some(thread) = stdout {
            let _ = thread.join();
        }
        if let Some(thread) = stderr {
            let _ = thread.join();
        }
        if let Ok(mut child) = execution.child.lock() {
            child.take();
        }
        let receipt = ExecutionReceipt {
            execution_id: execution.id,
            exit_code: status.and_then(|status| status.code()),
            elapsed_ns: started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
            stdout_bytes: execution.stdout_bytes.load(Ordering::Relaxed),
            stderr_bytes: execution.stderr_bytes.load(Ordering::Relaxed),
            stopped: execution.stopped.load(Ordering::Acquire),
        };
        if let Ok(mut stored) = execution.receipt.lock() {
            *stored = Some(receipt.clone());
        }
        execution.output.finish(receipt);
        if let Ok(mut completed_at) = execution.completed_at.lock() {
            *completed_at = Some(std::time::SystemTime::now());
        }
        if let Some(worker) = worker.upgrade() {
            let _ = worker.note_execution(false);
        }
        if let Some(executions) = executions.upgrade() {
            crate::registry::prune_execution_registry(&executions);
        }
    });
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
