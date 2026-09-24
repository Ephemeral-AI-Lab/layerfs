//! Bounded reuse of one authenticated Workspace control session.
//!
//! Every Workspace API call otherwise opens a TCP connection, completes the
//! Noise handshake and sends a checked Hello before the operation it was asked
//! for. Consecutive calls on one mounted Workspace are rapid, so the session
//! established by a checked lookup is retained and reused for the following
//! operation.
//!
//! The retention is deliberately narrow:
//!
//! - Reuse is keyed by sandbox and daemon instance. Before every operation the
//!   caller revalidates both, so a restarted daemon cannot answer an operation
//!   addressed to its predecessor.
//! - Only a *successful* operation keeps the session. Any failure discards it,
//!   so a broken or uncertain operation is never resent on a live socket; the
//!   next call performs a fresh checked lookup.
//! - The idle window stays well inside the control server's five-second idle
//!   timeout and the one-session admission, so a retained session is either
//!   provably live or replaced.
//! - Request identifiers stay monotone because the native `Client` carries its
//!   own `previous` marker, and the Hello occupies identifier 1.
use layerfs_api_core::SandboxId;
use layerfs_bridge::{
    adapters::native::client::Client,
    contract::{
        Code, Failure, Operation, Request, Response, SandboxHelloWire, WORKSPACE_STATUS_PROFILE,
    },
};
use std::{
    io,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Longest idle gap that still permits reuse. The control server ends an idle
/// session after five seconds; staying inside that bound keeps a reused socket
/// provably live from this side.
const IDLE_LIMIT: Duration = Duration::from_secs(2);

/// One retained control connection with the identity it was validated against.
///
/// `next_id` is the identifier the following operation will carry. The control
/// server rejects a request whose identifier does not exceed the previous one
/// on the same connection, so a reused session continues the sequence rather
/// than restarting it.
///
/// `session` is the connection's own incarnation marker, observed at Hello.
/// A restarted daemon reports a different marker, so a reused socket is only
/// handed back when the live daemon still claims the incarnation the caller
/// validated.
struct Retained {
    sandbox: SandboxId,
    instance: [u8; 32],
    client: Client,
    next_id: u64,
    idle_since: Instant,
}

/// Retains at most one checked control session across rapid Workspace calls.
#[derive(Default)]
pub(crate) struct Sessions {
    retained: Mutex<Option<Retained>>,
}

/// A connection to run one operation on, plus the identifier it must carry.
pub(crate) struct Lease {
    pub client: Client,
    pub id: u64,
}

impl Sessions {
    /// Runs `check` to obtain a freshly validated connection when no retained
    /// session matches `sandbox`/`instance`, and otherwise hands back the
    /// retained one after revalidating it with a Hello on that same socket.
    ///
    /// Revalidating on the retained socket is what makes reuse safe: the
    /// daemon is authoritative about its own instance, so a restarted daemon
    /// cannot answer an operation addressed to its predecessor. A socket the
    /// restart closed fails here, and the caller sees the failure rather than
    /// a replayed operation.
    pub(crate) fn lease(
        &self,
        sandbox: SandboxId,
        instance: [u8; 32],
        deadline: Instant,
        check: impl FnOnce() -> Result<(SandboxHelloWire, Client), Failure>,
    ) -> Result<Lease, Failure> {
        let mut retained = self.retained.lock().map_err(|_| Failure::from(Code::Io))?;
        if let Some(session) = retained.as_mut() {
            let usable = session.idle_since.elapsed() <= IDLE_LIMIT
                && session.sandbox == sandbox
                && session.instance == instance;
            if usable {
                match hello(&mut session.client, session.next_id, deadline) {
                    Ok(live) if live.instance == instance && live.sandbox == sandbox.0 => {
                        let session = retained.take().expect("retained session present");
                        return Ok(Lease {
                            client: session.client,
                            id: session.next_id + 1,
                        });
                    }
                    // A restarted or unreachable daemon invalidates the socket.
                    _ => *retained = None,
                }
            } else {
                *retained = None;
            }
        }
        drop(retained);
        let (_, client) = check()?;
        // The checked Hello occupied identifier 1.
        Ok(Lease { client, id: 2 })
    }

    /// Retains a session after a successful operation, replacing any previous
    /// one. `next_id` is the identifier the following operation will carry.
    pub(crate) fn retain(
        &self,
        sandbox: SandboxId,
        instance: [u8; 32],
        next_id: u64,
        client: Client,
    ) {
        let Ok(mut retained) = self.retained.lock() else {
            return;
        };
        *retained = Some(Retained {
            sandbox,
            instance,
            client,
            next_id,
            idle_since: Instant::now(),
        });
    }

    /// Drops any retained session. Called on every failure and on shutdown.
    pub(crate) fn discard(&self) {
        if let Ok(mut retained) = self.retained.lock() {
            *retained = None;
        }
    }
}

/// Sends one Hello on a live connection to revalidate the daemon instance.
fn hello(client: &mut Client, id: u64, deadline: Instant) -> Result<SandboxHelloWire, Failure> {
    let request = Request {
        id,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms: 5_000,
        response_bytes: 0,
        operation: Operation::SandboxHello,
    };
    let response = client.call_until(&request, &mut &[][..], &mut io::sink(), deadline)?;
    match response {
        Response::SandboxHello(hello) => Ok(hello),
        _ => Err(Code::Integrity.into()),
    }
}

/// Runs one Workspace operation on a session whose checked Hello was its
/// request 1.
pub(crate) fn call(
    client: &mut Client,
    id: u64,
    deadline_ms: u32,
    operation: Operation,
) -> Result<Response, Failure> {
    let deadline = Instant::now() + Duration::from_millis(u64::from(deadline_ms));
    let request = Request {
        id,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms,
        response_bytes: 0,
        operation,
    };
    client.call_until(&request, &mut &[][..], &mut io::sink(), deadline)
}
