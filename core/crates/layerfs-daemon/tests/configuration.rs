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
