//! The host worker's single-attempt client; its endpoint custody is
//! `endpoint.rs`.
use super::endpoint::{
    existing_socket, hold, host_check_peer, private_directory, unavailable, Endpoint, HostPaths,
};
use super::frame::{self, HOST_MAX_REPLY, HOST_MAX_REQUEST};
use super::hello::{build_identity, executable, token, HostHello, CORRELATION_LEN};
use crate::route::guest::frame::MAX_HELLO;
use layerfs_home::disk::{self, paths};
use layerfs_home::log::trace;
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_home::socket;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::fs::File;
use std::io;
use std::os::unix::{fs::MetadataExt, net::UnixStream, process::CommandExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const READY_WAIT: Duration = Duration::from_secs(10);
const TRANSFER_WAIT: Duration = Duration::from_secs(10);
const READY_POLL: Duration = Duration::from_millis(15);
/// How long an ordinary call waits for its hello once connected to a
/// running worker: three times the start and management deadline, so a call
/// queued behind 8 calls of ordinary length still gets a pool thread (§16.1
/// C7), while a stopped or wedged worker gives a pre-send `Unavailable` (§11).
const CALL_HELLO_WAIT: Duration = Duration::from_secs(30);
/// Streamed writes the worker holds at once, running or waiting for a
/// write thread: past it the write endpoint accepts no caller, which waits
/// in its listen queue and, past [`CALL_HELLO_WAIT`], is refused
/// `Unavailable` before it sends anything (§11).
pub const HOST_HELD_WRITES: usize = 32;
/// The caller's environment a spawned host worker keeps: `PATH` and `HOME`
/// for the platform's ordinary process lookups, `TMPDIR` for SQLite's
/// temporary files, the locale, and OpenSSL's trust-store overrides for a
/// PostgreSQL TLS check. `LAYERFS_HOME` is set explicitly.
const INHERITED: [&str; 8] = [
    "PATH",
    "HOME",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

/// One connection past its authenticated hello, with its request ready
/// to send under the worker's generation.
pub(super) struct Greeted {
    pub(super) stream: UnixStream,
    pub(super) id: String,
    /// The request with its generation, overwritten when dropped if it
    /// carries a login.
    pub(super) request: layerfs_home::secret::wipe::Guard,
    /// Whether the request carries a login: its encoded frame is
    /// overwritten too.
    pub(super) secret: bool,
    pub(super) began: Instant,
}

impl HostPaths {
    /// Whether an intended request fits before selecting the host route.
    /// A trial frame that carries a login, and its copy of `args`, are
    /// overwritten before they are freed (#819 P-B).
    pub fn request_fits(&self, op: &str, args: &SdkValue) -> bool {
        if !self.fits() {
            return false;
        }
        let placeholder = "0".repeat(CORRELATION_LEN);
        let trial = request(op, args.clone(), &placeholder, &placeholder);
        let secret = frame::carries_login(&trial);
        let trial = layerfs_home::secret::wipe::Guard::new(trial, secret);
        match frame::encode(trial.get(), HOST_MAX_REQUEST) {
            Ok(frame) => {
                frame::release(frame, secret);
                true
            }
            Err(_) => false,
        }
    }

    /// Selects the matching CLI for a lifecycle operation issued by an SDK.
    pub fn for_management(mut self) -> Result<Self, SdkRefusal> {
        self.cli = super::hello::management_cli()?;
        Ok(self)
    }

    /// Starts the host worker if needed and verifies its management endpoint.
    pub fn start(&self) -> Outcome {
        self.exchange(true, true, "ping", SdkValue::map([]), None, |_| true)?
            .ok_or_else(|| unavailable("start", "no accepting host endpoint"))
    }

    /// Makes one ordinary call, launching a worker only if the home has
    /// no live owner. Callers choose this route before invoking it.
    pub fn call(&self, op: &str, args: SdkValue) -> Outcome {
        self.call_validated(op, args, |_| true)
    }

    /// Makes one ordinary call whose success payload must satisfy `valid`.
    /// An invalid payload after the send is correlated to the original request.
    pub fn call_validated(
        &self,
        op: &str,
        args: SdkValue,
        valid: impl FnOnce(&SdkValue) -> bool,
    ) -> Outcome {
        self.exchange(false, true, op, args, None, valid)?
            .ok_or_else(|| {
                refused(
                    "Unavailable",
                    "host worker did not become ready; wait for it and try a new call",
                )
            })
    }

    /// Calls an existing endpoint without starting a worker. None means
    /// there was no accepting endpoint before any operation was sent.
    pub fn call_existing(
        &self,
        control: bool,
        op: &str,
        args: SdkValue,
    ) -> Result<Option<SdkValue>, SdkRefusal> {
        self.exchange(control, false, op, args, None, |_| true)
    }

    /// Calls a management endpoint only if it still has the generation
    /// from a preceding authenticated observation.
    pub fn call_existing_generation(
        &self,
        op: &str,
        args: SdkValue,
        generation: &str,
    ) -> Result<Option<SdkValue>, SdkRefusal> {
        self.exchange(true, false, op, args, Some(generation), |_| true)
    }

    fn exchange(
        &self,
        control: bool,
        start: bool,
        op: &str,
        args: SdkValue,
        expected_generation: Option<&str>,
        valid: impl FnOnce(&SdkValue) -> bool,
    ) -> Result<Option<SdkValue>, SdkRefusal> {
        let endpoint = Endpoint::of(control);
        let Some(greeted) = self.greet(endpoint, start, op, args, expected_generation)? else {
            return Ok(None);
        };
        let max = if control { MAX_HELLO } else { HOST_MAX_REQUEST };
        let body = frame::encode(greeted.request.get(), max)?;
        let path = self.endpoint(endpoint);
        let answer = send_request(
            &greeted.stream,
            path,
            (op, &greeted.id),
            &body,
            greeted.began,
            control,
            None,
        );
        frame::release(body, greeted.secret);
        drop(greeted.request);
        let answer = answer?;
        if !valid(&answer) {
            return Err(unknown(op, &greeted.id, "invalid success payload"));
        }
        Ok(Some(answer))
    }

    /// Connects to `endpoint` (starting a worker when `start`), checks the
    /// peer and reads the hello under one rule for every call: starting a
    /// worker and management wait [`READY_WAIT`]; an ordinary call or a
    /// streamed write to a running worker waits [`CALL_HELLO_WAIT`] for a
    /// pool thread or a write place, then is refused `Unavailable` naming
    /// that load before anything was sent. None when no
    /// worker accepts and `start` is false. A request carrying a login
    /// (#819 P-B) is overwritten on every path, the early returns included;
    /// the caller's [`Greeted::request`] guard overwrites it after sending.
    pub(super) fn greet(
        &self,
        endpoint: Endpoint,
        start: bool,
        op: &str,
        args: SdkValue,
        expected_generation: Option<&str>,
    ) -> Result<Option<Greeted>, SdkRefusal> {
        let secret = args.get("password").is_some();
        let args = layerfs_home::secret::wipe::Guard::new(args, secret);
        self.check_paths()?;
        let control = endpoint == Endpoint::Control;
        let path = self.endpoint(endpoint);
        let began = Instant::now();
        let end = began + READY_WAIT;
        let id = token()?;
        let request = request(op, args.take(), &id, &"0".repeat(CORRELATION_LEN));
        let request = layerfs_home::secret::wipe::Guard::new(request, secret);
        let max = if control { MAX_HELLO } else { HOST_MAX_REQUEST };
        frame::release(frame::encode(request.get(), max)?, secret);
        let build = match &self.cli {
            Some((_, build)) => build.clone(),
            None => build_identity()?,
        };
        trace::step("route");
        let Some((stream, startup, started)) = self.acquire(path, start, end)? else {
            trace::connection(&path.to_string_lossy(), op, false, began);
            return Ok(None);
        };
        trace::step("connect");
        // Only the caller that started the worker keeps the startup lock to
        // its hello. Starting a worker and management keep the 10 s
        // deadline; any other ordinary call reached a running worker and may
        // wait in its queue longer, even one that took the startup lock
        // while the listen queue was full.
        let startup = startup.filter(|_| started);
        let queued = !control && !started;
        let hello_by = if queued {
            Instant::now() + CALL_HELLO_WAIT
        } else {
            end
        };
        let generation = (|| {
            host_check_peer(&stream)?;
            let hello = Self::read_hello(&stream, Some(hello_by)).map_err(|error| {
                if queued && Instant::now() >= hello_by {
                    let wait = CALL_HELLO_WAIT.as_secs();
                    unavailable(
                        "admission",
                        match endpoint {
                            Endpoint::Writes => format!(
                                "no write place was free within {wait} s: the worker holds its \
                                 {HOST_HELD_WRITES} streamed writes or is not responding"
                            ),
                            _ => format!(
                                "no call thread was free within {wait} s: earlier calls are busy \
                                 or the worker is not responding"
                            ),
                        },
                    )
                } else {
                    error
                }
            })?;
            let generation = HostHello::generation_of(&hello, self, &build)?;
            if expected_generation.is_some_and(|expected| expected != generation) {
                return Err(unavailable("hello", "worker generation changed"));
            }
            Ok(generation)
        })();
        if generation.is_err() {
            trace::connection(&path.to_string_lossy(), op, false, began);
        }
        let generation = generation?;
        drop(startup);
        trace::step("ready");
        // Generation tokens are fixed-size hex, so exchanging the placeholder
        // leaves the already checked envelope size unchanged.
        let request = disk::with_field(request.take(), "generation", SdkValue::Text(generation));
        Ok(Some(Greeted {
            stream,
            id,
            request: layerfs_home::secret::wipe::Guard::new(request, secret),
            secret,
            began,
        }))
    }

    pub(super) fn read_hello(stream: &UnixStream, end: Option<Instant>) -> Outcome {
        frame::read_host_frame(stream, MAX_HELLO, end)?
            .ok_or_else(|| unavailable("hello", "endpoint closed before readiness"))
    }

    pub(super) fn diagnostic_ping(
        &self,
        stream: &UnixStream,
        generation: &str,
        began: Instant,
        end: Instant,
    ) -> Outcome {
        let id = token()?;
        let request = request("ping", SdkValue::map([]), &id, generation);
        let body = frame::encode(&request, MAX_HELLO)?;
        send_request(
            stream,
            &self.control,
            ("ping", &id),
            &body,
            began,
            true,
            Some(end),
        )
    }

    /// The connected stream, the startup lock if this caller took it, and
    /// whether this caller spawned the worker it reached.
    fn acquire(
        &self,
        path: &Path,
        start: bool,
        end: Instant,
    ) -> Result<Option<(UnixStream, Option<File>, bool)>, SdkRefusal> {
        if start {
            crate::record::machine::MachineRecord::admit_host(
                &self.home,
                layerfs_home::names::HOST_MACHINE,
            )?;
        }
        let mut observed = probe(path, end)?;
        let startup_path = paths::host_worker_startup_lock(&self.home);
        let mut startup = None;
        let mut child: Option<Child> = None;
        loop {
            match observed {
                Probe::Connected(stream) => {
                    return Ok(Some((stream, startup, child.is_some())));
                }
                Probe::Absent if !start => return Ok(None),
                _ => {}
            }
            if Instant::now() >= end {
                return Err(unavailable(
                    "readiness",
                    "listen queue or worker startup deadline expired",
                ));
            }
            if startup.is_none() && (start || matches!(observed, Probe::Unready)) {
                startup = hold(&startup_path)?;
                if startup.is_some() {
                    // Another caller may have completed startup while this
                    // caller waited for the lock.
                    observed = probe(path, end)?;
                    if let Probe::Connected(stream) = observed {
                        return Ok(Some((stream, startup, false)));
                    }
                }
            }
            // A socket without its private mode whose lifetime lock is free
            // has no owner: its worker died between bind and chmod. Only the
            // lock holder binds, so the next owner replaces it (§8.2).
            if matches!(observed, Probe::Unready)
                && startup.is_some()
                && child.is_none()
                && self.try_hold()?.is_some()
            {
                if !start {
                    return Ok(None);
                }
                observed = Probe::Absent;
            }
            if let Some(child) = &mut child {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| unavailable("startup", error))?
                {
                    return Err(unavailable(
                        "startup",
                        format!("worker exited ({status}); see log/worker.log"),
                    ));
                }
            } else if startup.is_some() && matches!(observed, Probe::Absent) {
                if let Some(unowned) = self.try_hold()? {
                    drop(unowned);
                    crate::record::machine::MachineRecord::admit_host(
                        &self.home,
                        layerfs_home::names::HOST_MACHINE,
                    )?;
                    child = Some(self.spawn()?);
                }
            }
            std::thread::sleep(READY_POLL.min(end.saturating_duration_since(Instant::now())));
            observed = probe(path, end)?;
        }
    }

    fn spawn(&self) -> Result<Child, SdkRefusal> {
        let folder = paths::log_dir(&self.home);
        disk::private_dir(&folder)?;
        private_directory(&folder)?;
        let log = paths::worker_log(&self.home);
        let opened =
            socket::private_file(&log, true).map_err(|error| unavailable("worker log", error))?;
        if opened
            .metadata()
            .map_err(|error| unavailable("worker log", error))?
            .len()
            >= layerfs_home::log::rotate::ROTATE_AT
        {
            layerfs_home::log::rotate::rotate(&log, layerfs_home::log::rotate::ROTATE_AT)
                .map_err(|error| unavailable("worker log rotation", error))?;
        }
        let log =
            socket::private_file(&log, true).map_err(|error| unavailable("worker log", error))?;
        let stderr = log
            .try_clone()
            .map_err(|error| unavailable("worker log", error))?;
        let executable = match &self.cli {
            Some((path, _)) => path.clone(),
            None => executable().map_err(|error| unavailable("startup", error))?,
        };
        let mut command = Command::new(executable);
        // The worker outlives this caller and serves others, so it takes
        // nothing from this caller's environment or directory but the names
        // below (spec P-B: no secrets from its own environment). Per-call
        // settings such as tracing travel in each request instead.
        command.env_clear().current_dir("/");
        for name in INHERITED {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command
            .args(["worker", "--host"])
            .env("LAYERFS_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(stderr)
            .process_group(0)
            .spawn()
            .map_err(|error| unavailable("startup", error))
    }
}

enum Probe {
    Connected(UnixStream),
    Absent,
    /// A socket that does not have the private mode yet.
    Unready,
    Waiting,
}

fn probe(path: &Path, end: Instant) -> Result<Probe, SdkRefusal> {
    let Some(metadata) = existing_socket(path)? else {
        return Ok(Probe::Absent);
    };
    if metadata.mode() & 0o777 != 0o600 {
        // The worker sets permissions after binding, in an owner-only directory.
        // A caller never dials until the endpoint has the required mode.
        return Ok(Probe::Unready);
    }
    match socket::connect(path, end) {
        Ok(stream) => Ok(Probe::Connected(stream)),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(Probe::Absent)
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(Probe::Waiting),
        Err(error) => Err(unavailable("connect", error)),
    }
}

fn request(op: &str, args: SdkValue, id: &str, generation: &str) -> SdkValue {
    SdkValue::map([
        ("op", SdkValue::text(op)),
        ("args", args),
        ("generation", SdkValue::text(generation)),
        ("request_id", SdkValue::text(id)),
        ("trace_step", SdkValue::Bool(trace::stepping())),
    ])
}

fn send_request(
    stream: &UnixStream,
    path: &Path,
    (op, id): (&str, &str),
    body: &[u8],
    began: Instant,
    control: bool,
    deadline: Option<Instant>,
) -> Outcome {
    let send_by = deadline.unwrap_or_else(|| Instant::now() + TRANSFER_WAIT);
    match frame::send(stream, body, send_by) {
        Err((started, refusal)) => {
            trace::connection(&path.to_string_lossy(), op, started, began);
            return Err(if started {
                unknown(op, id, &refusal.message)
            } else {
                refusal
            });
        }
        Ok(()) => trace::step("request"),
    }
    let receive_by = deadline.or_else(|| control.then(|| Instant::now() + TRANSFER_WAIT));
    let answer = frame::read_host_frame(stream, HOST_MAX_REPLY, receive_by)
        .map_err(|refusal| unknown(op, id, &refusal.message))
        .and_then(|frame| frame.ok_or_else(|| unknown(op, id, "endpoint closed")));
    trace::connection(&path.to_string_lossy(), op, true, began);
    let answer = answer?;
    trace::step("answer");
    decode_answer(op, id, answer)
}

pub(super) fn decode_answer(op: &str, id: &str, mut frame: SdkValue) -> Outcome {
    if frame.get_text("request_id") != Some(id) {
        return Err(unknown(op, id, "answer did not echo the request ID"));
    }
    if let Some(steps) = frame.get("steps") {
        trace::steps_from(steps);
    }
    let SdkValue::Map(ref mut fields) = frame else {
        return Err(unknown(op, id, "answer was not an object"));
    };
    let mut outcome = fields
        .drain(..)
        .filter(|(key, _)| matches!(key.as_str(), "ok" | "error"));
    match (outcome.next(), outcome.next()) {
        (Some((key, value)), None) if key == "ok" => {
            if op == "store.close" && !matches!(value.get("closed"), Some(SdkValue::Bool(_))) {
                return Err(unknown(
                    op,
                    id,
                    "answer did not report a Store close outcome",
                ));
            }
            Ok(value)
        }
        (Some((_, error)), None)
            if error.get_text("code").is_some() && error.get_text("message").is_some() =>
        {
            let mut refusal = crate::route::guest::client::refusal_of(&error);
            // an engine-uncertain outcome is kept under this id (§8.5):
            // the caller matches it to its record
            if refusal.code == "Unknown" {
                refusal.detail = disk::with_field(
                    std::mem::take(&mut refusal.detail),
                    "request_id",
                    SdkValue::text(id),
                );
            }
            Err(refusal)
        }
        _ => Err(unknown(op, id, "malformed final answer")),
    }
}

pub(super) fn unknown(op: &str, id: &str, why: &str) -> SdkRefusal {
    let mut refusal = refused(
        "Unknown",
        format!("{op}: host worker may have received request {id}: {why}"),
    );
    refusal.detail = SdkValue::map([("request_id", SdkValue::text(id))]);
    refusal
}
