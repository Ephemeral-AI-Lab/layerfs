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
    assert!(storage_dependencies.contains("blake3.workspace = true"));
    assert_eq!(
        storage_dependencies
            .lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
            .count(),
        1,
        "BLAKE3 must be the sole private L1 runtime dependency"
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
fn prohibited_runtime_dependencies_are_not_present() {
    let workspace = include_str!("../../../Cargo.toml");
    let storage = include_str!("../Cargo.toml");
    assert!(workspace.contains("blake3 = { version = \"=1.8.5\", default-features = false }"));
    assert!(section(storage, "dependencies").contains("blake3.workspace = true"));
    for forbidden in [
        "serde", "bincode", "opendal", "git2", "fuser", "wasmtime", "oci",
    ] {
        assert!(
            !workspace.to_ascii_lowercase().contains(forbidden),
            "forbidden dependency {forbidden}"
        );
    }
}

#[test]
fn fscas_remains_private_to_the_unpublished_storage_implementation() {
    let storage_manifest = include_str!("../Cargo.toml");
    let sdk = include_str!("../../layerfs-sdk/src/lib.rs");
    let driver = include_str!("../../layerfs-driver/src/lib.rs");

    assert!(storage_manifest.contains("publish = false"));
    for public_surface in [sdk, driver] {
        for private_name in [
            "FsCas",
            "fscas",
            "FsPrivatePack",
            "CompleteValidatedClosure",
        ] {
            assert!(
                !public_surface.contains(private_name),
                "private storage name {private_name} leaked into an SDK/driver surface"
            );
        }
    }
}

#[test]
fn storage_source_follows_the_domain_responsibility_map() {
    let lib = include_str!("../src/lib.rs");
    for forbidden_module in [
        "mod c3;",
        "mod cas_stream;",
        "mod fscas;",
        "mod tree;",
        "mod update;",
    ] {
        assert!(
            !lib.contains(forbidden_module),
            "legacy ownership module remains in lib.rs: {forbidden_module}"
        );
    }

    let required_domain_files = [
        include_str!("../src/identity/mod.rs"),
        include_str!("../src/identity/framing.rs"),
        include_str!("../src/identity/logical.rs"),
        include_str!("../src/identity/physical.rs"),
        include_str!("../src/cdc/mod.rs"),
        include_str!("../src/cdc/engine.rs"),
        include_str!("../src/cdc/resync.rs"),
        include_str!("../src/cdc/fastcdc/mod.rs"),
        include_str!("../src/cdc/fastcdc/scanner.rs"),
        include_str!("../src/cdc/fastcdc/gear.rs"),
        include_str!("../src/cdc/fastcdc/rejoin.rs"),
        include_str!("../src/cdc/seqcdc/mod.rs"),
        include_str!("../src/cdc/seqcdc/scanner.rs"),
        include_str!("../src/cdc/seqcdc/rejoin.rs"),
        include_str!("../src/content/mod.rs"),
        include_str!("../src/content/create.rs"),
        include_str!("../src/content/replace.rs"),
        include_str!("../src/content/update.rs"),
        include_str!("../src/cas/mod.rs"),
        include_str!("../src/cas/port.rs"),
        include_str!("../src/cas/fs.rs"),
        include_str!("../src/cas/admission.rs"),
        include_str!("../src/cas/catalog.rs"),
        include_str!("../src/cas/closure.rs"),
        include_str!("../src/cow/mod.rs"),
        include_str!("../src/cow/file.rs"),
        include_str!("../src/cow/tree.rs"),
    ];
    assert!(
        required_domain_files
            .iter()
            .all(|source| !source.is_empty()),
        "a required domain ownership file is empty"
    );
}
