//! The hidden `ns-exec-supervisor` entry (#538 skeleton).
//!
//! #499's one-shot `ns-exec` joins the daemon's mount namespace and becomes
//! the command. A session supervisor instead owns a command it did not become:
//! it receives the daemon's mount-namespace descriptor over the same
//! `SCM_RIGHTS` seam, spawns a child that joins the namespace, and then only
//! waits, reports and forwards signals.
//!
//! The supervisor must stay outside the Workspace. It never calls `setns` and
//! never `chdir`s, because a process inside a mount namespace can keep a
//! Workspace FUSE mount referenced and make `unmount` return `Busy`. It holds
//! the descriptor only long enough to pass it to the child by inheritance and
//! then closes its copy.
//!
//! This slice is deliberately narrow: process model, control socket and
//! `wait4`. Client-presence detection, the daemon's session registry,
//! force-kill orchestration and `close(force)` integration are not here.
use super::{
    ns_exec_control::{self, Message},
    ns_fd,
};
use nix::{
    errno::Errno,
    sched::{setns, CloneFlags},
    sys::{
        prctl::set_pdeathsig,
        signal::{kill, Signal},
        wait::{waitpid, WaitStatus},
    },
    unistd::{close, getpgid, getpid, getppid, setgid, setgroups, setsid, setuid, Gid, Pid, Uid},
};
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    os::{
        fd::RawFd,
        unix::{net::UnixListener, process::CommandExt},
    },
    path::PathBuf,
    process::{Child, Command, ExitCode},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// Exit code of a helper that refused before running the command.
const REFUSED: u8 = 125;
const USAGE: &str = "usage: layerfs-daemon ns-exec-supervisor --socket <path> --control <path> [--directory <path>] [--uid <uid> --gid <gid>] -- <command> [args...]";
const CHILD_USAGE: &str = "usage: layerfs-daemon ns-exec-child --namespace-fd <fd> --parent <pid> [--directory <path>] [--uid <uid> --gid <gid>] -- <command> [args...]";
/// How long the supervisor waits to observe the child's own session. `setsid`
/// runs before `exec`, so this is a few milliseconds in practice; a child that
/// exits first is reported by `wait4` instead.
const SESSION_TIMEOUT: Duration = Duration::from_secs(5);
/// `Stop` is a thin wrapper; a caller cannot park the supervisor's kill timer
/// for an unbounded time.
const MAX_GRACE_MS: u32 = 60_000;
const SESSION_POLL: Duration = Duration::from_millis(2);

/// The hidden supervisor: serve `--socket`, own one child, report on
/// `--control`.
pub(super) fn main(args: &[OsString]) -> ExitCode {
    match run(args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("layerfs-daemon ns-exec-supervisor: {message}");
            ExitCode::from(REFUSED)
        }
    }
}

/// The internal child invocation the supervisor starts for itself. Kept on the
/// same binary so there is no second executable to install, and hidden like
/// `ns-exec` because nothing else calls it.
pub(super) fn child_main(args: &[OsString]) -> ExitCode {
    match run_child(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("layerfs-daemon ns-exec-child: {message}");
            ExitCode::from(REFUSED)
        }
    }
}

struct Parsed {
    socket: PathBuf,
    control: PathBuf,
    directory: Option<PathBuf>,
    identity: Option<(u32, u32)>,
    command: Vec<OsString>,
}

fn run(args: &[OsString]) -> Result<ExitCode, String> {
    let parsed = parse(args)?;
    // The daemon's descriptor must stay inheritable until the child exists;
    // the guard closes this process's copy on every path, including a failed
    // spawn, so the supervisor never keeps a mount reference in the steady
    // state.
    let namespace = NamespaceFd(ns_fd::receive_inheritable(&parsed.socket).map_err(|error| {
        format!(
            "receive mount namespace from {}: {error}",
            parsed.socket.display()
        )
    })?);
    // Bind before spawning: a listener failure must not leave an unsupervised
    // child behind.
    let listener = ns_exec_control::listen(&parsed.control)
        .map_err(|error| format!("bind control socket {}: {error}", parsed.control.display()))?;
    let _cleanup = ControlPath(parsed.control.clone());
    let shared = Arc::new(Mutex::new(Session::default()));
    let own_pgid = getpgid(None)
        .map_err(|error| format!("read the supervisor's process group: {error}"))?
        .as_raw();
    let serving = shared.clone();
    thread::Builder::new()
        .name("layerfs-exec-control".to_owned())
        .spawn(move || serve(listener, serving, own_pgid))
        .map_err(|error| format!("start the control thread: {error}"))?;

    let mut child = spawn(&parsed, namespace.0)?;
    // The child inherited the descriptor; the supervisor must not keep its copy.
    drop(namespace);
    let pid = i32::try_from(child.id()).map_err(|_| "child pid does not fit".to_owned())?;
    if await_own_session(pid, Instant::now() + SESSION_TIMEOUT) {
        announce_started(&shared, pid);
    } else {
        eprintln!("layerfs-daemon ns-exec-supervisor: child {pid} did not enter its own session");
    }
    let status = reap(pid)?;
    let (code, signal) = outcome(status);
    let group_alive = group_alive(pid, own_pgid);
    finish(
        &shared,
        Message::Exited {
            code,
            signal,
            group_alive,
        },
    );
    // The child is reaped exactly once; `child` no longer waits on it. Dropping
    // the handle only releases the pid record.
    let _ = child.try_wait();
    Ok(exit_code(code, signal))
}

/// The supervisor's copy of the daemon's mount-namespace descriptor. It is a
/// bare descriptor (the received one is not close-on-exec), so the child
/// inherits it; this process closes it as soon as the spawn returns.
struct NamespaceFd(RawFd);

impl Drop for NamespaceFd {
    fn drop(&mut self) {
        let _ = close(self.0);
    }
}

/// Remove the control socket when the session ends, including on an error path
/// after the bind.
struct ControlPath(PathBuf);

impl Drop for ControlPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// What the supervisor knows about its one child. Everything is behind one
/// lock so a caller's registration, the start announcement and the final
/// `Exited` cannot interleave into a duplicate or missing event: a caller
/// either registers before the announcement and receives the broadcast, or
/// after it and receives the snapshot.
#[derive(Default)]
struct Session {
    pid: i32,
    pgid: i32,
    started: bool,
    exit: Option<Message>,
    clients: Vec<Arc<Client>>,
}

impl Session {
    fn snapshot(&self) -> Vec<Message> {
        let mut messages = Vec::with_capacity(2);
        if self.started {
            messages.push(Message::Started {
                pid: self.pid,
                pgid: self.pgid,
            });
        }
        if let Some(exit) = self.exit {
            messages.push(exit);
        }
        messages
    }
}

type Shared = Arc<Mutex<Session>>;

struct Client {
    stream: Mutex<std::os::unix::net::UnixStream>,
}

impl Client {
    fn send(&self, message: Message) -> io::Result<()> {
        let mut stream = self.stream.lock().expect("control client lock");
        ns_exec_control::send(&mut stream, message)
    }
}

fn write_clients(state: &mut Session, message: Message) {
    state.clients.retain(|client| client.send(message).is_ok());
}

fn announce_started(shared: &Shared, pid: i32) {
    let mut state = shared.lock().expect("session lock");
    state.pid = pid;
    state.pgid = pid;
    state.started = true;
    let message = Message::Started { pid, pgid: pid };
    write_clients(&mut state, message);
}

fn finish(shared: &Shared, exit: Message) {
    let mut state = shared.lock().expect("session lock");
    state.exit = Some(exit);
    write_clients(&mut state, exit);
}

/// One accept loop for the life of the supervisor. Threads here end with the
/// process; there is no reconnect or presence bookkeeping in this slice.
fn serve(listener: UnixListener, shared: Shared, own_pgid: i32) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if !ns_exec_control::admit(&stream) {
            continue;
        }
        let Ok(writer) = stream.try_clone() else {
            continue;
        };
        let client = Arc::new(Client {
            stream: Mutex::new(writer),
        });
        {
            let mut state = shared.lock().expect("session lock");
            state.clients.push(client.clone());
            for message in state.snapshot() {
                let _ = client.send(message);
            }
        }
        let reading = shared.clone();
        let owned = client.clone();
        let _ = thread::Builder::new()
            .name("layerfs-exec-client".to_owned())
            .spawn(move || handle_client(stream, owned, reading, own_pgid));
    }
}

fn handle_client(
    mut stream: std::os::unix::net::UnixStream,
    client: Arc<Client>,
    shared: Shared,
    own_pgid: i32,
) {
    while let Ok(Some(message)) = ns_exec_control::receive(&mut stream) {
        match message {
            Message::Status => {
                let _ = client.send(status(&shared, own_pgid));
            }
            Message::Signal { signo } => {
                if let Err(reason) = signal_group(&shared, signo, own_pgid) {
                    eprintln!("layerfs-daemon ns-exec-supervisor: signal {signo}: {reason}");
                }
            }
            Message::Stop { grace_ms } => stop(&shared, grace_ms, own_pgid),
            // Supervisor-to-caller events are not accepted from a caller.
            _ => (),
        }
    }
}

fn status(shared: &Shared, own_pgid: i32) -> Message {
    let state = shared.lock().expect("session lock");
    let (code, signal) = match state.exit {
        Some(Message::Exited { code, signal, .. }) => (code, signal),
        _ => (0, 0),
    };
    Message::StatusReply {
        running: state.started && state.exit.is_none(),
        pid: state.pid,
        pgid: state.pgid,
        code,
        signal,
        group_alive: state.started && group_alive(state.pgid, own_pgid),
    }
}

/// Signal only this session's child group. The supervisor's own group and
/// every other pid are refused; before the child has its own session `pgid`
/// names a group that does not exist (`ESRCH`), never the supervisor's.
fn signal_group(shared: &Shared, signo: i32, own_pgid: i32) -> Result<(), String> {
    let state = shared.lock().expect("session lock");
    if state.exit.is_some() {
        return Err("the child has already exited".to_owned());
    }
    if !state.started || state.pgid <= 1 {
        return Err("the child has no session yet".to_owned());
    }
    if state.pgid == own_pgid {
        return Err("refusing to signal the supervisor's own process group".to_owned());
    }
    let signal = Signal::try_from(signo).map_err(|_| format!("{signo} is not a signal"))?;
    kill(Pid::from_raw(-state.pgid), signal)
        .map_err(|error| format!("signal process group {}: {error}", state.pgid))
}

/// `Stop` is only a SIGTERM wrapper with a grace timer for SIGKILL; it carries
/// no presence, retry or whole-Workspace policy.
fn stop(shared: &Shared, grace_ms: u32, own_pgid: i32) {
    if signal_group(shared, Signal::SIGTERM as i32, own_pgid).is_err() {
        return;
    }
    let grace = Duration::from_millis(u64::from(grace_ms.min(MAX_GRACE_MS)));
    let shared = shared.clone();
    let _ = thread::Builder::new()
        .name("layerfs-exec-grace".to_owned())
        .spawn(move || {
            thread::sleep(grace);
            let state = shared.lock().expect("session lock");
            if state.exit.is_none() {
                let _ = kill(Pid::from_raw(-state.pgid), Signal::SIGKILL);
            }
        });
}

/// Start the child as this same binary in child mode. The namespace descriptor
/// and the command are the only state passed; the child re-reads nothing from
/// the daemon.
fn spawn(parsed: &Parsed, namespace: RawFd) -> Result<Child, String> {
    let program = std::env::current_exe()
        .map_err(|error| format!("locate the supervisor executable: {error}"))?;
    let mut command = Command::new(program);
    command
        .arg("ns-exec-child")
        .arg("--namespace-fd")
        .arg(namespace.to_string())
        .arg("--parent")
        .arg(getpid().as_raw().to_string());
    if let Some(directory) = &parsed.directory {
        command.arg("--directory").arg(directory);
    }
    if let Some((uid, gid)) = parsed.identity {
        command
            .arg("--uid")
            .arg(uid.to_string())
            .arg("--gid")
            .arg(gid.to_string());
    }
    command
        .arg("--")
        .args(&parsed.command)
        .spawn()
        .map_err(|error| format!("start the child: {error}"))
}

/// Observe the child's actual process group in `/proc` rather than assuming
/// `setsid` ran. True means the group is the child's own; false means the child
/// is gone or did not establish it within the timeout.
fn await_own_session(pid: i32, deadline: Instant) -> bool {
    loop {
        match process_group(pid) {
            Some(pgid) if pgid == pid => return true,
            None => return false,
            Some(_) if Instant::now() >= deadline => return false,
            Some(_) => thread::sleep(SESSION_POLL),
        }
    }
}

/// The `pgrp` field of `/proc/<pid>/stat`, read after the `comm` field because
/// the command name may itself contain spaces and parentheses.
fn process_group(pid: i32) -> Option<i32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(')')?.1;
    let mut fields = rest.split_whitespace();
    let _state = fields.next()?;
    let _ppid: i32 = fields.next()?.parse().ok()?;
    fields.next()?.parse().ok()
}

fn reap(pid: i32) -> Result<WaitStatus, String> {
    loop {
        match waitpid(Pid::from_raw(pid), None) {
            Ok(status) => return Ok(status),
            Err(Errno::EINTR) => continue,
            Err(error) => return Err(format!("wait for child {pid}: {error}")),
        }
    }
}

/// Separate the exit code from the terminating signal; the supervisor exits
/// `128 + signal` (the shell convention) because a process cannot exit by
/// signal without re-raising it, which this skeleton deliberately does not.
fn outcome(status: WaitStatus) -> (i32, i32) {
    match status {
        WaitStatus::Exited(_, code) => (code, 0),
        WaitStatus::Signaled(_, signal, _) => (0, signal as i32),
        other => {
            eprintln!("layerfs-daemon ns-exec-supervisor: unexpected wait status {other:?}");
            (i32::from(REFUSED), 0)
        }
    }
}

fn exit_code(code: i32, signal: i32) -> ExitCode {
    let code = if signal != 0 { 128 + signal } else { code };
    u8::try_from(code).map_or(ExitCode::from(REFUSED), ExitCode::from)
}

/// Is any process still in the group? `kill(-pgid, 0)` is the existence probe;
/// the supervisor never kills grandchildren in this slice, it only reports
/// that they remain.
fn group_alive(pgid: i32, own_pgid: i32) -> bool {
    pgid > 1 && pgid != own_pgid && kill(Pid::from_raw(-pgid), None).is_ok()
}

struct ChildParsed {
    namespace_fd: RawFd,
    parent: i32,
    directory: Option<PathBuf>,
    identity: Option<(u32, u32)>,
    command: Vec<OsString>,
}

/// Child mode: join the namespace, enter the Workspace, drop to the command
/// identity, take a new session, and exec. It is `ns-exec`'s sequence plus
/// `setsid`/`PDEATHSIG` so the supervisor has one group to report and signal.
fn run_child(args: &[OsString]) -> Result<(), String> {
    let parsed = parse_child(args)?;
    // Reopen the inherited descriptor as an owned file through `/proc`, which
    // also re-checks that it is a mount namespace; the bare inherited copy is
    // closed immediately so the command does not keep it.
    let namespace = ns_fd::inherit(parsed.namespace_fd)
        .map_err(|error| format!("inherit mount namespace {}: {error}", parsed.namespace_fd))?;
    let _ = close(parsed.namespace_fd);
    setns(&namespace, CloneFlags::CLONE_NEWNS)
        .map_err(|error| format!("join mount namespace {}: {error}", parsed.namespace_fd))?;
    drop(namespace);
    if let Some(directory) = &parsed.directory {
        std::env::set_current_dir(directory)
            .map_err(|error| format!("enter directory {}: {error}", directory.display()))?;
    }
    if let Some((uid, gid)) = parsed.identity {
        drop_to(uid, gid)?;
    }
    setsid().map_err(|error| format!("take a new session: {error}"))?;
    // The parent is the supervisor's main thread, which lives until the
    // supervisor exits; a crash before `wait4` therefore kills the child. The
    // check closes the race where the supervisor died before the call.
    set_pdeathsig(Signal::SIGKILL).map_err(|error| format!("set parent-death signal: {error}"))?;
    if getppid().as_raw() != parsed.parent {
        return Err("the supervisor exited before the child's session was established".to_owned());
    }
    let (program, rest) = parsed
        .command
        .split_first()
        .ok_or_else(|| CHILD_USAGE.to_owned())?;
    let error = Command::new(program).args(rest).exec();
    Err(format!("exec {}: {error}", program.to_string_lossy()))
}

/// Exactly `ns-exec`'s drop: no supplementary groups, then gid, then uid, so
/// the command runs as the ordinary command identity with no capability.
fn drop_to(uid: u32, gid: u32) -> Result<(), String> {
    setgroups(&[]).map_err(|error| format!("drop supplementary groups: {error}"))?;
    setgid(Gid::from_raw(gid)).map_err(|error| format!("drop to gid {gid}: {error}"))?;
    setuid(Uid::from_raw(uid)).map_err(|error| format!("drop to uid {uid}: {error}"))
}

fn parse(args: &[OsString]) -> Result<Parsed, String> {
    let mut rest = args.iter();
    let mut socket = None;
    let mut control = None;
    let mut directory = None;
    let mut uid = None;
    let mut gid = None;
    loop {
        match rest.next() {
            Some(flag) if flag == OsStr::new("--socket") && socket.is_none() => {
                socket = Some(path(rest.next())?)
            }
            Some(flag) if flag == OsStr::new("--control") && control.is_none() => {
                control = Some(path(rest.next())?)
            }
            Some(flag) if flag == OsStr::new("--directory") && directory.is_none() => {
                directory = Some(path(rest.next())?)
            }
            Some(flag) if flag == OsStr::new("--uid") && uid.is_none() => {
                uid = Some(number(rest.next(), "uid")?)
            }
            Some(flag) if flag == OsStr::new("--gid") && gid.is_none() => {
                gid = Some(number(rest.next(), "gid")?)
            }
            Some(flag) if flag == OsStr::new("--") => break,
            _ => return Err(USAGE.to_owned()),
        }
    }
    let command: Vec<OsString> = rest.cloned().collect();
    let (Some(socket), Some(control)) = (socket, control) else {
        return Err(USAGE.to_owned());
    };
    if command.is_empty() {
        return Err(USAGE.to_owned());
    }
    Ok(Parsed {
        socket,
        control,
        directory,
        identity: identity(uid, gid)?,
        command,
    })
}

fn parse_child(args: &[OsString]) -> Result<ChildParsed, String> {
    let mut rest = args.iter();
    let mut namespace_fd = None;
    let mut parent = None;
    let mut directory = None;
    let mut uid = None;
    let mut gid = None;
    loop {
        match rest.next() {
            Some(flag) if flag == OsStr::new("--namespace-fd") && namespace_fd.is_none() => {
                namespace_fd = Some(
                    i32::try_from(number(rest.next(), "namespace-fd")?)
                        .map_err(|_| "--namespace-fd is not a descriptor".to_owned())?,
                )
            }
            Some(flag) if flag == OsStr::new("--parent") && parent.is_none() => {
                parent = Some(
                    i32::try_from(number(rest.next(), "parent")?)
                        .map_err(|_| "--parent is not a pid".to_owned())?,
                )
            }
            Some(flag) if flag == OsStr::new("--directory") && directory.is_none() => {
                directory = Some(path(rest.next())?)
            }
            Some(flag) if flag == OsStr::new("--uid") && uid.is_none() => {
                uid = Some(number(rest.next(), "uid")?)
            }
            Some(flag) if flag == OsStr::new("--gid") && gid.is_none() => {
                gid = Some(number(rest.next(), "gid")?)
            }
            Some(flag) if flag == OsStr::new("--") => break,
            _ => return Err(CHILD_USAGE.to_owned()),
        }
    }
    let command: Vec<OsString> = rest.cloned().collect();
    let (Some(namespace_fd), Some(parent)) = (namespace_fd, parent) else {
        return Err(CHILD_USAGE.to_owned());
    };
    if command.is_empty() {
        return Err(CHILD_USAGE.to_owned());
    }
    Ok(ChildParsed {
        namespace_fd,
        parent,
        directory,
        identity: identity(uid, gid)?,
        command,
    })
}

fn identity(uid: Option<u32>, gid: Option<u32>) -> Result<Option<(u32, u32)>, String> {
    match (uid, gid) {
        (Some(0), _) | (_, Some(0)) => {
            Err("--uid and --gid must name the non-root command identity".to_owned())
        }
        (Some(uid), Some(gid)) => Ok(Some((uid, gid))),
        (None, None) => Ok(None),
        _ => Err("--uid and --gid must be given together".to_owned()),
    }
}

fn path(value: Option<&OsString>) -> Result<PathBuf, String> {
    value
        .ok_or_else(|| USAGE.to_owned())?
        .to_str()
        .map(PathBuf::from)
        .ok_or_else(|| "a path is not UTF-8".to_owned())
}

fn number(value: Option<&OsString>, name: &str) -> Result<u32, String> {
    value
        .and_then(|value| value.to_str())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("--{name} is not a number"))
}
