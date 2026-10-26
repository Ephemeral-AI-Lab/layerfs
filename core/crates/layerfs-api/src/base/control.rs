//! One attempted authenticated control exchange, retaining the original unknown.
use layerfs_bridge::{
    control::{Answer, Call, ControlError, Reply, Request, SdkValue},
    duplex::{ChannelError, ChannelWork, Connection},
};
use std::fmt;
/// One sequential control connection. Concurrent callers may use separate channels.
pub struct Control {
    connection: Connection,
    next: u64,
    failed: bool,
    /// While set, ordinary requests travel as traced observations.
    traced: Option<[u8; 32]>,
    /// Telemetry of traced replies, kept until the caller takes it.
    telemetry: Vec<SdkValue>,
}
/// Most traced replies kept between takes; later telemetry is dropped.
const TELEMETRY: usize = 16;
/// Exact point at which the original control exchange stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlPhase {
    /// No send has entered.
    Encode,
    /// Original send was attempted.
    Send,
    /// Original reply was awaited once.
    Receive,
    /// Received correlation/result validation.
    Validate,
    /// Session end was acknowledged; its original local socket fence was attempted.
    Fence,
}
/// Original transport/protocol failure; a failed attempted request can have effects.
#[derive(Debug)]
pub enum ControlCause {
    /// Original authenticated channel failure.
    Channel(ChannelError),
    /// Original bounded record/correlation refusal.
    Protocol(ControlError),
}
/// Exact request and reply custody; neither a reconnect nor a reread settles it.
#[derive(Debug)]
pub struct ControlFailure {
    /// Original request/correlation.
    pub call: Call,
    /// Original stopping point.
    pub phase: ControlPhase,
    /// True once sending was attempted, even if no acknowledgement arrived.
    pub attempted: bool,
    /// Original received reply, retained when it failed correlation validation.
    pub received: Option<Answer>,
    /// Original failure.
    pub cause: ControlCause,
    /// Separately retained failed socket fence.
    pub fence_error: Option<ChannelError>,
}
impl Control {
    /// Takes a completed authenticated channel, including one used for installation.
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            next: 1,
            failed: false,
            traced: None,
            telemetry: Vec::new(),
        }
    }
    /// Sends later ordinary requests as traced observations under `scope`,
    /// or plainly again with `None`. Results are unchanged either way.
    pub(crate) fn trace(&mut self, scope: Option<[u8; 32]>) {
        self.traced = scope;
    }
    /// The telemetry of the traced replies received since the last take.
    pub(crate) fn take_telemetry(&mut self) -> Vec<SdkValue> {
        std::mem::take(&mut self.telemetry)
    }
    /// True once an exchange on this connection failed: it is quarantined,
    /// and every later call on it is refused before sending.
    pub fn is_failed(&self) -> bool {
        self.failed
    }
    /// Sends once and awaits the original reply once. Typed remote refusals are
    /// replies; failed delivery retains an unknown, never replays the command.
    pub fn call(&mut self, request: Request) -> Result<Reply, Box<ControlFailure>> {
        self.call_with(request, &[])
    }
    /// As [`Control::call`], with `bodies` sent after the request record, each
    /// in records of at most one channel plaintext, before the one reply is
    /// awaited. The request names their lengths; the daemon reads exactly
    /// those bytes, a record at a time. A failure while sending them is a
    /// failed attempted send: the request may have taken effect.
    pub(crate) fn call_with(
        &mut self,
        request: Request,
        bodies: &[Vec<u8>],
    ) -> Result<Reply, Box<ControlFailure>> {
        let request = match (self.traced, request) {
            (Some(scope), request)
                if !matches!(
                    request,
                    Request::Observed { .. } | Request::Hello(_) | Request::EndSession
                ) =>
            {
                Request::Observed {
                    scope,
                    request: Box::new(request),
                    traced: true,
                }
            }
            (_, request) => request,
        };
        let traced = matches!(request, Request::Observed { traced: true, .. });
        let call = Call {
            id: self.next,
            request,
        };
        let mut phase = ControlPhase::Encode;
        let mut attempted = false;
        let mut received = None;
        let result = (|| {
            if self.failed {
                return Err(ControlCause::Channel(ChannelError::Quarantined));
            }
            let record = call.encode().map_err(ControlCause::Protocol)?;
            let next = self
                .next
                .checked_add(1)
                .ok_or(ControlCause::Protocol(ControlError(
                    "control correlation exhausted",
                )))?;
            self.next = next;
            phase = ControlPhase::Send;
            attempted = true;
            self.connection
                .send
                .send(&record)
                .map_err(ControlCause::Channel)?;
            for body in bodies {
                for record in body.chunks(layerfs_bridge::duplex::MAX_PLAINTEXT_BYTES) {
                    self.connection
                        .send
                        .send(record)
                        .map_err(ControlCause::Channel)?;
                }
            }
            phase = ControlPhase::Receive;
            let answer = Answer::decode(
                self.connection
                    .receive
                    .receive()
                    .map_err(ControlCause::Channel)?,
            )
            .map_err(ControlCause::Protocol)?;
            phase = ControlPhase::Validate;
            received = Some(answer);
            let answer = received.as_ref().expect("original answer");
            // A traced answer is validated by the operation's own reply.
            let reply = match &answer.reply {
                Reply::Observed { reply, .. } if traced => reply.as_ref(),
                reply => reply,
            };
            if answer.id != call.id || !matches_reply(&call.request, reply) {
                return Err(ControlCause::Protocol(ControlError(
                    "original control correlation or result",
                )));
            }
            if matches!(answer.reply, Reply::SessionEnded) {
                self.failed = true;
                phase = ControlPhase::Fence;
                // Quarantine the underlying owner as well: transferring it into
                // another Control cannot reset an ended duplex session.
                self.connection
                    .send
                    .close()
                    .map_err(ControlCause::Channel)?;
            }
            Ok(
                match received.take().expect("checked original answer").reply {
                    Reply::Observed { reply, telemetry } => {
                        if self.telemetry.len() < TELEMETRY {
                            self.telemetry.push(telemetry);
                        }
                        *reply
                    }
                    reply => reply,
                },
            )
        })();
        match result {
            Ok(reply) => Ok(reply),
            Err(cause) => {
                let fence_error = if attempted {
                    self.failed = true;
                    if matches!(cause, ControlCause::Channel(_)) {
                        None
                    } else {
                        self.connection.send.close().err()
                    }
                } else {
                    None
                };
                Err(Box::new(ControlFailure {
                    call,
                    phase,
                    attempted,
                    received,
                    cause,
                    fence_error,
                }))
            }
        }
    }
    /// Installs one sealed Project through this same control owner without resetting correlation.
    /// Failed attempted installation retains its original failure and makes this owner terminal.
    pub(crate) fn install_project(
        &mut self,
        project: &crate::SealedProject,
    ) -> Result<crate::Installed, Box<crate::InstallFailure>> {
        self.install_file(&project.store, &project.manifest)
    }
    pub(crate) fn install_file(
        &mut self,
        store: &layerfs_persistence::SealedStore,
        manifest: &layerfs_bridge::provision::StoreManifest,
    ) -> Result<crate::Installed, Box<crate::InstallFailure>> {
        if self.failed {
            return Err(Box::new(crate::InstallFailure {
                source: store.path.clone(),
                manifest: manifest.clone(),
                phase: layerfs_bridge::control::InstallPhase::Request,
                request_attempted: false,
                accepted: false,
                acknowledged: None,
                work: crate::InstallWork::default(),
                error: crate::InstallError::Channel(ChannelError::Quarantined),
                fence_error: None,
            }));
        }
        layerfs_project::store::install_file(store, manifest, &mut self.connection).inspect_err(
            |failure| {
                if failure.request_attempted {
                    self.failed = true;
                }
            },
        )
    }
    pub(crate) fn clear_io_waits(&self) -> Result<(), ChannelError> {
        self.connection.clear_io_waits()
    }
    /// Actual cumulative duplex send/receive work for this connection.
    pub fn channel_work(&self) -> (ChannelWork, ChannelWork) {
        (self.connection.send.work(), self.connection.receive.work())
    }
    /// Transfers the original channel owner; no reconnect or failure reset occurs.
    pub(crate) fn into_connection(self) -> Connection {
        self.connection
    }
}
fn matches_reply(request: &Request, reply: &Reply) -> bool {
    let Ok(request) = request.without_observation() else {
        return false;
    };
    match (request, reply) {
        (_, Reply::Refused(_)) => true,
        (
            Request::Cleanup(token),
            Reply::Cleanup {
                token: observed, ..
            },
        ) => token == observed,
        (Request::EndSession, Reply::SessionEnded) => true,
        (Request::Sdk(_), Reply::Sdk(_) | Reply::SdkRefused(_)) => true,
        (Request::Hello(request), Reply::Hello(status)) => request
            .expected_instance
            .is_none_or(|expected| expected == status.instance),
        (Request::Mount { workspace, branch }, Reply::Bound { token, binding }) => {
            token.workspace == *workspace && binding.branch.id == *branch
        }
        (
            Request::Commit(_),
            Reply::Committed(_) | Reply::MergeCommitted(_) | Reply::Conflict(_),
        ) => true,
        (Request::Status(token), Reply::Status(status)) => *token == status.token,
        (Request::Unmount(control), Reply::Unmounted(closed)) => control.token == *closed,
        (Request::Unmount(control), Reply::Dirty(report)) => control.token == report.token,
        (Request::Attach(token), Reply::Ready(ready)) => *token == ready.token,
        (Request::ForceUnmount { token, .. }, Reply::ForceUnmounted(closed)) => {
            *token == closed.token
        }
        (
            Request::Attach(token) | Request::ForceUnmount { token, .. },
            Reply::Retained(custody),
        ) => *token == custody.token,
        (Request::Unmount(control), Reply::Retained(custody)) => control.token == custody.token,
        (Request::Locate(workspace), Reply::Located(status)) => {
            *workspace == status.token.workspace
        }
        (Request::Fork(request), Reply::Forked(binding)) => {
            request.branch == binding.branch.id
                && request.stack == binding.branch.stack
                && request.name == binding.branch.name
        }
        (
            Request::Branches {
                stack,
                page: request,
            },
            Reply::Branches(page),
        ) => {
            page.records.len() <= request.limit as usize
                && (!page.records.is_empty() || page.continuation.is_none())
                && page.records.iter().all(|record| record.stack == *stack)
                && page
                    .records
                    .windows(2)
                    .all(|rows| rows[0].name.as_str() < rows[1].name.as_str())
        }
        (Request::History(request), Reply::History(page)) => {
            page.records.len() <= request.limit as usize
                && (!page.records.is_empty() || page.continuation.is_none())
        }
        _ => false,
    }
}

impl fmt::Display for ControlFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "control {:?}: {:?}", self.phase, self.cause)
    }
}
impl std::error::Error for ControlFailure {}
