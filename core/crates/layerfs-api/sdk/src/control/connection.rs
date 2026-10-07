//! One attempted authenticated control exchange, retaining the original unknown.
use layerfs_bridge::{
    control::{Answer, Call, ControlError, Reply, Request},
    native::{ChannelError, ChannelWork, Connection},
};
use std::fmt;
/// One sequential control connection. Concurrent callers may use separate channels.
pub struct Control {
    connection: Connection,
    next: u64,
    failed: bool,
}
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
        }
    }
    /// Sends once and awaits the original reply once. Typed remote refusals are
    /// replies; failed delivery retains an unknown, never replays the command.
    pub fn call(&mut self, request: Request) -> Result<Reply, Box<ControlFailure>> {
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
            if answer.id != call.id || !matches_reply(&call.request, &answer.reply) {
                return Err(ControlCause::Protocol(ControlError(
                    "original control correlation or result",
                )));
            }
            Ok(received.take().expect("checked original answer").reply)
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
    /// Actual cumulative native send/receive work for this connection.
    pub fn channel_work(&self) -> (ChannelWork, ChannelWork) {
        (self.connection.send.work(), self.connection.receive.work())
    }
    /// Transfers the original channel owner; no reconnect or failure reset occurs.
    pub fn into_connection(self) -> Connection {
        self.connection
    }
}
fn matches_reply(request: &Request, reply: &Reply) -> bool {
    match (request, reply) {
        (_, Reply::Refused(_)) => true,
        (Request::Mount { workspace, branch }, Reply::Bound { token, binding }) => {
            token.workspace == *workspace && binding.branch.id == *branch
        }
        (Request::Commit(_), Reply::Committed(_)) => true,
        (Request::Status(token), Reply::Status(status)) => *token == status.token,
        (Request::Unmount(token), Reply::Unmounted(closed)) => token == closed,
        (Request::Fork(request), Reply::Forked(binding)) => {
            request.branch == binding.branch.id
                && request.stack == binding.branch.stack
                && request.name == binding.branch.name
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
