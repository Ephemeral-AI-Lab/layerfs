//! One authenticated control session; the acceptor owns refusal and shutdown.
use crate::config::{ControlConfig, ControlGrant};
use layerfs_bridge::{
    adapters::native::{connection::accept, server::serve},
    contract::{
        Code, Failure, Operation, Request, Response, VerifiedPeer, WorkspaceLifecycleOutcome,
        WorkspaceLifecycleWire, WorkspaceStatusWire,
    },
};
use layerfs_fuse::{MountError, MountHandle};
use layerfs_workspace::{Workspace, WorkspaceError};
use std::{
    io::{self, Read},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, TryLockError,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Session {
    socket: TcpStream,
    worker: JoinHandle<()>,
}

#[derive(Clone)]
struct Target {
    workspace: Workspace,
    incarnation: [u8; 32],
    mount: Arc<Mutex<Option<MountHandle>>>,
    stopping: Arc<AtomicBool>,
}

pub(crate) struct Control {
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), Failure>>>,
}

impl Control {
    pub fn start(
        config: ControlConfig,
        listener: TcpListener,
        private: [u8; 32],
        workspace: Workspace,
        incarnation: [u8; 32],
        mount: Arc<Mutex<Option<MountHandle>>>,
    ) -> Result<Self, Failure> {
        listener.set_nonblocking(true)?;
        let stopping = Arc::new(AtomicBool::new(false));
        let target = Target {
            workspace,
            incarnation,
            mount,
            stopping: Arc::clone(&stopping),
        };
        let worker = thread::Builder::new()
            .name("layerfs-control".into())
            .stack_size(2 * 1024 * 1024)
            .spawn(move || run(config, listener, private, target))?;
        Ok(Self {
            stopping,
            worker: Some(worker),
        })
    }

    /// Admission ends before socket shutdown. A timeout retains the thread owner;
    /// callers must not claim that control shutdown or later mount cleanup passed.
    pub fn stop(&mut self, deadline: Instant) -> Result<(), Failure> {
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = &self.worker {
            while !worker.is_finished() {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .ok_or(Code::Deadline)?;
                thread::park_timeout(remaining.min(Duration::from_millis(1)));
            }
        }
        match self.worker.take() {
            Some(worker) => worker.join().map_err(|_| Failure::from(Code::Io))?,
            None => Ok(()),
        }
    }
}

fn run(
    config: ControlConfig,
    listener: TcpListener,
    private: [u8; 32],
    target: Target,
) -> Result<(), Failure> {
    let config = Arc::new(config);
    let mut session: Option<Session> = None;
    let result = (|| {
        while !target.stopping.load(Ordering::Acquire) {
            if session
                .as_ref()
                .is_some_and(|session| session.worker.is_finished())
            {
                session
                    .take()
                    .ok_or(Code::Io)?
                    .worker
                    .join()
                    .map_err(|_| Code::Io)?;
            }
            let stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::park_timeout(Duration::from_millis(10));
                    continue;
                }
                Err(_) => return Err(Code::Io.into()),
            };
            // Q=0: an accepted connection beyond the one live/closing session is
            // immediately closed, including during handshake. No waiting worker.
            if session.is_some() || target.stopping.load(Ordering::Acquire) {
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            let socket = stream.try_clone()?;
            let config = Arc::clone(&config);
            let target = target.clone();
            let worker = thread::Builder::new()
                .name("layerfs-control-session".into())
                .stack_size(2 * 1024 * 1024)
                .spawn(move || {
                    if let Ok(connection) = accept(stream, &private, &config.peers) {
                        let _ = serve(connection, |peer, request, input, _, deadline| {
                            dispatch(&target, &config.grants, peer, request, input, deadline)
                        });
                    }
                })?;
            session = Some(Session { socket, worker });
        }
        Ok(())
    })();
    drop(listener);
    if let Some(session) = session {
        // The clone shares the same socket and closes blocked handshake/input/
        // output. It adds no session and cannot authorize an operation replay.
        let _ = session.socket.shutdown(Shutdown::Both);
        session.worker.join().map_err(|_| Code::Io)?;
    }
    result
}

fn authorized(grants: &[ControlGrant], peer: &VerifiedPeer, operation: u8) -> Result<(), Failure> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Code::Denied)?
        .as_secs();
    if !grants.iter().any(|grant| {
        grant.public == *peer.public_key()
            && grant.operations & operation != 0
            && grant.expires_unix > now
    }) {
        return Err(Code::Denied.into());
    }
    Ok(())
}

fn dispatch(
    target: &Target,
    grants: &[ControlGrant],
    peer: &VerifiedPeer,
    request: &Request,
    input: &mut dyn Read,
    deadline: Instant,
) -> Result<Response, Failure> {
    request.validate()?;
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let (requested_workspace, requested_incarnation, operation) = match &request.operation {
        Operation::WorkspaceStatus {
            workspace,
            incarnation,
        } => (workspace, incarnation, 1),
        Operation::WorkspaceUnmount {
            workspace,
            incarnation,
        } => (workspace, incarnation, 2),
        Operation::WorkspaceCloseClean {
            workspace,
            incarnation,
        } => (workspace, incarnation, 4),
        Operation::WorkspaceMount {
            workspace,
            incarnation,
        } => (workspace, incarnation, 8),
        _ => return Err(Code::Unsupported.into()),
    };
    authorized(grants, peer, operation)?;
    if requested_workspace.as_slice() != target.workspace.id().as_bytes()
        || *requested_incarnation != target.incarnation
    {
        return Err(Code::Denied.into());
    }
    // Even an empty logical input requires its authenticated END_INPUT frame.
    if input.read(&mut [0u8; 1])? != 0 {
        return Err(Code::InvalidInput.into());
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    authorized(grants, peer, operation)?;
    if target.stopping.load(Ordering::Acquire) {
        return Err(Code::Busy.into());
    }
    if operation != 1 {
        // Reserve terminal delivery time from the original request budget. This
        // is an observation bound; it cannot preempt a native kernel syscall.
        let native_deadline = deadline
            .checked_sub(Duration::from_millis(100))
            .ok_or(Code::Deadline)?;
        let mut result = Box::new(WorkspaceLifecycleWire {
            workspace: requested_workspace.clone(),
            incarnation: target.incarnation,
            outcome: WorkspaceLifecycleOutcome::Completed,
        });
        result.validate()?;
        if Instant::now() >= native_deadline {
            return Err(Code::Deadline.into());
        }
        let mut mount = target.mount.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => Code::Busy,
            TryLockError::Poisoned(_) => Code::Io,
        })?;
        let local = target.workspace.status().map_err(|_| Code::Io)?;
        if operation == 8 && (mount.is_some() || local.mounted || local.stopping || local.closed) {
            return Err(Code::Busy.into());
        }
        authorized(grants, peer, operation)?;
        if Instant::now() >= native_deadline {
            return Err(Code::Deadline.into());
        }
        if target.stopping.load(Ordering::Acquire) {
            return Err(Code::Busy.into());
        }
        let outcome = if operation == 2 {
            match mount.as_mut() {
                Some(owner) => owner.unmount(native_deadline).map(|()| *mount = None),
                None => Ok(()),
            }
        } else if operation == 8 {
            match layerfs_fuse::mount(&target.workspace, native_deadline) {
                Ok(owner) => {
                    *mount = Some(owner);
                    Ok(())
                }
                Err(failure) => {
                    let failure = *failure;
                    if failure.retained.is_none() {
                        return Err(mount_failure_code(&failure.cause).into());
                    }
                    // Custody precedes the entered-attempt result, including
                    // when its terminal is subsequently lost in transport.
                    *mount = failure.retained;
                    Err(failure.cause)
                }
            }
        } else if local.closed {
            Ok(())
        } else {
            match target.workspace.close_clean_until(native_deadline) {
                Ok(()) | Err(WorkspaceError::Closed) => Ok(()),
                Err(error) => Err(MountError::Workspace(error)),
            }
        };
        drop(mount);
        if let Err(error) = outcome {
            result.outcome = WorkspaceLifecycleOutcome::Retained(mount_failure_code(&error));
            let operation = if operation == 2 {
                "unmount"
            } else if operation == 8 {
                "mount"
            } else {
                "close clean"
            };
            layerfs_bridge::adapters::native::pipe::diagnostic(&format!(
                "control {operation} retained: {error}\n"
            ));
        }
        return Ok(if operation == 2 {
            Response::WorkspaceUnmount(result)
        } else if operation == 8 {
            Response::WorkspaceMount(result)
        } else {
            Response::WorkspaceCloseClean(result)
        });
    }
    let local = target.workspace.status().map_err(|_| Code::Io)?;
    let result = WorkspaceStatusWire {
        workspace: requested_workspace.clone(),
        incarnation: target.incarnation,
        mounted: local.mounted,
        stopping: local.stopping,
        closed: local.closed,
        active_operations: local.active_operations as u64,
        nodes: local.nodes as u64,
        handles: local.handles as u64,
        cookies: local.cookies as u64,
        consumer_accounted_bytes: local.accounted_bytes as u64,
    };
    result.validate()?;
    Ok(Response::WorkspaceStatus(Box::new(result)))
}

fn mount_failure_code(error: &MountError) -> Code {
    match error {
        MountError::Deadline | MountError::Workspace(WorkspaceError::Deadline) => Code::Deadline,
        MountError::Workspace(WorkspaceError::Backing(failure))
            if failure.kind == io::ErrorKind::TimedOut =>
        {
            Code::Deadline
        }
        MountError::Unsupported | MountError::Workspace(WorkspaceError::Unsupported) => {
            Code::Unsupported
        }
        MountError::Workspace(WorkspaceError::Busy | WorkspaceError::Closed) => Code::Busy,
        _ => Code::Io,
    }
}
