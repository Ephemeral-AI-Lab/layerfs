//! One bounded authenticated daemon observation; no pool, lease or reconnect:
//! `sandbox.daemon_keys` to `sandbox.daemon_ready`, the daemon's Workspace
//! list (`workspace_target`) and `sandbox.session`.
use super::workspace_target::{Listed, Listing};
use super::{remove_fix, Budget, Checks};
use crate::doctor::report::NoVerdict;
use layerfs_bridge::control::{DaemonPhase, HelloRequest, Reply, Request, SdkCall, SdkValue};
use layerfs_bridge::duplex::{initiate_observed, ChannelError};
use layerfs_home::secrets;
use layerfs_sandbox::daemon::channel::Control;
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

// A socket clone fences the real shared socket at an absolute deadline, including
// trickling handshake/records. Drop cancels and joins the fence owner.
struct Fence {
    stop: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Fence {
    fn new(socket: &TcpStream, deadline: Instant) -> std::io::Result<Self> {
        let socket = socket.try_clone()?;
        let (stop, wait) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            if matches!(
                wait.recv_timeout(deadline.saturating_duration_since(Instant::now())),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                let _ = socket.shutdown(Shutdown::Both);
            }
        });
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Fence {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What the daemon said of its Workspaces: how its list went, and its rows.
pub(super) type Observed = (Listing, Vec<Listed>);

/// Sandbox `sandbox`'s daemon at its saved `keys`, under `budget`.
pub(super) fn observe(
    sandbox: &str,
    keys: &SdkValue,
    budget: &Budget,
    checks: &mut Checks,
) -> Observed {
    let nothing = (Listing::NotAsked, Vec::new());
    let endpoint = keys
        .get_text("endpoint")
        .and_then(|s| s.parse::<SocketAddr>().ok())
        .filter(|a| a.ip().is_loopback() && a.port() != 0);
    let (Some(endpoint), Ok(secret), Ok(peer), Ok(instance)) = (
        endpoint,
        secrets::key::<32>(keys, "secret"),
        secrets::key::<32>(keys, "peer"),
        secrets::key::<32>(keys, "instance"),
    ) else {
        checks.fail(
            "sandbox.daemon_keys",
            "its daemon's saved endpoint or identities are malformed",
        );
        checks.fix("sandbox.daemon_keys", remove_fix(sandbox));
        return nothing;
    };
    checks.pass(
        "sandbox.daemon_keys",
        "its daemon's saved loopback endpoint and identities are readable",
    );
    const CONNECT: &str = "sandbox.daemon_connect";
    let duration = budget.remaining().unwrap_or(Duration::from_secs(10));
    if duration.is_zero() {
        checks.timeout(CONNECT, "its time ran out before its daemon was dialled");
        return nothing;
    }
    let deadline = Instant::now() + duration;
    let socket = match TcpStream::connect_timeout(&endpoint, duration) {
        Ok(socket) => socket,
        // nothing listens there: an answer, not a lost one
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            checks.fail(
                CONNECT,
                "its daemon does not answer: nothing listens on its saved endpoint",
            );
            checks.fix(CONNECT, remove_fix(sandbox));
            return nothing;
        }
        Err(_) => {
            checks.lost(CONNECT, "its daemon's saved endpoint was not reached");
            return nothing;
        }
    };
    checks.pass(CONNECT, "its daemon accepts a connection");
    const AUTH: &str = "sandbox.daemon_auth";
    let Ok(_fence) = Fence::new(&socket, deadline) else {
        checks.unknown(AUTH, "the doctor's own socket deadline could not be set");
        return nothing;
    };
    let connection = match initiate_observed(socket, &secret, peer).result {
        Ok(connection) => connection,
        Err(ChannelError::Noise(_)) => {
            checks.fail(AUTH, "its daemon refused the saved identities");
            checks.fix(AUTH, remove_fix(sandbox));
            return nothing;
        }
        Err(_) => {
            // a full admission queue and a lost transport look the same here
            checks.lost(AUTH, "the handshake with its daemon did not complete");
            return nothing;
        }
    };
    checks.pass(AUTH, "its daemon accepts the saved identities");
    let mut control = Control::new(connection);
    let observed = inspect(sandbox, &mut control, instance, deadline, checks);
    const SESSION: &str = "sandbox.session";
    if control.is_failed() || Instant::now() >= deadline {
        checks.lost(
            SESSION,
            "the doctor's session ended uncertain or out of time; nothing was sent again",
        );
        return observed;
    }
    // Ends only this control conversation; the protocol explicitly preserves
    // filesystem ownership. The original socket deadline still applies.
    match control.call(Request::EndSession) {
        Ok(Reply::SessionEnded) => checks.pass(SESSION, "the doctor's own session ended cleanly"),
        _ => checks.lost(
            SESSION,
            "the end of the doctor's own session was not acknowledged; not sent again",
        ),
    }
    observed
}

fn inspect(
    sandbox: &str,
    control: &mut Control,
    instance: [u8; 32],
    deadline: Instant,
    checks: &mut Checks,
) -> Observed {
    let nothing = (Listing::NotAsked, Vec::new());
    const INSTANCE: &str = "sandbox.daemon_instance";
    const READY: &str = "sandbox.daemon_ready";
    // Observe identity explicitly so a restarted instance is not collapsed into a
    // generic correlation failure. No startup wait or lifecycle action is requested.
    match control.call(Request::Hello(HelloRequest {
        expected_instance: None,
        wait_for_store: false,
    })) {
        Ok(Reply::Hello(status)) if status.instance != instance => {
            checks.fail(
                INSTANCE,
                "another daemon answers than the one it was started with",
            );
            checks.fix(INSTANCE, remove_fix(sandbox));
            return nothing;
        }
        Ok(Reply::Hello(status)) => {
            checks.pass(INSTANCE, "the daemon is the one it was started with");
            match status.phase {
                DaemonPhase::ControlReady => {
                    checks.pass(READY, "its daemon is ready for calls");
                }
                DaemonPhase::Retained => {
                    checks.fail(READY, "its daemon's startup failed and is kept as it ended");
                    checks.fix(READY, remove_fix(sandbox));
                    return nothing;
                }
                phase => {
                    let step = match phase {
                        DaemonPhase::Installing => "its Store is being installed",
                        _ => "its Store is not installed yet",
                    };
                    checks.warning(READY, format!("its daemon is still starting ({step})"));
                    return nothing;
                }
            }
        }
        Ok(Reply::Refused(_)) => {
            checks.unknown(READY, "its daemon refused to say whether it is ready");
            return nothing;
        }
        _ => {
            checks.lost(
                READY,
                "its daemon's readiness reply was lost; not asked again",
            );
            return nothing;
        }
    }
    workspaces(control, deadline)
}

/// The daemon's whole Workspace list, in bounded pages under one
/// generation; any page that does not fit gives no verdict, and nothing is
/// asked again.
fn workspaces(control: &mut Control, deadline: Instant) -> Observed {
    let lost = || (Listing::NoVerdict(NoVerdict::Lost), Vec::new());
    let mut offset = 0i64;
    let mut generation: Option<String> = None;
    let mut rows_seen: Vec<Listed> = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return (Listing::NoVerdict(NoVerdict::Timeout), Vec::new());
        }
        let args = SdkValue::map([
            ("offset", SdkValue::Int(offset)),
            (
                "generation",
                generation.clone().map_or(SdkValue::Null, SdkValue::text),
            ),
        ]);
        let answer = control.call(Request::Sdk(SdkCall {
            method: "sandbox.doctor_workspaces".into(),
            args,
        }));
        let value = match answer {
            Ok(Reply::Sdk(value)) => value,
            // the registry's admission is held: no retry
            Ok(Reply::SdkRefused(refusal)) if refusal.code == "Busy" => {
                return (Listing::NoVerdict(NoVerdict::Busy), Vec::new())
            }
            // the doctor's own deadline closed its socket
            Err(_) if Instant::now() >= deadline => {
                return (Listing::NoVerdict(NoVerdict::Timeout), Vec::new())
            }
            // membership changed between pages, a daemon without the
            // diagnostic list, or a lost reply: no stable answer
            _ => return lost(),
        };
        let Some(SdkValue::List(rows)) = value.get("workspaces") else {
            return lost();
        };
        let Some(current) = value
            .get_text("generation")
            .filter(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            return lost();
        };
        if generation
            .as_deref()
            .is_some_and(|expected| expected != current)
            || rows.len() > 32
        {
            return lost();
        }
        generation = Some(current.to_owned());
        for row in rows {
            let Some(id) = row
                .get_text("workspace")
                .filter(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            else {
                return lost();
            };
            // ids come in order, each once
            if rows_seen
                .last()
                .is_some_and(|previous| previous.id.as_str() >= id)
            {
                return lost();
            }
            let activity = row.get_text("activity").unwrap_or("unknown");
            if !matches!(
                activity,
                "binding"
                    | "idle"
                    | "committing"
                    | "closing"
                    | "uncertain"
                    | "failed"
                    | "attaching"
            ) {
                return lost();
            }
            let Some(SdkValue::Bool(mounted)) = row.get("mounted") else {
                return lost();
            };
            rows_seen.push(Listed {
                id: id.to_owned(),
                activity: activity.to_owned(),
                mounted: *mounted,
            });
        }
        let observed = i64::try_from(rows_seen.len()).unwrap_or(i64::MAX);
        match value.get("next_offset") {
            Some(SdkValue::Int(next)) if *next == offset + rows.len() as i64 && *next > offset => {
                offset = *next
            }
            Some(SdkValue::Null) if value.get_int("total") == Some(observed) => break,
            _ => return lost(),
        }
    }
    (Listing::Read, rows_seen)
}
