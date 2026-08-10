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
    use std::path::Path;

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
        include_str!("../src/error.rs"),
        include_str!("../src/limits.rs"),
        include_str!("../src/profile.rs"),
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
        include_str!("../src/format/mod.rs"),
        include_str!("../src/format/codec.rs"),
        include_str!("../src/format/path.rs"),
        include_str!("../src/object/mod.rs"),
        include_str!("../src/object/model.rs"),
        include_str!("../src/object/encode.rs"),
        include_str!("../src/object/decode.rs"),
        include_str!("../src/object/port_decode.rs"),
        include_str!("../src/object/traversal.rs"),
        include_str!("../src/content/mod.rs"),
        include_str!("../src/content/file.rs"),
        include_str!("../src/content/create.rs"),
        include_str!("../src/content/replace.rs"),
        include_str!("../src/content/update.rs"),
        include_str!("../src/content/read.rs"),
        include_str!("../src/cas/mod.rs"),
        include_str!("../src/cas/port.rs"),
        include_str!("../src/cas/fs.rs"),
        include_str!("../src/cas/admission.rs"),
        include_str!("../src/cas/catalog.rs"),
        include_str!("../src/cas/closure.rs"),
        include_str!("../src/cas/closure_storage.rs"),
        include_str!("../src/cas/locator.rs"),
        include_str!("../src/cas/locator_index.rs"),
        include_str!("../src/cas/operation_admission.rs"),
        include_str!("../src/lifecycle/mod.rs"),
        include_str!("../src/lifecycle/preparation.rs"),
        include_str!("../src/pack/mod.rs"),
        include_str!("../src/pack/complete_writer.rs"),
        include_str!("../src/pack/operation_index.rs"),
        include_str!("../src/read/mod.rs"),
        include_str!("../src/read/extraction.rs"),
        include_str!("../src/read/range.rs"),
        include_str!("../src/read/object_reader.rs"),
        include_str!("../src/cow/mod.rs"),
        include_str!("../src/cow/file.rs"),
        include_str!("../src/cow/tree.rs"),
        include_str!("../src/cow/view.rs"),
        include_str!("../src/cow/mutate.rs"),
        include_str!("../src/bin/c3_qualification.rs"),
    ];
    assert!(
        required_domain_files
            .iter()
            .all(|source| !source.is_empty()),
        "a required domain ownership file is empty"
    );

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for prohibited in ["object.rs", "traversal.rs", "pack.rs", "lifecycle.rs", "c3"] {
        assert!(
            !source_root.join(prohibited).exists(),
            "prohibited hybrid/catch-all source path remains: {prohibited}"
        );
    }
}

#[test]
fn concrete_storage_modules_and_c3_grants_are_not_a_dependent_crate_sdk() {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let fixture = std::env::temp_dir().join(format!(
        "layerfs-l155-private-surface-{}-{sequence:016x}",
        std::process::id()
    ));
    let source_dir = fixture.join("src");
    let target_dir = fixture.join("target");
    fs::create_dir_all(&source_dir).expect("create compile-fail fixture");

    let dependency_path = manifest_dir
        .to_str()
        .expect("storage manifest path must be UTF-8");
    fs::write(
        fixture.join("Cargo.toml"),
        format!(
            "[package]\nname = \"layerfs-l155-private-surface-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nlayerfs-storage = {{ path = {dependency_path:?}, features = [\"c3-polymorphism\"] }}\n"
        ),
    )
    .expect("write compile-fail manifest");
    fs::write(
        source_dir.join("main.rs"),
        r#"
use layerfs_storage::cas::{FsCasV1, FsOperationCapabilityV1};
use layerfs_storage::content::{
    request_c3_create_qualification_v1, run_c3_create_v1,
    C3QualificationCreateGrantV1,
};
use layerfs_storage::cow::CanonicalDirectoryTreeV1;
use layerfs_storage::lifecycle::{C3StorageOperationV1, C3StorageResidentPlanV1};
use layerfs_storage::limits::{OperationReservationV1, ResourceLedgerV1};
use layerfs_storage::pack::SealedPackV1;
use layerfs_storage::read::extraction::C3ReadResultV1;

fn main() {
    let _ = core::mem::size_of::<FsCasV1>();
    let _ = core::mem::size_of::<FsOperationCapabilityV1<'static>>();
    let _ = core::mem::size_of::<C3QualificationCreateGrantV1<'static>>();
    let _ = request_c3_create_qualification_v1::<()>;
    let _ = run_c3_create_v1::<(), ()>;
    let _ = core::mem::size_of::<CanonicalDirectoryTreeV1>();
    let _ = core::mem::size_of::<C3StorageOperationV1<'static>>();
    let _ = core::mem::size_of::<C3StorageResidentPlanV1>();
    let _ = core::mem::size_of::<OperationReservationV1<'static>>();
    let _ = core::mem::size_of::<ResourceLedgerV1>();
    let _ = core::mem::size_of::<SealedPackV1>();
    let _ = core::mem::size_of::<C3ReadResultV1>();
}
"#,
    )
    .expect("write compile-fail source");

    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .arg("check")
        .arg("--offline")
        .arg("--quiet")
        .current_dir(&fixture)
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run dependent-crate compile-fail check");

    let _ = fs::remove_dir_all(&fixture);
    assert!(
        !output.status.success(),
        "dependent crate unexpectedly compiled concrete L1.5.5 storage internals"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E0603"),
        "unexpected compiler failure: {stderr}"
    );
    for private_module in [
        "cas",
        "content",
        "cow",
        "lifecycle",
        "limits",
        "pack",
        "read",
    ] {
        assert!(
            stderr.contains(&format!("module `{private_module}` is private")),
            "compiler did not enforce private {private_module} module: {stderr}"
        );
    }
}

#[test]
fn complete_content_depends_only_on_lifecycle_semantic_ports() {
    let create = include_str!("../src/content/create.rs");
    let production = create
        .split("#[cfg(test)]")
        .next()
        .expect("production content section");
    for forbidden in [
        "crate::cas",
        "crate::pack",
        "FsCas",
        "FsOperationSpool",
        "DirectPack",
        "FileChunkReferenceSpool",
        "FilePackIndexSpool",
        "FileClosureObjectSpool",
        "FileGlobalSeenSpool",
        "pack-index",
        "closure-objects",
        "global-seen",
        "private_pack",
    ] {
        assert!(
            !production.contains(forbidden),
            "content/create.rs crossed a concrete storage boundary: {forbidden}"
        );
    }
    assert_eq!(
        production.matches("run_c3_lifecycle_v1(").count(),
        2,
        "one-file and multi-entry Create must both enter the same lifecycle coordinator"
    );
    for duplicated_terminal in [
        "C3OperationPreparationV1",
        "begin_storage_session_v1",
        "complete_closure_fence_storage_v1",
        ".finish(control)",
    ] {
        assert!(
            !production.contains(duplicated_terminal),
            "content duplicated lifecycle terminal mechanics: {duplicated_terminal}"
        );
    }
}

#[test]
fn storage_mechanics_follow_semantic_module_ownership() {
    let cas = include_str!("../src/cas/mod.rs");
    let cas_fs = include_str!("../src/cas/fs.rs");
    let locator = include_str!("../src/cas/locator.rs");
    let locator_index = include_str!("../src/cas/locator_index.rs");
    let lifecycle = include_str!("../src/lifecycle/mod.rs");
    let pack = include_str!("../src/pack/mod.rs");
    let object = include_str!("../src/object/mod.rs");
    let read = include_str!("../src/read/mod.rs");
    let cow = include_str!("../src/cow/mod.rs");
    let content = include_str!("../src/content/mod.rs");

    for forbidden in ["mod c3_storage;", "mod operation_storage;"] {
        assert!(
            !cas.contains(forbidden),
            "CAS catch-all remains: {forbidden}"
        );
    }
    for owned in [
        "mod closure_storage;",
        "mod locator;",
        "mod locator_index;",
        "mod operation_admission;",
    ] {
        assert!(cas.contains(owned), "missing CAS-owned module: {owned}");
    }
    for owned in ["mod complete_writer;", "mod operation_index;"] {
        assert!(pack.contains(owned), "missing pack-owned module: {owned}");
    }
    for owned in [
        "mod model;",
        "mod encode;",
        "mod decode;",
        "mod port_decode;",
        "mod traversal;",
    ] {
        assert!(
            object.contains(owned),
            "missing object-owned module: {owned}"
        );
    }
    for owned in ["mod extraction;", "mod range;", "mod object_reader;"] {
        assert!(read.contains(owned), "missing read-owned module: {owned}");
    }
    for owned in ["mod file;", "mod tree;", "mod view;", "mod mutate;"] {
        assert!(cow.contains(owned), "missing COW-owned module: {owned}");
    }
    for owned in [
        "mod file;",
        "mod create;",
        "mod replace;",
        "mod update;",
        "mod read;",
    ] {
        assert!(
            content.contains(owned),
            "missing content-owned module: {owned}"
        );
    }
    assert!(lifecycle.contains("mod preparation;"));
    assert!(lifecycle.contains("pub(crate) fn run_c3_lifecycle_v1"));
    assert!(lifecycle.contains("preparation.finish(control)"));

    assert!(locator.contains("b\"LFSOBJ01\""));
    assert!(locator.contains("encode_persistent_locator_v1"));
    assert!(locator.contains("decode_persistent_locator_v1"));
    assert!(locator.contains("matches_binding"));
    for duplicated_persistent_owner in [
        "const OBJECT_LOCATOR_MAGIC",
        "fn encode_object_locator",
        "fn decode_object_locator",
        "struct ObjectLocatorV1",
    ] {
        assert!(
            !cas_fs.contains(duplicated_persistent_owner),
            "cas/fs.rs still owns persistent locator policy: {duplicated_persistent_owner}"
        );
    }
    for forbidden_transient_authority in [
        "LFSOBJ01",
        "encode_persistent_locator",
        "decode_persistent_locator",
        "publish_small_marker",
        "catalog",
    ] {
        assert!(
            !locator_index.contains(forbidden_transient_authority),
            "transient locator index gained publication authority: {forbidden_transient_authority}"
        );
    }
}

#[test]
fn cargo_targets_do_not_reactivate_the_historical_c3_sdk() {
    let manifest = include_str!("../Cargo.toml");
    let lib = include_str!("../src/lib.rs");
    let content = include_str!("../src/content/mod.rs");
    let cas = include_str!("../src/cas/mod.rs");
    let cow = include_str!("../src/cow/mod.rs");
    let create = include_str!("../src/content/create.rs");

    assert!(manifest.contains("autobins = false"));
    assert!(manifest.contains("autotests = false"));
    assert!(!manifest.contains("name = \"c3-qualification\""));
    for module in ["cas", "content", "cow", "limits", "pack"] {
        assert!(lib.contains(&format!("pub(crate) mod {module};")));
        assert!(!lib.contains(&format!("pub mod {module};")));
    }
    assert!(!content.contains("pub use create::*"));
    assert!(!cas.contains("pub use port::*"));
    assert!(!cow.contains("pub use tree::*"));
    for leaked_surface in [
        "pub struct C3QualificationCreateGrantV1",
        "pub fn request_c3_create_qualification_v1",
        "pub fn run_c3_create_v1",
    ] {
        assert!(
            !create.contains(leaked_surface),
            "public C3 surface leaked: {leaked_surface}"
        );
    }
}
