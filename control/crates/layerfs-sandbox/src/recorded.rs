//! What a Sandbox's record says runs it (#536 §9.2): a Docker container,
//! a local daemon process (native) or a namespaces Sandbox, read from the
//! record alone and reopened or deleted through its backend.
#[cfg(target_os = "linux")]
use crate::runtime::NAMESPACES;
use crate::runtime::NATIVE;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers;
use layerfs_home::outcome::{failed, refused};
use layerfs_sandbox_backend::docker::{Docker, DockerProvider, SandboxRequest};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_sandbox_backend::host_process;
use layerfs_sandbox_backend::local::LocalProcess;
use layerfs_sandbox_backend::{ContainerId, Sandbox, WireFailure};
use layerfs_sandbox_backend::{LocalRecord, SandboxRecord};
use std::path::PathBuf;
use std::time::Duration;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A Sandbox whose container no longer runs it: the Engine no longer has the
/// container, or has it stopped (it closed when idle, or Docker restarted),
/// which lost its daemon and every mount in it. Only its records, and a
/// stopped container, hold its number; the next mount reclaims them.
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
/// The labels a Sandbox record keeps; a record written before #373 has none.
fn labels(value: &SdkValue) -> Result<Vec<(String, String)>> {
    match value.get("labels") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|item| {
                Ok((
                    helpers::text(item, "key")?.to_owned(),
                    helpers::text(item, "value")?.to_owned(),
                ))
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}
/// The Engine, container and deployment a Docker Sandbox record names.
fn runtime(value: &SdkValue) -> Result<(DockerProvider, ContainerId, SandboxRecord)> {
    let docker =
        Docker::new(helpers::text(value, "docker")?, Duration::from_secs(30)).map_err(failed)?;
    let request = SandboxRequest {
        image: helpers::text(value, "image")?.to_owned(),
        // a PostgreSQL Store's Sandbox mounts no volume (#506)
        store_volume: value.get_text("volume").map(str::to_owned),
        port: 30421,
        labels: labels(value)?,
    };
    let container = ContainerId::parse(helpers::text(value, "container")?).map_err(failed)?;
    let provider = DockerProvider::new(docker);
    Ok((provider, container, SandboxRecord::Docker(request.into())))
}
/// What a Sandbox record names: a Docker container, a local daemon process
/// (`runtime` `native`) with its directory and port, or a namespace Sandbox
/// (`runtime` `namespaces`) with its own runtime, record and directory.
pub enum Recorded {
    Docker(DockerProvider, ContainerId, SandboxRecord),
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
pub fn recorded(args: &SdkValue, id: &str, value: &SdkValue) -> Result<Recorded> {
    #[cfg(target_os = "linux")]
    if value.get_text("runtime") == Some(NAMESPACES) {
        return namespaces_record(args, id, value);
    }
    if value.get_text("runtime") == Some(NATIVE) {
        return local_record(args, id, value);
    }
    if let Some(other) = value.get_text("runtime") {
        // the former `local` runtime (an earlier, unmerged build), or one
        // this host does not run: nothing it names is used or deleted
        return Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {id} is a {other} Sandbox, which this host never reopens, so the record \
                 is neither used nor deleted. Remove what it left by hand, then delete \
                 the folder {}",
                crate::files::sandbox_dir(args)?.display()
            ),
        ));
    }
    let (docker, container, request) = runtime(value)?;
    Ok(Recorded::Docker(docker, container, request))
}
/// A local record is used only where this deployment puts that Sandbox:
/// its directory must be exactly `<LAYERFS_LOCAL_DIR>/<prefix><id>`
/// and named so, so a damaged, edited or stale record (another
/// LAYERFS_LOCAL_DIR or prefix) never points a delete elsewhere. A record
/// without `boot` was written before boot ids were kept: its daemon is
/// matched by id and start time as then, and never signalled.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_record(args: &SdkValue, id: &str, value: &SdkValue) -> Result<Recorded> {
    let (directory, record) = local_record_of(args, id, value)?;
    local(directory, record, value)
}
/// The local Sandbox in `directory` that `record` names.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local(directory: PathBuf, record: LocalRecord, value: &SdkValue) -> Result<Recorded> {
    Ok(Recorded::Local {
        directory,
        id: ContainerId::parse(helpers::text(value, "container")?).map_err(failed)?,
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
fn local_record_of(args: &SdkValue, id: &str, value: &SdkValue) -> Result<(PathBuf, LocalRecord)> {
    let directory = PathBuf::from(helpers::text(value, "directory")?);
    let expected = crate::launch::local::directory(args, id, directory.parent())?;
    // the Sandbox's own directory name, exactly as this deployment names it
    let name = helpers::text(value, "name")?;
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
                crate::files::sandbox_dir(args)?.display()
            ),
        ));
    }
    if (args
        .get("deploy")
        .and_then(|deploy| deploy.get_text("localDir")))
    .is_none()
    {
        crate::launch::local::owned(args, id, &directory)?;
    }
    local_parts(value)
}
/// A local record's directory and physical facts, unchecked.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_parts(value: &SdkValue) -> Result<(PathBuf, LocalRecord)> {
    let directory = PathBuf::from(helpers::text(value, "directory")?);
    let name = helpers::text(value, "name")?;
    let process = recorded_process(value)?;
    let port = value
        .get_int("port")
        .ok_or_else(|| refused("InvalidInput", "local Sandbox record port"))?;
    let record = LocalRecord {
        name: name.to_owned(),
        pid: process.pid,
        start_time: process.start,
        boot: process.boot,
        port: u16::try_from(port).map_err(failed)?,
        labels: labels(value)?,
    };
    Ok((directory, record))
}
/// The daemon (native) or init (namespaces) process a local record names:
/// its id, start time and boot. A record from before boot ids has none.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn recorded_process(value: &SdkValue) -> Result<LocalProcess> {
    let pid = value
        .get_int("pid")
        .ok_or_else(|| refused("InvalidInput", "local Sandbox record pid"))?;
    let boot = match value.get_text("boot") {
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
        start: helpers::text(value, "start")?
            .parse::<u64>()
            .map_err(failed)?,
        boot,
    })
}
/// A namespaces record is used only where its directory checks put that
/// Sandbox ([`local_record_of`]), by any deployment (#529: the record
/// decides).
#[cfg(target_os = "linux")]
fn namespaces_record(args: &SdkValue, id: &str, value: &SdkValue) -> Result<Recorded> {
    let (directory, local) = local_record_of(args, id, value)?;
    namespaces(id, directory, local, value)
}
/// The namespaces Sandbox in `directory` that `local` and its record name.
#[cfg(target_os = "linux")]
fn namespaces(
    id: &str,
    directory: PathBuf,
    local: LocalRecord,
    value: &SdkValue,
) -> Result<Recorded> {
    let image =
        layerfs_sandbox_backend::Image::parse(helpers::text(value, "image")?).map_err(|error| {
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
        ContainerId::parse(helpers::text(value, "container")?).map_err(failed)?,
        SandboxRecord::Namespaces(record),
    ))
}

/// What a layout 2 Sandbox record names (#536 H10), read for its home's
/// conversion with no directory check: the conversion checks that a local
/// one's directory is in the home's old local root itself.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn unchecked(id: &str, value: &SdkValue) -> Result<Recorded> {
    match value.get_text("runtime") {
        None => {
            let (docker, container, request) = runtime(value)?;
            Ok(Recorded::Docker(docker, container, request))
        }
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
fn local_record(_: &SdkValue, id: &str, _: &SdkValue) -> Result<Recorded> {
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
            Self::Docker(provider, id, record) => provider
                .reopen(id, &record)
                .map(|s| Box::new(Sandbox::Docker(s))),
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
            Self::Docker(provider, id, record) => {
                let instance = provider
                    .reopen_stopped(id, &record)
                    .map_err(failed)
                    .expect("validated record");
                Box::new(Sandbox::Docker(instance))
            }
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
