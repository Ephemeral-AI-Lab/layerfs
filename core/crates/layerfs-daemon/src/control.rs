//! One authenticated control session; the acceptor owns refusal and shutdown.
use crate::{
    config::{ControlConfig, ControlGrant},
    lifecycle::{mount_failure_code, Lifecycle},
};
use layerfs_bridge::{
    adapters::native::{connection::accept, server::serve},
    contract::{
        Code, Failure, Operation, Request, Response, VerifiedPeer, WorkspaceLifecycleOutcome,
        WorkspaceLifecycleWire,
    },
};
use layerfs_fuse::MountError;
use layerfs_telemetry::timer::{Active, TimingScope};
use nix::poll::{poll, PollFd, PollFlags};
use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    os::fd::AsFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, TryLockError,
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
    lifecycle: Arc<Lifecycle>,
    stopping: Arc<AtomicBool>,
    identity: Option<layerfs_bridge::contract::SandboxHelloWire>,
    telemetry: layerfs_telemetry::runtime::Runtime,
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
        lifecycle: Arc<Lifecycle>,
        telemetry: layerfs_telemetry::runtime::Runtime,
    ) -> Result<Self, Failure> {
        listener.set_nonblocking(true)?;
        let stopping = Arc::new(AtomicBool::new(false));
        let target = Target {
            lifecycle,
            stopping: Arc::clone(&stopping),
            identity: config.identity.clone(),
            telemetry,
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

    /// Called under lifecycle ownership after checked closure, before that slot
    /// can admit a replacement. A failed cleanup leaves control available.
    pub fn stop_admission(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    /// Join after ending admission. A timeout retains the control thread owner.
    pub fn stop(&mut self, deadline: Instant) -> Result<(), Failure> {
        self.stop_admission();
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
                    let mut ready = [PollFd::new(listener.as_fd(), PollFlags::POLLIN)];
                    poll(&mut ready, 10u16).map_err(|_| Code::Io)?;
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
                        let _ = serve(connection, |peer, request, input, output, deadline| {
                            let (result, diagnostic) = target.telemetry.recorder().run(
                                request.id,
                                request.operation.label(),
                                |scope| {
                                    dispatch(
                                        &target,
                                        &config.grants,
                                        peer,
                                        request,
                                        (input, output),
                                        deadline,
                                        scope,
                                    )
                                },
                            );
                            target.telemetry.publish(diagnostic);
                            result
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
    streams: (&mut dyn Read, &mut dyn Write),
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<Response, Failure> {
    let (input, output) = streams;
    request.validate()?;
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let (requested_workspace, requested_incarnation, operation) = match &request.operation {
        Operation::SandboxHello => {
            authorized(grants, peer, 64)?;
            if input.read(&mut [0u8; 1])? != 0 {
                return Err(Code::InvalidInput.into());
            }
            authorized(grants, peer, 64)?;
            if target.stopping.load(Ordering::Acquire) {
                return Err(Code::Busy.into());
            }
            return Ok(Response::SandboxHello(
                target.identity.clone().ok_or(Code::Unsupported)?,
            ));
        }
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
        Operation::WorkspaceAttach {
            workspace,
            incarnation,
        } => (workspace, incarnation, 16),
        Operation::WorkspaceCommit {
            workspace,
            incarnation,
        } => (workspace, incarnation, 32),
        Operation::WorkspaceOpen {
            workspace,
            incarnation,
            ..
        } => (workspace, incarnation, 16),
        Operation::WorkspaceExec {
            workspace,
            incarnation,
            ..
        } => (workspace, incarnation, 128),
        _ => return Err(Code::Unsupported.into()),
    };
    authorized(grants, peer, operation)?;
    // Even an empty logical input requires its authenticated END_INPUT frame.
    if input.read(&mut [0u8; 1])? != 0 {
        return Err(Code::InvalidInput.into());
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    authorized(grants, peer, operation)?;
    if let Operation::WorkspaceOpen { instance, .. } = &request.operation {
        if target.identity.as_ref().map(|identity| identity.instance) != Some(*instance) {
            return Err(Code::Denied.into());
        }
    }
    if target.stopping.load(Ordering::Acquire) {
        return Err(Code::Busy.into());
    }
    let mut slot = target
        .lifecycle
        .slot
        .try_lock()
        .map_err(|error| match error {
            TryLockError::WouldBlock => Code::Busy,
            TryLockError::Poisoned(_) => Code::Io,
        })?;
    if operation == 16 {
        target.lifecycle.can_attach(&slot)?;
    } else {
        let selected = slot.selected.as_ref().ok_or(Code::Denied)?;
        if requested_workspace.as_slice() != selected.id.as_bytes()
            || *requested_incarnation != selected.incarnation
        {
            return Err(Code::Denied.into());
        }
    }
    if operation == 1 {
        return target.lifecycle.status(&slot);
    }
    // Reserve terminal delivery time from the original request budget. This is
    // an observation bound; it cannot preempt a native kernel syscall.
    let native_deadline = deadline
        .checked_sub(Duration::from_millis(100))
        .ok_or(Code::Deadline)?;
    let mut result = Box::new(WorkspaceLifecycleWire {
        workspace: requested_workspace.clone(),
        incarnation: *requested_incarnation,
        outcome: WorkspaceLifecycleOutcome::Completed,
    });
    result.validate()?;
    let workspace = slot
        .selected
        .as_ref()
        .and_then(|selected| selected.workspace.clone());
    if matches!(operation, 2 | 8) {
        let local = workspace
            .as_ref()
            .ok_or(Code::Busy)?
            .status()
            .map_err(|_| Code::Io)?;
        if operation == 8
            && (slot.mount.is_some() || local.mounted || local.stopping || local.closed)
        {
            return Err(Code::Busy.into());
        }
    }
    authorized(grants, peer, operation)?;
    if Instant::now() >= native_deadline {
        return Err(Code::Deadline.into());
    }
    if target.stopping.load(Ordering::Acquire) {
        return Err(Code::Busy.into());
    }
    if let Operation::WorkspaceOpen {
        project,
        branch,
        commit,
        ..
    } = &request.operation
    {
        let attached = scope.child("daemon.workspace_attach").run(|_| {
            target.lifecycle.attach_selected(
                &mut slot,
                requested_workspace,
                *requested_incarnation,
                layerfs_workspace::Base::BranchAt {
                    project: *project,
                    branch: *branch,
                    commit: *commit,
                },
                native_deadline,
            )
        })?;
        let Response::WorkspaceAttach(mut result) = attached else {
            return Err(Code::Integrity.into());
        };
        if result.outcome == layerfs_bridge::contract::WorkspaceAttachOutcome::Completed {
            let workspace = slot
                .selected
                .as_ref()
                .and_then(|s| s.workspace.as_ref())
                .ok_or(Code::Io)?;
            let mounted = scope
                .child("daemon.fuse_mount")
                .run(|_| crate::lifecycle::mount(workspace, native_deadline));
            match mounted {
                Ok(mount) => slot.mount = Some(mount),
                Err(mut failure) => {
                    slot.mount = failure.retained.take();
                    result.outcome = layerfs_bridge::contract::WorkspaceAttachOutcome::Retained(
                        mount_failure_code(&failure.cause),
                    );
                }
            }
        }
        return Ok(Response::WorkspaceAttach(result));
    }
    if let Operation::WorkspaceExec { command, .. } = &request.operation {
        if slot.mount.is_none() {
            return Err(Code::Busy.into());
        }
        let workspace = slot
            .selected
            .as_ref()
            .and_then(|s| s.workspace.as_ref())
            .ok_or(Code::Busy)?;
        return crate::execution::execute(
            workspace,
            requested_incarnation,
            command,
            output,
            native_deadline,
            scope,
        );
    }
    if operation == 16 {
        return target.lifecycle.attach(
            &mut slot,
            requested_workspace,
            *requested_incarnation,
            native_deadline,
        );
    }
    if operation == 32 {
        let selected = slot.selected.as_ref().ok_or(Code::Denied)?;
        let workspace = selected.workspace.as_ref().ok_or(Code::Busy)?;
        return scope
            .child("daemon.commit")
            .run(|_| crate::control_commit::commit(selected, workspace, native_deadline));
    }
    let outcome = if operation == 2 {
        match slot.mount.as_mut() {
            Some(owner) => owner.unmount(native_deadline).map(|()| slot.mount = None),
            None => Ok(()),
        }
    } else if operation == 8 {
        match crate::lifecycle::mount(workspace.as_ref().ok_or(Code::Busy)?, native_deadline) {
            Ok(owner) => {
                slot.mount = Some(owner);
                Ok(())
            }
            Err(failure) => {
                let failure = *failure;
                if failure.retained.is_none() {
                    return Err(mount_failure_code(&failure.cause).into());
                }
                // Custody precedes the terminal, including a subsequently lost terminal.
                slot.mount = failure.retained;
                Err(failure.cause)
            }
        }
    } else {
        target
            .lifecycle
            .close(&mut slot, native_deadline)
            .map_err(MountError::Workspace)
    };
    drop(slot);
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
    Ok(if operation == 2 {
        Response::WorkspaceUnmount(result)
    } else if operation == 8 {
        Response::WorkspaceMount(result)
    } else {
        Response::WorkspaceCloseClean(result)
    })
}
