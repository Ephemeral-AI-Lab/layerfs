//! A Sandbox's control connections (#352 F-017).
//!
//! The daemon serves each authenticated connection on its own worker, so
//! concurrent callers need separate connections: one shared connection made
//! every call wait for the one before it, a 1 ms read behind a whole Commit. A
//! Sandbox keeps up to [`CONNECTIONS`] connections and opens one only when
//! every open one is busy. Each exchange runs whole on one connection, sent
//! once. A connection whose exchange failed is quarantined by its `Control`
//! and is closed rather than reused; the failed call is never sent again.
//! When the daemon admits no more connections, the Sandbox keeps the ones it
//! has and callers take turns on them. Opening a connection is not an attempt
//! of any operation: nothing is sent before the connection is ready.
use crate::outcome::{failed, refused};
use layerfs_api::internal::Control;
use layerfs_bridge::control::{DaemonPhase, HelloRequest, Reply, Request, SdkRefusal, SdkValue};
use layerfs_bridge::duplex::initiate_observed;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Most control connections one process keeps to one Sandbox. A Sandbox this
/// binding creates admits [`crate::sandbox::DAEMON_CONNECTIONS`]; the rest
/// stay for other processes.
pub(crate) const CONNECTIONS: usize = 6;

/// The host this process dials for a daemon's loopback-published port:
/// `deploy.daemonHost` (the client's `daemonHost`, else LAYERFS_DAEMON_HOST),
/// for a client in a container that does not share the Docker host's
/// loopback. `None` dials 127.0.0.1 as published.
pub(crate) fn daemon_host(args: &SdkValue) -> Option<String> {
    args.get("deploy")
        .and_then(|deploy| deploy.get_text("daemonHost"))
        .filter(|host| !host.is_empty())
        .map(str::to_owned)
}

/// How to open another connection to the same daemon instance.
#[derive(Clone)]
pub(crate) struct Dial {
    pub(crate) endpoint: SocketAddr,
    /// Host to dial instead of the endpoint's loopback address
    /// ([`daemon_host`]); `None` dials the endpoint.
    pub(crate) host: Option<String>,
    pub(crate) secret: [u8; 32],
    pub(crate) peer: [u8; 32],
    pub(crate) instance: [u8; 32],
}

impl Dial {
    /// One authenticated connection, checked to reach the same daemon.
    pub(crate) fn open(&self) -> Result<Control, SdkRefusal> {
        let address =
            layerfs_api::connect_address(self.endpoint, self.host.as_deref()).map_err(failed)?;
        let stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(10))
            .map_err(failed)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(failed)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .map_err(failed)?;
        let connection = initiate_observed(stream, &self.secret, self.peer)
            .result
            .map_err(failed)?;
        connection.clear_io_waits().map_err(failed)?;
        let mut control = Control::new(connection);
        match control
            .call(Request::Hello(HelloRequest {
                expected_instance: Some(self.instance),
                wait_for_store: false,
            }))
            .map_err(failed)?
        {
            Reply::Hello(status) if status.phase == DaemonPhase::ControlReady => Ok(control),
            _ => Err(refused("StaleSandbox", "saved daemon is not ControlReady")),
        }
    }
}

struct Pool {
    idle: Vec<Control>,
    /// Connections open, idle or lent.
    open: usize,
    /// Most connections this Sandbox keeps: [`CONNECTIONS`], or fewer once
    /// the daemon admitted no more.
    limit: usize,
}

/// A Sandbox's connections.
pub(crate) struct Controls {
    pool: Mutex<Pool>,
    freed: Condvar,
    dial: Dial,
}

/// One connection lent for one call's exchanges.
pub(crate) struct Lease<'a> {
    controls: &'a Controls,
    control: Option<Control>,
}

impl Controls {
    /// The connections of a Sandbox whose first connection is `first`.
    pub(crate) fn new(first: Control, dial: Dial) -> Self {
        Self {
            pool: Mutex::new(Pool {
                idle: vec![first],
                open: 1,
                limit: CONNECTIONS,
            }),
            freed: Condvar::new(),
            dial,
        }
    }

    /// A connection for one call, and how long the call waited for it.
    pub(crate) fn lend(&self) -> Result<(Lease<'_>, Duration), SdkRefusal> {
        let started = Instant::now();
        let poisoned = || refused("Failed", "binding state poisoned");
        let mut pool = self.pool.lock().map_err(|_| poisoned())?;
        loop {
            if let Some(control) = pool.idle.pop() {
                return Ok((self.lease(control), started.elapsed()));
            }
            if pool.open < pool.limit {
                pool.open += 1;
                drop(pool);
                let opened = self.dial.open();
                pool = self.pool.lock().map_err(|_| poisoned())?;
                match opened {
                    Ok(control) => return Ok((self.lease(control), started.elapsed())),
                    Err(refusal) => {
                        pool.open -= 1;
                        if pool.open == 0 {
                            return Err(refusal);
                        }
                        // The daemon admits no more: take turns on the open ones.
                        pool.limit = pool.open;
                        continue;
                    }
                }
            }
            pool = self.freed.wait(pool).map_err(|_| poisoned())?;
        }
    }

    fn lease(&self, control: Control) -> Lease<'_> {
        Lease {
            controls: self,
            control: Some(control),
        }
    }
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let Some(control) = self.control.take() else {
            return;
        };
        let mut pool = self
            .controls
            .pool
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if control.is_failed() {
            // Quarantined: closed, and a later call may open another.
            pool.open -= 1;
            drop(control);
        } else {
            pool.idle.push(control);
        }
        drop(pool);
        self.controls.freed.notify_one();
    }
}

impl Deref for Lease<'_> {
    type Target = Control;
    fn deref(&self) -> &Control {
        self.control.as_ref().expect("lent connection")
    }
}

impl DerefMut for Lease<'_> {
    fn deref_mut(&mut self) -> &mut Control {
        self.control.as_mut().expect("lent connection")
    }
}
