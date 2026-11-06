//! A Sandbox's runtime: Docker (default), `LAYERFS_SANDBOX=native` on macOS
//! (loopback NFS, caller identity), or `LAYERFS_SANDBOX=namespaces` on Linux
//! (root FUSE daemon, nonroot commands). Linux starts no native Sandbox
//! (#533 O27); a native record written there before 2.0 is still reopened
//! by its record. All use the same authenticated daemon control pipeline
//! and the same concrete `layerfs_sandbox::Sandbox` handle. A Sandbox also
//! names its platform (#529), recorded beside the runtime; every later call
//! follows the record, not the caller's deployment.
use crate::common::outcome::{failed, refused};
use crate::machine::worker::RUNNING;
use crate::sandbox::output::Spool;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
#[cfg(target_os = "linux")]
use layerfs_sandbox::Image;
use layerfs_sandbox::SandboxRecord;
use layerfs_sandbox::{CommandIdentity, Sandbox};
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The image a native Sandbox records and reports, and the `runtime` of a
/// record from the former `local` runtime, which is never used (a Docker
/// record has no `runtime`).
pub(crate) const LOCAL: &str = "local";
/// A native Sandbox record's `runtime` (LAYERFS_SANDBOX=native): a local
/// daemon serving loopback NFS on macOS, or FUSE on Linux before 2.0.
pub(crate) const NATIVE: &str = "native";
/// Why a native Sandbox does not start on Linux: its host-process pairing is
/// removed (#533 O27), and a Linux host runs local Sandboxes in namespaces.
/// Refused before any effect; another backend is never chosen in its place.
#[cfg(target_os = "linux")]
pub(crate) const NATIVE_ON_LINUX: &str =
    "deploy.sandbox (LAYERFS_SANDBOX) native starts host-process Sandboxes on macOS only; \
     on Linux set LAYERFS_SANDBOX=namespaces";
/// A namespace Sandbox record's `runtime` (LAYERFS_SANDBOX=namespaces,
/// Linux): a local daemon in its own mount, PID and IPC namespaces and cgroup.
pub(crate) const NAMESPACES: &str = "namespaces";

/// Which local backend a deployment runs inside its Sandbox directory: the
/// plain native one, or the Linux namespace one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Backend {
    HostProcess,
    Namespaces,
}

/// The backend a deployment selects (#527 D3): `deploy.sandbox`
/// (LAYERFS_SANDBOX) unset or `docker` is Docker (`None`), `native` and
/// `namespaces` the local backends. This is the one place the setting is
/// validated; the agent only checks that each value's fields are present.
///
/// The former pair keeps working for one release: `native` with
/// `deploy.sandboxBackend` (LAYERFS_SANDBOX_BACKEND) `namespaces` is
/// namespaces, and with it unset or `native` is native. Any other
/// combination with that setting set (`namespaces` with `native`, or `docker`
/// with either) is refused, naming both settings. The namespace backend is
/// Linux only. Every refusal comes before any effect.
pub(crate) fn backend(deploy: &SdkValue) -> Result<Option<Backend>, SdkRefusal> {
    let sandbox = deploy.get_text("sandbox");
    let selected = match sandbox {
        None | Some("docker") => None,
        Some(NATIVE) => Some(Backend::HostProcess),
        Some(NAMESPACES) => Some(Backend::Namespaces),
        Some(other) => {
            return Err(refused(
                "InvalidInput",
                format!(
                    "deploy.sandbox (LAYERFS_SANDBOX) is docker, native (Linux or macOS) or \
                     namespaces (Linux), not {other}"
                ),
            ))
        }
    };
    let selected = match (selected, deploy.get_text("sandboxBackend")) {
        (selected, None) => selected,
        (Some(Backend::HostProcess), Some(NATIVE)) => Some(Backend::HostProcess),
        (Some(Backend::HostProcess | Backend::Namespaces), Some(NAMESPACES)) => {
            Some(Backend::Namespaces)
        }
        (_, Some(old)) if old != NATIVE && old != NAMESPACES => {
            return Err(refused(
                "InvalidInput",
                format!(
                    "deploy.sandboxBackend (LAYERFS_SANDBOX_BACKEND) is native or namespaces, \
                     not {old}; it is replaced by LAYERFS_SANDBOX=native or namespaces"
                ),
            ))
        }
        (_, Some(old)) => {
            return Err(refused(
                "InvalidInput",
                format!(
                    "deploy.sandbox (LAYERFS_SANDBOX) {} conflicts with deploy.sandboxBackend \
                     (LAYERFS_SANDBOX_BACKEND) {old}: LAYERFS_SANDBOX_BACKEND, kept for one \
                     release, only refines LAYERFS_SANDBOX=native. Set LAYERFS_SANDBOX to docker, \
                     native or namespaces and leave LAYERFS_SANDBOX_BACKEND unset",
                    sandbox.unwrap_or("unset (docker)")
                ),
            ))
        }
    };
    if selected == Some(Backend::Namespaces) && !cfg!(target_os = "linux") {
        return Err(refused(
            "Unsupported",
            "deploy.sandbox (LAYERFS_SANDBOX) namespaces is Linux only; use native",
        ));
    }
    Ok(selected)
}

/// The platforms a Sandbox names (#529 P4): what a caller chooses and
/// reads. The record keeps the lower-level `runtime` beside it.
pub(crate) const PLATFORMS: [&str; 8] = [
    "docker",
    "mac",
    "linux",
    "apple-container",
    "microsandbox",
    "lima",
    "firecracker",
    "ssh",
];

/// The platform a `sandbox.create` call names (`platform`), checked against
/// the backend its deployment selects before any effect: `docker` is a
/// Docker container; `mac` a native daemon on macOS; `linux` a namespaces
/// daemon on Linux (a native one is refused at start, #533 O27); a VM
/// provider a namespaces daemon in the project's VM, which is Linux. The agent chooses which platforms a home
/// allows and maps each to its deployment; this refuses a call whose two
/// facts disagree. A call that names none (an older caller) records the
/// one its backend implies.
pub(crate) fn platform(args: &SdkValue, backend: Option<Backend>) -> Result<String, SdkRefusal> {
    let implied = match backend {
        None => "docker",
        Some(_) if cfg!(target_os = "macos") => "mac",
        Some(_) => "linux",
    };
    let Some(named) = args.get_text("platform") else {
        return Ok(implied.to_owned());
    };
    if !PLATFORMS.contains(&named) {
        return Err(refused(
            "InvalidInput",
            format!("platform is one of {}, not {named}", PLATFORMS.join(", ")),
        ));
    }
    let fits = match named {
        "docker" | "mac" | "linux" => named == implied,
        // a VM provider's Sandbox is namespaces inside its Linux VM
        _ => backend == Some(Backend::Namespaces) && implied == "linux",
    };
    if !fits {
        return Err(refused(
            "InvalidInput",
            format!(
                "platform {named} does not run the deployment's Sandbox backend ({}) on this host",
                backend.map_or("docker", |b| match b {
                    Backend::HostProcess => NATIVE,
                    Backend::Namespaces => NAMESPACES,
                })
            ),
        ));
    }
    Ok(named.to_owned())
}

/// A running Sandbox's `runtime`, as its record names it.
pub(crate) fn kind(runtime: &Sandbox) -> &'static str {
    match runtime {
        Sandbox::Docker(_) => "docker",
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Sandbox::HostProcess(_) => NATIVE,
        #[cfg(target_os = "linux")]
        Sandbox::Namespaces(_) => NAMESPACES,
    }
}

/// A record's `runtime` as a caller reads it: `docker` (a Docker record has
/// none), `native`, `namespaces`, or another recorded name as it is.
pub(crate) fn runtime_name(record: &SdkValue) -> &str {
    record.get_text("runtime").unwrap_or("docker")
}

/// The image a Sandbox reports: its container's, the digest a namespaces
/// Sandbox's root is an overlay of, or `local` for a native one.
pub(crate) fn image(runtime: &Sandbox) -> String {
    match runtime.record() {
        Some(SandboxRecord::Docker(docker)) => docker.image,
        #[cfg(target_os = "linux")]
        Some(SandboxRecord::Namespaces(record)) => record.image().as_str().to_owned(),
        _ => LOCAL.to_owned(),
    }
}

/// The image a namespaces deployment's Sandbox root is an overlay of, and
/// the docker-archive that holds it (`deploy.imageArchive`,
/// LAYERFS_IMAGE_ARCHIVE). The image is `deploy.image` when the caller gave
/// one, else the one image the archive names by its config digest (#529):
/// `docker` keeps LAYERFS_IMAGE, which Docker's image store may name by
/// another digest. Both are read before any effect, so a bundle without its
/// image, or a given image the archive does not hold, refuses the start
/// `InvalidInput`.
#[cfg(target_os = "linux")]
pub(crate) fn namespaces_image(deploy: &SdkValue) -> Result<(Image, PathBuf), SdkRefusal> {
    let archive = deploy.get_text("imageArchive").ok_or_else(|| {
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
    let image = match deploy.get_text("image") {
        Some(digest) => {
            let image = Image::parse(digest)
                .map_err(|error| refused("InvalidInput", format!("deploy.image: {error}")))?;
            layerfs_sandbox::namespaces::rootfs::holds(&archive, &image).map_err(in_archive)?;
            image
        }
        None => layerfs_sandbox::namespaces::rootfs::image_of(&archive).map_err(in_archive)?,
    };
    Ok((image, archive))
}

/// Where its commands keep scratch files: `/tmp` in a container, the
/// Sandbox's own `tmp` directory for a native one (deleted with it), so a
/// native Sandbox leaves nothing in this machine's `/tmp`.
pub(crate) fn scratch(runtime: &Sandbox) -> String {
    runtime.scratch_directory()
}

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline.
pub(crate) fn run(
    runtime: &Sandbox,
    identity: CommandIdentity,
    arguments: Vec<String>,
    environment: Vec<String>,
    directory: &str,
    spool: bool,
    progress: Option<&mut (dyn FnMut(SdkValue) + Send)>,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let progress = Mutex::new(progress);
    let mut stdout = Reporting::new(Spool::new(spool), &progress, "stdout");
    let mut stderr = Reporting::new(Spool::new(spool), &progress, "stderr");
    let request = layerfs_sandbox::ExecRequest {
        identity,
        arguments,
        environment,
        directory: directory.to_owned(),
        stdin: false,
    };
    // a command whose process is alive is progressing, even when it writes
    // nothing: a `sleep` or a quiet build keeps its caller's wait fresh. It
    // is the worker's own observation, so it refreshes the wait and is not
    // forwarded as an operation event; it stops with the process.
    let mut running = |pid: Option<u32>| {
        let Ok(mut slot) = progress.lock() else {
            return;
        };
        let Some(report) = slot.as_mut() else {
            return;
        };
        let mut fields = vec![
            ("phase".to_owned(), SdkValue::text("running")),
            ("worker".to_owned(), SdkValue::Bool(true)),
        ];
        if let Some(pid) = pid {
            fields.push(("pid".to_owned(), SdkValue::Int(i64::from(pid))));
        }
        report(SdkValue::map([("progress", SdkValue::Map(fields))]));
    };
    let reporting = progress.lock().is_ok_and(|slot| slot.is_some());
    let liveness = reporting.then_some(layerfs_sandbox::Liveness {
        every: RUNNING,
        report: &mut running,
    });
    let exit = runtime
        .run_reporting(&request, &mut stdout, &mut stderr, liveness)
        .map_err(failed)?;
    Ok((exit.code, stdout.into_spool(), stderr.into_spool()))
}

/// How often a command's output reports progress. The first bytes report at
/// once; later ones at most this often, so a chatty command does not flood
/// the caller's stream, and a quiet one still gets its bytes to the spool.
const REPORT: Duration = Duration::from_millis(500);

/// One command stream that keeps all its output and tells the caller how
/// much has arrived. The sink reaches the runtime's copy threads, so it is
/// `Send`; the one progress callback behind [`Mutex`] is shared by the two
/// streams and called from whichever stream is being copied.
struct Reporting<'a, 'b> {
    spool: Spool,
    progress: &'a Mutex<Option<&'b mut (dyn FnMut(SdkValue) + Send)>>,
    stream: &'static str,
    written: u64,
    reported: u64,
    last: Instant,
}

impl<'a, 'b> Reporting<'a, 'b> {
    fn new(
        spool: Spool,
        progress: &'a Mutex<Option<&'b mut (dyn FnMut(SdkValue) + Send)>>,
        stream: &'static str,
    ) -> Self {
        Self {
            spool,
            progress,
            stream,
            written: 0,
            reported: 0,
            last: Instant::now(),
        }
    }

    fn into_spool(self) -> Spool {
        self.spool
    }

    /// Reports the bytes written since the last report, at most [`REPORT`]
    /// apart, as an operation progress value: `{ phase: "output", stream,
    /// bytes }`. An operation that produces output is progressing even when
    /// its command is long.
    fn report(&mut self) {
        if self.written == self.reported {
            return;
        }
        if self.reported != 0 && self.last.elapsed() < REPORT {
            return;
        }
        self.last = Instant::now();
        self.reported = self.written;
        let Ok(mut slot) = self.progress.lock() else {
            return;
        };
        if let Some(progress) = slot.as_mut() {
            progress(SdkValue::map([(
                "progress",
                SdkValue::map([
                    ("phase", SdkValue::text("output")),
                    ("stream", SdkValue::text(self.stream)),
                    (
                        "bytes",
                        SdkValue::Int(i64::try_from(self.written).unwrap_or(i64::MAX)),
                    ),
                ]),
            )]));
        }
    }
}

impl Write for Reporting<'_, '_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let written = self.spool.write(data)?;
        self.written += written as u64;
        self.report();
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.spool.flush()
    }
}
