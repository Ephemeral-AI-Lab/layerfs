//! A Sandbox's runtime: `LAYERFS_SANDBOX=process` on macOS (loopback NFS,
//! caller identity) or `LAYERFS_SANDBOX=namespaces` on Linux (root FUSE
//! daemon, nonroot commands; a VM's worker and the docker machine's worker
//! container, D-8). Linux starts no process Sandbox (#533 O27), and a process
//! record found there is refused, its fix the records-only `sandbox
//! remove` (RV3 FAIL 3). The
//! docker backend is retired (D-8): a deployment naming it is refused. All
//! use the same authenticated daemon control pipeline and the same concrete
//! `crate::backend::dispatch::Sandbox` handle. A Sandbox also names its
//! platform (#529), recorded beside the runtime; every later call follows
//! the record, not the caller's deployment.
use crate::exec::output::{Place, Spool};
use crate::route::machine::Deploy;
#[cfg(target_os = "linux")]
use crate::Image;
#[cfg(target_os = "linux")]
use crate::SandboxRecord;
use crate::{CommandIdentity, ExecSlot, Sandbox};
use layerfs_home::reply::outcome::refused;
use layerfs_protocol::sdk::SdkRefusal;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

/// The image a host-process Sandbox records and reports, and the `runtime` of a
/// record from the former `local` runtime, which is never used (a record of
/// the retired docker backend has no `runtime`).
pub const LOCAL: &str = "local";
/// A host-process Sandbox record's `runtime` (LAYERFS_SANDBOX=process; the
/// `process` backend stores this word, G6): a local daemon serving
/// loopback NFS on macOS. A runtime's name, not the machine `process`.
pub const NATIVE: &str = "process";
/// Why a process Sandbox does not start on Linux: its host-process pairing is
/// removed (#533 O27), and a Linux host runs local Sandboxes in namespaces.
/// Refused before any effect; another backend is never chosen in its place.
#[cfg(target_os = "linux")]
pub const NATIVE_ON_LINUX: &str =
    "deploy.sandbox (LAYERFS_SANDBOX) process starts host-process Sandboxes on macOS only; \
     on Linux set LAYERFS_SANDBOX=namespaces";
/// A namespace Sandbox record's `runtime` (LAYERFS_SANDBOX=namespaces,
/// Linux): a local daemon in its own mount, PID and IPC namespaces and cgroup.
pub const NAMESPACES: &str = "namespaces";

/// Which local backend a deployment runs inside its Sandbox directory: the
/// plain process one, or the Linux namespace one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    HostProcess,
    Namespaces,
}

/// The backend a deployment selects (#527 D3): `deploy.sandbox`
/// (LAYERFS_SANDBOX) `process` or `namespaces`. Unset or `docker` named the
/// docker backend, retired by D-8 (layerfs-project#111), and is refused.
/// This is the one place the setting is validated; the agent only checks
/// that each value's fields are present. The former refinement
/// `deploy.sandboxBackend` (LAYERFS_SANDBOX_BACKEND) is gone: no deployment
/// carries it since both sides build `deploy` (layerfs-project#108). The
/// namespace backend is Linux only. Every refusal comes before any effect.
pub fn backend(deploy: &Deploy) -> Result<Backend, SdkRefusal> {
    let sandbox = deploy.sandbox.as_deref();
    let selected = match sandbox {
        Some(NATIVE) => Backend::HostProcess,
        Some(NAMESPACES) => Backend::Namespaces,
        None | Some("docker") => {
            return Err(refused(
                "Unsupported",
                "deploy.sandbox (LAYERFS_SANDBOX) docker, or unset, named the docker Sandbox \
                 backend, which is retired (D-8): set it to process (macOS) or namespaces (Linux); \
                 the docker machine's Sandboxes run in namespaces in its worker",
            ))
        }
        Some(other) => {
            return Err(refused(
                "InvalidInput",
                format!(
                    "deploy.sandbox (LAYERFS_SANDBOX) is process (macOS) or namespaces (Linux), \
                     not {other}"
                ),
            ))
        }
    };
    if selected == Backend::Namespaces && !cfg!(target_os = "linux") {
        return Err(refused(
            "Unsupported",
            "deploy.sandbox (LAYERFS_SANDBOX) namespaces is Linux only; use process",
        ));
    }
    Ok(selected)
}

/// The platforms a Sandbox names (#529 P4): what a caller chooses and
/// reads. The record keeps the lower-level `runtime` beside it.
pub const PLATFORMS: [&str; 5] = ["docker", "mac", "linux", "apple-container", "lima"];

/// The platform a `sandbox.create` call names (`platform`), checked against
/// the backend its deployment (`deploy`) selects before any effect:
/// `docker` a namespaces daemon in the docker machine's worker container
/// (D-8, which its deployment names as its `platform`); `mac` a process
/// daemon on macOS; `linux` a namespaces daemon on Linux (a process one is
/// refused at start, #533 O27); a VM provider a namespaces daemon in the
/// project's VM, which is Linux. The agent chooses which platforms a home
/// allows and maps each to its deployment; this refuses a call whose two
/// facts disagree. A call that names none (an older caller) records the
/// one its deployment implies.
pub fn platform(named: Option<&str>, deploy: &Deploy) -> Result<String, SdkRefusal> {
    let backend = backend(deploy)?;
    let docker_machine = deploy.platform.as_deref() == Some("docker");
    let implied = match backend {
        Backend::Namespaces if docker_machine => "docker",
        _ if cfg!(target_os = "macos") => "mac",
        _ => "linux",
    };
    let Some(named) = named else {
        return Ok(implied.to_owned());
    };
    if let Some(reason) = layerfs_machine::table::removed::removed_reason(named) {
        return Err(refused("Unsupported", reason));
    }
    if !PLATFORMS.contains(&named) {
        return Err(refused(
            "InvalidInput",
            format!("platform is one of {}, not {named}", PLATFORMS.join(", ")),
        ));
    }
    let fits = match named {
        "docker" | "mac" | "linux" => named == implied,
        // a VM provider's Sandbox is namespaces inside its Linux VM
        _ => backend == Backend::Namespaces && implied == "linux",
    };
    if !fits {
        return Err(refused(
            "InvalidInput",
            format!(
                "platform {named} does not run the deployment's Sandbox backend ({}) on this host",
                match backend {
                    Backend::HostProcess => NATIVE,
                    Backend::Namespaces => NAMESPACES,
                }
            ),
        ));
    }
    Ok(named.to_owned())
}

/// A running Sandbox's `runtime`, as its record names it.
pub fn kind(runtime: &Sandbox) -> &'static str {
    match runtime {
        #[cfg(target_os = "macos")]
        Sandbox::Process(_) => NATIVE,
        #[cfg(target_os = "linux")]
        Sandbox::Namespaces(_) => NAMESPACES,
    }
}

/// A record's `runtime` as a caller reads it: `process`, `namespaces`,
/// another recorded name as it is, or `docker` for a record with none
/// (only the retired docker backend wrote none).
pub fn runtime_name(record: &crate::record::DaemonRecord) -> &str {
    record.runtime().unwrap_or("docker")
}

/// The image a Sandbox reports: the digest a namespaces Sandbox's root is
/// an overlay of, or `local` for a process one.
pub fn image(runtime: &Sandbox) -> String {
    match runtime.record() {
        #[cfg(target_os = "linux")]
        Some(SandboxRecord::Namespaces(record)) => record.image().as_str().to_owned(),
        _ => LOCAL.to_owned(),
    }
}

/// A namespaces Sandbox's own network, `<host end> <guest address>/31`;
/// none for a process one.
pub fn network(runtime: &Sandbox) -> Option<String> {
    match runtime.record() {
        #[cfg(target_os = "linux")]
        Some(SandboxRecord::Namespaces(record)) => record.network().map(str::to_owned),
        _ => None,
    }
}

/// The image a namespaces deployment's Sandbox root is an overlay of, and
/// the docker-archive that holds it (`deploy.imageArchive`,
/// LAYERFS_IMAGE_ARCHIVE). The image is `deploy.image` when the caller gave
/// one (held by the archive, or already unpacked in this machine's image
/// cache: an image copied from Docker), else the one image the archive names by its config digest (#529;
/// a Docker image id may name the same image by another digest, so the
/// docker machine's worker image is never taken for it, D-8). Both are read before any effect, so a bundle without its
/// image, or a given image the archive does not hold, refuses the start
/// `InvalidInput`.
#[cfg(target_os = "linux")]
pub fn namespaces_image(deploy: &Deploy) -> Result<(Image, PathBuf), SdkRefusal> {
    let archive = deploy.image_archive.as_deref().ok_or_else(|| {
        refused(
            "InvalidInput",
            "deploy.imageArchive is not configured (LAYERFS_IMAGE_ARCHIVE); a namespaces Sandbox \
             needs the docker-archive the bundle carries",
        )
    })?;
    let archive = PathBuf::from(archive);
    if !archive.is_absolute() {
        return Err(refused(
            "InvalidInput",
            "deploy.imageArchive (LAYERFS_IMAGE_ARCHIVE) must be an absolute path",
        ));
    }
    let in_archive = |error| {
        refused(
            "InvalidInput",
            format!("deploy.imageArchive {}: {error}", archive.display()),
        )
    };
    let image = match deploy.image.as_deref() {
        Some(digest) => {
            let image = Image::parse(digest)
                .map_err(|error| refused("InvalidInput", format!("deploy.image: {error}")))?;
            // an image this machine already holds unpacked (one copied from
            // Docker, `machine.load`) needs no archive
            if !crate::image::cache::unpacked(deploy, &image) {
                crate::namespaces::rootfs::holds(&archive, &image).map_err(in_archive)?;
            }
            image
        }
        None => crate::namespaces::rootfs::image_of(&archive).map_err(in_archive)?,
    };
    Ok((image, archive))
}

/// Where its commands keep scratch files: the Sandbox's own `tmp`
/// directory (deleted with it), so a Sandbox leaves nothing in this
/// machine's `/tmp`.
pub fn scratch(runtime: &Sandbox) -> String {
    runtime.scratch_directory()
}

/// One command to its end: exit code, stdout, stderr. A stream past the
/// inline window spills to its file at `place` (`crate::exec::output`); `stdin`
/// is given to the command whole. `slot` names the command for its stop
/// handle and is told it at spawn.
pub fn run(
    (sandbox, runtime): (&str, &Sandbox),
    identity: CommandIdentity,
    (arguments, environment, stdin): (Vec<String>, Vec<String>, Option<Vec<u8>>),
    (directory, place): (&str, Option<&Place>),
    slot: ExecSlot<'_>,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let (mut stdout, mut stderr) = (Spool::at(place, "stdout"), Spool::at(place, "stderr"));
    let request = crate::ExecRequest {
        identity,
        arguments,
        environment,
        directory: directory.to_owned(),
        stdin,
    };
    let exit = runtime
        .run_in(&request, slot, &mut stdout, &mut stderr)
        .map_err(|failure| no_helper(sandbox, failure))?;
    Ok((exit.code, stdout, stderr))
}

/// A command's failure `failure` in Sandbox `sandbox`; one refused because
/// its namespaces Sandbox has no helper copy (an earlier build made it)
/// names the records-only `sandbox remove` as its fix (RV3 NOTE 14).
fn no_helper(sandbox: &str, failure: Box<crate::WireFailure>) -> SdkRefusal {
    #[cfg(target_os = "linux")]
    if matches!(
        failure.cause,
        crate::RuntimeError::Unsupported(why)
            if why == crate::namespaces::NO_HELPER_COPY
    ) {
        let refusal = refused(
            "Unsupported",
            format!("Sandbox {sandbox}: {}", failure.cause),
        );
        return layerfs_home::reply::outcome::with_fix(
            refusal,
            crate::status::state::remove_fix(sandbox),
        );
    }
    let _ = sandbox;
    crate::route::daemon::refusal::wire(failure)
}
