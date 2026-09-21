use std::process::Command;

#[test]
fn mount_startup_rejects_invalid_inputs_before_connecting() {
    let good = vec![
        "--mount-readonly".to_string(),
        "read".into(),
        "71".repeat(32),
        "1".into(),
        "21".repeat(32),
        "0".into(),
        "0".into(),
    ];
    let cases = [
        ("LAYERFS_WORKSPACE_ROOT", "relative"),
        ("LAYERFS_WORKSPACE_MAX_COUNT", "0"),
        ("LAYERFS_WORKSPACE_MAX_COUNT", "unlimited"),
        ("LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES", "0"),
        ("LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES", "-1"),
        ("LAYERFS_WORKSPACE_DISK_BUDGET_BYTES", "0"),
        ("LAYERFS_WORKSPACE_DISK_BUDGET_BYTES", "not-bytes"),
    ];
    for (name, value) in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"))
            .env_clear()
            .env("LAYERFS_WORKSPACE_ROOT", "/layerfs")
            .env("LAYERFS_WORKSPACE_MAX_COUNT", "2")
            .env(name, value)
            .args(&good)
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {name}={value}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("InvalidInput"));
        assert!(output.stdout.is_empty());
    }
    for arguments in [
        vec!["--mount-readonly".into()],
        {
            let mut arguments = good.clone();
            arguments[4] = "branch:é".into();
            arguments
        },
        {
            let mut arguments = good;
            arguments[5] = "-1".into();
            arguments
        },
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"))
            .env_clear()
            .env("LAYERFS_WORKSPACE_ROOT", "/layerfs")
            .env("LAYERFS_WORKSPACE_MAX_COUNT", "2")
            .args(arguments)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("InvalidInput"));
    }
}

#[test]
fn control_configuration_is_explicit_separate_and_bounded() {
    let args = [
        "--mount-readonly".to_string(),
        "read".into(),
        "71".repeat(32),
        "1".into(),
        "21".repeat(32),
        "0".into(),
        "0".into(),
    ];
    let peer = format!("1,{},9999999999,1", "11".repeat(32));
    for (listen, peers, headless) in [
        (Some("127.0.0.1:0"), None, false),
        (None, Some(peer.clone()), false),
        (Some("127.0.0.1:0"), Some(peer.clone()), true),
        (
            Some("127.0.0.1:0"),
            Some(format!("1,{},9999999999,64", "11".repeat(32))),
            false,
        ),
        (Some("127.0.0.1:0"), Some(format!("{peer};{peer}")), false),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"));
        command
            .env_clear()
            .env("LAYERFS_WORKSPACE_ROOT", "/layerfs")
            .env("LAYERFS_WORKSPACE_MAX_COUNT", "2");
        if let Some(value) = listen {
            command.env("LAYERFS_CONTROL_LISTEN", value);
        }
        if let Some(value) = peers {
            command.env("LAYERFS_CONTROL_PEERS", value);
        }
        if !headless {
            command.args(&args);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("InvalidInput"));
    }
}

#[test]
fn writable_profile_requires_branch_disk_budget_and_commit_control_endpoint() {
    let branch = format!("branch:11{}", "61".repeat(16));
    for (base, disk, control) in [
        (branch.clone(), None, true),
        (branch.clone(), Some("0"), true),
        (branch.clone(), Some("invalid"), true),
        ("21".repeat(32), Some("67108864"), true),
        (branch, Some("67108864"), false),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"));
        command
            .env_clear()
            .env("LAYERFS_WORKSPACE_ROOT", "/layerfs")
            .env("LAYERFS_WORKSPACE_MAX_COUNT", "2")
            .env("LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES", "8388608")
            .env("LAYERFS_ENDPOINT", "127.0.0.1:1")
            .env("LAYERFS_SELECTOR", "1")
            .env("LAYERFS_PRIVATE_KEY", "11".repeat(32))
            .env("LAYERFS_SERVER_KEY", "22".repeat(32))
            .args([
                "--mount-writable",
                "read",
                &"71".repeat(32),
                "1",
                &base,
                "0",
                "0",
            ]);
        if let Some(disk) = disk {
            command.env("LAYERFS_WORKSPACE_DISK_BUDGET_BYTES", disk);
        }
        if control {
            command.env("LAYERFS_CONTROL_LISTEN", "127.0.0.1:0").env(
                "LAYERFS_CONTROL_PEERS",
                format!("1,{},9999999999,32", "11".repeat(32)),
            );
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("InvalidInput"));
        assert!(output.stdout.is_empty());
    }
    for peers in [
        String::new(),
        format!("1,{},9999999999,31", "11".repeat(32)),
        format!("1,{},1,32", "11".repeat(32)),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"))
            .env_clear()
            .env("LAYERFS_WORKSPACE_ROOT", "/layerfs")
            .env("LAYERFS_WORKSPACE_MAX_COUNT", "2")
            .env("LAYERFS_WORKSPACE_DISK_BUDGET_BYTES", "67108864")
            .env("LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES", "8388608")
            .env("LAYERFS_ENDPOINT", "127.0.0.1:1")
            .env("LAYERFS_SELECTOR", "1")
            .env("LAYERFS_PRIVATE_KEY", "11".repeat(32))
            .env("LAYERFS_SERVER_KEY", "22".repeat(32))
            .env("LAYERFS_CONTROL_LISTEN", "127.0.0.1:0")
            .env("LAYERFS_CONTROL_PEERS", peers)
            .args([
                "--mount-writable",
                "read",
                &"71".repeat(32),
                "1",
                &format!("branch:11{}", "61".repeat(16)),
                "0",
                "0",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("InvalidInput"));
        assert!(output.stdout.is_empty());
    }
}
