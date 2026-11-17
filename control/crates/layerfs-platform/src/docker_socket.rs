//! Which Docker Engine socket this host offers, read from the host alone:
//! `DOCKER_HOST`, the Docker CLI's contexts (`~/.docker`, or `DOCKER_CONFIG`;
//! the current one from `DOCKER_CONTEXT` or `config.json`'s
//! `currentContext`), the sockets Docker Desktop, colima and OrbStack make,
//! and `/var/run/docker.sock`. Only `unix://` endpoints are usable; another
//! (`tcp://`, `ssh://`, `npipe://`) is listed as not usable and never
//! followed. Reading these starts no process; [`socket_state`] is one bounded
//! `GET /_ping` on one socket. The caller decides which socket a machine uses
//! and records it; nothing here reads the LayerFS home.
use crate::facts::json_string;
use nix::errno::Errno;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Docker's own default socket, used when nothing else names one.
pub const DEFAULT_DOCKER_SOCKET: &str = "/var/run/docker.sock";

/// The sockets other engines make, under the user's home: Docker Desktop's,
/// colima's default profile's and OrbStack's.
const WELL_KNOWN: [(&str, &str); 3] = [
    ("desktop", ".docker/run/docker.sock"),
    ("colima", ".colima/default/docker.sock"),
    ("orbstack", ".orbstack/run/docker.sock"),
];

/// What the host's environment says about Docker: the variables the Docker
/// CLI reads and the user's home folder.
#[derive(Clone, Debug, Default)]
pub struct DockerEnv {
    /// `DOCKER_HOST`.
    pub host: Option<String>,
    /// `DOCKER_CONTEXT`.
    pub context: Option<String>,
    /// `DOCKER_CONFIG`; absent, `<home>/.docker`.
    pub config: Option<PathBuf>,
    /// `HOME`.
    pub home: Option<PathBuf>,
}

impl DockerEnv {
    /// This process's environment (empty values count as unset).
    pub fn current() -> Self {
        let text = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        Self {
            host: text("DOCKER_HOST"),
            context: text("DOCKER_CONTEXT"),
            config: text("DOCKER_CONFIG").map(PathBuf::from),
            home: text("HOME").map(PathBuf::from),
        }
    }

    fn config_dir(&self) -> Option<PathBuf> {
        self.config
            .clone()
            .or_else(|| self.home.as_ref().map(|home| home.join(".docker")))
    }

    /// The current context's name: `DOCKER_CONTEXT`, else `config.json`'s
    /// `currentContext`; `None` for Docker's `default` context.
    fn current_context(&self) -> Option<String> {
        let name = self.context.clone().or_else(|| {
            let config = std::fs::read_to_string(self.config_dir()?.join("config.json")).ok()?;
            json_string(&config, &["currentContext"]).ok().flatten()
        })?;
        (!name.is_empty() && name != "default").then_some(name)
    }
}

/// One Docker endpoint the host names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerCandidate {
    /// The socket, for a `unix://` endpoint; `None` for one LayerFS cannot use.
    pub path: Option<PathBuf>,
    /// The endpoint as Docker writes it (`unix:///…`, `tcp://…`).
    pub host: String,
    /// Where it was found: `env:DOCKER_HOST`, `context:<name>`, `desktop`,
    /// `colima`, `orbstack` or `default`.
    pub source: String,
}

impl DockerCandidate {
    fn of(host: &str, source: String) -> Self {
        let path = host
            .strip_prefix("unix://")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());
        Self {
            path,
            host: host.to_owned(),
            source,
        }
    }

    fn socket(path: PathBuf, source: &str) -> Self {
        Self {
            host: format!("unix://{}", path.display()),
            path: Some(path),
            source: source.to_owned(),
        }
    }

    /// Whether LayerFS can use it: a `unix://` socket.
    pub fn usable(&self) -> bool {
        self.path.is_some()
    }
}

/// Every endpoint the host names, most specific first and each socket once:
/// `DOCKER_HOST`, the current context, the other contexts (by name), the
/// Docker Desktop, colima and OrbStack sockets that exist, and
/// `/var/run/docker.sock`.
pub fn docker_candidates(env: &DockerEnv) -> Vec<DockerCandidate> {
    let current = env.current_context();
    let mut found: Vec<DockerCandidate> = env_host(env).into_iter().collect();
    let mut all = contexts(env);
    // the current context first, then the others by name
    all.sort_by_key(|(name, _)| Some(name) != current.as_ref());
    found.extend(all.into_iter().map(|(_, candidate)| candidate));
    found.extend(well_known(env));
    found.push(DockerCandidate::socket(
        PathBuf::from(DEFAULT_DOCKER_SOCKET),
        "default",
    ));
    let mut seen = Vec::new();
    found.retain(|candidate| {
        let key = candidate
            .path
            .as_ref()
            .map_or_else(|| candidate.host.clone(), |p| p.display().to_string());
        let fresh = !seen.contains(&key);
        seen.push(key);
        fresh
    });
    found
}

/// The socket a docker machine that never started uses: `DOCKER_HOST` when
/// it is `unix://`, else the current context's when it is, else the first
/// Docker Desktop, colima or OrbStack socket that exists, else
/// `/var/run/docker.sock`.
pub fn docker_automatic(env: &DockerEnv) -> DockerCandidate {
    let chosen = automatic(env);
    // named as the candidates name the same socket (a context naming
    // Docker Desktop's socket wins over `desktop`)
    docker_candidates(env)
        .into_iter()
        .find(|candidate| candidate.path.is_some() && candidate.path == chosen.path)
        .unwrap_or(chosen)
}

fn automatic(env: &DockerEnv) -> DockerCandidate {
    env_host(env)
        .filter(DockerCandidate::usable)
        .or_else(|| {
            let name = env.current_context()?;
            let current = contexts(env).into_iter().find(|(found, _)| *found == name);
            current
                .map(|(_, candidate)| candidate)
                .filter(DockerCandidate::usable)
        })
        .or_else(|| well_known(env).into_iter().next())
        .unwrap_or_else(|| DockerCandidate::socket(PathBuf::from(DEFAULT_DOCKER_SOCKET), "default"))
}

fn env_host(env: &DockerEnv) -> Option<DockerCandidate> {
    let host = env.host.as_deref()?;
    Some(DockerCandidate::of(host, "env:DOCKER_HOST".to_owned()))
}

/// Every context with a `meta.json` under `contexts/meta/`, by name.
fn contexts(env: &DockerEnv) -> Vec<(String, DockerCandidate)> {
    let Some(dir) = env.config_dir().map(|c| c.join("contexts").join("meta")) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, DockerCandidate)> = entries
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.ok()?.path().join("meta.json")).ok()?;
            let name = json_string(&text, &["Name"]).ok().flatten()?;
            let host = json_string(&text, &["Endpoints", "docker", "Host"])
                .ok()
                .flatten()?;
            let candidate = DockerCandidate::of(&host, format!("context:{name}"));
            Some((name, candidate))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn well_known(env: &DockerEnv) -> Vec<DockerCandidate> {
    let Some(home) = &env.home else {
        return Vec::new();
    };
    WELL_KNOWN
        .iter()
        .map(|(source, rest)| (source, home.join(rest)))
        .filter(|(_, path)| path.exists())
        .map(|(source, path)| DockerCandidate::socket(path, source))
        .collect()
}

/// What a Docker socket does when asked: it answers `GET /_ping`, is not
/// there, refuses this user (`EACCES`), or is there and does not answer
/// (nothing listening, or no answer within the deadline).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketState {
    Answering,
    Absent,
    Denied,
    Silent,
}

impl SocketState {
    /// Its stable lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Answering => "answering",
            Self::Absent => "absent",
            Self::Denied => "denied",
            Self::Silent => "silent",
        }
    }
}

/// One non-blocking connect on Linux, where a blocking one to a listener
/// whose backlog is full waits for room: it answers at once (`EAGAIN`). The
/// stream is then blocking, with deadlines on its reads and writes.
#[cfg(target_os = "linux")]
fn connect(path: &Path) -> Result<UnixStream, Errno> {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    use nix::sys::socket::{socket, AddressFamily, SockFlag, SockType, UnixAddr};
    use std::os::fd::AsRawFd;
    let fd = socket(
        AddressFamily::Unix,
        SockType::Stream,
        SockFlag::empty(),
        None,
    )?;
    let blocking = OFlag::from_bits_truncate(fcntl(&fd, FcntlArg::F_GETFL)?);
    fcntl(&fd, FcntlArg::F_SETFL(blocking | OFlag::O_NONBLOCK))?;
    nix::sys::socket::connect(fd.as_raw_fd(), &UnixAddr::new(path)?)?;
    fcntl(&fd, FcntlArg::F_SETFL(blocking))?;
    Ok(UnixStream::from(fd))
}

/// Elsewhere (macOS) a connect to a full backlog is refused at once, and
/// the standard library's socket does not raise `SIGPIPE`.
#[cfg(not(target_os = "linux"))]
fn connect(path: &Path) -> Result<UnixStream, Errno> {
    UnixStream::connect(path).map_err(|error| {
        error
            .raw_os_error()
            .map_or(Errno::UnknownErrno, Errno::from_raw)
    })
}

/// How long [`socket_state`] waits for each read and write.
pub const SOCKET_ANSWER: Duration = Duration::from_secs(2);

/// Connects to `path` once and sends `GET /_ping`, each read and write
/// bounded by `timeout`; one attempt.
pub fn socket_state(path: &Path, timeout: Duration) -> SocketState {
    let mut stream = match connect(path) {
        Ok(stream) => stream,
        Err(Errno::ENOENT | Errno::ENOTDIR) => return SocketState::Absent,
        Err(Errno::EACCES | Errno::EPERM) => return SocketState::Denied,
        Err(_) => return SocketState::Silent,
    };
    let answered = stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .and_then(|()| stream.write_all(b"GET /_ping HTTP/1.0\r\nHost: docker\r\n\r\n"))
        .and_then(|()| {
            let mut head = [0u8; 12];
            stream.read_exact(&mut head).map(|()| head)
        });
    match answered {
        Ok(head) if head.starts_with(b"HTTP/1.") && head.ends_with(b"200") => {
            SocketState::Answering
        }
        _ => SocketState::Silent,
    }
}
