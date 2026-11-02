//! What the caller passes for one machine: its size, images, ssh host and
//! lasting guest state. It names no LayerFS home path; every path and setting
//! comes from the caller.
use super::error::{Code, MachineError};
use super::kind::Kind;
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
    pub kind: Kind,
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
    /// The VM's own system: an OCI reference (`apple-container`,
    /// `microsandbox`, and the `docker` worker container on macOS) or a Lima
    /// image URL or absolute path. `None` takes the kind's default.
    pub os_image: Option<String>,
    /// `lima` only: a `vz` instance with nested virtualization (Firecracker hosts).
    pub nested: bool,
    /// `firecracker` only: the spec of the `nested` Lima machine it runs in.
    pub host: Option<Box<Spec>>,
    /// `ssh` only.
    pub ssh: Option<SshSpec>,
    /// Lasting guest state the caller stored from an earlier create.
    pub state: Lasting,
    /// The endpoint last returned for this machine (its `worker.json`), if any.
    pub worker: Option<Endpoint>,
    /// A folder of the caller's for this machine: logs, the accepted ssh host key.
    pub work_dir: PathBuf,
    pub tools: Tools,
}

/// An ssh host, connected rather than created.
#[derive(Clone, Debug)]
pub struct SshSpec {
    pub host: String,
    /// Default `root`: the worker and the setup run as this user.
    pub user: String,
    /// Default 22.
    pub port: u16,
    /// A key file; without one ssh-agent and the default keys are used.
    pub identity: Option<PathBuf>,
    /// The known_hosts file holding the host's key (checked strictly).
    pub known_hosts: PathBuf,
    /// Default `/layerfs/data`; on ext4, xfs or btrfs (O18).
    pub data_root: String,
}

impl SshSpec {
    pub fn new(host: impl Into<String>, known_hosts: PathBuf) -> Self {
        Self {
            host: host.into(),
            user: "root".into(),
            port: 22,
            identity: None,
            known_hosts,
            data_root: DATA.into(),
        }
    }
}

/// Guest state that outlives one call; the caller stores and passes it back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lasting {
    /// The host loopback port a VM's configuration forwards to its worker.
    pub port: Option<u16>,
    /// `firecracker`: `n` of its `172.30.<n>.0/30` network (1 to 250).
    pub network: Option<u8>,
}

/// The sealed Linux bundle: `layerfs`, `layerfs-daemon`,
/// `layerfs-sandbox-helper` and `images/<digest>.tar`, copied into a guest
/// once per `hash`.
#[derive(Clone, Debug)]
pub struct Bundle {
    pub dir: PathBuf,
    pub hash: String,
}

/// How the host reaches its tools.
#[derive(Clone, Debug)]
pub struct Tools {
    /// The search path for provider CLIs (`limactl`, `ssh`, `tar`, ...).
    pub path: OsString,
    /// The Docker Engine socket the `docker` CLI is pointed at.
    pub docker_socket: PathBuf,
}

impl Tools {
    /// This process's `PATH` and Docker's usual socket.
    pub fn from_env() -> Self {
        Self {
            path: std::env::var_os("PATH").unwrap_or_default(),
            docker_socket: PathBuf::from("/var/run/docker.sock"),
        }
    }
}

/// A machine's worker as the host reaches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// A host loopback address.
    pub address: SocketAddr,
    pub secret: String,
    /// The detached `ssh -L` forward that carries it, if any (`ssh`,
    /// `firecracker`): it outlives the call that started it.
    pub forward_pid: Option<u32>,
}

/// A started machine: its endpoint, and the lasting state to record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    pub endpoint: Endpoint,
    pub lasting: Lasting,
}

/// Guest paths every VM and ssh machine shares.
pub const DATA: &str = "/layerfs/data";
pub const BUNDLE: &str = "/layerfs/bundle";
pub const STATE: &str = "/layerfs/state";
pub const IMAGE: &str = "/layerfs/image";

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
        if self.kind != Kind::Native && !plain_name(&self.vm_name) {
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
        if self.nested && self.kind != Kind::Lima {
            return refuse("only a lima machine can be nested".into());
        }
        match (&self.host, self.kind) {
            (None, Kind::Firecracker) => {
                return refuse("a firecracker machine names its lima host".into())
            }
            (Some(host), Kind::Firecracker) => {
                if host.kind != Kind::Lima || !host.nested {
                    return refuse(format!(
                        "the firecracker host {} is not a nested lima machine",
                        host.name
                    ));
                }
                host.validate()?;
            }
            (Some(_), _) => return refuse("only a firecracker machine has a host".into()),
            (None, _) => {}
        }
        if let Some(n) = self.state.network {
            if !(1..=250).contains(&n) {
                return refuse(format!("network number {n} is outside 1 to 250"));
            }
        }
        match (&self.ssh, self.kind) {
            (None, Kind::Ssh) => refuse("an ssh machine needs its host".into()),
            (Some(ssh), Kind::Ssh) => ssh.validate(),
            (Some(_), _) => refuse("only an ssh machine has an ssh host".into()),
            (None, _) => Ok(()),
        }
    }
}

impl SshSpec {
    fn validate(&self) -> Result<(), MachineError> {
        let refuse = |message: String| Err(MachineError::new(Code::InvalidInput, message));
        let host_ok = !self.host.is_empty()
            && !self.host.starts_with('-')
            && self
                .host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | ':' | '_'));
        if !host_ok {
            return refuse(format!(
                "ssh host {:?} is not a host name or address",
                self.host
            ));
        }
        if self.user.is_empty() || !plain_name(&self.user) {
            return refuse(format!("ssh user {:?} is not a plain name", self.user));
        }
        if self.port == 0 {
            return refuse("ssh port 0".into());
        }
        if !self.data_root.starts_with('/') || self.data_root.contains(['\n', '\0']) {
            return refuse(format!(
                "data root {:?} is not an absolute path",
                self.data_root
            ));
        }
        Ok(())
    }
}
