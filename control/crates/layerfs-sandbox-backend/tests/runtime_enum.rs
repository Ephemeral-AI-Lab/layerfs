//! What each backend's adapter fixes (its stored name, description and
//! capabilities), and the Runtime enum's refusal of a bind where the backend
//! does not bind. The docker backend is retired (D-8, layerfs-project#111).
#![cfg(unix)]
use layerfs_sandbox_backend::{Backend, IsolationModel, MountDriver};

/// Records keep their values: `host_process` is stored as `"native"` (G6).
#[test]
fn stored_names_parse_back_and_do_not_follow_the_rust_names() {
    let names: Vec<&str> = Backend::ALL.iter().map(|b| b.stored_name()).collect();
    assert_eq!(names, ["native", "namespaces"]);
    for backend in Backend::ALL {
        assert_eq!(Backend::parse(backend.stored_name()), Some(backend));
        assert!(!backend.description().is_empty(), "{backend:?}");
    }
    assert_eq!(Backend::parse("host_process"), None);
    assert_eq!(Backend::parse("Native"), None);
    // a record of the retired docker backend names no backend
    assert_eq!(Backend::parse("docker"), None);
}

#[test]
fn each_backend_fixes_its_capabilities() {
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

/// #535 §6.3: only the namespaces backend binds host directories; a
/// host-process daemon shares the host's files, so a bind is refused before
/// any effect (nothing is made under the root).
#[cfg(target_os = "macos")]
#[test]
fn a_bind_is_refused_where_the_backend_does_not_bind() {
    use layerfs_sandbox_backend::{
        Bind, CommandIdentity, HostProcessProvider, LaunchRequest, ProviderLaunchOptions, Runtime,
        RuntimeError,
    };
    use std::time::Duration;
    let bind = Bind::store(std::path::Path::new("/srv/store/store.sqlite")).unwrap();
    let request = LaunchRequest {
        name: "binds-1".into(),
        daemon_executable: "/does-not-exist/layerfs-daemon".into(),
        daemon_config: b"setup".to_vec(),
        labels: Vec::new(),
        startup_timeout: Duration::from_secs(1),
        options: ProviderLaunchOptions::Local {
            command_identity: CommandIdentity { uid: 501, gid: 20 },
            environment: Vec::new(),
        },
    };
    let root = std::env::temp_dir().join(format!("lfs-binds-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let runtime = Runtime::HostProcess(HostProcessProvider::new(&root).unwrap());
    let failure = runtime.launch_with_binds(request, vec![bind]).unwrap_err();
    assert!(
        matches!(failure.cause, RuntimeError::Unsupported(why) if why.contains("no bind")),
        "{:?}",
        failure.cause
    );
    assert!(failure.instance.is_none());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir_all(&root).unwrap();
}
