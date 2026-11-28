//! One namespace Sandbox start: its private directory (setup, executable,
//! overlay, mounts), its cgroup, the machine's firewall table and one spawn
//! of the init stage, then the wait for its daemon's listener. Before any
//! effect it checks what the Sandbox's own network needs of the machine
//! (forwarding on, no forward chain that drops it, a free pool, a resolver
//! the Sandbox reaches). Nothing is retried, and a failure returns what it
//! made so the caller can end and delete it once.
use super::{
    super::{
        linux::host_checks,
        rootfs::{self, ImageRoot},
    },
    cgroup,
    handle::Sandbox,
    paths::Paths,
};
use crate::network::{self, pool::Ipv4Net, resolver::Resolver};
use crate::spawn::listener;
use crate::spawn::{search_path::search_path, Local, LocalPhase};
use crate::{Bind, CommandIdentity, Image, RuntimeError};
use crate::{HelperSpec, SpecNetwork};
use layerfs_os::process::LocalProcess;
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
    /// The bundle's runtime layer beside the daemon (R5), when it has one:
    /// copied into the Sandbox and named in its spec.
    pub(crate) runtime: Option<PathBuf>,
    /// The launcher's own listening ports that are not on loopback (a
    /// worker on `0.0.0.0`), closed to the Sandbox's commands by the
    /// firewall table; none for a loopback worker or `process`.
    pub(crate) closed_ports: Vec<u16>,
}

/// What a start checked of the machine's network, before any effect.
struct NetworkPlan {
    pool: Ipv4Net,
    resolver: Resolver,
}

/// The Sandbox directory's resolver file for its commands, written when
/// the launcher's names a loopback or IPv6 nameserver.
const RESOLVER_FILE: &str = "resolv.conf";
/// The file the init stage writes the Sandbox's network to.
const NETWORK_FILE: &str = "network";

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
    let (paths, image, hold, plan) = start_request(local, helper, request, options)
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
    let log = match prepare(&paths, (request, helper), options, (&id, &plan), &image) {
        Ok(log) => log,
        Err(cause) => return Err(fail(LocalPhase::Create, Some(sandbox), cause)),
    };
    drop(hold);
    match cgroup::create(&id) {
        Ok(_) => {}
        Err(error) => return Err(fail(LocalPhase::Create, Some(sandbox), error.into())),
    }
    // the table carries the current pool and closed ports: written again,
    // whole, at every start (and at every worker start)
    if let Err(cause) = network::table(plan.pool, &options.closed_ports) {
        return Err(fail(LocalPhase::Create, Some(sandbox), cause));
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
            // written by the init stage before the daemon started
            let line = fs::read_to_string(paths.local.directory.join(NETWORK_FILE));
            match line.map(|line| line.trim().to_owned()) {
                Ok(line) if !line.is_empty() => sandbox.network = Some(line),
                Ok(_) => {
                    let cause = RuntimeError::Protocol("the Sandbox's network file is empty");
                    return Err(fail(LocalPhase::Listener, Some(sandbox), cause));
                }
                Err(error) => return Err(fail(LocalPhase::Listener, Some(sandbox), error.into())),
            }
            Ok(sandbox)
        }
        Err(cause) => {
            // an init stage that found the machine changed since
            // `network_plan` said so in its log: `Unavailable`, as there
            let cause = unavailable(&paths.local.log).map_or(cause, RuntimeError::Unavailable);
            Err(fail(LocalPhase::Listener, Some(sandbox), cause))
        }
    }
}

/// The reason the init stage's log gives on its `UNAVAILABLE` line, read
/// once, bounded: a stage that failed before its daemon wrote little.
fn unavailable(log: &Path) -> Option<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    let file = File::open(log).ok()?;
    file.take(64 * 1024).read_to_end(&mut bytes).ok()?;
    network::pool::unavailable_in(&String::from_utf8_lossy(&bytes))
}

/// Everything a start refuses before it makes anything: the request's own
/// fields, this host's runtime prerequisites, what the Sandbox's network
/// needs of this machine (`network_plan`), each bind against the private
/// root's (none for a PostgreSQL Store), the helper, and the image the
/// bundle carries. Getting
/// the image's root may unpack it, once per digest for the whole runtime root.
fn start_request(
    local: &Local,
    helper: &Path,
    request: &crate::LaunchRequest,
    options: &Options,
) -> Result<(Paths, ImageRoot, rootfs::ImageHold, NetworkPlan), RuntimeError> {
    let paths = {
        let named = local.clone().naming(&request.name)?;
        Paths::of(named.paths()?)?
    };
    host_checks::require_root()?;
    host_checks::require_user_namespaces_blocked()?;
    host_checks::require_cgroup_v2()?;
    host_checks::require_fuse()?;
    let root = (paths.local.directory.parent())
        .ok_or(RuntimeError::Protocol("namespaces Sandbox root"))?;
    host_checks::require_overlay_upper(root)?;
    let plan = network_plan()?;
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
    Ok((paths, image, hold, plan))
}

/// What the Sandbox's own network needs of this machine, checked before any
/// effect: IPv4 forwarding on (never written here), no other table's
/// forward chain dropping its traffic, a pool free of this machine's routes
/// and addresses, and a resolver its namespace can reach
/// (`network::resolver`).
fn network_plan() -> Result<NetworkPlan, RuntimeError> {
    network::forwarding()?;
    network::forward_policy()?;
    let pool = network::pool()?;
    let launcher = match fs::read_to_string(network::resolver::LAUNCHER) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let systemd = fs::read_to_string(network::resolver::SYSTEMD_UPSTREAM).ok();
    let resolver = network::resolver::resolver_file(&launcher, systemd.as_deref());
    if let Resolver::Refuse(why) = resolver {
        return Err(RuntimeError::Unavailable(why));
    }
    Ok(NetworkPlan { pool, resolver })
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

/// The Sandbox's own directories and files, its copied daemon, helper and
/// runtime layer, its init spec and the empty log; returns the log file. Its overlay's writable layer
/// and work directory are made here, empty: the init stage mounts them with
/// the image underneath.
fn prepare(
    paths: &Paths,
    (request, helper): (&crate::LaunchRequest, &Path),
    options: &Options,
    (id, plan): (&crate::ContainerId, &NetworkPlan),
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
    // no `workspaces` here: the daemon mounts at `/layerfs/workspaces` in
    // the Sandbox's own root (#846)
    fs::DirBuilder::new()
        .mode(0o755)
        .create(&paths.bin_directory)?;
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
    if let Some(runtime) = &options.runtime {
        copy_runtime(runtime, &paths.runtime)?;
    }
    let resolver = match &plan.resolver {
        Resolver::Write(text) => {
            let file = paths.local.directory.join(RESOLVER_FILE);
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o644)
                .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
                .open(&file)?
                .write_all(text.as_bytes())?;
            Some(file)
        }
        Resolver::Bind | Resolver::Refuse(_) => None,
    };
    let spec = HelperSpec {
        root: paths.root.clone(),
        sandbox: paths.local.directory.clone(),
        daemon: paths.bin.clone(),
        config: paths.local.config.clone(),
        // the leaf, so the Sandbox's own group never holds a process
        cgroup: cgroup::leaf(&cgroup::path(id)),
        lower: image.directory().to_owned(),
        upper: paths.upper.clone(),
        work: paths.work.clone(),
        binds: options.binds.clone(),
        runtime: options.runtime.as_ref().map(|_| paths.runtime.clone()),
        network: Some(SpecNetwork {
            pool: plan.pool,
            link: network::pool::link_name(id.as_str()),
        }),
        resolver,
    };
    private(&paths.spec)?.write_all(&spec.encode()?)?;
    Ok(private(&paths.local.log)?)
}

/// Copies the runtime layer at `source` (`lib/` and `bin/`, regular files
/// only, as `tools/linux_bundle.py` seals it) to `target`, root-owned: the
/// libraries 0644, the loader and the programs 0755, the directories 0755.
/// Anything else in it (a link, a device, a deeper directory) is refused.
fn copy_runtime(source: &Path, target: &Path) -> Result<(), RuntimeError> {
    let refuse = || RuntimeError::Protocol("the bundle's runtime layer holds an unexpected entry");
    fs::DirBuilder::new().mode(0o755).create(target)?;
    for (directory, mode) in [("lib", 0o644), ("bin", 0o755)] {
        let into = target.join(directory);
        fs::DirBuilder::new().mode(0o755).create(&into)?;
        for entry in fs::read_dir(source.join(directory))? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(refuse());
            }
            let file = into.join(entry.file_name());
            fs::copy(entry.path(), &file)?;
            let loader = layerfs_os::runtime::LOADER;
            let mode = match loader.is_some_and(|loader| entry.file_name() == loader) {
                true => 0o755,
                false => mode,
            };
            fs::set_permissions(&file, Permissions::from_mode(mode))?;
        }
    }
    for entry in fs::read_dir(source)? {
        let name = entry?.file_name();
        if name != "lib" && name != "bin" {
            return Err(refuse());
        }
    }
    Ok(())
}
