//! One namespace Sandbox start: its private directory (setup, executable,
//! overlay, mounts), its cgroup and one spawn of the init stage, then the
//! wait for its daemon's listener. Nothing is retried, and a failure returns
//! what it made so the caller can end and delete it once.
use super::{
    cgroup, host,
    paths::Paths,
    rootfs::{self, ImageRoot},
    sandbox::Sandbox,
};
use crate::local::listener;
use crate::local::{search::search_path, Local, LocalPhase};
use crate::HelperSpec;
use crate::{Bind, CommandIdentity, Image, RuntimeError};
use layerfs_bridge::process::LocalProcess;
use nix::fcntl::OFlag;
use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{self, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

/// What one namespaces launch was asked for, after validation.
#[derive(Clone, Debug)]
pub(crate) struct Options {
    pub(crate) command_identity: CommandIdentity,
    pub(crate) environment: Vec<(String, String)>,
    /// The host directories bound into the Sandbox's private root (a
    /// sqlite Store's directory; none for a PostgreSQL Store).
    pub(crate) binds: Vec<Bind>,
    /// The image digest the Sandbox's root is an overlay of, and the
    /// docker-archive in the bundle that holds it.
    pub(crate) image: Image,
    pub(crate) archive: PathBuf,
}

/// A failed start. `sandbox` holds what it made (its directory, cgroup and,
/// once spawned, its init stage) for the caller to end and delete once.
#[derive(Debug)]
pub(crate) struct CreateFailure {
    pub(crate) phase: LocalPhase,
    pub(crate) sandbox: Option<Sandbox>,
    pub(crate) cause: RuntimeError,
}

/// The init stage of `request`'s Sandbox, started once over `local`'s root.
pub(crate) fn create(
    local: &Local,
    helper: &Path,
    request: &crate::LaunchRequest,
    options: &Options,
) -> Result<Sandbox, Box<CreateFailure>> {
    let fail = |phase, sandbox, cause| {
        Box::new(CreateFailure {
            phase,
            sandbox,
            cause,
        })
    };
    // the image's lock stays shared until the spec names it (`hold`), so a
    // prune never removes the image between its unpack and this Sandbox
    let (paths, image, hold) = start_request(local, helper, request, options)
        .map_err(|cause| fail(LocalPhase::Validate, None, cause))?;
    let id =
        crate::ContainerId::random().map_err(|cause| fail(LocalPhase::Validate, None, cause))?;
    let cgroup = cgroup::path(&id);
    // one create of the directory: a name another start holds fails here
    fs::DirBuilder::new()
        .mode(0o755)
        .create(&paths.local.directory)
        .map_err(|error| fail(LocalPhase::Create, None, error.into()))?;
    let mut sandbox = Sandbox::unstarted(
        id,
        paths.clone(),
        cgroup.clone(),
        request.labels.clone(),
        helper.to_owned(),
        image.clone(),
    );
    let log = match prepare(&paths, (request, helper), options, &id, &image) {
        Ok(log) => log,
        Err(cause) => return Err(fail(LocalPhase::Create, Some(sandbox), cause)),
    };
    drop(hold);
    match cgroup::create(&id) {
        Ok(_) => {}
        Err(error) => return Err(fail(LocalPhase::Create, Some(sandbox), error.into())),
    }
    let spawned = log.try_clone().and_then(|output| {
        Command::new(helper)
            .arg("init")
            .arg("--spec")
            .arg(&paths.spec)
            .env_clear()
            .env("PATH", search_path())
            .envs(options.environment.iter().map(|(k, v)| (k, v)))
            .current_dir(&paths.local.directory)
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(log)
            .process_group(0)
            .spawn()
    });
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return Err(fail(LocalPhase::Start, Some(sandbox), error.into())),
    };
    match LocalProcess::observe(child.id()) {
        Ok(init) => sandbox.init = Some(init),
        Err(error) => {
            // spawned but unobservable: end it here, it has no identity to record
            let _ = child.kill();
            let _ = child.wait();
            return Err(fail(LocalPhase::Start, Some(sandbox), error.into()));
        }
    }
    *sandbox
        .child
        .get_mut()
        .unwrap_or_else(|error| error.into_inner()) = Some(child);
    match listener::await_listener(&paths.local.log, request.startup_timeout, &|| {
        sandbox.reap();
        sandbox.init.is_some_and(LocalProcess::alive)
    }) {
        Ok(port) => {
            sandbox.port = port;
            Ok(sandbox)
        }
        Err(cause) => Err(fail(LocalPhase::Listener, Some(sandbox), cause)),
    }
}

/// Everything a start refuses before it makes anything: the request's own
/// fields, this host's runtime prerequisites, each bind against the private
/// root's (none for a PostgreSQL Store), the helper, and the image the
/// bundle carries. Getting
/// the image's root may unpack it, once per digest for the whole runtime root.
fn start_request(
    local: &Local,
    helper: &Path,
    request: &crate::LaunchRequest,
    options: &Options,
) -> Result<(Paths, ImageRoot, rootfs::ImageHold), RuntimeError> {
    let paths = {
        let named = local.clone().naming(&request.name)?;
        Paths::of(named.paths()?)?
    };
    host::require_root()?;
    host::require_user_namespaces_blocked()?;
    host::require_cgroup_v2()?;
    host::require_fuse()?;
    let root = (paths.local.directory.parent())
        .ok_or(RuntimeError::Protocol("namespaces Sandbox root"))?;
    host::require_overlay_upper(root)?;
    options.command_identity.check()?;
    let executable = &request.daemon_executable;
    let metadata = fs::metadata(executable)?;
    if !executable.is_absolute() || !metadata.is_file() || metadata.len() == 0 {
        return Err(RuntimeError::Protocol("namespaces daemon executable"));
    }
    let helper_metadata = fs::metadata(helper)?;
    if !helper.is_absolute() || !helper_metadata.is_file() || helper_metadata.len() == 0 {
        return Err(RuntimeError::Protocol("namespaces helper executable"));
    }
    if request.daemon_config.is_empty()
        || request.daemon_config.len() > 8192
        || request.startup_timeout.is_zero()
    {
        return Err(RuntimeError::Protocol(
            "namespaces daemon setup/startup wait",
        ));
    }
    check_environment(&options.environment)?;
    for bind in &options.binds {
        check_bind(&paths, bind)?;
    }
    for path in [&paths.local.directory, &paths.root] {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => return Err(RuntimeError::Protocol("namespaces Sandbox path exists")),
        }
    }
    let root = (paths.local.directory.parent())
        .ok_or(RuntimeError::Protocol("namespaces Sandbox root"))?;
    let (image, hold) =
        rootfs::ensure_held(&options.archive, &options.image, &rootfs::cache(root))?;
    Ok((paths, image, hold))
}

/// The daemon starts with the search path, this backend's kernel route
/// (FUSE, which only the provider names) and nothing else but these: no
/// `PATH` or kernel route override.
fn check_environment(environment: &[(String, String)]) -> Result<(), RuntimeError> {
    let invalid = environment.iter().any(|(key, value)| {
        key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') || key == "PATH"
    });
    if invalid {
        return Err(RuntimeError::Protocol("namespaces environment"));
    }
    Ok(())
}

/// A bind's host directory (a sqlite Store's, bound into every Sandbox of
/// it) must not hold the runtime root, which would expose every Sandbox's
/// directory to all of them, and must be a real directory that is not the
/// filesystem root. Its guest path must not be `/` or hold the Sandbox's
/// own directory, which the private root binds first.
fn check_bind(paths: &Paths, bind: &Bind) -> Result<(), RuntimeError> {
    let plain = |path: &Path| {
        path.is_absolute()
            && path != Path::new("/")
            && !(path.components()).any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    };
    let metadata = fs::metadata(&bind.host)?;
    let root = paths
        .local
        .directory
        .parent()
        .ok_or(RuntimeError::Protocol("namespaces Sandbox root"))?;
    if !plain(&bind.host)
        || !plain(&bind.guest)
        || !metadata.is_dir()
        || root.starts_with(&bind.host)
        || paths.local.directory.starts_with(&bind.host)
        || paths.root.starts_with(&bind.host)
        || paths.local.directory.starts_with(&bind.guest)
    {
        return Err(RuntimeError::Protocol(
            "namespaces bind directory (must not hold the runtime root)",
        ));
    }
    Ok(())
}

/// The Sandbox's own directories and files, its copied daemon and helper, its
/// init spec and the empty log; returns the log file. Its overlay's writable layer
/// and work directory are made here, empty: the init stage mounts them with
/// the image underneath.
fn prepare(
    paths: &Paths,
    (request, helper): (&crate::LaunchRequest, &Path),
    options: &Options,
    id: &crate::ContainerId,
    image: &ImageRoot,
) -> Result<File, RuntimeError> {
    fs::set_permissions(&paths.local.directory, Permissions::from_mode(0o755))?;
    let private = |path: &Path| {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
            .open(path)
    };
    for file in [&paths.local.config, &paths.local.overlay] {
        let parent = file
            .parent()
            .ok_or(RuntimeError::Protocol("namespaces path"))?;
        fs::DirBuilder::new().mode(0o700).create(parent)?;
    }
    for directory in [&paths.bin_directory, &paths.local.mounts] {
        fs::DirBuilder::new().mode(0o755).create(directory)?;
    }
    fs::DirBuilder::new().mode(0o700).create(&paths.local.tmp)?;
    fs::set_permissions(&paths.local.tmp, Permissions::from_mode(0o1777))?;
    fs::DirBuilder::new().mode(0o755).create(&paths.root)?;
    for directory in [&paths.upper, &paths.work] {
        fs::DirBuilder::new().mode(0o700).create(directory)?;
    }
    private(&paths.local.config)?.write_all(&request.daemon_config)?;
    fs::copy(&request.daemon_executable, &paths.bin)?;
    fs::set_permissions(&paths.bin, Permissions::from_mode(0o755))?;
    fs::copy(helper, &paths.helper)?;
    fs::set_permissions(&paths.helper, Permissions::from_mode(0o755))?;
    let spec = HelperSpec {
        root: paths.root.clone(),
        sandbox: paths.local.directory.clone(),
        daemon: paths.bin.clone(),
        config: paths.local.config.clone(),
        cgroup: cgroup::path(id),
        lower: image.directory().to_owned(),
        upper: paths.upper.clone(),
        work: paths.work.clone(),
        binds: options.binds.clone(),
    };
    private(&paths.spec)?.write_all(&spec.encode()?)?;
    Ok(private(&paths.local.log)?)
}
