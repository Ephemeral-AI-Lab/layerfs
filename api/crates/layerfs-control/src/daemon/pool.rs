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
//! has and callers take turns on them, each waiting at most [`LEND_WAIT`]
//! before it is refused `Busy`. Opening a connection is not an attempt
//! of any operation: nothing is sent before the connection is ready.
use crate::common::contract::Capacity;
use crate::common::outcome::{failed, refused};
use crate::internal::Control;
use layerfs_bridge::control::{DaemonPhase, HelloRequest, Reply, Request, SdkRefusal, SdkValue};
use layerfs_bridge::duplex::initiate_observed;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Most control connections one process keeps to one Sandbox. A Sandbox this
/// binding creates admits [`crate::sandbox::serve::DAEMON_CONNECTIONS`]; the rest
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

/// How long a process waits for the daemon to admit its first connection
/// while other processes hold every admitted one (the same bound as the
/// project's discovery lock).
pub(crate) const ADMISSION_WAIT: Duration = Duration::from_secs(60);

/// How long a call waits for one of this process's connections to come free
/// once every one is lent (#508). Control replies have no deadline, so behind
/// calls that never end an unbounded wait would last forever while holding
/// the caller's discovery turn. The wait is readiness: nothing is attempted
/// before it ends, and its `Busy` refusal is not retried.
pub(crate) const LEND_WAIT: Duration = ADMISSION_WAIT;

/// Whether a failed open is the daemon closing a connection it did not admit:
/// the stream ended before the Hello reply. Through a port proxy the client
/// reads the end (`UnexpectedEof`); dialing the daemon directly, its handshake
/// write can meet the closed socket first (`BrokenPipe`, `ConnectionReset`).
/// Nothing but Hello was sent, so no operation was attempted and opening
/// again is safe.
fn not_admitted(refusal: &SdkRefusal) -> bool {
    refusal.code == "Failed"
        && ["UnexpectedEof", "BrokenPipe", "ConnectionReset"]
            .iter()
            .any(|kind| refusal.message.contains(kind))
}

impl Dial {
    /// The first connection a process opens to a Sandbox. While the daemon
    /// admits no more connections (other processes hold all of them), waits
    /// for one to be released, up to [`ADMISSION_WAIT`], instead of failing
    /// the caller's operation with a bare `UnexpectedEof`.
    pub(crate) fn open_waiting(&self) -> Result<Control, SdkRefusal> {
        self.open_admitted(ADMISSION_WAIT)
    }

    /// [`Dial::open_waiting`], waiting at most `wait`.
    fn open_admitted(&self, wait: Duration) -> Result<Control, SdkRefusal> {
        let started = Instant::now();
        let mut pause = Duration::from_millis(20);
        loop {
            match self.open() {
                Err(refusal) if not_admitted(&refusal) && started.elapsed() < wait => {
                    std::thread::sleep(pause);
                    pause = (pause * 2).min(Duration::from_millis(500));
                }
                Err(refusal) if not_admitted(&refusal) => {
                    return Err(refused(
                        "Busy",
                        format!(
                            "the Sandbox's daemon admitted no connection for {}s: other processes hold all {} of its connections",
                            wait.as_secs(),
                            crate::sandbox::serve::DAEMON_CONNECTIONS
                        ),
                    ))
                }
                other => return other,
            }
        }
    }

    /// One authenticated connection, checked to reach the same daemon.
    pub(crate) fn open(&self) -> Result<Control, SdkRefusal> {
        let address =
            crate::connect_address(self.endpoint, self.host.as_deref()).map_err(failed)?;
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
    /// Callers waiting for a connection now (#551 B8-160).
    waiting: usize,
}

/// A Sandbox's connections.
pub(crate) struct Controls {
    pool: Mutex<Pool>,
    freed: Condvar,
    dial: Dial,
    /// The Store this daemon serves, named in a `Capacity` refusal.
    store: String,
}

/// One connection lent for one call's exchanges.
pub(crate) struct Lease<'a> {
    controls: &'a Controls,
    control: Option<Control>,
}

impl Controls {
    /// The connections of a Sandbox whose first connection is `first`,
    /// serving `store` (named in a `Capacity` refusal).
    pub(crate) fn new(first: Control, dial: Dial, store: impl Into<String>) -> Self {
        Self {
            pool: Mutex::new(Pool {
                idle: vec![first],
                open: 1,
                limit: CONNECTIONS,
                waiting: 0,
            }),
            freed: Condvar::new(),
            dial,
            store: store.into(),
        }
    }

    /// A connection for one call, and how long the call waited for it.
    pub(crate) fn lend(&self) -> Result<(Lease<'_>, Duration), SdkRefusal> {
        self.lend_within(LEND_WAIT)
    }

    /// A connection for one call that may wait at most `budget`; a full
    /// queue or an exhausted budget is refused `Capacity`, naming the Store,
    /// its in-flight count and the queue bound (#551 B8-160, B8-161).
    pub(crate) fn lend_within(&self, budget: Duration) -> Result<(Lease<'_>, Duration), SdkRefusal> {
        let started = Instant::now();
        let poisoned = || refused("Failed", "binding state poisoned");
        let mut pool = self.pool.lock().map_err(|_| poisoned())?;
        loop {
            if let Some(control) = pool.idle.pop() {
                return Ok((self.lease(control), started.elapsed()));
            }
            if pool.open < pool.limit {
                pool.open += 1;
                // the only connection: like a process's first, wait to be admitted
                let only = pool.open == 1;
                drop(pool);
                let opened = if only {
                    self.dial.open_waiting()
                } else {
                    self.dial.open()
                };
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
            let in_flight = (pool.open - pool.idle.len()) as u32;
            let queued = u32::try_from(pool.waiting).unwrap_or(u32::MAX);
            // A full queue is a refusal before any wait (#551 B8-160).
            if pool.waiting >= Capacity::CONTRACT.queue as usize {
                return Err(Capacity::CONTRACT.refusal(
                    &self.store,
                    in_flight,
                    queued,
                    started.elapsed(),
                ));
            }
            let remaining = budget.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                // queue time is inside the caller's read budget (#551 B8-161)
                return Err(Capacity::CONTRACT.refusal(
                    &self.store,
                    in_flight,
                    queued,
                    started.elapsed(),
                ));
            }
            pool.waiting += 1;
            let mut waited = self
                .freed
                .wait_timeout(pool, remaining)
                .map_err(|_| poisoned())?
                .0;
            waited.waiting -= 1;
            pool = waited;
        }
    }

    /// This Store's measured capacity (#551 B8-157, B8-160): calls in flight
    /// through this process's connections and callers waiting now.
    pub(crate) fn measured(&self) -> (u32, u32) {
        let Ok(pool) = self.pool.lock() else {
            return (0, 0);
        };
        (
            u32::try_from(pool.open.saturating_sub(pool.idle.len())).unwrap_or(u32::MAX),
            u32::try_from(pool.waiting).unwrap_or(u32::MAX),
        )
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
