//! What a Sandbox's record says runs it (#536 §9.2): a local daemon process
//! (native) or a namespaces Sandbox, read from the record alone and
//! reopened or deleted through its backend. A record of the retired docker
//! backend (D-8, layerfs-project#111) is refused: nothing it names is
//! reopened or deleted here.
use crate::host::Host;
use crate::record::DaemonRecord;
#[cfg(target_os = "linux")]
use crate::runtime::NAMESPACES;
use crate::runtime::NATIVE;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers;
use layerfs_home::outcome::{failed, refused};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_sandbox_backend::host_process;
use layerfs_sandbox_backend::local::LocalProcess;
use layerfs_sandbox_backend::LocalRecord;
#[cfg(target_os = "linux")]
use layerfs_sandbox_backend::SandboxRecord;
use layerfs_sandbox_backend::{ContainerId, Sandbox, WireFailure};
use std::path::PathBuf;

/// A daemon record's runtime id, required.
fn container(value: &DaemonRecord) -> Result<&str> {
    helpers::required(value.container(), "container")
}

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A Sandbox whose daemon no longer runs: its process is gone, or stopped
/// (it closed when idle), which lost every mount in it. Only its records
/// hold its name; the next mount reclaims them.
pub fn gone(id: &str, container: &str, stopped: bool) -> SdkRefusal {
    let state = if stopped { "stopped" } else { "gone" };
    SdkRefusal {
        code: "NoSuchSandbox".to_owned(),
        message: format!(
            "the container of Sandbox {id} is {state}, with every Workspace that was in it"
        ),
        detail: SdkValue::map([
            ("sandbox", SdkValue::text(id)),
            ("container", SdkValue::text(container)),
            ("containerGone", SdkValue::Bool(true)),
            ("containerStopped", SdkValue::Bool(stopped)),
        ]),
    }
}
/// The refusal of every call but `sandbox.close` on Sandbox `id`, recorded
/// under the retired docker backend (D-8, layerfs-project#111): its
/// container `container` is never reopened, stopped or deleted here.
pub fn retired(id: &str, container: &str) -> SdkRefusal {
    refused(
        "Unsupported",
        format!(
            "Sandbox {id} was created on the docker Sandbox backend, which this build no longer \
             runs (a docker machine's Sandboxes now run in namespaces in its worker): \
             `layerfs sandbox close {id}` forgets it, `docker rm -f {container}` removes its \
             container, and `layerfs sandbox create` makes it again"
        ),
    )
}

/// Whether a side's daemon record (`record.json`'s `daemon` on `native`, a
/// worker's own record) names the retired docker backend: `runtime`
/// `docker`, or none at all, which only a docker record lacked.
pub fn retired_record(value: &DaemonRecord) -> bool {
    value.retired()
}
/// What a Sandbox record names: a local daemon process (`runtime`
/// `native`) with its directory and port, or a namespace Sandbox (`runtime`
/// `namespaces`) with its own runtime, record and directory.
pub enum Recorded {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Local {
        directory: PathBuf,
        id: ContainerId,
        process: LocalProcess,
        port: u16,
        labels: Vec<(String, String)>,
    },
    #[cfg(target_os = "linux")]
    Namespaces(layerfs_sandbox_backend::Runtime, ContainerId, SandboxRecord),
}
/// Reads Sandbox `id`'s record's runtime.
/// The record alone decides (#529): a caller's deployment backend is not
/// consulted, so every call on a Sandbox reopens it by its own runtime.
pub fn recorded(host: &Host, id: &str, value: &DaemonRecord) -> Result<Recorded> {
    #[cfg(target_os = "linux")]
    if value.runtime() == Some(NAMESPACES) {
        return namespaces_record(host, id, value);
    }
    if value.runtime() == Some(NATIVE) {
        return local_record(host, id, value);
    }
    match value.runtime() {
        None | Some("docker") => Err(retired(id, value.name().unwrap_or(""))),
        // the former `local` runtime (an earlier, unmerged build), or one
        // this host does not run: nothing it names is used or deleted
        Some(other) => Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {id} is a {other} Sandbox, which this host never reopens, so the record \
                 is neither used nor deleted. Remove what it left by hand, then delete \
                 the folder {}",
                crate::files::sandbox_dir(host)?.display()
            ),
        )),
    }
}
/// A local record is used only where this deployment puts that Sandbox:
/// its directory must be exactly `<LAYERFS_LOCAL_DIR>/<prefix><id>`
/// and named so, so a damaged, edited or stale record (another
/// LAYERFS_LOCAL_DIR or prefix) never points a delete elsewhere. A record
/// without `boot` was written before boot ids were kept: its daemon is
/// matched by id and start time as then, and never signalled.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_record(host: &Host, id: &str, value: &DaemonRecord) -> Result<Recorded> {
    let (directory, record) = local_record_of(host, id, value)?;
    local(directory, record, value)
}
/// The local Sandbox in `directory` that `record` names.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local(directory: PathBuf, record: LocalRecord, value: &DaemonRecord) -> Result<Recorded> {
    Ok(Recorded::Local {
        directory,
        id: ContainerId::parse(container(value)?).map_err(failed)?,
        process: LocalProcess {
            pid: record.pid,
            start: record.start_time,
            boot: record.boot,
        },
        port: record.port,
        labels: record.labels,
    })
}
/// A record's directory and physical facts, checked against where this
/// deployment puts that Sandbox: under its LAYERFS_LOCAL_DIR when it sets
/// one, else (#529: the record decides, so a caller of another backend has
/// none) beside the record's own directory, which must still carry the
/// name this deployment gives Sandbox `id` and be owned by this record
/// ([`crate::launch::local::owned`]).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_record_of(host: &Host, id: &str, value: &DaemonRecord) -> Result<(PathBuf, LocalRecord)> {
    let directory = PathBuf::from(helpers::required(value.directory(), "directory")?);
    let expected = crate::launch::local::directory(host.deploy(), id, directory.parent())?;
    // the Sandbox's own directory name, exactly as this deployment names it
    let name = helpers::required(value.name(), "name")?;
    if directory != expected || expected.file_name().and_then(|n| n.to_str()) != Some(name) {
        return Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {id}'s local record names {} (as {name}), but this deployment puts it at \
                 {}: the record is damaged or from another LAYERFS_LOCAL_DIR or prefix, so it is \
                 neither used nor deleted. Remove what it left by hand, then delete \
                 the folder {}",
                directory.display(),
                expected.display(),
                crate::files::sandbox_dir(host)?.display()
            ),
        ));
    }
    if (host.deploy())
        .and_then(|deploy| deploy.local_dir.as_deref())
        .is_none()
    {
        crate::launch::local::owned(host, id, &directory)?;
    }
    local_parts(value)
}
/// A local record's directory and physical facts, unchecked.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_parts(value: &DaemonRecord) -> Result<(PathBuf, LocalRecord)> {
    let directory = PathBuf::from(helpers::required(value.directory(), "directory")?);
    let name = helpers::required(value.name(), "name")?;
    let process = recorded_process(value)?;
    let port = value
        .port()
        .ok_or_else(|| refused("InvalidInput", "local Sandbox record port"))?;
    let record = LocalRecord {
        name: name.to_owned(),
        pid: process.pid,
        start_time: process.start,
        boot: process.boot,
        port: u16::try_from(port).map_err(failed)?,
        labels: value.labels()?,
    };
    Ok((directory, record))
}
/// The daemon (native) or init (namespaces) process a local record names:
/// its id, start time and boot. A record from before boot ids has none.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn recorded_process(value: &DaemonRecord) -> Result<LocalProcess> {
    let pid = value
        .pid()
        .ok_or_else(|| refused("InvalidInput", "local Sandbox record pid"))?;
    let boot = match value.boot() {
        None => None,
        Some(hex) => Some(
            (hex.len() == 32)
                .then(|| u128::from_str_radix(hex, 16).ok())
                .flatten()
                .ok_or_else(|| refused("InvalidInput", "local Sandbox record boot"))?,
        ),
    };
    Ok(LocalProcess {
        pid: u32::try_from(pid).map_err(failed)?,
        start: helpers::required(value.start(), "start")?
            .parse::<u64>()
            .map_err(failed)?,
        boot,
    })
}
/// A namespaces record is used only where its directory checks put that
/// Sandbox ([`local_record_of`]), by any deployment (#529: the record
/// decides).
#[cfg(target_os = "linux")]
fn namespaces_record(host: &Host, id: &str, value: &DaemonRecord) -> Result<Recorded> {
    let (directory, local) = local_record_of(host, id, value)?;
    namespaces(id, directory, local, value)
}
/// The namespaces Sandbox in `directory` that `local` and its record name.
#[cfg(target_os = "linux")]
fn namespaces(
    id: &str,
    directory: PathBuf,
    local: LocalRecord,
    value: &DaemonRecord,
) -> Result<Recorded> {
    let image = helpers::required(value.image(), "image")?;
    let image = layerfs_sandbox_backend::Image::parse(image).map_err(|error| {
        refused(
            "InvalidInput",
            format!(
                "Sandbox {id} is a namespaces Sandbox whose record names no image digest \
                 ({error}); it cannot be given a root, so it is neither reopened nor deleted"
            ),
        )
    })?;
    let record = layerfs_sandbox_backend::NamespaceRecord { local, image };
    let root = (directory.parent())
        .ok_or_else(|| refused("InvalidInput", "namespaces Sandbox directory"))?;
    let provider =
        layerfs_sandbox_backend::namespaces::NamespaceProvider::new(root).map_err(|error| {
            refused(
                "Unsupported",
                format!(
                    "namespaces Sandbox at {} cannot run on this host: {error}",
                    directory.display()
                ),
            )
        })?;
    Ok(Recorded::Namespaces(
        layerfs_sandbox_backend::Runtime::Namespaces(provider),
        ContainerId::parse(container(value)?).map_err(failed)?,
        SandboxRecord::Namespaces(record),
    ))
}

/// What a layout 2 Sandbox record names (#536 H10), read for its home's
/// conversion with no directory check: the conversion checks that a local
/// one's directory is in the home's old local root itself.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn unchecked(id: &str, value: &DaemonRecord) -> Result<Recorded> {
    match value.runtime() {
        None | Some("docker") => Err(retired(id, value.name().unwrap_or(""))),
        Some(NATIVE) => {
            let (directory, record) = local_parts(value)?;
            local(directory, record, value)
        }
        #[cfg(target_os = "linux")]
        Some(NAMESPACES) => {
            let (directory, record) = local_parts(value)?;
            namespaces(id, directory, record, value)
        }
        Some(other) => Err(refused(
            "InvalidInput",
            format!("Sandbox {id} is a {other} Sandbox, which this host never reopens"),
        )),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn local_record(_: &Host, id: &str, _: &DaemonRecord) -> Result<Recorded> {
    Err(refused(
        "Unsupported",
        format!(
            "Sandbox {id} is a native Sandbox (LAYERFS_SANDBOX=native), which needs Linux or macOS"
        ),
    ))
}
impl Recorded {
    /// The runtime, observed running once (it starts nothing).
    pub fn reopen(self) -> std::result::Result<Box<Sandbox>, Box<WireFailure>> {
        match self {
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => host_process::Sandbox::reopen(directory, id, process, port, labels)
                .map(|s| Box::new(Sandbox::HostProcess(s))),
            #[cfg(target_os = "linux")]
            Self::Namespaces(runtime, id, record) => runtime.reopen(id, &record).map(Box::new),
        }
    }
    /// The runtime found stopped: only its one Delete is admitted.
    pub fn stopped(self) -> Box<Sandbox> {
        match self {
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => Box::new(Sandbox::HostProcess(host_process::Sandbox::stopped(
                directory, id, process, port, labels,
            ))),
            #[cfg(target_os = "linux")]
            Self::Namespaces(runtime, id, record) => Box::new(
                runtime
                    .reopen_stopped(id, &record)
                    .map_err(failed)
                    .expect("validated record"),
            ),
        }
    }
}
