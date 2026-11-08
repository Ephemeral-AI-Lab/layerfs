//! One bounded authenticated daemon observation; no pool, lease or reconnect.
use super::{Budget, Checks};
use layerfs_bridge::control::{DaemonPhase, HelloRequest, Reply, Request, SdkCall, SdkValue};
use layerfs_bridge::duplex::{initiate_observed, ChannelError};
use layerfs_home::secrets;
use layerfs_sandbox::daemon::channel::Control;
use std::collections::BTreeMap;
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

pub(super) fn observe(keys: &SdkValue, budget: &Budget, checks: &mut Checks) {
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
            "Daemon endpoint or authentication/instance record is malformed",
        );
        checks.blocked(
            "sandbox.daemon",
            "Daemon discovery keys are invalid",
            "sandbox.daemon_keys",
        );
        return;
    };
    checks.pass(
        "sandbox.daemon_keys",
        "Saved loopback endpoint and fixed-size authentication identities are valid",
    );
    let duration = budget.remaining().unwrap_or(Duration::from_secs(10));
    if duration.is_zero() {
        checks.unknown(
            "sandbox.daemon",
            "Deadline expired before daemon connection",
        );
        return;
    }
    let deadline = Instant::now() + duration;
    let socket = match TcpStream::connect_timeout(&endpoint, duration) {
        Ok(socket) => socket,
        Err(_) => {
            checks.unknown(
                "sandbox.daemon",
                "Saved daemon endpoint was not reached; daemon health is unknown",
            );
            return;
        }
    };
    checks.pass(
        "sandbox.daemon_connect",
        "Saved daemon endpoint accepted a connection",
    );
    let Ok(_fence) = Fence::new(&socket, deadline) else {
        checks.unknown(
            "sandbox.daemon",
            "Could not establish the diagnostic socket deadline",
        );
        return;
    };
    let connection = match initiate_observed(socket, &secret, peer).result {
        Ok(connection) => connection,
        Err(ChannelError::Noise(_)) => {
            checks.fail(
                "sandbox.daemon_auth",
                "Saved daemon authentication or expected peer identity was rejected",
            );
            checks.blocked(
                "sandbox.workspaces",
                "Daemon authentication failed",
                "sandbox.daemon_auth",
            );
            return;
        }
        Err(_) => {
            checks.unknown("sandbox.daemon_auth", "Handshake did not complete; admission saturation and transport loss cannot be distinguished from this observation");
            return;
        }
    };
    checks.pass(
        "sandbox.daemon_auth",
        "Daemon peer authenticated with saved control identities",
    );
    let mut control = Control::new(connection);
    inspect(&mut control, instance, deadline, checks);
    if control.is_failed() || Instant::now() >= deadline {
        checks.unknown(
            "sandbox.session",
            "Diagnostic exchange ended uncertain or expired; no EndSession was sent or replayed",
        );
        return;
    }
    // Ends only this control conversation; the protocol explicitly preserves
    // filesystem ownership. The original socket deadline still applies.
    match control.call(Request::EndSession) {
        Ok(Reply::SessionEnded) => checks.pass(
            "sandbox.session",
            "Own diagnostic control session ended and its socket was fenced",
        ),
        _ => checks.unknown(
            "sandbox.session",
            "Own-session end acknowledgement or socket fence was uncertain; not replayed",
        ),
    }
}

fn inspect(control: &mut Control, instance: [u8; 32], deadline: Instant, checks: &mut Checks) {
    // Observe identity explicitly so a restarted instance is not collapsed into a
    // generic correlation failure. No startup wait or lifecycle action is requested.
    match control.call(Request::Hello(HelloRequest {
        expected_instance: None,
        wait_for_store: false,
    })) {
        Ok(Reply::Hello(status)) if status.instance != instance => {
            checks.fail(
                "sandbox.daemon_instance",
                "Authenticated daemon is a different instance than the saved sandbox",
            );
            checks.blocked(
                "sandbox.workspaces",
                "Saved daemon instance is stale",
                "sandbox.daemon_instance",
            );
            return;
        }
        Ok(Reply::Hello(status)) => {
            checks.pass(
                "sandbox.daemon_instance",
                "Daemon incarnation matches the saved sandbox",
            );
            if status.phase != DaemonPhase::ControlReady {
                checks.fail(
                    "sandbox.daemon_ready",
                    format!("Daemon is {:?}, not ControlReady", status.phase),
                );
                checks.blocked(
                    "sandbox.workspaces",
                    "Daemon is not ControlReady",
                    "sandbox.daemon_ready",
                );
                return;
            }
            checks.pass(
                "sandbox.daemon_ready",
                "Daemon reports ControlReady without waiting for startup",
            );
        }
        Ok(Reply::Refused(_)) => {
            checks.unknown(
                "sandbox.daemon_ready",
                "Daemon refused the readiness observation",
            );
            return;
        }
        _ => {
            checks.unknown(
                "sandbox.daemon_ready",
                "Readiness reply was lost or invalid; no request was replayed",
            );
            return;
        }
    }
    workspaces(control, deadline, checks);
    checks.skip("sandbox.filesystem_serving", "Control observations do not verify application FUSE/NFS reads, overlay integrity or Store integrity");
}

fn workspaces(control: &mut Control, deadline: Instant, checks: &mut Checks) {
    let mut offset = 0i64;
    let mut generation: Option<String> = None;
    let mut observed = 0i64;
    let mut activities: BTreeMap<String, i64> = BTreeMap::new();
    let mut last: Option<String> = None;
    let mut mounted = 0i64;
    let mut retained_examples = Vec::new();
    loop {
        if Instant::now() >= deadline {
            checks.unknown(
                "sandbox.workspaces",
                format!("Deadline expired after inspecting {observed} workspace records"),
            );
            return;
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
            Ok(Reply::SdkRefused(refusal)) => {
                match refusal.code.as_str() {
                    "Busy" => checks.unknown("sandbox.workspaces", "Busy: daemon registry admission is saturated; no retry performed"),
                    "Changed" => checks.unknown("sandbox.workspaces", "Workspace membership changed during pagination; no stable coverage verdict"),
                    "UnknownMethod" => checks.unknown("sandbox.workspaces", "Daemon does not support bounded diagnostic workspace enumeration"),
                    _ => checks.unknown("sandbox.workspaces", "Daemon refused diagnostic workspace enumeration"),
                }
                return;
            }
            _ => {
                checks.unknown("sandbox.workspaces", format!("Workspace reply lost or invalid after {observed} records; read not replayed"));
                return;
            }
        };
        let Some(SdkValue::List(rows)) = value.get("workspaces") else {
            checks.unknown(
                "sandbox.workspaces",
                "Daemon returned an invalid diagnostic page",
            );
            return;
        };
        let Some(current) = value
            .get_text("generation")
            .filter(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            checks.unknown(
                "sandbox.workspaces",
                "Daemon returned no valid membership generation",
            );
            return;
        };
        if generation
            .as_deref()
            .is_some_and(|expected| expected != current)
            || rows.len() > 32
        {
            checks.unknown(
                "sandbox.workspaces",
                "Daemon diagnostic coverage changed or exceeded its page bound",
            );
            return;
        }
        generation = Some(current.to_owned());
        for row in rows {
            let Some(id) = row
                .get_text("workspace")
                .filter(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            else {
                checks.unknown(
                    "sandbox.workspaces",
                    "Daemon returned an invalid workspace identity",
                );
                return;
            };
            if last.as_deref().is_some_and(|previous| previous >= id) {
                checks.unknown(
                    "sandbox.workspaces",
                    "Workspace enumeration repeated or reordered an identity",
                );
                return;
            }
            last = Some(id.to_owned());
            let state = row.get_text("activity").unwrap_or("unknown");
            if !matches!(
                state,
                "binding"
                    | "idle"
                    | "committing"
                    | "closing"
                    | "uncertain"
                    | "failed"
                    | "attaching"
            ) {
                checks.unknown(
                    "sandbox.workspaces",
                    "Daemon returned an unknown workspace activity",
                );
                return;
            }
            match row.get("mounted") {
                Some(SdkValue::Bool(true)) => mounted += 1,
                Some(SdkValue::Bool(false)) => {}
                _ => {
                    checks.unknown(
                        "sandbox.workspaces",
                        "Daemon returned no valid mount observation",
                    );
                    return;
                }
            }
            if matches!(state, "failed" | "uncertain") && retained_examples.len() < 16 {
                retained_examples.push(SdkValue::map([
                    ("workspace", SdkValue::text(id)),
                    ("activity", SdkValue::text(state)),
                    ("mounted", SdkValue::Bool(row.get_bool("mounted"))),
                ]));
            }
            *activities.entry(state.to_owned()).or_default() += 1;
            observed += 1;
        }
        match value.get("next_offset") {
            Some(SdkValue::Int(next)) if *next == offset + rows.len() as i64 && *next > offset => {
                offset = *next
            }
            Some(SdkValue::Null) if value.get_int("total") == Some(observed) => break,
            _ => {
                checks.unknown(
                    "sandbox.workspaces",
                    "Daemon diagnostic pagination did not account for every workspace",
                );
                return;
            }
        }
    }
    let retained = activities.get("failed").copied().unwrap_or(0)
        + activities.get("uncertain").copied().unwrap_or(0);
    checks.pass(
        "sandbox.workspaces",
        format!("Inspected all {observed} registry workspace records across bounded pages"),
    );
    checks.evidence(
        "sandbox.workspaces",
        SdkValue::map([
            ("count", SdkValue::Int(observed)),
            ("mounted", SdkValue::Int(mounted)),
            ("retained_examples", SdkValue::List(retained_examples)),
            ("retained_examples_truncated", SdkValue::Bool(retained > 16)),
            (
                "activities",
                SdkValue::Map(
                    activities
                        .into_iter()
                        .map(|(k, v)| (k, SdkValue::Int(v)))
                        .collect(),
                ),
            ),
        ]),
    );
    if retained > 0 {
        checks.warning("sandbox.workspace_retained", format!("{retained} workspaces retain failed or uncertain activity; inspect their original operation evidence"));
    }
    checks.skip("sandbox.overlay_state", "Dirty counts and pending merges are not read by the registry-only diagnostic; no overlay or Store integrity claim");
}
