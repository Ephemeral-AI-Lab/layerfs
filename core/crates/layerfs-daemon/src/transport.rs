//! Sequence-scoped reuse of the authenticated daemon-to-host Service transport.
//!
//! One connection and its authenticated handshake serve a bounded run of
//! consecutive upstream calls. The native server admits many successful
//! requests per connection, so a Mount that issues HistoryQuery, Inspect and
//! a Commit that issues EditFile, UpdatePortableMetadata and HistoryCommand
//! no longer pay a fresh TCP connect and Noise handshake each.
//!
//! Reuse is deliberately narrow:
//!
//! - The daemon keeps at most one session across delivery threads. Its lock
//!   serializes calls, and a lower request ID opens a new session before use.
//! - Reuse never crosses a failure. Any error, including a remote refusal,
//!   drops the connection and the next call opens a fresh one. A mutation
//!   whose outcome is uncertain is never resent on a reused socket.
//! - The idle window is short and bounded. A session left idle past it is
//!   closed and replaced, so a server that has already timed out its side
//!   cannot leave a client believing in a live connection.
//! - Request identifiers stay monotone across the whole session because the
//!   native `Client` carries its own `previous` marker.
use layerfs_bridge::{
    adapters::native::{client::Client, connection::connect_until},
    contract::{Failure, Request, Response, Source},
};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{
    io::Write,
    net::SocketAddr,
    time::{Duration, Instant},
};

/// Longest idle gap that still permits reuse. The Service server ends an idle
/// connection after five seconds; staying well inside that bound keeps a
/// reused socket provably live from this side.
const IDLE_LIMIT: Duration = Duration::from_secs(2);

/// A bounded run of upstream calls sharing one authenticated connection.
pub(crate) struct Transport {
    address: SocketAddr,
    selector: u32,
    private: [u8; 32],
    server: [u8; 32],
    session: Option<Session>,
}

struct Session {
    client: Client,
    idle_since: Instant,
}

impl Transport {
    pub(crate) fn new(
        address: SocketAddr,
        selector: u32,
        private: [u8; 32],
        server: [u8; 32],
    ) -> Self {
        Self {
            address,
            selector,
            private,
            server,
            session: None,
        }
    }

    /// Delivers one request on the retained session, or on a fresh one when the
    /// retained session is absent, idle past its bound, or failed.
    ///
    /// The first failure closes the session and is returned unchanged. Nothing
    /// is retried: the caller sees the original outcome.
    pub(crate) fn call(
        &mut self,
        request: &Request,
        input: &mut dyn Source,
        output: &mut dyn Write,
        deadline: Instant,
        scope: &TimingScope<'_, Active>,
    ) -> Result<Response, Failure> {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.idle_since.elapsed() > IDLE_LIMIT)
        {
            self.session = None;
        }
        if self.session.is_none() {
            let connection = scope.child("daemon.service_connect").run(|_| {
                connect_until(
                    self.address,
                    self.selector,
                    &self.private,
                    &self.server,
                    deadline,
                )
            })?;
            let client = scope
                .child("daemon.service_hello")
                .run(|_| Client::new(connection))?;
            self.session = Some(Session {
                client,
                idle_since: Instant::now(),
            });
        }
        let session = self.session.as_mut().expect("session opened above");
        let result = scope
            .child("daemon.service_call")
            .run(|_| session.client.call_until(request, input, output, deadline));
        session.idle_since = Instant::now();
        if result.is_err() {
            self.session = None;
        }
        result
    }
}
