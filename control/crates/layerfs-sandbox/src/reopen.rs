//! What a Sandbox's record says runs it (#536 §9.2): a local daemon process
//! (native, macOS) or a namespaces Sandbox (Linux), read from the record
//! alone and reopened or deleted through its backend. A record of the
//! retired docker backend (D-8, layerfs-project#111) is refused: nothing it
//! names is reopened or deleted here. A local record this build does not
//! read ([`unread`]: a field missing or of another shape, or a native
//! record on Linux) is refused with the records-only `sandbox remove` as
//! its fix, and that remove takes it without a signal (RV3 FAIL 2, 3).
use crate::host::Host;
use crate::record::DaemonRecord;
#[cfg(target_os = "linux")]
use crate::runtime::NAMESPACES;
use crate::runtime::NATIVE;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_os::process::LocalProcess;
use layerfs_home::helpers;
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{failed, refused, with_fix};
#[cfg(target_os = "macos")]
use crate::process;
use crate::LocalRecord;
#[cfg(target_os = "linux")]
use crate::SandboxRecord;
use crate::{ContainerId, Sandbox, WireFailure};
use std::path::PathBuf;

/// A daemon record's runtime id, required.
fn container(value: &DaemonRecord) -> Result<&str> {
    helpers::required(value.container(), "container")
}

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A Sandbox whose daemon no longer runs: its process is gone, or stopped
/// (it closed when idle), which lost every mount in it. Only its records
/// hold its name; the next mount reclaims them.
/// Its daemon is a process on either backend (native or namespaces), so
/// the message names the daemon, not a container.
pub fn gone(id: &str, container: &str, stopped: bool) -> SdkRefusal {
    let state = if stopped { "has stopped" } else { "is gone" };
    SdkRefusal {
        code: "NoSuchSandbox".to_owned(),
        message: format!(
            "the daemon of Sandbox {id} {state}, and every Workspace that was in it with it"
        ),
        detail: SdkValue::map([
            ("sandbox", SdkValue::text(id)),
            ("container", SdkValue::text(container)),
            ("containerGone", SdkValue::Bool(true)),
            ("containerStopped", SdkValue::Bool(stopped)),
        ]),
    }
}
/// The refusal of every call but `sandbox.remove` on Sandbox `id`, recorded
/// under the retired docker backend (D-8, layerfs-project#111): its
/// container (`container`, when its record names it) is never reopened,
/// stopped or deleted here.
pub fn retired(id: &str, container: Option<&str>) -> SdkRefusal {
    refused(
        "Unsupported",
        format!(
            "Sandbox {id} was created on the docker Sandbox backend, which this build no longer \
             runs (a docker machine's Sandboxes now run in namespaces in its worker): \
             `layerfs sandbox remove {id}` removes its records, {}, and `layerfs sandbox create` \
             makes it again",
            container_fix(container)
        ),
    )
}

/// How the container of a Sandbox of the retired docker backend goes:
/// `docker rm -f` with the name its record keeps; a record that keeps none
/// says so, since this build does not compute an older name.
pub fn container_fix(container: Option<&str>) -> String {
    match container {
        Some(container) => format!("`docker rm -f {container}` removes its container"),
        None => "its record does not name its container (`docker ps -a` lists them)".to_owned(),
    }
}

/// What a Sandbox record names: a local daemon process (`runtime`
/// `native`) with its directory and port, or a namespace Sandbox (`runtime`
/// `namespaces`) with its own runtime, record and directory.
pub enum Recorded {
    #[cfg(target_os = "macos")]
    Local {
        directory: PathBuf,
        id: ContainerId,
        process: LocalProcess,
        port: u16,
        labels: Vec<(String, String)>,
    },
    #[cfg(target_os = "linux")]
    Namespaces(crate::Runtime, ContainerId, SandboxRecord),
}
/// Reads Sandbox `id`'s record's runtime.
/// The record alone decides (#529): a caller's deployment backend is not
/// consulted, so every call on a Sandbox reopens it by its own runtime.
pub fn recorded(host: &Host, id: &str, value: &DaemonRecord) -> Result<Recorded> {
    if let Some(why) = unread(value) {
        let refusal = refused(
            "InvalidInput",
            format!(
                "Sandbox {id}'s record is not one this build reads ({why}), so it is neither \
                 reopened nor signalled: `layerfs sandbox remove {id}` removes its records alone"
            ),
        );
        return Err(with_fix(refusal, crate::state::remove_fix(id)));
    }
    #[cfg(target_os = "linux")]
    if value.runtime() == Some(NAMESPACES) {
        return namespaces_record(host, id, value);
    }
    #[cfg(target_os = "macos")]
    if value.runtime() == Some(NATIVE) {
        return local_record(host, id, value);
    }
    match value.runtime() {
        None | Some("docker") => Err(retired(id, value.name())),
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
/// Why a local Sandbox record (`runtime` `native` or `namespaces`) is not
/// one this build reads, or `None` when it is, or is not local. A record
/// with a `forward` is not local whatever its `runtime` says: the caller's
/// home keeps only the forward to a worker's Sandbox (its name, runtime,
/// platform and image), and the worker reads its own local record (B10:
/// checking it here sent every Sandbox remove on a machine down the
/// records-only path). A local record is unread when it is a native
/// record anywhere but macOS (Linux starts none, #533 O27), or a field a
/// 2.0 record always has that is missing or of another shape (`directory`,
/// `name`, `container`, `pid`, `boot`, `start`, `port`, `labels`; a
/// namespaces record's `image`). Its identity cannot be checked, so
/// nothing is signalled through it.
pub fn unread(value: &DaemonRecord) -> Option<String> {
    if value.forward().is_some() {
        return None;
    }
    let runtime = value.runtime()?;
    if runtime == NATIVE && !cfg!(target_os = "macos") {
        return Some("a native Sandbox, which this build runs on macOS only".to_owned());
    }
    if runtime != NATIVE && runtime != crate::runtime::NAMESPACES {
        return None;
    }
    let checked = || -> Result<()> {
        helpers::required(value.directory(), "directory")?;
        helpers::required(value.name(), "name")?;
        ContainerId::parse(container(value)?).map_err(failed)?;
        recorded_process(value)?;
        let port = value
            .port()
            .ok_or_else(|| refused("InvalidInput", "local Sandbox record port"))?;
        u16::try_from(port).map_err(failed)?;
        value.labels()?;
        if runtime == crate::runtime::NAMESPACES {
            let image = helpers::required(value.image(), "image")?;
            crate::Image::parse(image).map_err(failed)?;
        }
        Ok(())
    };
    checked().err().map(|refusal| refusal.message)
}
/// A local record is used only where this deployment puts that Sandbox:
/// its own `name` must be exactly the one this deployment gives Sandbox `id`
/// (`<prefix><id>`, [`crate::lifecycle::sandbox_name`]) and its
/// directory exactly `<LAYERFS_LOCAL_DIR>/<that name>`, so a damaged,
/// edited or stale record (another
/// LAYERFS_LOCAL_DIR or prefix) never points a delete elsewhere.
#[cfg(target_os = "macos")]
fn local_record(host: &Host, id: &str, value: &DaemonRecord) -> Result<Recorded> {
    let (directory, record) = local_record_of(host, id, value)?;
    local(directory, record, value)
}
/// The local Sandbox in `directory` that `record` names.
#[cfg(target_os = "macos")]
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
/// none) beside the record's own directory, which must still carry a
/// name this deployment gives Sandbox `id`
/// ([`crate::lifecycle::sandbox_name`]) and be owned by this record
/// ([`crate::launch::local::owned`]).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_record_of(host: &Host, id: &str, value: &DaemonRecord) -> Result<(PathBuf, LocalRecord)> {
    let directory = PathBuf::from(helpers::required(value.directory(), "directory")?);
    // the Sandbox's own name, exactly the one this deployment gives it:
    // nothing is stripped from it
    let name = helpers::required(value.name(), "name")?;
    let given = crate::lifecycle::sandbox_name(host.deploy(), id)?;
    let named = name == given;
    let expected = crate::launch::local::directory(host.deploy(), id, &given, directory.parent())?;
    if !named || directory != expected {
        let refusal = refused(
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
        );
        // a name this deployment does not give: `sandbox remove` removes
        // its records alone
        return Err(match named {
            false => layerfs_home::outcome::with_fix(
                refusal,
                format!("layerfs sandbox remove {id}", id = shell_word(id)),
            ),
            true => refusal,
        });
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
/// its id, start time and boot.
pub fn recorded_process(value: &DaemonRecord) -> Result<LocalProcess> {
    let pid = value
        .pid()
        .ok_or_else(|| refused("InvalidInput", "local Sandbox record pid"))?;
    let boot = value
        .boot()
        .filter(|hex| hex.len() == 32)
        .and_then(|hex| u128::from_str_radix(hex, 16).ok())
        .ok_or_else(|| refused("InvalidInput", "local Sandbox record boot"))?;
    Ok(LocalProcess {
        pid: u32::try_from(pid).map_err(failed)?,
        start: helpers::required(value.start(), "start")?
            .parse::<u64>()
            .map_err(failed)?,
        boot: Some(boot),
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
    let image = crate::Image::parse(image).map_err(|error| {
        refused(
            "InvalidInput",
            format!(
                "Sandbox {id} is a namespaces Sandbox whose record names no image digest \
                 ({error}); it cannot be given a root, so it is neither reopened nor deleted"
            ),
        )
    })?;
    let network = value.network().map(str::to_owned);
    let record = crate::NamespaceRecord {
        local,
        image,
        network,
    };
    let root = (directory.parent())
        .ok_or_else(|| refused("InvalidInput", "namespaces Sandbox directory"))?;
    let provider =
        crate::namespaces::NamespaceProvider::new(root).map_err(|error| {
            refused(
                "Unsupported",
                format!(
                    "namespaces Sandbox at {} cannot run on this host: {error}",
                    directory.display()
                ),
            )
        })?;
    Ok(Recorded::Namespaces(
        crate::Runtime::Namespaces(provider),
        ContainerId::parse(container(value)?).map_err(failed)?,
        SandboxRecord::Namespaces(record),
    ))
}

impl Recorded {
    /// Stop an exact recorded runtime without endpoint admission or daemon RPC.
    pub fn stop_now(self) -> Result<()> {
        match self.for_stop()? {
            Some(mut owner) => owner.stop(0).map_err(crate::daemon::values::wire),
            None => Ok(()),
        }
    }
    /// The exact recorded runtime, reopened for a stop that has not started:
    /// nothing is signalled here, so a refusal leaves the runtime as it was.
    /// `None`: the runtime is already stopped.
    pub fn for_stop(self) -> Result<Option<Box<Sandbox>>> {
        Ok(Some(match self {
            #[cfg(target_os = "macos")]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => Box::new(Sandbox::Process(
                process::Sandbox::reopen_for_stop(directory, id, process, port, labels)
                    .map_err(crate::daemon::values::wire)?,
            )),
            #[cfg(target_os = "linux")]
            Self::Namespaces(runtime, id, record) => match runtime.reopen(id, &record) {
                Ok(owner) => Box::new(owner),
                Err(error) if error.container_stopped() => return Ok(None),
                Err(error) => return Err(crate::daemon::values::wire(error)),
            },
        }))
    }
    /// The runtime, observed running once (it starts nothing).
    pub fn reopen(self) -> std::result::Result<Box<Sandbox>, Box<WireFailure>> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => process::Sandbox::reopen(directory, id, process, port, labels)
                .map(|s| Box::new(Sandbox::Process(s))),
            #[cfg(target_os = "linux")]
            Self::Namespaces(runtime, id, record) => runtime.reopen(id, &record).map(Box::new),
        }
    }
    /// The runtime found stopped: only its one Delete is admitted.
    pub fn stopped(self) -> Box<Sandbox> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => Box::new(Sandbox::Process(process::Sandbox::stopped(
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
