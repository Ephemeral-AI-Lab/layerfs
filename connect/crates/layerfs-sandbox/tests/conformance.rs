//! The Sandbox backend conformance kit (#536 §11 step 9): checks every
//! backend passes, run over `Backend::ALL`, so a new backend is covered as
//! soon as it is a variant. No Docker Engine or root is needed: a docker
//! runtime names a socket that does not exist, and only the lifecycle a
//! backend can run as this user on this host runs.
//!
//! Per backend it checks:
//! - its stored name round-trips (`stored_name`, `parse`) and is unique;
//! - its capabilities against the kernel route every `connect/SUPPORT.json`
//!   row that names it serves, and that some row names it;
//! - binds (#535 §6.3): a backend that takes none refuses a non-empty list
//!   in `launch_with_binds` before any effect (`Unsupported`); a backend
//!   that binds carries them through the helper spec, which round-trips
//!   them;
//! - its runtime on this host, where it has one: the backend and
//!   capabilities it reports, and that another backend's launch options and
//!   records are refused before any effect or attempt;
//! - the lifecycle it can run here (launch, inspect, record, reopen, stop,
//!   reopen stopped, delete), or that a launch it cannot finish keeps no
//!   owner.
//!
//! What a backend declares for the kit is in the exhaustive matches below
//! ([`binds`], [`options`], [`record`], [`runtime`], [`lifecycle`]): a new
//! backend does not compile here until it states them.
#![cfg(unix)]
#[path = "support/support_json.rs"]
mod support_json;
use layerfs_sandbox::docker::{Docker, DockerProvider};
use layerfs_sandbox::{
    Backend, Bind, CommandIdentity, ContainerId, DockerRecord, HelperSpec, Image, LaunchPhase,
    LaunchRequest, LocalRecord, MountDriver, NamespaceRecord, ProviderLaunchOptions, Runtime,
    RuntimeError, SandboxRecord,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ---------------------------------------------------------------------------
// What each backend declares
// ---------------------------------------------------------------------------

/// Whether a backend binds host directories into a Sandbox's root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Binds {
    /// It takes its Store otherwise (a volume, or the host's own files) and
    /// refuses a bind before any effect.
    Refused,
    /// The binds travel to its init stage in the helper spec.
    Accepted,
}

fn binds(backend: Backend) -> Binds {
    match backend {
        Backend::Docker | Backend::HostProcess => Binds::Refused,
        Backend::Namespaces => Binds::Accepted,
    }
}

/// The launch options of `backend`, well formed for this user.
fn options(backend: Backend) -> ProviderLaunchOptions {
    let command_identity = CommandIdentity {
        uid: nix::unistd::Uid::effective().as_raw(),
        gid: nix::unistd::Gid::effective().as_raw(),
    };
    match backend {
        Backend::Docker => ProviderLaunchOptions::Docker {
            image: format!("sha256:{}", "a".repeat(64)),
            store_volume: None,
            port: 30421,
            connect_host: None,
        },
        Backend::HostProcess => ProviderLaunchOptions::Local {
            command_identity,
            environment: Vec::new(),
        },
        Backend::Namespaces => ProviderLaunchOptions::Namespaces {
            command_identity,
            environment: Vec::new(),
            image: image(),
            archive: PathBuf::from("/layerfs/bundle/images/a.tar"),
        },
    }
}

/// A record of the kind `backend` writes.
fn record(backend: Backend) -> SandboxRecord {
    let local = LocalRecord {
        name: "kit-1".into(),
        pid: 4242,
        start_time: 7,
        boot: Some(1),
        port: 41000,
        labels: Vec::new(),
    };
    match backend {
        Backend::Docker => SandboxRecord::Docker(DockerRecord {
            image: format!("sha256:{}", "a".repeat(64)),
            store_volume: Some("lfs-kit".into()),
            port: 30421,
            labels: Vec::new(),
        }),
        Backend::HostProcess => SandboxRecord::Local(local),
        Backend::Namespaces => SandboxRecord::Namespaces(NamespaceRecord {
            local,
            image: image(),
        }),
    }
}

/// The backend a record names.
fn record_backend(record: &SandboxRecord) -> Backend {
    match record {
        SandboxRecord::Docker(_) => Backend::Docker,
        SandboxRecord::Local(_) => Backend::HostProcess,
        SandboxRecord::Namespaces(_) => Backend::Namespaces,
    }
}

/// `backend`'s runtime over `root` on this host: `None` where the host has
/// no runtime of it, else what selecting it answers (a host that cannot
/// hold it is refused, `Unsupported`, before any effect).
fn runtime(backend: Backend, root: &Path) -> Option<Result<Runtime, RuntimeError>> {
    match backend {
        Backend::Docker => {
            let docker = Docker::new("/does-not-exist/docker.sock", Duration::from_secs(1));
            Some(docker.map(|docker| Runtime::Docker(DockerProvider::new(docker))))
        }
        #[cfg(target_os = "macos")]
        Backend::HostProcess => {
            Some(layerfs_sandbox::HostProcessProvider::new(root).map(Runtime::HostProcess))
        }
        #[cfg(target_os = "linux")]
        Backend::Namespaces => {
            Some(layerfs_sandbox::NamespaceProvider::new(root).map(Runtime::Namespaces))
        }
        _ => None,
    }
}

/// How much of a backend's lifecycle the kit runs on a host where it has
/// a runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lifecycle {
    /// All of it, as this user, with a stand-in daemon.
    Runs,
    /// None: the service it launches through is absent (the kit's docker
    /// runtime names a socket that does not exist), so a launch fails and
    /// keeps no owner.
    ServiceAbsent,
    /// None: it needs root and a prepared kernel (live checks run it).
    NeedsRoot,
}

fn lifecycle(backend: Backend) -> Lifecycle {
    match backend {
        Backend::Docker => Lifecycle::ServiceAbsent,
        Backend::HostProcess => Lifecycle::Runs,
        Backend::Namespaces => Lifecycle::NeedsRoot,
    }
}

fn image() -> Image {
    Image::parse(&format!("sha256:{}", "a".repeat(64))).unwrap()
}

// ---------------------------------------------------------------------------
// Names, capabilities and the support table
// ---------------------------------------------------------------------------

#[test]
fn each_stored_name_round_trips_and_is_unique() {
    let mut names = BTreeSet::new();
    let mut capability_names = BTreeSet::new();
    for backend in Backend::ALL {
        let name = backend.stored_name();
        assert!(!name.is_empty(), "{backend:?}");
        assert!(names.insert(name), "{name} stored twice");
        assert_eq!(Backend::parse(name), Some(backend));
        assert_eq!(Backend::parse(&name.to_uppercase()), None, "{name}");
        assert!(!backend.description().is_empty(), "{backend:?}");
        let capabilities = backend.capabilities();
        assert!(capability_names.insert(capabilities.name), "{backend:?}");
    }
}

/// The SUPPORT.json kernel route a mount driver serves.
fn route(driver: MountDriver) -> Option<&'static str> {
    match driver {
        MountDriver::Fuse => Some("fuse"),
        MountDriver::LoopbackNfs => Some("nfs"),
        MountDriver::SharedFs | MountDriver::None => None,
    }
}

#[test]
fn each_backends_mount_driver_serves_its_support_rows_kernel_route() {
    let rows = support_json::rows();
    for row in &rows {
        let at = format!("{} on {}", row.platform, row.host);
        let Some(name) = &row.backend else {
            assert_eq!(row.kernel, None, "{at}: a kernel route with no backend");
            continue;
        };
        let backend = Backend::parse(name).unwrap_or_else(|| panic!("{at}: {name}"));
        let kernel = row.kernel.as_deref();
        assert!(kernel.is_some(), "{at}: a backend with no kernel route");
        assert_eq!(route(backend.capabilities().mount_driver), kernel, "{at}");
        assert!(row.description.ends_with(backend.description()), "{at}");
    }
    for backend in Backend::ALL {
        let named = rows
            .iter()
            .any(|r| r.backend.as_deref() == Some(backend.stored_name()));
        assert!(named, "no platform runs {backend:?} on any host");
    }
}

// ---------------------------------------------------------------------------
// Binds
// ---------------------------------------------------------------------------

fn store_bind() -> Bind {
    Bind::store(Path::new("/layerfs/data/stores/app/store.sqlite")).unwrap()
}

fn request(backend: Backend, daemon: &Path, name: &str) -> LaunchRequest {
    LaunchRequest {
        name: name.into(),
        daemon_executable: daemon.to_owned(),
        daemon_config: b"kit setup".to_vec(),
        labels: Vec::new(),
        startup_timeout: Duration::from_secs(2),
        options: options(backend),
    }
}

/// A fresh, empty runtime root this user owns.
struct Root(PathBuf);

impl Root {
    fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("lfs-kit-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self(dir)
    }

    fn is_empty(&self) -> bool {
        std::fs::read_dir(&self.0).unwrap().next().is_none()
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_backend_that_takes_no_bind_refuses_one_before_any_effect() {
    for backend in Backend::ALL
        .into_iter()
        .filter(|b| binds(*b) == Binds::Refused)
    {
        let root = Root::new("binds");
        let Some(Ok(runtime)) = runtime(backend, &root.0) else {
            continue;
        };
        let daemon = root.0.join("absent-daemon");
        let failure = runtime
            .launch_with_binds(request(backend, &daemon, "binds-1"), vec![store_bind()])
            .unwrap_err();
        assert_eq!(failure.phase, LaunchPhase::Validate, "{backend:?}");
        assert!(failure.instance.is_none(), "{backend:?}");
        assert!(
            matches!(failure.cause, RuntimeError::Unsupported(_)),
            "{backend:?}: {:?}",
            failure.cause
        );
        assert!(root.is_empty(), "{backend:?} made something");
    }
}

#[test]
fn a_backend_that_binds_carries_its_binds_through_the_helper_spec() {
    let read_only = Bind {
        host: PathBuf::from("/layerfs/data/shared"),
        guest: PathBuf::from("/mnt/shared"),
        read_write: false,
    };
    for backend in Backend::ALL
        .into_iter()
        .filter(|b| binds(*b) == Binds::Accepted)
    {
        for list in [
            Vec::new(),
            vec![store_bind()],
            vec![store_bind(), read_only.clone()],
        ] {
            let spec = HelperSpec {
                root: "/layerfs/local/kit-1.root".into(),
                sandbox: "/layerfs/local/kit-1".into(),
                daemon: "/layerfs/local/kit-1/bin/layerfs-daemon".into(),
                config: "/layerfs/local/kit-1/config/daemon.setup".into(),
                cgroup: "/sys/fs/cgroup/layerfs/kit-1".into(),
                lower: "/layerfs/local/images/aaaa".into(),
                upper: "/layerfs/local/kit-1/overlay/upper".into(),
                work: "/layerfs/local/kit-1/overlay/work".into(),
                binds: list,
            };
            let encoded = spec.encode().unwrap();
            assert_eq!(HelperSpec::parse(&encoded).unwrap(), spec, "{backend:?}");
        }
        // the store bind is the Store file's directory, read-write at its own path
        let bind = store_bind();
        assert_eq!(bind.host, Path::new("/layerfs/data/stores/app"));
        assert_eq!((bind.guest, bind.read_write), (bind.host, true));
    }
}

// ---------------------------------------------------------------------------
// Runtimes on this host
// ---------------------------------------------------------------------------

/// The backend of the `native` platform on this host, as SUPPORT.json
/// writes it: this host's own runtime.
fn native_backend_here() -> Option<Backend> {
    let host = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return None;
    };
    let rows = support_json::rows();
    let row = rows
        .iter()
        .find(|r| r.host == host && r.platform == "native")
        .expect("a native row for this host");
    row.backend
        .as_deref()
        .map(|name| Backend::parse(name).unwrap())
}

#[test]
fn each_runtime_here_is_its_backend_or_refused_before_any_effect() {
    if let Some(native) = native_backend_here() {
        let root = Root::new("native");
        assert!(
            runtime(native, &root.0).is_some(),
            "this host's native backend {native:?} has no runtime here"
        );
    }
    for backend in Backend::ALL {
        let root = Root::new("runtime");
        match runtime(backend, &root.0) {
            None => {}
            Some(Ok(runtime)) => {
                assert_eq!(runtime.backend(), backend);
                assert_eq!(runtime.capabilities().unwrap(), backend.capabilities());
            }
            Some(Err(error)) => {
                assert!(matches!(error, RuntimeError::Unsupported(_)), "{error:?}");
            }
        }
        assert!(root.is_empty(), "{backend:?}: selecting it made something");
    }
}

#[test]
fn each_runtime_refuses_another_backends_options_and_records() {
    for backend in Backend::ALL {
        let root = Root::new("foreign");
        let Some(Ok(runtime)) = runtime(backend, &root.0) else {
            continue;
        };
        let daemon = root.0.join("absent-daemon");
        let id = ContainerId::parse(&"1".repeat(64)).unwrap();
        for other in Backend::ALL.into_iter().filter(|o| *o != backend) {
            let at = format!("{backend:?} given {other:?}'s");
            let mut launch = request(other, &daemon, "foreign-1");
            launch.options = options(other);
            let failure = runtime.launch(launch).unwrap_err();
            assert_eq!(failure.phase, LaunchPhase::Validate, "{at} options");
            assert!(failure.instance.is_none(), "{at} options");
            assert!(
                matches!(failure.cause, RuntimeError::Unsupported(_)),
                "{at} options: {:?}",
                failure.cause
            );
            let foreign = record(other);
            assert_eq!(record_backend(&foreign), other);
            for failure in [
                runtime.reopen(id, &foreign).unwrap_err(),
                runtime.reopen_stopped(id, &foreign).unwrap_err(),
            ] {
                assert!(!failure.attempted, "{at} record");
                assert!(
                    matches!(failure.cause, RuntimeError::Unsupported(_)),
                    "{at} record: {:?}",
                    failure.cause
                );
            }
        }
        assert!(root.is_empty(), "{backend:?} made something");
    }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// A stand-in daemon that announces a listener and waits to be stopped.
fn stand_in_daemon(root: &Root, port: u16) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let binary = root.0.join("daemon");
    let script =
        format!("#!/bin/sh\necho LAYERFS_DAEMON_LISTEN 127.0.0.1:{port}\nexec /bin/sleep 30\n");
    std::fs::write(&binary, script).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    binary
}

#[test]
fn each_backend_runs_the_lifecycle_it_can_run_here() {
    for (i, backend) in Backend::ALL.into_iter().enumerate() {
        let root = Root::new("life");
        let Some(Ok(runtime)) = runtime(backend, &root.0) else {
            continue;
        };
        let daemon = stand_in_daemon(&root, 40480 + i as u16);
        match lifecycle(backend) {
            Lifecycle::Runs => {}
            Lifecycle::NeedsRoot => continue,
            Lifecycle::ServiceAbsent => {
                let failure = runtime
                    .launch_with_binds(request(backend, &daemon, "life-1"), Vec::new())
                    .unwrap_err();
                assert!(failure.instance.is_none(), "{backend:?}: {failure:?}");
                assert!(
                    !matches!(failure.cause, RuntimeError::Unsupported(_)),
                    "{backend:?} refused a plain launch: {failure:?}"
                );
                continue;
            }
        }
        let mut sandbox = runtime
            .launch_with_binds(request(backend, &daemon, "life-1"), Vec::new())
            .unwrap_or_else(|f| panic!("{backend:?}: {f:?}"));
        assert_eq!(sandbox.backend(), backend);
        let id = sandbox.identity();
        assert!(sandbox.inspect().unwrap().running, "{backend:?}");
        let record = sandbox.record().expect("a launched Sandbox has a record");
        assert_eq!(record_backend(&record), backend);
        let reopened = runtime.reopen(id, &record).unwrap();
        assert_eq!(reopened.backend(), backend);
        assert_eq!(reopened.identity(), id);
        assert_eq!(reopened.endpoint().unwrap(), sandbox.endpoint().unwrap());
        drop(reopened);
        sandbox.stop(1).unwrap();
        assert!(!sandbox.inspect().unwrap().running, "{backend:?}");
        let stopped = runtime.reopen(id, &record).unwrap_err();
        assert!(stopped.container_stopped(), "{backend:?}: {stopped:?}");
        let known = runtime.reopen_stopped(id, &record).unwrap();
        assert_eq!((known.backend(), known.identity()), (backend, id));
        drop(known);
        sandbox.delete().unwrap();
        // the stand-in daemon is the only file left in the root
        let left: Vec<_> = std::fs::read_dir(&root.0)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left, ["daemon"], "{backend:?} left its Sandbox behind");
    }
}
