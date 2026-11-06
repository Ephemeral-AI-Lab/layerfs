//! The Runtime enum's Docker selection (no local directory, container
//! capabilities) and what each backend's adapter fixes: its stored name,
//! description and capabilities.
#![cfg(unix)]
use layerfs_sandbox_backend::docker::{Docker, DockerProvider};
use layerfs_sandbox_backend::{Backend, IsolationModel, MountDriver, Runtime};
use std::time::Duration;

/// Selecting Docker names an Engine endpoint only; nothing is dialed here.
#[test]
fn docker_reports_container_capabilities_and_owns_no_local_directory() {
    let docker = Docker::new("/does-not-exist/docker.sock", Duration::from_secs(1)).unwrap();
    let runtime = Runtime::Docker(DockerProvider::new(docker));
    assert_eq!(runtime.backend(), Backend::Docker);
    let capabilities = runtime.capabilities().unwrap();
    assert_eq!(capabilities.name, "docker");
    assert!(capabilities.requires_image);
    assert!(runtime.local_directory("anything").unwrap().is_none());
}

/// Records keep their values: `host_process` is stored as `"native"` (G6).
#[test]
fn stored_names_parse_back_and_do_not_follow_the_rust_names() {
    let names: Vec<&str> = Backend::ALL.iter().map(|b| b.stored_name()).collect();
    assert_eq!(names, ["docker", "native", "namespaces"]);
    for backend in Backend::ALL {
        assert_eq!(Backend::parse(backend.stored_name()), Some(backend));
        assert!(!backend.description().is_empty(), "{backend:?}");
    }
    assert_eq!(Backend::parse("host_process"), None);
    assert_eq!(Backend::parse("Native"), None);
}

#[test]
fn each_backend_fixes_its_capabilities() {
    let docker = Backend::Docker.capabilities();
    assert_eq!(
        (docker.name, docker.isolation, docker.mount_driver),
        ("docker", IsolationModel::Container, MountDriver::Fuse)
    );
    let host = Backend::HostProcess.capabilities();
    assert_eq!(
        (host.name, host.isolation, host.mount_driver),
        (
            "native-macos",
            IsolationModel::HostProcess,
            MountDriver::LoopbackNfs
        )
    );
    assert!(!host.requires_image);
    let namespaces = Backend::Namespaces.capabilities();
    assert_eq!(
        (
            namespaces.name,
            namespaces.isolation,
            namespaces.mount_driver
        ),
        (
            "namespaces-linux",
            IsolationModel::MicroVm,
            MountDriver::Fuse
        )
    );
    assert!(namespaces.requires_image);
}

/// #535 §6.3: only the namespaces backend binds host directories; docker
/// takes its Store as a volume and a host-process daemon shares the host's
/// files, so a bind is refused before any effect (no Engine is dialled,
/// nothing is made under the root). Without binds the launch is the plain
/// one (here it fails later, at the missing Engine socket).
#[test]
fn a_bind_is_refused_where_the_backend_does_not_bind() {
    use layerfs_sandbox_backend::{
        Bind, LaunchPhase, LaunchRequest, ProviderLaunchOptions, RuntimeError,
    };
    let bind = Bind::store(std::path::Path::new("/srv/store/store.sqlite")).unwrap();
    let request = |options| LaunchRequest {
        name: "binds-1".into(),
        daemon_executable: "/does-not-exist/layerfs-daemon".into(),
        daemon_config: b"setup".to_vec(),
        labels: Vec::new(),
        startup_timeout: Duration::from_secs(1),
        options,
    };
    let docker = Docker::new("/does-not-exist/docker.sock", Duration::from_secs(1)).unwrap();
    let runtime = Runtime::Docker(DockerProvider::new(docker));
    let docker_options = || ProviderLaunchOptions::Docker {
        image: format!("sha256:{}", "a".repeat(64)),
        store_volume: None,
        port: 30421,
        connect_host: None,
    };
    let failure = runtime
        .launch_with_binds(request(docker_options()), vec![bind.clone()])
        .unwrap_err();
    assert_eq!(failure.phase, LaunchPhase::Validate);
    assert!(failure.instance.is_none());
    assert!(
        matches!(failure.cause, RuntimeError::Unsupported(why) if why.contains("volume")),
        "{:?}",
        failure.cause
    );
    let plain = runtime
        .launch_with_binds(request(docker_options()), Vec::new())
        .unwrap_err();
    assert!(
        !matches!(plain.cause, RuntimeError::Unsupported(_)),
        "{:?}",
        plain.cause
    );
    #[cfg(target_os = "macos")]
    {
        let root = std::env::temp_dir().join(format!("lfs-binds-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let runtime =
            Runtime::HostProcess(layerfs_sandbox_backend::HostProcessProvider::new(&root).unwrap());
        let options = ProviderLaunchOptions::Local {
            command_identity: layerfs_sandbox_backend::CommandIdentity { uid: 501, gid: 20 },
            environment: Vec::new(),
        };
        let failure = runtime
            .launch_with_binds(request(options), vec![bind])
            .unwrap_err();
        assert!(
            matches!(failure.cause, RuntimeError::Unsupported(why) if why.contains("no bind")),
            "{:?}",
            failure.cause
        );
        assert!(failure.instance.is_none());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
