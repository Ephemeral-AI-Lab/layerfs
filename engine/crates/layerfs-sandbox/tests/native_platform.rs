//! Platform refusal precedes filesystem access; a runtime never selects another backend.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_sandbox::{
    provider::{DockerRecord, LaunchPhase, LaunchRequest, ProviderLaunchOptions, SandboxRecord},
    CommandIdentity, ContainerId, Runtime, RuntimeError,
};
use std::time::Duration;

#[test]
fn wrong_platform_is_refused_before_root_access() {
    #[cfg(target_os = "macos")]
    let result = layerfs_sandbox::backend::native::NativeLinuxProvider::new(
        "/does-not-exist/native-provider",
    );
    #[cfg(target_os = "linux")]
    let result = layerfs_sandbox::backend::native::NativeMacosProvider::new(
        "/does-not-exist/native-provider",
    );
    assert!(matches!(result, Err(RuntimeError::Unsupported(_))));
}

#[test]
fn native_linux_requires_root_before_root_access() {
    if cfg!(target_os = "linux") && !nix::unistd::Uid::effective().is_root() {
        assert!(matches!(
            layerfs_sandbox::backend::native::NativeLinuxProvider::new(
                "/does-not-exist/native-provider"
            ),
            Err(RuntimeError::Unsupported(_))
        ));
    }
}

/// Windows has no host runtime: the placeholder variant refuses every
/// operation before any effect, and never falls back to Docker or native.
#[test]
fn windows_is_refused_before_any_effect() {
    let runtime = Runtime::Windows;
    assert!(matches!(
        runtime.capabilities(),
        Err(RuntimeError::Unsupported(_))
    ));
    assert!(matches!(
        runtime.local_directory("one"),
        Err(RuntimeError::Unsupported(_))
    ));
    let request = LaunchRequest {
        name: "one".into(),
        daemon_executable: "/does-not-exist/daemon".into(),
        daemon_config: b"setup".to_vec(),
        labels: vec![],
        startup_timeout: Duration::from_secs(1),
        options: ProviderLaunchOptions::Local {
            command_identity: CommandIdentity { uid: 501, gid: 20 },
            environment: vec![],
        },
    };
    let failure = runtime.launch(request).err().unwrap();
    assert_eq!(failure.phase, LaunchPhase::Validate);
    assert!(failure.instance.is_none());
    assert!(matches!(failure.cause, RuntimeError::Unsupported(_)));
    let record = SandboxRecord::Docker(DockerRecord {
        image: format!("sha256:{}", "a".repeat(64)),
        store_volume: Some("lfs-store".into()),
        port: 30421,
        labels: vec![],
    });
    let id = ContainerId::parse(&"b".repeat(64)).unwrap();
    assert!(runtime.reopen(id, &record).is_err());
    assert!(runtime.reopen_stopped(id, &record).is_err());
}
