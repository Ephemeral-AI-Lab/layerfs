//! Real native runtime lifecycle and partial launch custody through the Runtime enum.
//! Native is macOS only (#533 O27).
#![cfg(target_os = "macos")]
use layerfs_sandbox_backend::host_process::HostProcessProvider as NativeProvider;
use layerfs_sandbox_backend::{
    CommandIdentity, Image, LaunchPhase, LaunchRequest, ProviderLaunchOptions, Runtime,
    SandboxRecord,
};
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

/// The native runtime this host offers, over `root`.
fn native(root: &std::path::Path) -> Runtime {
    Runtime::HostProcess(NativeProvider::new(root).unwrap())
}

#[test]
fn runtime_preserves_partial_launch_owner_and_reopens_the_exact_process() {
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
    let provider = native(&base);
    assert_eq!(provider.capabilities().unwrap().name, "native-macos");
    let request = |name: &str| LaunchRequest {
        name: name.into(),
        daemon_executable: binary.clone(),
        daemon_config: b"private setup".to_vec(),
        labels: vec![],
        startup_timeout: Duration::from_secs(2),
        options: ProviderLaunchOptions::Local {
            command_identity: CommandIdentity {
                uid: nix::unistd::Uid::effective().as_raw(),
                gid: nix::unistd::Gid::effective().as_raw(),
            },
            environment: vec![],
        },
    };
    let mut sandbox = provider.launch(request("success")).unwrap();
    let id = sandbox.identity();
    let local_record = sandbox.local_record().unwrap();
    let record = sandbox.record().unwrap();
    assert_eq!(record, SandboxRecord::Local(local_record.clone()));
    assert!(sandbox.inspect().unwrap().running);
    let reopened = provider.reopen(id, &record).unwrap();
    assert_eq!(reopened.identity(), id);
    assert_eq!(reopened.endpoint().unwrap(), sandbox.endpoint().unwrap());
    // another backend's record of the same facts is refused before any effect
    let foreign = SandboxRecord::Namespaces(layerfs_sandbox_backend::NamespaceRecord {
        local: local_record.clone(),
        image: Image::parse(&format!("sha256:{}", "a".repeat(64))).unwrap(),
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
    wrong_options.options = ProviderLaunchOptions::Namespaces {
        command_identity: CommandIdentity { uid: 501, gid: 20 },
        environment: vec![],
        image: Image::parse(&format!("sha256:{}", "b".repeat(64))).unwrap(),
        archive: "/does-not-exist/image.tar".into(),
    };
    assert_eq!(
        provider.launch(wrong_options).err().unwrap().phase,
        LaunchPhase::Validate
    );
    fs::write(
        &binary,
        "#!/bin/sh\necho LAYERFS_DAEMON_LISTEN 127.0.0.1:40405\nexec /bin/sleep 30\n",
    )
    .unwrap();
    let mut chosen = provider.launch(request("chosen-route")).unwrap();
    chosen.stop(1).unwrap();
    chosen.delete().unwrap();
    fs::remove_dir_all(base).unwrap();
}
