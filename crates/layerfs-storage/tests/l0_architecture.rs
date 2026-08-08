fn section<'a>(manifest: &'a str, name: &str) -> &'a str {
    let header = format!("[{name}]");
    let Some(header_start) = manifest.find(&header) else {
        return "";
    };
    let start = header_start + header.len();
    let rest = &manifest[start..];
    rest.find("\n[").map(|end| &rest[..end]).unwrap_or(rest)
}

#[test]
fn workspace_shape_and_package_boundaries_are_stable() {
    let workspace = include_str!("../../../Cargo.toml");
    assert!(workspace.contains("resolver = \"2\""));
    for member in [
        "crates/layerfs-sdk",
        "crates/layerfs-storage",
        "crates/layerfs-driver",
    ] {
        assert!(
            workspace.contains(&format!("\"{member}\"")),
            "missing member {member}"
        );
    }
    assert_eq!(workspace.matches("    \"crates/").count(), 3);

    let sdk = include_str!("../../layerfs-sdk/Cargo.toml");
    let storage = include_str!("../Cargo.toml");
    let driver = include_str!("../../layerfs-driver/Cargo.toml");

    assert!(sdk.contains("name = \"layerfs-sdk\""));
    assert!(sdk.contains("name = \"layerfs\""));
    assert!(sdk.contains("publish = true"));
    assert!(storage.contains("name = \"layerfs-storage\""));
    assert!(storage.contains("publish = false"));
    assert!(driver.contains("name = \"layerfs-driver\""));
    assert!(driver.contains("publish = false"));

    let storage_dependencies = section(storage, "dependencies");
    assert!(
        storage_dependencies.trim().is_empty(),
        "runtime storage dependencies must remain empty"
    );
    let driver_dependencies = section(driver, "dependencies");
    assert!(driver_dependencies.contains("layerfs-storage"));
    assert!(!driver_dependencies.contains("layerfs-sdk"));
    let sdk_dependencies = section(sdk, "dependencies");
    assert!(sdk_dependencies.contains("layerfs-driver"));
    assert!(sdk_dependencies.contains("layerfs-storage"));
    assert!(!storage.contains("layerfs-driver"));
    assert!(!driver.contains("layerfs-sdk"));
}

#[test]
fn prohibited_l0_runtime_dependencies_are_not_present() {
    let workspace = include_str!("../../../Cargo.toml");
    let storage = include_str!("../Cargo.toml");
    assert!(workspace.contains("blake3 = { version = \"=1.8.5\", default-features = false }"));
    assert!(storage.contains("[dev-dependencies]"));
    assert!(!section(storage, "dependencies").contains("blake3"));
    for forbidden in [
        "serde", "bincode", "opendal", "git2", "fuser", "wasmtime", "oci",
    ] {
        assert!(
            !workspace.to_ascii_lowercase().contains(forbidden),
            "forbidden dependency {forbidden}"
        );
    }
}
