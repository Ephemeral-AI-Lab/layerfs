//! `machine.update`.
use super::list::changed;
use super::spec::machine_of;
use crate::Related;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{lock, paths};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_platform::HostOs;
use std::path::Path;

/// What an update answers beside `ok` and `outcome`.
const ANSWER: [&str; 9] = [
    "machine",
    "platform",
    "cpus",
    "memory_bytes",
    "disk_bytes",
    "image",
    "docker",
    "wsl",
    "daemon",
];

/// A `machine.update` request. The name stays text: it is checked under the
/// home lock.
pub struct MachineUpdate {
    /// The machine's name.
    pub name: String,
    /// A VM's new CPUs.
    pub cpus: Option<i64>,
    /// A VM's new memory in bytes.
    pub memory: Option<i64>,
    /// The image.
    pub image: Option<String>,
    /// A built-in machine's Docker socket.
    pub docker: Option<String>,
    /// A built-in machine's WSL distribution.
    pub wsl: Option<String>,
    /// A built-in machine's daemon.
    pub daemon: Option<String>,
}

/// `machine.update(name, { cpus, memory, image, docker, wsl, daemon })`
/// (O11): the built-in machines' image, Docker socket (`""` clears it: the
/// next start chooses and records one again), WSL distribution and the
/// daemon their Sandboxes run (an absolute path to a file). Resizing a VM
/// is refused while
/// Sandboxes run on it.
pub fn update(home: &Path, request: &MachineUpdate, related: Related) -> Outcome {
    let home = home.to_path_buf();
    let name = request.name.as_str();
    let _lock = lock::take(&home)?;
    let record = machine_of(&home, name)?;
    let kind = record.platform()?;
    let resize = request.cpus.is_some() || request.memory.is_some();
    if kind.built_in() && resize {
        return Err(refused(
            "InvalidInput",
            format!("{name} is built in; it has no size of its own"),
        ));
    }
    let built_in_only = [&request.docker, &request.wsl, &request.daemon];
    if !kind.built_in() && built_in_only.iter().any(|value| value.is_some()) {
        return Err(refused(
            "InvalidInput",
            "docker, wsl and daemon are the built-in machines' settings",
        ));
    }
    if let Some(daemon) = &request.daemon {
        let path = std::path::Path::new(daemon);
        if !path.is_absolute() || !path.is_file() {
            return Err(refused(
                "InvalidInput",
                format!("daemon must be the absolute path of a file; {daemon} is not one"),
            ));
        }
    }
    let docker = request.docker.as_deref().map(docker_socket).transpose()?;
    // its worker container is in the Engine it records: moving the socket
    // under it would leave the container running where no call reaches it
    if docker.is_some() && paths::worker_record(&home, name).exists() {
        let refusal = refused(
            "InUse",
            format!("machine {name} is running on its Docker socket: stop it before changing the socket"),
        );
        return Err(layerfs_home::outcome::with_fix(
            refusal,
            format!("layerfs machine stop {name}"),
        ));
    }
    if request.wsl.is_some() && HostOs::current() != HostOs::Windows {
        return Err(refused("InvalidInput", "wsl is a Windows host's setting"));
    }
    // a running Sandbox keeps the image and settings it started with
    (related.can_change)(&home, name)?;
    if resize {
        return Err(refused(
            "Unsupported",
            format!(
                "resizing a {} machine is not built yet; nothing was changed",
                kind.name()
            ),
        ));
    }
    let mut record = record;
    let settings = [
        ("image", request.image.as_deref()),
        ("docker", docker.flatten()),
        ("wsl", request.wsl.as_deref()),
        ("daemon", request.daemon.as_deref()),
    ];
    for (field, value) in settings {
        if let Some(value) = value {
            record = record.with(field, SdkValue::text(value));
        }
    }
    // `--docker ""`: back to the automatic choice until the next start
    // records one; null, not absent, which for a machine that ran means the
    // default socket it ran on before sockets were recorded
    if docker == Some(None) {
        record = record.with("docker", SdkValue::Null);
    }
    record.write(&home)?;
    changed(&record, "updated", &ANSWER)
}

/// The Docker socket `--docker` names: an absolute path, or `unix://` and
/// one; `None` for `""`, which clears the recorded one.
fn docker_socket(value: &str) -> Result<Option<&str>, layerfs_bridge::control::SdkRefusal> {
    if value.is_empty() {
        return Ok(None);
    }
    let path = value.strip_prefix("unix://").unwrap_or(value);
    if !std::path::Path::new(path).is_absolute() {
        return Err(refused(
            "InvalidInput",
            format!(
                "docker must be the absolute path of the Engine's socket (or unix:// and it), or \"\" to choose it again at the next start; {value} is not"
            ),
        ));
    }
    Ok(Some(path))
}
