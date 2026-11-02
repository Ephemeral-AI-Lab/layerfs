//! The Runtime enum's Docker selection (no local directory, container
//! capabilities) and what each backend's adapter fixes: its stored name,
//! description and capabilities.
#![cfg(unix)]
use layerfs_sandbox::docker::{Docker, DockerProvider};
use layerfs_sandbox::{Backend, IsolationModel, MountDriver, Runtime};
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
