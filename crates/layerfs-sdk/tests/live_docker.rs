#[test]
fn prepared_container_gate_is_explicit() {
    if std::env::var_os("LAYERFS_LIVE_DOCKER").is_none() {
        return;
    }
    assert!(std::process::Command::new("docker")
        .arg("info")
        .status()
        .is_ok_and(|status| status.success()));
}
