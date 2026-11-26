//! The host's endpoint custody and single-attempt client.
use super::frame::MAX_HELLO;
use super::host_frame::{self, HOST_MAX_REPLY, HOST_MAX_REQUEST};
use super::host_hello::{build_identity, executable, token, HostHello, CORRELATION_LEN};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::{
    home::{self, flock, host_socket, paths},
    outcome::{refused, Outcome},
    trace,
};
use std::fs::{self, File};
use std::io;
use std::os::unix::{
    fs::{FileTypeExt, MetadataExt},
    net::{UnixListener, UnixStream},
    process::CommandExt,
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const READY_WAIT: Duration = Duration::from_secs(10);
const TRANSFER_WAIT: Duration = Duration::from_secs(10);
const READY_POLL: Duration = Duration::from_millis(15);

/// Canonical home authority and its two endpoints.
pub struct HostPaths {
    /// Canonical pathname of the existing home.
    pub home: PathBuf,
    /// Identity from the authoritative home record.
    pub home_id: String,
    /// Ordinary request endpoint.
    pub socket: PathBuf,
    /// Management endpoint with independent admission.
    pub control: PathBuf,
}

/// Ownership of the stable lifetime lock. Drop only after removing the sockets.
pub struct HostGuard {
    lock: File,
    directory: File,
    home: PathBuf,
}

/// Accepts only this process's user, before greeting or sending.
pub fn host_check_peer(stream: &UnixStream) -> Result<(), SdkRefusal> {
    let uid =
        host_socket::peer_uid(stream).map_err(|error| unavailable("peer credentials", error))?;
    if uid != flock::euid() {
        return Err(refused(
            "Denied",
            "host worker peer has a different effective UID",
        ));
    }
    Ok(())
}

fn unavailable(action: &str, error: impl std::fmt::Display) -> SdkRefusal {
    refused(
        "Unavailable",
        format!("host worker {action}: {error}; no operation was sent"),
    )
}

fn private_directory(path: &Path) -> Result<(), SdkRefusal> {
    let metadata = fs::symlink_metadata(path).map_err(|error| unavailable("directory", error))?;
    // Passing through without listing is permitted for a namespaces Sandbox.
    if !metadata.is_dir() || metadata.uid() != flock::euid() || metadata.mode() & 0o066 != 0 {
        return Err(refused(
            "Denied",
            format!(
                "{} is not a private directory owned by this user",
                path.display()
            ),
        ));
    }
    Ok(())
}

pub(super) fn existing_socket(path: &Path) -> Result<Option<fs::Metadata>, SdkRefusal> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unavailable("socket metadata", error)),
    };
    if !metadata.file_type().is_socket() || metadata.uid() != flock::euid() {
        return Err(refused(
            "Denied",
            format!("{} is not a socket owned by this user", path.display()),
        ));
    }
    Ok(Some(metadata))
}

fn hold(path: &Path) -> Result<Option<File>, SdkRefusal> {
    let file =
        host_socket::private_file(path, false).map_err(|error| unavailable("lock file", error))?;
    flock::try_lock(&file, true)
        .map(|acquired| acquired.then_some(file))
        .map_err(|error| unavailable("lock", error))
}

impl HostPaths {
    /// Reads an open home's identity without creating or changing any entry.
    pub fn new(home: &Path) -> Result<Self, SdkRefusal> {
        let canonical = fs::canonicalize(home).map_err(|error| unavailable("home", error))?;
        let file = match home::detect(&canonical)? {
            home::Layout::Current(file) => file,
            _ => {
                return Err(refused(
                    "Unavailable",
                    "host worker needs an open LayerFS home",
                ))
            }
        };
        Ok(Self {
            socket: paths::host_worker_socket(&canonical),
            control: paths::host_worker_control(&canonical),
            home_id: file.id,
            home: canonical,
        })
    }

    /// Pre-dial routing check. Both endpoint names must fit the kernel limit.
    pub fn fits(&self) -> bool {
        host_socket::path_fits(&self.socket) && host_socket::path_fits(&self.control)
    }

    pub(super) fn verify_directories(&self) -> Result<(), SdkRefusal> {
        private_directory(&self.home)?;
        private_directory(&self.home.join("machine"))?;
        private_directory(&paths::machine_dir(&self.home, "native"))
    }

    /// Takes the worker lifetime lock without waiting. The existing Home
    /// supplies the private native machine directory.
    pub fn try_hold(&self) -> Result<Option<HostGuard>, SdkRefusal> {
        self.check_paths()?;
        let directory = File::open(paths::machine_dir(&self.home, "native"))
            .map_err(|error| unavailable("directory", error))?;
        let Some(lock) = hold(&paths::host_worker_lock(&self.home))? else {
            return Ok(None);
        };
        let guard = HostGuard {
            lock,
            directory,
            home: self.home.clone(),
        };
        self.check_guard(&guard)?;
        Ok(Some(guard))
    }

    fn check_paths(&self) -> Result<(), SdkRefusal> {
        if !self.fits() {
            return Err(refused(
                "Unavailable",
                "host worker socket path is too long",
            ));
        }
        self.verify_directories()
    }

    fn check_guard(&self, guard: &HostGuard) -> Result<(), SdkRefusal> {
        if self.home != guard.home {
            return Err(refused(
                "Denied",
                "host worker lock belongs to another home",
            ));
        }
        self.check_paths()?;
        let current_home = home::opened(&self.home)?;
        let same = |file: &File, path: PathBuf| {
            let held = file
                .metadata()
                .map_err(|error| unavailable("held lock", error))?;
            let current =
                fs::symlink_metadata(path).map_err(|error| unavailable("lock pathname", error))?;
            Ok::<_, SdkRefusal>(held.dev() == current.dev() && held.ino() == current.ino())
        };
        if self.home_id != current_home.id
            || !same(&guard.directory, paths::machine_dir(&self.home, "native"))?
            || !same(&guard.lock, paths::host_worker_lock(&self.home))?
        {
            return Err(refused(
                "Unavailable",
                "host worker home or lock identity changed",
            ));
        }
        Ok(())
    }

    /// Recovers a stale socket and binds a private endpoint while holding
    /// its home's worker lifetime lock.
    pub fn bind(&self, control: bool, guard: &HostGuard) -> Result<UnixListener, SdkRefusal> {
        self.check_guard(guard)?;
        let path = self.endpoint(control);
        let name = path
            .file_name()
            .ok_or_else(|| unavailable("bind", "missing socket name"))?;
        host_socket::remove_socket(&guard.directory, name)
            .map_err(|error| unavailable("remove stale socket", error))?;
        let listener = UnixListener::bind(path).map_err(|error| unavailable("bind", error))?;
        self.check_guard(guard)?;
        if let Err(error) = host_socket::set_socket_private(&guard.directory, name) {
            let _ = host_socket::remove_socket(&guard.directory, name);
            return Err(unavailable("socket permissions", error));
        }
        Ok(listener)
    }

    /// Removes both socket files under the lifetime lock after listeners close.
    pub fn remove_endpoints(&self, guard: &HostGuard) -> Result<(), SdkRefusal> {
        self.check_guard(guard)?;
        let remove = |path: &Path| {
            let name = path
                .file_name()
                .ok_or_else(|| unavailable("cleanup", "missing socket name"))?;
            host_socket::remove_socket(&guard.directory, name)
                .map_err(|error| unavailable("remove socket", error))
        };
        let ordinary = remove(&self.socket);
        let management = remove(&self.control);
        ordinary.and(management)
    }

    fn endpoint(&self, control: bool) -> &Path {
        if control {
            &self.control
        } else {
            &self.socket
        }
    }

    /// Whether an intended request fits before selecting the host route.
    pub fn request_fits(&self, op: &str, args: &SdkValue) -> bool {
        let placeholder = "0".repeat(CORRELATION_LEN);
        self.fits()
            && host_frame::encode(
                &request(op, args.clone(), &placeholder, &placeholder),
                HOST_MAX_REQUEST,
            )
            .is_ok()
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
        self.check_paths()?;
        let path = self.endpoint(control);
        let began = Instant::now();
        let end = began + READY_WAIT;
        let id = token()?;
        let request = request(op, args, &id, &"0".repeat(CORRELATION_LEN));
        let max = if control { MAX_HELLO } else { HOST_MAX_REQUEST };
        host_frame::encode(&request, max)?;
        let build = build_identity()?;
        trace::step("route");
        let Some((stream, startup)) = self.acquire(path, start, end)? else {
            trace::connection(&path.to_string_lossy(), op, false, began);
            return Ok(None);
        };
        trace::step("connect");
        let generation = (|| {
            host_check_peer(&stream)?;
            let hello = Self::read_hello(&stream, end)?;
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
        let request = home::with_field(request, "generation", SdkValue::Text(generation));
        let body = host_frame::encode(&request, max)?;
        let answer = send_request(&stream, path, (op, &id), &body, began, control, None)?;
        if !valid(&answer) {
            return Err(unknown(op, &id, "invalid success payload"));
        }
        Ok(Some(answer))
    }

    pub(super) fn read_hello(stream: &UnixStream, end: Instant) -> Outcome {
        host_frame::read_host_frame(stream, MAX_HELLO, Some(end))?
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
        let body = host_frame::encode(&request, MAX_HELLO)?;
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

    fn acquire(
        &self,
        path: &Path,
        start: bool,
        end: Instant,
    ) -> Result<Option<(UnixStream, Option<File>)>, SdkRefusal> {
        let mut observed = probe(path, end)?;
        let startup_path = paths::host_worker_startup_lock(&self.home);
        let mut startup = None;
        let mut child: Option<Child> = None;
        loop {
            match observed {
                Probe::Connected(stream) => return Ok(Some((stream, startup))),
                Probe::Absent if !start => return Ok(None),
                _ => {}
            }
            if Instant::now() >= end {
                return Err(unavailable(
                    "readiness",
                    "listen queue or worker startup deadline expired",
                ));
            }
            if start && startup.is_none() {
                startup = hold(&startup_path)?;
                if startup.is_some() {
                    // Another caller may have completed startup while this
                    // caller waited for the lock.
                    observed = probe(path, end)?;
                    if let Probe::Connected(stream) = observed {
                        return Ok(Some((stream, startup)));
                    }
                }
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
                    child = Some(self.spawn()?);
                }
            }
            std::thread::sleep(READY_POLL.min(end.saturating_duration_since(Instant::now())));
            observed = probe(path, end)?;
        }
    }

    fn spawn(&self) -> Result<Child, SdkRefusal> {
        let folder = paths::log_dir(&self.home);
        home::private_dir(&folder)?;
        private_directory(&folder)?;
        let log = paths::worker_log(&self.home);
        let opened = host_socket::private_file(&log, true)
            .map_err(|error| unavailable("worker log", error))?;
        if opened
            .metadata()
            .map_err(|error| unavailable("worker log", error))?
            .len()
            >= layerfs_bridge::log_file::ROTATE_AT
        {
            layerfs_bridge::log_file::rotate(&log, layerfs_bridge::log_file::ROTATE_AT)
                .map_err(|error| unavailable("worker log rotation", error))?;
        }
        let log = host_socket::private_file(&log, true)
            .map_err(|error| unavailable("worker log", error))?;
        let stderr = log
            .try_clone()
            .map_err(|error| unavailable("worker log", error))?;
        let executable = executable().map_err(|error| unavailable("startup", error))?;
        Command::new(executable)
            .args(["worker", "--host"])
            .env("LAYERFS_HOME", &self.home)
            .env_remove("LAYERFS_TRACE")
            .env_remove("LAYERFS_TRACE_STEP")
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
    Waiting,
}

fn probe(path: &Path, end: Instant) -> Result<Probe, SdkRefusal> {
    let Some(metadata) = existing_socket(path)? else {
        return Ok(Probe::Absent);
    };
    if metadata.mode() & 0o777 != 0o600 {
        // The worker sets permissions after binding, in an owner-only directory.
        // A caller never dials until the endpoint has the required mode.
        return Ok(Probe::Waiting);
    }
    match host_socket::connect(path, end) {
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
    match host_frame::send(stream, body, send_by) {
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
    let answer = host_frame::read_host_frame(stream, HOST_MAX_REPLY, receive_by)
        .map_err(|refusal| unknown(op, id, &refusal.message))
        .and_then(|frame| frame.ok_or_else(|| unknown(op, id, "endpoint closed")));
    trace::connection(&path.to_string_lossy(), op, true, began);
    let answer = answer?;
    trace::step("answer");
    decode_answer(op, id, answer)
}

fn decode_answer(op: &str, id: &str, mut frame: SdkValue) -> Outcome {
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
            Err(super::client::refusal_of(&error))
        }
        _ => Err(unknown(op, id, "malformed final answer")),
    }
}

fn unknown(op: &str, id: &str, why: &str) -> SdkRefusal {
    let mut refusal = refused(
        "Unknown",
        format!("{op}: host worker may have received request {id}: {why}"),
    );
    refusal.detail = SdkValue::map([("request_id", SdkValue::text(id))]);
    refusal
}
