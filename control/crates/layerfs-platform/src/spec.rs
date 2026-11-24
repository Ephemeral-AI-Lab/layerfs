//! What the caller passes for one machine: its size, images and lasting
//! guest state. It names no LayerFS home path; every path and setting
//! comes from the caller.
use crate::error::{Code, MachineError};
use crate::platform::Platform;
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;

/// The machine's guest worker port, inside the guest.
pub const WORKER_PORT: u16 = 7070;

/// One machine, as its records describe it.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The machine's name (`dev`).
    pub name: String,
    pub kind: Platform,
    /// The provider's name for the machine's VM or worker container,
    /// `<prefix><machine>`, chosen by the caller.
    pub vm_name: String,
    pub cpus: u32,
    /// Bytes.
    pub memory: u64,
    /// The data disk, in bytes, mounted at `/layerfs/data`.
    pub disk: u64,
    /// The machine's Sandbox image archive on the host (`<digest>.tar`),
    /// copied into the guest once.
    pub image: Option<PathBuf>,
    /// The VM's own system: an OCI reference (`apple-container` and the
    /// `docker` worker container on macOS and Linux) or a Lima
    /// image URL or absolute path. `None` takes the kind's default.
    pub os_image: Option<String>,
    /// Lasting guest state the caller stored from an earlier create.
    pub state: Lasting,
    /// The endpoint last returned for this machine (its `worker.json`), if any.
    pub worker: Option<Endpoint>,
    /// The ordered incarnation the machine's start persists for the worker
    /// it launches: the worker serves with it, and every mount it places
    /// carries it. `None` for a stop or status spec, and for a worker an
    /// older host started (that worker mints its own).
    pub incarnation: Option<String>,
    /// A folder of the caller's for this machine: its logs.
    pub work_dir: PathBuf,
    pub tools: Tools,
}

/// Guest state that outlives one call; the caller stores and passes it back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lasting {
    /// The host loopback port a VM's configuration forwards to its worker.
    pub port: Option<u16>,
    /// `apple-container`: its network's gateway, where the guest reaches
    /// this host, read once when the machine starts (#66) and replaced by
    /// every start; a Sandbox's PostgreSQL reach uses it with no lookup.
    pub gateway: Option<std::net::Ipv4Addr>,
}

/// The sealed Linux bundle: `layerfs`, `layerfs-daemon`,
/// `layerfs-sandbox-helper` and `sandbox-image.tar`. Guests reuse unchanged
/// files by manifest identity; `hash` remains the worker compatibility seal.
#[derive(Clone, Debug)]
pub struct Bundle {
    pub dir: PathBuf,
    pub hash: String,
}

/// A provider's own limit on a VM's name on this host, beyond the
/// plain-name rule (63 characters): what [`crate::name_limit`] answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameLimit {
    /// The longest VM name the provider takes here, in bytes.
    pub longest: usize,
    /// The provider's rule, for a refusal's message.
    pub why: String,
}

/// How the host reaches its tools.
#[derive(Clone, Debug)]
pub struct Tools {
    /// The search path for provider CLIs (`limactl`, `ssh`, `tar`, ...),
    /// and the `PATH` every provider command runs with.
    pub path: OsString,
    /// The Docker Engine socket the `docker` CLI is pointed at.
    pub docker_socket: PathBuf,
}

/// The folders searched after `PATH` on macOS, where an app started from the
/// Dock or by launchd has a short `PATH` without Homebrew's or Docker
/// Desktop's: so `docker`, `limactl` and `container` are found there too.
const MACOS_FALLBACKS: [&str; 3] = [
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/Applications/Docker.app/Contents/Resources/bin",
];

impl Tools {
    /// This process's `PATH` with the [`tool_fallbacks`] after it, and
    /// Docker's default socket (the caller sets the one a machine uses).
    pub fn from_env() -> Self {
        Self {
            path: search_path(
                &std::env::var_os("PATH").unwrap_or_default(),
                &tool_fallbacks(),
            ),
            docker_socket: PathBuf::from(crate::docker_socket::DEFAULT_DOCKER_SOCKET),
        }
    }
}

/// The folders searched for tools after `PATH`: `LAYERFS_TOOL_FALLBACKS`
/// when it is set (a `PATH`-style list; empty for none), else on macOS
/// Homebrew's, `/usr/local/bin` and Docker Desktop's, and none elsewhere.
pub fn tool_fallbacks() -> Vec<PathBuf> {
    match std::env::var_os("LAYERFS_TOOL_FALLBACKS") {
        Some(list) => std::env::split_paths(&list)
            .filter(|dir| !dir.as_os_str().is_empty())
            .collect(),
        None if cfg!(target_os = "macos") => MACOS_FALLBACKS.iter().map(PathBuf::from).collect(),
        None => Vec::new(),
    }
}

/// `path` with each of `fallbacks` it does not hold already after it.
pub fn search_path(path: &OsString, fallbacks: &[PathBuf]) -> OsString {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect();
    for dir in fallbacks {
        if !dirs.contains(dir) {
            dirs.push(dir.clone());
        }
    }
    std::env::join_paths(&dirs).unwrap_or_else(|_| path.clone())
}

/// A machine's worker as the host reaches it.
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// A host loopback address.
    pub address: SocketAddr,
    pub secret: String,
}

/// The secret is never shown.
impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("address", &self.address)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// A Sandbox daemon's port as this host reaches it (`machine::forward`,
/// #535 G4): Workspace calls dial `address`. `pid` is the detached
/// `ssh -L` that carries it, if one does (`lima`); the caller
/// records both and ends it with `machine::forward_close`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Forward {
    /// A host loopback address.
    pub address: SocketAddr,
    pub pid: Option<u32>,
}

/// A started machine: its endpoint, and the lasting state to record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    pub endpoint: Endpoint,
    pub lasting: Lasting,
}

/// Guest paths every VM machine shares.
pub const DATA: &str = "/layerfs/data";
pub const BUNDLE: &str = "/layerfs/bundle";
pub const STATE: &str = "/layerfs/state";
pub const IMAGE: &str = "/layerfs/image";
/// The worker's log on every machine kind: on the data disk, so it stays
/// across restarts and, on docker, across the containers each start makes
/// (`log/worker.log` under the worker's root; layerfs-project#178 step 0).
pub const WORKER_LOG: &str = "/layerfs/data/log/worker.log";

/// A size: bytes, or a number with `K`, `M`, `G` or `T` (binary units, as
/// Lima reads them), optionally followed by `iB` or `B`.
pub fn parse_size(text: &str) -> Result<u64, MachineError> {
    let bad = || MachineError::new(Code::InvalidInput, format!("not a size: {text:?}"));
    let trimmed = text.trim();
    let unit_start = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (digits, unit) = trimmed.split_at(unit_start);
    let number: u64 = digits.parse().map_err(|_| bad())?;
    let prefix = match unit {
        "" | "B" => "",
        _ => unit
            .strip_suffix("iB")
            .or_else(|| unit.strip_suffix('B'))
            .unwrap_or(unit),
    };
    let shift = match prefix {
        "" if unit.len() <= 1 => 0,
        "K" | "k" => 10,
        "M" | "m" => 20,
        "G" | "g" => 30,
        "T" | "t" => 40,
        _ => return Err(bad()),
    };
    let bytes = number.checked_mul(1 << shift).ok_or_else(bad)?;
    if bytes == 0 {
        return Err(bad());
    }
    Ok(bytes)
}

/// A machine or VM name: a letter or digit, then letters, digits, `-`, `_` or `.`.
pub(crate) fn plain_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && name.len() <= 63
}

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

impl Spec {
    /// Refuses a spec that cannot describe a machine of its kind, before any effect.
    pub fn validate(&self) -> Result<(), MachineError> {
        let refuse = |message: String| Err(MachineError::new(Code::InvalidInput, message));
        if !plain_name(&self.name) {
            return refuse(format!("machine name {:?} is not a plain name", self.name));
        }
        if self.kind != Platform::Native && !plain_name(&self.vm_name) {
            return refuse(format!("VM name {:?} is not a plain name", self.vm_name));
        }
        if !self.work_dir.is_absolute() {
            return refuse("the machine's work folder must be an absolute path".into());
        }
        if self.kind.created() {
            if self.cpus == 0 {
                return refuse("a VM needs at least one CPU".into());
            }
            if self.memory < 256 * MIB {
                return refuse("a VM needs at least 256M of memory".into());
            }
            if self.disk < GIB {
                return refuse("a VM's data disk needs at least 1G".into());
            }
        }
        Ok(())
    }
}
