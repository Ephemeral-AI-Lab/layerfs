//! The Runtime enum's Docker selection: no local directory, container capabilities.
use layerfs_sandbox::backend::docker::{Docker, DockerProvider};
use layerfs_sandbox::Runtime;
use std::time::Duration;

/// Selecting Docker names an Engine endpoint only; nothing is dialed here.
#[test]
fn docker_reports_container_capabilities_and_owns_no_local_directory() {
    let docker = Docker::new("/does-not-exist/docker.sock", Duration::from_secs(1)).unwrap();
    let runtime = Runtime::Docker(DockerProvider::new(docker));
    let capabilities = runtime.capabilities().unwrap();
    assert_eq!(capabilities.name, "docker");
    assert!(capabilities.requires_image);
    assert!(runtime.local_directory("anything").unwrap().is_none());
}
