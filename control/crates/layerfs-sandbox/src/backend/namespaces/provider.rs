//! The Linux namespace runtime as a concrete backend: the same local runtime
//! root and daemon control as the process one, with each Sandbox's daemon in
//! its own mount, PID and IPC namespaces and its own cgroup, and its commands
//! entering them as a nonroot identity. The VM that holds this kernel is the
//! boundary; the namespaces keep Sandboxes on one Store out of each other.
use super::{
    instance::{
        cgroup,
        create::{self, CreateFailure, Options},
        handle::Sandbox,
        paths::Paths,
    },
    linux::host_checks,
    rootfs,
};
use crate::spawn::{Local, LocalPaths};
use crate::{Bind, ContainerId, RuntimeError, WireFailure};
use crate::{LaunchFailure, LaunchPhase, LaunchRequest, ProviderLaunchOptions, SandboxRecord};
use std::{
    env,
    path::{Path, PathBuf},
};

/// The helper executable, installed beside the daemon it starts.
const HELPER: &str = "layerfs-sandbox-helper";
/// The bundle's runtime layer, a directory beside the daemon.
const RUNTIME: &str = "runtime";
/// Names the helper explicitly, when it is not beside the daemon.
const HELPER_ENVIRONMENT: &str = "LAYERFS_SANDBOX_HELPER";

/// A Linux namespace runtime over one local root directory.
#[derive(Clone, Debug)]
pub struct NamespaceProvider {
    local: Local,
}

impl NamespaceProvider {
    /// Selects `root`, the parent of every Sandbox directory, and refuses a
    /// host that cannot hold namespace Sandboxes before any effect: not root,
    /// user namespaces not blocked, no cgroup v2, or no `/dev/fuse`.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let local = Local::new(&root)?;
        host_checks::require_root()?;
        host_checks::require_user_namespaces_blocked()?;
        host_checks::require_cgroup_v2()?;
        host_checks::require_fuse()?;
        host_checks::require_overlay_upper(root.as_ref())?;
        Ok(Self { local })
    }

    /// The named Sandbox's paths, for its daemon setup.
    pub fn paths(&self, name: &str) -> Result<LocalPaths, RuntimeError> {
        self.local.clone().naming(name)?.paths()
    }

    /// Starts one Sandbox once with exactly `binds` in its private root
    /// (#535 §6.3): a sqlite Store's [`Bind::store`], or none for a
    /// PostgreSQL Store. A failed start keeps the acknowledged owner.
    pub fn launch_with_binds(
        &self,
        request: LaunchRequest,
        binds: Vec<Bind>,
    ) -> Result<Sandbox, Box<LaunchFailure>> {
        let fail = refused;
        let ProviderLaunchOptions::Namespaces {
            command_identity,
            environment,
            image,
            archive,
            closed_ports,
        } = request.options.clone()
        else {
            return Err(fail(RuntimeError::Unsupported(
                "namespaces provider requires namespaces launch options",
            )));
        };
        let helper = helper(Some(&request.daemon_executable)).map_err(fail)?;
        self.local.clone().naming(&request.name).map_err(fail)?;
        let options = Options {
            command_identity,
            environment,
            binds,
            image,
            archive,
            runtime: runtime(&request.daemon_executable),
            closed_ports,
        };
        create::create(&self.local, &helper, &request, &options).map_err(|failure| {
            let CreateFailure {
                phase,
                sandbox,
                cause,
            } = *failure;
            Box::new(LaunchFailure {
                phase: match phase {
                    crate::spawn::LocalPhase::Validate => LaunchPhase::Validate,
                    crate::spawn::LocalPhase::Create => LaunchPhase::Create,
                    crate::spawn::LocalPhase::Start => LaunchPhase::Start,
                    crate::spawn::LocalPhase::Listener => LaunchPhase::Listener,
                },
                instance: sandbox
                    .map(|sandbox| Box::new(crate::Sandbox::Namespaces(Box::new(sandbox)))),
                cause,
            })
        })
    }

    /// Reopen starts nothing: it checks the recorded init process, its cgroup,
    /// that it holds a network namespace of its own, and its listener.
    pub fn reopen(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        let record = record.namespaces().ok_or_else(|| {
            WireFailure::local(RuntimeError::Unsupported(
                "namespaces provider requires a namespaces record",
            ))
        })?;
        let paths = self
            .record_paths(&record.local().name)
            .map_err(WireFailure::local)?;
        let image = self.image(record).map_err(WireFailure::local)?;
        // the copy its create put in its own `bin` directory, so a later
        // process runs the helper it was started with; a Sandbox without
        // one (an earlier build's) is refused when a command needs it
        let helper = paths.helper.clone();
        let recorded = super::instance::handle::Recorded {
            init: super::instance::handle::process(record.local()),
            port: record.local().port,
            labels: record.local().labels.clone(),
            image,
            network: record.network.clone(),
        };
        Sandbox::reopen(paths, cgroup::path(&id), id, helper, recorded)
    }

    /// A known stopped record admits only the existing runtime's single Delete.
    pub fn reopen_stopped(
        &self,
        id: ContainerId,
        record: &SandboxRecord,
    ) -> Result<Sandbox, Box<WireFailure>> {
        let record = record.namespaces().ok_or_else(|| {
            WireFailure::local(RuntimeError::Unsupported(
                "namespaces provider requires a namespaces record",
            ))
        })?;
        let paths = self
            .record_paths(&record.local().name)
            .map_err(WireFailure::local)?;
        let image = self.image(record).map_err(WireFailure::local)?;
        // the copy its create put in its own `bin` directory, so a later
        // process runs the helper it was started with; a Sandbox without
        // one (an earlier build's) is refused when a command needs it
        let helper = paths.helper.clone();
        let recorded = super::instance::handle::Recorded {
            init: super::instance::handle::process(record.local()),
            port: record.local().port,
            labels: record.local().labels.clone(),
            image,
            network: record.network.clone(),
        };
        Ok(Sandbox::stopped(
            paths,
            cgroup::path(&id),
            id,
            helper,
            recorded,
        ))
    }

    fn record_paths(&self, name: &str) -> Result<Paths, RuntimeError> {
        Paths::of(self.paths(name)?)
    }

    /// The record's image, as the root this runtime already holds: the lower
    /// layer the Sandbox's spec names, which its create unpacked. A reopen
    /// starts and unpacks nothing, so a record whose image is gone is refused
    /// rather than given a different root.
    fn image(&self, record: &crate::NamespaceRecord) -> Result<rootfs::ImageRoot, RuntimeError> {
        let paths = self.record_paths(&record.local().name)?;
        let root = (paths.local.directory.parent())
            .ok_or(RuntimeError::Protocol("namespaces Sandbox root"))?;
        let spec = std::fs::read(&paths.spec).map_err(|error| {
            rootfs::refuse(format!(
                "the Sandbox's spec {} cannot be read ({error}): its root cannot be found, so it \
                 is neither reopened nor deleted",
                paths.spec.display()
            ))
        })?;
        let lower = crate::HelperSpec::parse(&spec)?.lower;
        rootfs::lookup(record.image(), &rootfs::cache(root), &lower)
    }
}

/// The bundle's runtime layer (R5): `runtime/` beside the daemon, as
/// `tools/linux_bundle.py` seals it, when it holds this architecture's
/// loader. A bundle without one starts its daemon on the image's own
/// loader and libraries, as before.
fn runtime(daemon: &Path) -> Option<PathBuf> {
    let runtime = daemon.parent()?.join(RUNTIME);
    layerfs_os::runtime::holds(&runtime).then_some(runtime)
}

/// A launch refused before any effect.
fn refused(cause: RuntimeError) -> Box<LaunchFailure> {
    Box::new(LaunchFailure {
        phase: LaunchPhase::Validate,
        instance: None,
        cause,
    })
}

/// The helper this backend runs: `LAYERFS_SANDBOX_HELPER` when set, else
/// beside `daemon` or beside this process (or its Cargo test runner).
fn helper(daemon: Option<&Path>) -> Result<PathBuf, RuntimeError> {
    if let Some(path) = env::var_os(HELPER_ENVIRONMENT) {
        let path = PathBuf::from(path);
        if let Ok(metadata) = std::fs::metadata(&path) {
            if path.is_absolute() && metadata.is_file() && metadata.len() > 0 {
                return Ok(path);
            }
        }
    }
    let mut candidates = Vec::new();
    if let Some(parent) = daemon.and_then(Path::parent) {
        candidates.push(parent.join(HELPER));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join(HELPER));
            if let Some(grandparent) = parent.parent() {
                candidates.push(grandparent.join(HELPER));
            }
        }
    }
    for candidate in candidates {
        if let Ok(metadata) = std::fs::metadata(&candidate) {
            if candidate.is_absolute() && metadata.is_file() && metadata.len() > 0 {
                return Ok(candidate);
            }
        }
    }
    Err(RuntimeError::Unsupported(
        "the namespaces Sandbox runtime needs the layerfs-sandbox-helper executable beside the          daemon (or LAYERFS_SANDBOX_HELPER set)",
    ))
}
