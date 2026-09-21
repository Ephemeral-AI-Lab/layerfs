//! One authenticated Status session; the acceptor owns refusal and shutdown.
use crate::config::{ControlConfig, StatusGrant};
use layerfs_bridge::{
    adapters::native::{connection::accept, server::serve},
    contract::{Code, Failure, Operation, Request, Response, VerifiedPeer, WorkspaceStatusWire},
};
use layerfs_workspace::Workspace;
use std::{
    io::{self, Read},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Session {
    socket: TcpStream,
    worker: JoinHandle<()>,
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
    ) -> Result<Self, Failure> {
        listener.set_nonblocking(true)?;
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopping);
        let worker = thread::Builder::new()
            .name("layerfs-control".into())
            .stack_size(2 * 1024 * 1024)
            .spawn(move || run(config, listener, private, workspace, incarnation, &stop))?;
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
    workspace: Workspace,
    incarnation: [u8; 32],
    stopping: &AtomicBool,
) -> Result<(), Failure> {
    let config = Arc::new(config);
    let mut session: Option<Session> = None;
    let result = (|| {
        while !stopping.load(Ordering::Acquire) {
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
            if session.is_some() || stopping.load(Ordering::Acquire) {
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            let socket = stream.try_clone()?;
            let config = Arc::clone(&config);
            let workspace = workspace.clone();
            let worker = thread::Builder::new()
                .name("layerfs-control-session".into())
                .stack_size(2 * 1024 * 1024)
                .spawn(move || {
                    if let Ok(connection) = accept(stream, &private, &config.peers) {
                        let _ = serve(connection, |peer, request, input, _, deadline| {
                            status(
                                &workspace,
                                incarnation,
                                &config.grants,
                                peer,
                                request,
                                input,
                                deadline,
                            )
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

fn authorized(grants: &[StatusGrant], peer: &VerifiedPeer) -> Result<(), Failure> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Code::Denied)?
        .as_secs();
    if !grants
        .iter()
        .any(|grant| grant.public == *peer.public_key() && grant.status && grant.expires_unix > now)
    {
        return Err(Code::Denied.into());
    }
    Ok(())
}

fn status(
    workspace: &Workspace,
    incarnation: [u8; 32],
    grants: &[StatusGrant],
    peer: &VerifiedPeer,
    request: &Request,
    input: &mut dyn Read,
    deadline: Instant,
) -> Result<Response, Failure> {
    request.validate()?;
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    authorized(grants, peer)?;
    let Operation::WorkspaceStatus {
        workspace: target,
        incarnation: requested_incarnation,
    } = &request.operation
    else {
        return Err(Code::Unsupported.into());
    };
    if target.as_slice() != workspace.id().as_bytes() || *requested_incarnation != incarnation {
        return Err(Code::Denied.into());
    }
    // Even an empty logical input requires its authenticated END_INPUT frame.
    if input.read(&mut [0u8; 1])? != 0 {
        return Err(Code::InvalidInput.into());
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    authorized(grants, peer)?;
    let local = workspace.status().map_err(|_| Code::Io)?;
    let result = WorkspaceStatusWire {
        workspace: target.clone(),
        incarnation,
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
