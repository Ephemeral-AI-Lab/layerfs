//! Real provider lifecycle and partial launch custody through the public port.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[cfg(target_os = "linux")]
use layerfs_sandbox::backend::local::NativeLinuxProvider as NativeProvider;
#[cfg(target_os = "macos")]
use layerfs_sandbox::backend::local::NativeMacosProvider as NativeProvider;
use layerfs_sandbox::{
    provider::{
        DockerRecord, LaunchPhase, LaunchRequest, ProviderLaunchOptions, SandboxProvider,
        SandboxRecord,
    },
    CommandIdentity,
};
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

#[test]
fn provider_preserves_partial_launch_owner_and_reopens_the_exact_process() {
    if cfg!(target_os = "linux") && !nix::unistd::Uid::effective().is_root() {
        assert!(NativeProvider::new("/no-access-required").is_err());
        return;
    }
    let base = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("layerfs-provider-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).unwrap();
    let binary = base.join("daemon");
    fs::write(
        &binary,
        "#!/bin/sh\necho LAYERFS_DAEMON_LISTEN 127.0.0.1:40404\nexec /bin/sleep 30\n",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let provider = NativeProvider::new(&base).unwrap();
    assert_eq!(
        provider.capabilities().name,
        if cfg!(target_os = "linux") {
            "native-linux"
        } else {
            "native-macos"
        }
    );
    let request = |name: &str| LaunchRequest {
        name: name.into(),
        daemon_executable: binary.clone(),
        daemon_config: b"private setup".to_vec(),
        labels: vec![],
        startup_timeout: Duration::from_secs(2),
        options: ProviderLaunchOptions::Local {
            command_identity: CommandIdentity {
                uid: if cfg!(target_os = "linux") {
                    501
                } else {
                    nix::unistd::Uid::effective().as_raw()
                },
                gid: if cfg!(target_os = "linux") {
                    20
                } else {
                    nix::unistd::Gid::effective().as_raw()
                },
            },
            environment: vec![],
        },
    };
    let mut sandbox = provider.launch(request("success")).unwrap();
    let id = sandbox.identity();
    let local_record = sandbox.local_record().unwrap();
    let record = sandbox.record().unwrap();
    assert_eq!(record, SandboxRecord::Local(local_record));
    assert!(sandbox.inspect().unwrap().running);
    let reopened = provider.reopen(id, &record).unwrap();
    assert_eq!(reopened.identity(), id);
    assert_eq!(reopened.endpoint().unwrap(), sandbox.endpoint().unwrap());
    let foreign = SandboxRecord::Docker(DockerRecord {
        image: format!("sha256:{}", "a".repeat(64)),
        store_volume: "lfs-foreign".into(),
        port: 30421,
        labels: vec![],
    });
    assert!(!provider.reopen(id, &foreign).err().unwrap().attempted);
    drop(reopened);
    sandbox.stop(1).unwrap();
    assert!(!sandbox.inspect().unwrap().running);
    assert!(provider
        .reopen(id, &record)
        .err()
        .unwrap()
        .container_stopped());
    sandbox.delete().unwrap();
    fs::write(&binary, "#!/bin/sh\nexit 3\n").unwrap();
    let mut failure = provider.launch(request("failure")).err().unwrap();
    assert_eq!(failure.phase, LaunchPhase::Listener);
    let mut partial = failure.instance.take().expect("created resource retained");
    partial.stop(1).unwrap();
    partial.delete().unwrap();
    let mut invalid = request("../escape");
    invalid.daemon_config = b"NEVER_PRINT_THIS_SECRET".to_vec();
    assert!(!format!("{invalid:?}").contains("NEVER_PRINT_THIS_SECRET"));
    assert_eq!(
        provider.launch(invalid).err().unwrap().phase,
        LaunchPhase::Validate
    );
    let mut wrong_options = request("wrong-options");
    wrong_options.options = ProviderLaunchOptions::Docker {
        image: format!("sha256:{}", "b".repeat(64)),
        store_volume: "lfs-foreign".into(),
        port: 30421,
        connect_host: None,
    };
    assert_eq!(
        provider.launch(wrong_options).err().unwrap().phase,
        LaunchPhase::Validate
    );
    fs::remove_dir_all(base).unwrap();
}
