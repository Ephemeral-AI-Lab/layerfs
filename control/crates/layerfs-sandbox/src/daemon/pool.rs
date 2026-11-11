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
use crate::daemon::fence::Fence;
use crate::internal::Control;
use layerfs_bridge::control::{DaemonPhase, HelloRequest, Reply, Request, SdkRefusal, SdkValue};
use layerfs_bridge::duplex::{
    admission::{AdmissionNotice, AdmissionReason},
    initiate_observed, ChannelError,
};
use layerfs_home::outcome::{failed, refused};
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Most control connections one process keeps to one Sandbox. A Sandbox this
/// binding creates admits [`crate::lifecycle::DAEMON_CONNECTIONS`]; the rest
/// stay for other processes.
pub const CONNECTIONS: usize = 6;

/// How to open another connection to the same daemon instance.
#[derive(Clone)]
pub struct Dial {
    pub endpoint: SocketAddr,
    pub secret: [u8; 32],
    pub peer: [u8; 32],
    pub instance: [u8; 32],
}

/// How long a process waits for the daemon to admit its first connection
/// while other processes hold every admitted one (the same bound as the
/// project's discovery lock).
pub const ADMISSION_WAIT: Duration = Duration::from_secs(60);

/// How long a call waits for one of this process's connections to come free
/// once every one is lent (#508). Control replies have no deadline, so behind
/// calls that never end an unbounded wait would last forever while holding
/// the caller's discovery turn. The wait is readiness: nothing is attempted
/// before it ends, and its `Busy` refusal is not retried.
pub const LEND_WAIT: Duration = ADMISSION_WAIT;

/// Why an open ended before the daemon's Hello reply. Every one means the
/// call was not admitted and nothing was attempted, so opening again is
/// safe; they name different observable causes (#540).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NotAdmitted {
    /// The daemon had no vacant pool slot. `retained` is exact when the
    /// advisory capacity notice arrived, and unknown when only EOF was seen.
    NoVacantSlot { retained: Option<u8> },
    /// The connection itself failed before Hello (`BrokenPipe`,
    /// `ConnectionReset`).
    ConnectionFailed,
}

impl NotAdmitted {
    fn code(self) -> &'static str {
        match self {
            Self::NoVacantSlot { .. } => "Capacity",
            Self::ConnectionFailed => "Unavailable",
        }
    }

    fn admission(self) -> &'static str {
        match self {
            Self::NoVacantSlot { .. } => "no-vacant-slot",
            Self::ConnectionFailed => "connection-failed",
        }
    }
}

/// Classify a failed open that never reached the Hello reply. Through a port
/// proxy the client reads the end (`UnexpectedEof`, a clean close by the
/// daemon's accept loop); dialing the daemon directly, its handshake write can
/// meet the closed socket first (`BrokenPipe`, `ConnectionReset`). The old
/// message claimed "other processes hold all 8 connections"; that is not
/// knowledge the client has (#540).
fn not_admitted(refusal: &SdkRefusal) -> Option<(NotAdmitted, &'static str)> {
    if refusal.code != "Failed" {
        return None;
    }
    if refusal.message.contains("UnexpectedEof") {
        return Some((
            NotAdmitted::NoVacantSlot { retained: None },
            "UnexpectedEof",
        ));
    }
    for kind in ["BrokenPipe", "ConnectionReset"] {
        if refusal.message.contains(kind) {
            return Some((NotAdmitted::ConnectionFailed, kind));
        }
    }
    None
}

/// The exact refusal for a decoded advisory capacity notice. This is the
/// precise form: it names the resource and the retained record count.
fn notice_refusal(notice: AdmissionNotice) -> SdkRefusal {
    match notice.reason {
        AdmissionReason::NoPoolSlot | AdmissionReason::ReservedBusy => SdkRefusal {
            code: "Capacity".to_owned(),
            message: format!(
                "the Sandbox's daemon has no vacant control slot: {} pool slot(s) and {} reserved slot(s) are vacant, with {} retained failure record(s). The call was not admitted; retrying is safe",
                notice.pool_vacant, notice.reserved_vacant, notice.retained
            ),
            detail: SdkValue::map([
                ("resource", SdkValue::text("daemon_control_slot")),
                ("retained", SdkValue::Int(i64::from(notice.retained))),
                ("pool_vacant", SdkValue::Int(i64::from(notice.pool_vacant))),
                (
                    "reserved_vacant",
                    SdkValue::Int(i64::from(notice.reserved_vacant)),
                ),
                ("retryable", SdkValue::Bool(true)),
                ("admission", SdkValue::text(notice.reason.as_str())),
            ]),
        },
        AdmissionReason::Starting => SdkRefusal {
            code: "Unavailable".to_owned(),
            message: "the Sandbox's daemon is still starting and admits no control connection yet. The call was not admitted; retrying is safe"
                .to_owned(),
            detail: SdkValue::map([
                ("resource", SdkValue::Null),
                ("retained", SdkValue::Null),
                ("retryable", SdkValue::Bool(true)),
                ("admission", SdkValue::text("starting")),
            ]),
        },
    }
}

/// The exact refusal after `wait`: what the client observed, not a guess at
/// what another process is doing.
fn admission_refusal(wait: Duration, admission: NotAdmitted, kind: &str) -> SdkRefusal {
    let message = match admission {
        NotAdmitted::NoVacantSlot { retained } => format!(
            "the Sandbox's daemon admitted no control connection for {}s: it closed the connection before Hello ({kind}), which it does when it has no vacant control slot{}. The call was not admitted; retrying is safe",
            wait.as_secs(),
            retained.map_or(String::new(), |count| format!(
                " (it reported {count} retained failure record(s))"
            ))
        ),
        NotAdmitted::ConnectionFailed => format!(
            "the connection to the Sandbox's daemon failed before Hello for {}s ({kind}): the call was not admitted; retrying is safe",
            wait.as_secs()
        ),
    };
    let retained = match admission {
        NotAdmitted::NoVacantSlot { retained } => {
            retained.map_or(SdkValue::Null, |count| SdkValue::Int(i64::from(count)))
        }
        _ => SdkValue::Null,
    };
    SdkRefusal {
        code: admission.code().to_owned(),
        message,
        detail: SdkValue::map([
            (
                "resource",
                match admission {
                    NotAdmitted::NoVacantSlot { .. } => SdkValue::text("daemon_control_slot"),
                    _ => SdkValue::Null,
                },
            ),
            ("retained", retained),
            ("retryable", SdkValue::Bool(true)),
            ("admission", SdkValue::text(admission.admission())),
            ("observed", SdkValue::text(kind)),
        ]),
    }
}

/// Whether this refusal says nothing was attempted and a later open may
/// succeed. Only these exact refusals are retried.
fn retryable(refusal: &SdkRefusal) -> bool {
    matches!(refusal.code.as_str(), "Capacity" | "Unavailable")
        && refusal.detail.get_bool("retryable")
}

impl Dial {
    /// The first connection a process opens to a Sandbox. While the daemon
    /// refuses admission, waits for one to be released, up to
    /// [`ADMISSION_WAIT`], instead of failing the caller's operation with a
    /// bare `UnexpectedEof`. Its final refusal names what the client actually
    /// observed: no vacant control slot, or a failed connection (#540).
    pub fn open_waiting(&self) -> Result<Control, SdkRefusal> {
        self.open_admitted(ADMISSION_WAIT)
    }

    /// [`Dial::open_within`], with the fence that ends this connection at
    /// the same deadline: connecting, the handshake and its Hello must finish
    /// inside that time, and a daemon that trickles any of them is shut down
    /// at `deadline` rather than kept waiting. The connection keeps those I/O
    /// waits for the one bounded call made on it. Nothing is retried and no
    /// admission wait applies: a daemon that refuses or does not answer is
    /// reported as it came.
    pub fn open_within(&self, within: Duration) -> Result<(Control, Fence), SdkRefusal> {
        let deadline = Instant::now() + within;
        let remaining = || {
            deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(1))
        };
        let stream =
            std::net::TcpStream::connect_timeout(&self.endpoint, remaining()).map_err(failed)?;
        stream.set_read_timeout(Some(remaining())).map_err(failed)?;
        stream
            .set_write_timeout(Some(remaining()))
            .map_err(failed)?;
        // armed before the handshake: a trickling far side cannot outlive the
        // deadline this open promised, and neither can the one call made on it
        let fence = Fence::on(stream.try_clone().map_err(failed)?, deadline);
        let connection = match initiate_observed(stream, &self.secret, self.peer).result {
            Ok(connection) => connection,
            Err(ChannelError::Admission(notice)) => return Err(notice_refusal(notice)),
            Err(error) => return Err(failed(error)),
        };
        // the waits stay: the caller makes exactly one call and drops it
        let mut control = Control::new(connection);
        let hello = control
            .call(Request::Hello(HelloRequest {
                expected_instance: Some(self.instance),
                wait_for_store: false,
            }))
            .map_err(failed)?;
        match hello {
            Reply::Hello(status) if status.phase == DaemonPhase::ControlReady => {
                Ok((control, fence))
            }
            _ => Err(refused("StaleSandbox", "saved daemon is not ControlReady")),
        }
    }

    /// [`Dial::open_waiting`], waiting at most `wait`.
    fn open_admitted(&self, wait: Duration) -> Result<Control, SdkRefusal> {
        let started = Instant::now();
        let mut pause = Duration::from_millis(20);
        loop {
            match self.open() {
                Err(refusal) => {
                    let legacy = not_admitted(&refusal);
                    let may_retry = retryable(&refusal) || legacy.is_some();
                    if may_retry && started.elapsed() < wait {
                        std::thread::sleep(pause);
                        pause = (pause * 2).min(Duration::from_millis(500));
                        continue;
                    }
                    if let Some((admission, kind)) = legacy {
                        return Err(admission_refusal(wait, admission, kind));
                    }
                    // A notice-based or transport refusal is already exact.
                    return Err(refusal);
                }
                other => return other,
            }
        }
    }

    /// One authenticated connection, checked to reach the same daemon.
    pub fn open(&self) -> Result<Control, SdkRefusal> {
        let stream = std::net::TcpStream::connect_timeout(&self.endpoint, Duration::from_secs(10))
            .map_err(failed)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(failed)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .map_err(failed)?;
        let connection = match initiate_observed(stream, &self.secret, self.peer).result {
            Ok(connection) => connection,
            Err(ChannelError::Admission(notice)) => return Err(notice_refusal(notice)),
            Err(error) => return Err(failed(error)),
        };
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
pub struct Controls {
    pool: Mutex<Pool>,
    freed: Condvar,
    dial: Dial,
}

/// One connection lent for one call's exchanges.
pub struct Lease<'a> {
    controls: &'a Controls,
    control: Option<Control>,
}

impl Controls {
    /// The connections of a Sandbox whose first connection is `first`.
    pub fn new(first: Control, dial: Dial) -> Self {
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
    pub fn lend(&self) -> Result<(Lease<'_>, Duration), SdkRefusal> {
        self.lend_within(LEND_WAIT)
    }

    /// [`Controls::lend`], waiting at most `wait` for admission and for an
    /// in-use connection. A caller that must not hold an operation intent
    /// while a daemon refuses connections (an explicit close) uses a short
    /// wait and reports the exact refusal (#540).
    pub fn lend_within(&self, wait: Duration) -> Result<(Lease<'_>, Duration), SdkRefusal> {
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
                    self.dial.open_admitted(wait)
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
            let remaining = wait.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(refused(
                    "Busy",
                    format!(
                        "all {} of this process's connections to the Sandbox stayed in use for {}s; nothing was attempted",
                        pool.open,
                        wait.as_secs()
                    ),
                ));
            }
            pool = self
                .freed
                .wait_timeout(pool, remaining)
                .map_err(|_| poisoned())?
                .0;
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
