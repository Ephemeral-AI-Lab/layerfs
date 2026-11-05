//! The DigitalOcean Kubernetes environment a `doks` machine is made from:
//! its namespace, pod and PVC, and the ssh endpoint that reaches the pod.
//!
//! The adapter reads no LayerFS home.  A caller supplies the values as
//! environment variables; [`DoksEnv::from_env`] resolves the pod IP through
//! `kubectl` when `DOKS_SSH_HOST` is unset, and [`DoksEnv::from_pairs`]
//! takes exactly what it is given, so tests need no cluster.

use crate::error::{Error, Result};
use layerfs_machine::SshSpec;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Kubernetes namespace of the pod (`default`).
pub const NAMESPACE: &str = "DOKS_NAMESPACE";
/// Pod that runs the machine's sshd and worker (required).
pub const POD: &str = "DOKS_POD";
/// PersistentVolumeClaim mounted at the machine's data root (required).
pub const PVC: &str = "DOKS_PVC";
/// Guest path the PVC is mounted at (`/layerfs/data`).
pub const DATA_ROOT: &str = "DOKS_DATA_ROOT";
/// ssh host or address; resolved from the pod IP when unset.
pub const SSH_HOST: &str = "DOKS_SSH_HOST";
/// ssh user (`root`).
pub const SSH_USER: &str = "DOKS_SSH_USER";
/// ssh port (`22`).
pub const SSH_PORT: &str = "DOKS_SSH_PORT";
/// ssh key file; unset uses ssh-agent and the default keys.
pub const SSH_IDENTITY: &str = "DOKS_SSH_IDENTITY";
/// known_hosts file holding the pod's key (`$HOME/.ssh/known_hosts`).
pub const KNOWN_HOSTS: &str = "DOKS_KNOWN_HOSTS";

const DEFAULT_NAMESPACE: &str = "default";
const DEFAULT_DATA_ROOT: &str = "/layerfs/data";
const DEFAULT_USER: &str = "root";
const DEFAULT_PORT: u16 = 22;
const NAME_MAX: usize = 253;

/// The DOKS machine's route, parsed from its environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoksEnv {
    pub namespace: String,
    pub pod: String,
    pub pvc: String,
    pub data_root: String,
    pub ssh_host: String,
    pub ssh_user: String,
    pub ssh_port: u16,
    pub ssh_identity: Option<PathBuf>,
    pub known_hosts: PathBuf,
}

/// The pod and PVC phases [`preflight`] read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preflight {
    pub pod_phase: String,
    pub pvc_phase: String,
}

impl DoksEnv {
    /// The process environment; the pod IP is resolved with `kubectl` when
    /// `DOKS_SSH_HOST` is unset.
    pub fn from_env() -> Result<Self> {
        let mut values: BTreeMap<String, String> = std::env::vars().collect();
        if optional(&values, SSH_HOST).is_none() {
            let namespace =
                optional(&values, NAMESPACE).unwrap_or_else(|| DEFAULT_NAMESPACE.to_owned());
            let pod = required(&values, POD)?;
            let ip = pod_ip(Path::new("kubectl"), &namespace, &pod)?;
            values.insert(SSH_HOST.to_owned(), ip);
        }
        Self::from_map(&values)
    }

    /// The given pairs, with no `kubectl` call and no default beyond the
    /// documented ones.  `DOKS_SSH_HOST` is required here.
    pub fn from_pairs<I, K, V>(pairs: I) -> Result<Self>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let values = pairs
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_owned(), value.as_ref().to_owned()))
            .collect();
        Self::from_map(&values)
    }

    /// The ssh host half of the machine's [`layerfs_machine::Spec`].
    pub fn ssh_spec(&self) -> SshSpec {
        SshSpec {
            host: self.ssh_host.clone(),
            user: self.ssh_user.clone(),
            port: self.ssh_port,
            identity: self.ssh_identity.clone(),
            known_hosts: self.known_hosts.clone(),
            data_root: self.data_root.clone(),
        }
    }

    fn from_map(values: &BTreeMap<String, String>) -> Result<Self> {
        let namespace = optional(values, NAMESPACE).unwrap_or_else(|| DEFAULT_NAMESPACE.to_owned());
        plain(NAMESPACE, &namespace, 63)?;
        let pod = required(values, POD)?;
        plain(POD, &pod, NAME_MAX)?;
        let pvc = required(values, PVC)?;
        plain(PVC, &pvc, NAME_MAX)?;
        let ssh_host = required(values, SSH_HOST)?;
        ssh_host_name(&ssh_host)?;
        let ssh_user = optional(values, SSH_USER).unwrap_or_else(|| DEFAULT_USER.to_owned());
        user_name(&ssh_user)?;
        let ssh_port = match optional(values, SSH_PORT) {
            Some(text) => parse_port(&text)?,
            None => DEFAULT_PORT,
        };
        let data_root = optional(values, DATA_ROOT).unwrap_or_else(|| DEFAULT_DATA_ROOT.to_owned());
        absolute_path(DATA_ROOT, &data_root)?;
        let ssh_identity = optional(values, SSH_IDENTITY).map(PathBuf::from);
        let known_hosts = match optional(values, KNOWN_HOSTS) {
            Some(path) => PathBuf::from(path),
            None => default_known_hosts()?,
        };
        Ok(Self {
            namespace,
            pod,
            pvc,
            data_root,
            ssh_host,
            ssh_user,
            ssh_port,
            ssh_identity,
            known_hosts,
        })
    }
}

/// The pod's IP, as `kubectl` reports it.
pub fn pod_ip(kubectl: &Path, namespace: &str, pod: &str) -> Result<String> {
    let ip = run(
        kubectl,
        &[
            "get",
            "pod",
            "-n",
            namespace,
            pod,
            "-o",
            "jsonpath={.status.podIP}",
        ],
    )?;
    ssh_host_name(&ip)?;
    Ok(ip)
}

/// The pod is `Running` and the machine's PVC is `Bound`, before a connect.
pub fn preflight(kubectl: &Path, env: &DoksEnv) -> Result<Preflight> {
    let pod_phase = run(
        kubectl,
        &[
            "get",
            "pod",
            "-n",
            &env.namespace,
            &env.pod,
            "-o",
            "jsonpath={.status.phase}",
        ],
    )?;
    let pvc_phase = run(
        kubectl,
        &[
            "get",
            "pvc",
            "-n",
            &env.namespace,
            &env.pvc,
            "-o",
            "jsonpath={.status.phase}",
        ],
    )?;
    if pod_phase != "Running" {
        return Err(Error::new(format!(
            "pod {}/{} is {pod_phase}, not Running",
            env.namespace, env.pod
        )));
    }
    if pvc_phase != "Bound" {
        return Err(Error::new(format!(
            "PVC {}/{} is {pvc_phase}, not Bound",
            env.namespace, env.pvc
        )));
    }
    Ok(Preflight {
        pod_phase,
        pvc_phase,
    })
}

fn run(kubectl: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(kubectl)
        .args(args)
        .output()
        .map_err(|error| Error::new(format!("cannot run {}: {error}", kubectl.display())))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(format!(
            "{} {} failed: {}",
            kubectl.display(),
            args.join(" "),
            detail.trim()
        )));
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        return Err(Error::new(format!(
            "{} {} returned nothing",
            kubectl.display(),
            args.join(" ")
        )));
    }
    Ok(value)
}

fn optional(values: &BTreeMap<String, String>, key: &str) -> Option<String> {
    values.get(key).filter(|value| !value.is_empty()).cloned()
}

fn required(values: &BTreeMap<String, String>, key: &str) -> Result<String> {
    optional(values, key).ok_or_else(|| Error::new(format!("{key} is unset")))
}

fn plain(field: &str, value: &str, max: usize) -> Result<()> {
    let ok = !value.is_empty()
        && value.len() <= max
        && value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.'));
    if ok {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{field} {value:?} is not a Kubernetes name"
        )))
    }
}

fn ssh_host_name(value: &str) -> Result<()> {
    let ok = !value.is_empty()
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | ':' | '_'));
    if ok {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{SSH_HOST} {value:?} is not a host name or address"
        )))
    }
}

fn user_name(value: &str) -> Result<()> {
    let ok = !value.is_empty()
        && value.len() <= 63
        && value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{SSH_USER} {value:?} is not a user name"
        )))
    }
}

fn parse_port(value: &str) -> Result<u16> {
    let port: u16 = value
        .parse()
        .map_err(|_| Error::new(format!("{SSH_PORT} {value:?} is not a port")))?;
    if port == 0 {
        return Err(Error::new(format!("{SSH_PORT} 0")));
    }
    Ok(port)
}

fn absolute_path(field: &str, value: &str) -> Result<()> {
    if value.starts_with('/') && !value.chars().any(|c| matches!(c, '\n' | '\0')) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{field} {value:?} is not an absolute path"
        )))
    }
}

fn default_known_hosts() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".ssh").join("known_hosts"))
        .ok_or_else(|| Error::new(format!("{KNOWN_HOSTS} is unset and so is HOME")))
}
