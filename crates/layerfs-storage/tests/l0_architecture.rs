mod support;

fn assert_path_absent(path: &std::path::Path) {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(metadata) => {
            panic!("expected {path:?} to be absent, but metadata succeeded: {metadata:?}")
        }
        Err(error) => panic!("expected {path:?} to be absent, but lookup failed: {error}"),
    }
}

fn section<'a>(manifest: &'a str, name: &str) -> &'a str {
    let header = format!("[{name}]");
    let Some(header_start) = manifest.find(&header) else {
        return "";
    };
    let start = header_start + header.len();
    let rest = &manifest[start..];
    rest.find("\n[").map(|end| &rest[..end]).unwrap_or(rest)
}

fn function_body<'a>(source: &'a str, name: &str) -> &'a str {
    let signature = format!("fn {name}");
    let start = source
        .find(&signature)
        .unwrap_or_else(|| panic!("missing function {name}"));
    let open = source[start..]
        .find('{')
        .map(|offset| start + offset)
        .unwrap_or_else(|| panic!("function {name} has no body"));
    let bytes = source.as_bytes();
    let mut depth = 0_u32;
    let mut index = open;
    let mut line_comment = false;
    let mut block_comment_depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment_depth != 0 {
            if bytes.get(index..index + 2) == Some(b"/*") {
                block_comment_depth += 1;
                index += 2;
            } else if bytes.get(index..index + 2) == Some(b"*/") {
                block_comment_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            index += 1;
            continue;
        }
        if character {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'\'' {
                character = false;
            }
            index += 1;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            line_comment = true;
            index += 2;
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            block_comment_depth = 1;
            index += 2;
            continue;
        }
        match byte {
            b'"' => string = true,
            b'\'' => character = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open + 1..index];
                }
            }
            _ => {}
        }
        index += 1;
    }
    panic!("unterminated function {name}");
}

// Active PB-06 implementation evidence only. This compact map intentionally
// stays beside the architecture seam so the source/test custody is visible in
// the current tree; it is not benchmark, qualification, or final-custody
// evidence.
struct Pb06BoundaryTraceabilityV1 {
    boundary: &'static str,
    source_path: &'static str,
    source_symbol: &'static str,
    test_path: &'static str,
    test_symbol: &'static str,
    assertion_markers: &'static [&'static str],
}

const PB06_BOUNDARY_TRACEABILITY_V1: &[Pb06BoundaryTraceabilityV1] = &[
    Pb06BoundaryTraceabilityV1 {
        boundary: "publication/no-replace",
        source_path: "src/cas/fs.rs",
        source_symbol: "publish_small_marker_controlled",
        test_path: "tests/c3_fscas.rs",
        test_symbol: "partial_multi_object_locator_publication_is_fully_rolled_back",
        assertion_markers: &["FsCasErrorV1::Core(CoreError::Cancelled)", "read_dir"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "incumbent/equality/collision",
        source_path: "src/cas/locator.rs",
        source_symbol: "decide_persistent_locator_install_v1",
        test_path: "tests/c3_fscas.rs",
        test_symbol: "existing_catalog_classifies_valid_binding_and_unequal_incumbents",
        assertion_markers: &[
            "FsCasErrorV1::UnequalOccupant",
            "unreachable_installed_residue_bytes",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "rollback custody",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_admission",
        test_path: "tests/c3_operation.rs",
        test_symbol: "locator_rollback_preserves_directional_unlink_faults_and_dependency_custody",
        assertion_markers: &[
            "FsCasCleanupTargetV1::ObjectLocator",
            "storage_bytes_retained",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "restart/wrong operation-generation-incarnation deletion attempt",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_admission",
        test_path: "src/cas/fs.rs",
        test_symbol: "reopened_incarnation_reusing_numeric_nonce_cannot_rollback_earlier_locator",
        assertion_markers: &["spawn_worker", "assert_eq!(after, before)", "after_usage"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "file-backed maximum probe",
        source_path: "src/cas/locator_index.rs",
        source_symbol: "lookup",
        test_path: "src/cas/locator_index.rs",
        test_symbol: "file_backed_index_reaches_the_real_maximum_collision_probe",
        assertion_markers: &["GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1", "maximum_probe"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "cleanup/residue custody",
        source_path: "src/cas/fs.rs",
        source_symbol: "retain_all_live_v1",
        test_path: "tests/c3_operation.rs",
        test_symbol: "locator_cleanup_unwind_attempts_every_remaining_locator_and_carrier_once",
        assertion_markers: &["locator_cleanup_calls", "storage_bytes_retained"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "invalidation",
        source_path: "src/cas/fs.rs",
        source_symbol: "invalidate_root_controlled_v1",
        test_path: "tests/c3_operation.rs",
        test_symbol:
            "post_link_alias_cleanup_failure_retains_visible_dependencies_and_invalidates_reopen",
        assertion_markers: &["FsCasErrorV1::Invalidated", "storage_inodes_retained"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "cross-carrier lookup",
        source_path: "src/cas/fs.rs",
        source_symbol: "gather_object_locator_incumbent_evidence",
        test_path: "tests/c3_fscas.rs",
        test_symbol:
            "cross_carrier_object_validation_read_failures_are_typed_and_cleanup_the_candidate",
        assertion_markers: &["CarrierObjectRead", "unreachable_installed_residue_bytes"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "simultaneous same-key publication",
        source_path: "src/cas/fs.rs",
        source_symbol: "publish_small_marker_controlled",
        test_path: "tests/c3_fscas.rs",
        test_symbol: "simultaneous_reopened_pack_callers_publish_one_canonical_shared_locator",
        assertion_markers: &["shared_id", "FsPackAdmissionOutcomeV1::Installed"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "locator prepare/install/revalidate/cleanup faults",
        source_path: "src/cas/fs.rs",
        source_symbol: "install_object_locators",
        test_path: "tests/c3_fscas.rs",
        test_symbol: "every_fresh_admission_boundary_cleans_or_counts_exact_residue",
        assertion_markers: &[
            "AfterObjectLocatorPublication",
            "AfterCatalogPublication",
            "unreachable_installed_residue_bytes",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "post-validation locator replacement",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_admission",
        test_path: "src/cas/fs.rs",
        test_symbol: "locator_rollback_rejects_foreign_replacement_after_final_validation",
        assertion_markers: &["control.replaced", "held-locator-during-rollback"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "post-validation carrier replacement",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_carrier",
        test_path: "src/cas/fs.rs",
        test_symbol: "carrier_rollback_rejects_foreign_replacement_after_final_validation",
        assertion_markers: &["control.replaced", "held-carrier-during-rollback"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "catalog adoption before rollback",
        source_path: "src/cas/fs.rs",
        source_symbol: "rollback_unpublished_admission",
        test_path: "src/cas/fs.rs",
        test_symbol: "catalog_adoption_before_rollback_retains_the_complete_dependency_chain",
        assertion_markers: &[
            "control.adopted",
            "decode_catalog_marker",
            "exact_namespace_usage",
        ],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "locator revalidation before visibility",
        source_path: "src/cas/fs.rs",
        source_symbol: "revalidate_active_pack_marker_incumbent_controlled_v1",
        test_path: "tests/c3_fscas.rs",
        test_symbol: "post_comparison_locator_path_replacement_fails_before_catalog_publication",
        assertion_markers: &["catalog", "read_dir", "Integrity"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "exact locator rollback policy",
        source_path: "src/cas/locator.rs",
        source_symbol: "decide_persistent_locator_rollback_v1",
        test_path: "src/cas/locator.rs",
        test_symbol: "locator_rollback_policy_requires_exact_receipt_and_current_operation",
        assertion_markers: &["Authorized", "Foreign", "snapshot_matches"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "combined frozen byte compatibility",
        source_path: "src/cas/fs.rs",
        source_symbol: "frozen_compatibility_all_five_byte_domains_round_trip_and_hash_exactly",
        test_path: "src/cas/fs.rs",
        test_symbol: "frozen_compatibility_all_five_byte_domains_round_trip_and_hash_exactly",
        assertion_markers: &[
            "read_generation_marker",
            "decode_existing_root_owner",
            "digest_hex",
        ],
    },
];

fn test_is_registered(
    source: &str,
    registration_source: &str,
    test_path: &str,
    test_symbol: &str,
) -> bool {
    let signature = format!("fn {test_symbol}");
    let Some(start) = source.find(&signature) else {
        return false;
    };
    let prefix = &source[..start];
    let Some(attribute) = prefix.rfind("#[test]") else {
        return false;
    };
    !prefix[attribute + "#[test]".len()..].contains("fn ")
        && (test_path.starts_with("src/")
            || registration_source.contains(&format!("#[path = \"../{test_path}\"]")))
}

#[test]
fn pb06_boundary_to_test_traceability_is_explicit_and_current() {
    let source_files = [
        ("src/lib.rs", include_str!("../src/lib.rs")),
        ("src/cas/fs.rs", include_str!("../src/cas/fs.rs")),
        ("src/cas/locator.rs", include_str!("../src/cas/locator.rs")),
        (
            "src/cas/locator_index.rs",
            include_str!("../src/cas/locator_index.rs"),
        ),
    ];
    let test_files = [
        ("tests/c3_fscas.rs", include_str!("c3_fscas.rs")),
        ("tests/c3_operation.rs", include_str!("c3_operation.rs")),
        ("src/cas/fs.rs", include_str!("../src/cas/fs.rs")),
        ("src/cas/locator.rs", include_str!("../src/cas/locator.rs")),
        (
            "src/cas/locator_index.rs",
            include_str!("../src/cas/locator_index.rs"),
        ),
    ];
    let registration_source = include_str!("../src/lib.rs");

    assert_eq!(
        PB06_BOUNDARY_TRACEABILITY_V1.len(),
        16,
        "the PB-06 map must retain one explicit row for every required proof boundary"
    );
    for row in PB06_BOUNDARY_TRACEABILITY_V1 {
        let source = source_files
            .iter()
            .find(|(path, _)| *path == row.source_path)
            .map(|(_, content)| *content)
            .unwrap_or_else(|| panic!("missing mapped source file {}", row.source_path));
        function_body(source, row.source_symbol);

        let tests = test_files
            .iter()
            .find(|(path, _)| *path == row.test_path)
            .map(|(_, content)| *content)
            .unwrap_or_else(|| panic!("missing mapped test file {}", row.test_path));
        let test_body = function_body(tests, row.test_symbol);
        assert!(
            test_is_registered(tests, registration_source, row.test_path, row.test_symbol,),
            "PB-06 boundary {} names an unregistered test {} in {}",
            row.boundary,
            row.test_symbol,
            row.test_path
        );
        for marker in row.assertion_markers {
            assert!(
                test_body.contains(marker),
                "PB-06 boundary {} lost assertion marker {} in {}::{}",
                row.boundary,
                marker,
                row.test_path,
                row.test_symbol
            );
        }
    }
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
        assert_path_absent(&source_root.join(prohibited));
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
            "[package]\nname = \"layerfs-l155-private-surface-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nlayerfs-storage = {{ path = {dependency_path:?}, features = [\"operation-polymorphism\"] }}\n"
        ),
    )
    .expect("write compile-fail manifest");
    fs::write(
        source_dir.join("main.rs"),
        r#"
use layerfs_storage::cas::{FsCasV1, FsOperationCapabilityV1};
use layerfs_storage::content::{
    request_create_operation_v1, run_create_v1,
    CreateOperationGrantV1,
};
use layerfs_storage::cow::CanonicalDirectoryTreeV1;
use layerfs_storage::lifecycle::{StorageOperationV1, StorageResidentPlanV1};
use layerfs_storage::limits::{OperationReservationV1, ResourceLedgerV1};
use layerfs_storage::pack::SealedPackV1;
use layerfs_storage::read::extraction::ReadResultV1;

fn main() {
    let _ = core::mem::size_of::<FsCasV1>();
    let _ = core::mem::size_of::<FsOperationCapabilityV1<'static>>();
    let _ = core::mem::size_of::<CreateOperationGrantV1<'static>>();
    let _ = request_create_operation_v1::<()>;
    let _ = run_create_v1::<(), ()>;
    let _ = core::mem::size_of::<CanonicalDirectoryTreeV1>();
    let _ = core::mem::size_of::<StorageOperationV1<'static>>();
    let _ = core::mem::size_of::<StorageResidentPlanV1>();
    let _ = core::mem::size_of::<OperationReservationV1<'static>>();
    let _ = core::mem::size_of::<ResourceLedgerV1>();
    let _ = core::mem::size_of::<SealedPackV1>();
    let _ = core::mem::size_of::<ReadResultV1>();
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
        production.matches("run_lifecycle_v1(").count(),
        2,
        "one-file and multi-entry Create must both enter the same lifecycle coordinator"
    );
    for duplicated_terminal in [
        "OperationPreparationV1",
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
    assert!(
        cas_fs.contains("read_sealed_pack_shape_v1(&mut reader)"),
        "cas/fs.rs must delegate sealed-pack shape decoding to pack"
    );
    assert!(
        !cas_fs.contains("fn read_sealed_shape("),
        "cas/fs.rs still owns sealed-pack shape decoding"
    );
    for duplicated_pack_layout in ["header[48..52]", "header[56..64]", "len - 32"] {
        assert!(
            !cas_fs.contains(duplicated_pack_layout),
            "cas/fs.rs duplicated pack-owned layout: {duplicated_pack_layout}"
        );
    }
    let sealed_shape = function_body(pack, "read_sealed_pack_shape_v1");
    for required in [
        "PACK_HEADER_BYTES + PACK_TRAILER_BYTES",
        "pack.len().map_err(map_read_port)",
        "pack.read_exact_at(0, &mut header)",
        "be_u32(&header[48..52])",
        "be_u64(&header[56..64])",
        "checked_sub(DIGEST_BYTES as u64)",
        "SealedPackV1::from_validated_parts",
    ] {
        assert!(
            sealed_shape.contains(required),
            "pack sealed-shape decoder lacks owned semantics: {required}"
        );
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
    assert!(lifecycle.contains("pub(crate) fn run_lifecycle_v1"));
    assert!(lifecycle.contains("preparation.finish(control)"));

    assert!(locator.contains("b\"LFSOBJ01\""));
    assert!(locator.contains("encode_persistent_locator_v1"));
    assert!(locator.contains("decode_persistent_locator_v1"));
    assert!(locator.contains("locator_transaction_tag_v1"));
    assert!(locator.contains("PersistentLocatorPublicationEvidenceV1"));
    assert!(locator.contains("PersistentLocatorPublicationDecisionV1"));
    assert!(locator.contains("decide_persistent_locator_publication_v1"));
    assert!(locator.contains("PersistentLocatorBindingEvidenceV1"));
    assert!(locator.contains("PersistentLocatorBindingDecisionV1"));
    assert!(locator.contains("decide_persistent_locator_binding_v1"));
    assert!(locator.contains("PersistentLocatorCatalogBindingDecisionV1"));
    assert!(locator.contains("decide_persistent_locator_catalog_binding_v1"));
    assert!(locator.contains("PersistentLocatorIncumbentEvidenceV1"));
    assert!(locator.contains("PersistentLocatorIncumbentDecisionV1"));
    assert!(locator.contains("decide_persistent_locator_incumbent_v1"));
    assert!(locator.contains("PersistentCatalogIncumbentDecisionV1"));
    assert!(locator.contains("decide_persistent_catalog_incumbent_v1"));
    for forbidden in ["std::fs", "OpenOptions", "hard_link", "lock_visibility"] {
        assert!(
            !locator.contains(forbidden),
            "locator policy module crossed into filesystem mechanics: {forbidden}"
        );
    }

    let publication_decision = function_body(locator, "decide_persistent_locator_publication_v1");
    for required in [
        "evidence.locator.transaction() == evidence.transaction",
        "evidence.locator.sealed() == evidence.sealed",
        "evidence.locator.entry() == evidence.entry",
        "PersistentLocatorPublicationDecisionV1::Authenticated",
        "PersistentLocatorPublicationDecisionV1::Foreign",
    ] {
        assert!(
            publication_decision.contains(required),
            "locator publication decision lacks substantive custody policy: {required}"
        );
    }
    let binding_decision = function_body(locator, "decide_persistent_locator_binding_v1");
    for required in [
        "locator.sealed == catalog",
        "locator.entry == indexed",
        "PersistentLocatorBindingDecisionV1::Authenticated",
        "PersistentLocatorBindingDecisionV1::Collision",
    ] {
        assert!(
            binding_decision.contains(required),
            "locator binding decision lacks substantive policy: {required}"
        );
    }
    let catalog_binding_decision =
        function_body(locator, "decide_persistent_locator_catalog_binding_v1");
    for required in [
        "locator.sealed == catalog",
        "PersistentLocatorCatalogBindingDecisionV1::Authenticated",
        "PersistentLocatorCatalogBindingDecisionV1::Collision",
    ] {
        assert!(
            catalog_binding_decision.contains(required),
            "catalog binding decision lacks substantive policy: {required}"
        );
    }
    let incumbent_decision = function_body(locator, "decide_persistent_locator_incumbent_v1");
    for required in [
        "decide_persistent_locator_binding_v1",
        "same_object_identity_v1",
        "evidence.object_bytes_equal",
        "PersistentLocatorIncumbentDecisionV1::EqualReuse",
        "PersistentLocatorIncumbentDecisionV1::BindingCollision",
        "PersistentLocatorIncumbentDecisionV1::UnequalObject",
    ] {
        assert!(
            incumbent_decision.contains(required),
            "locator incumbent decision lacks substantive policy: {required}"
        );
    }
    let object_identity = function_body(locator, "same_object_identity_v1");
    for required in [".id()", ".object_len()", ".object_checksum()", "&&"] {
        assert!(
            object_identity.contains(required),
            "locator object identity policy is too thin: {required}"
        );
    }
    let catalog_incumbent = function_body(locator, "decide_persistent_catalog_incumbent_v1");
    for required in [
        "incumbent.id() != expected.id()",
        "incumbent == expected",
        "PersistentCatalogIncumbentDecisionV1::Authenticated",
        "PersistentCatalogIncumbentDecisionV1::Collision",
        "PersistentCatalogIncumbentDecisionV1::Unequal",
    ] {
        assert!(
            catalog_incumbent.contains(required),
            "catalog incumbent decision lacks substantive policy: {required}"
        );
    }

    assert!(cas_fs.contains("gather_object_locator_incumbent_evidence"));
    assert!(cas_fs.contains("decide_persistent_locator_install_v1"));
    assert!(cas_fs.contains("PersistentLocatorInstallObservationV1::Incumbent"));
    assert!(cas_fs.contains("map_persistent_locator_install_decision_v1"));
    assert!(cas_fs.contains("PersistentLocatorIncumbentEvidenceV1::new"));
    let gather_incumbent = function_body(cas_fs, "gather_object_locator_incumbent_evidence");
    for required in [
        "open_occupant",
        "locate_validated_pack_index_entry_controlled_v1",
        "validate_validated_pack_object_controlled_v1",
        "compare_complete_object_bytes",
        "revalidate_immutable_file_snapshot_v1",
        "PersistentLocatorIncumbentEvidenceV1::new",
    ] {
        assert!(
            gather_incumbent.contains(required),
            "fs evidence gatherer does not own the required physical authentication step: {required}"
        );
    }
    for forbidden in [
        "same_object_identity_v1",
        "PersistentLocatorIncumbentDecisionV1::EqualReuse",
        "locator.sealed == catalog",
        "locator.entry == indexed",
    ] {
        assert!(
            !gather_incumbent.contains(forbidden),
            "fs evidence gatherer reimplemented locator policy: {forbidden}"
        );
    }
    let install_locators = function_body(cas_fs, "install_object_locators");
    for required in [
        "gather_object_locator_incumbent_evidence",
        "decode_persistent_locator_for_install_v1",
        "decide_persistent_locator_install_v1",
        "PersistentLocatorInstallObservationV1::Incumbent",
        "map_persistent_locator_install_decision_v1",
    ] {
        assert!(
            install_locators.contains(required),
            "fs installation path does not delegate locator meaning through the typed seam: {required}"
        );
    }
    assert!(!install_locators.contains("same_object_identity_v1"));
    for forbidden in [
        "decide_persistent_locator_incumbent_v1",
        "PersistentLocatorIncumbentDecisionV1::EqualReuse",
        "PersistentLocatorIncumbentDecisionV1::BindingCollision",
        "PersistentLocatorIncumbentDecisionV1::UnequalObject",
        "PersistentLocatorBindingDecisionV1::Authenticated",
        "PersistentLocatorBindingDecisionV1::Collision",
        "receipt.locator.transaction()",
        "decode_persistent_locator_v1",
        "map_persistent_locator_codec_error_v1",
    ] {
        assert!(
            !install_locators.contains(forbidden),
            "fs installation path still interprets locator policy directly: {forbidden}"
        );
    }
    let install_decision = function_body(locator, "decide_persistent_locator_install_v1");
    for required in [
        "decide_persistent_locator_incumbent_v1",
        "PersistentLocatorInstallDecisionV1::Installed",
        "PersistentLocatorInstallDecisionV1::EqualReuse",
        "PersistentLocatorInstallDecisionV1::BindingCollision",
        "PersistentLocatorInstallDecisionV1::UnequalObject",
    ] {
        assert!(
            install_decision.contains(required),
            "locator install decision lacks substantive policy: {required}"
        );
    }
    let decode_install = function_body(locator, "decode_persistent_locator_for_install_v1");
    for required in [
        "decode_persistent_locator_v1",
        "PersistentLocatorCodecErrorV1::Malformed",
        "PersistentLocatorCodecErrorV1::BindingMismatch",
        "PersistentLocatorInstallDecisionV1::Malformed",
        "PersistentLocatorInstallDecisionV1::BindingCollision",
    ] {
        assert!(
            decode_install.contains(required),
            "locator install decoder does not own codec classification: {required}"
        );
    }
    let decode_receipt = function_body(cas_fs, "decode_locator_publication_receipt_v1");
    assert!(
        decode_receipt.contains("decode_persistent_locator_self_describing_v1"),
        "receipt decoder must delegate persistent locator binding interpretation"
    );
    for forbidden in [
        "PhysicalObjectKindV1",
        "from_kind_and_digest",
        "locator_bytes[8]",
        "locator_bytes[16..48]",
    ] {
        assert!(
            !decode_receipt.contains(forbidden),
            "receipt decoder duplicated persistent locator layout: {forbidden}"
        );
    }
    let self_describing = function_body(locator, "decode_persistent_locator_self_describing_v1");
    for required in [
        "PhysicalObjectKindV1::try_from(bytes[8])",
        "bytes[16..48]",
        "TypedPhysicalObjectIdV1::from_kind_and_digest",
        "decode_persistent_locator_v1",
    ] {
        assert!(
            self_describing.contains(required),
            "locator self-describing decoder lacks owned binding interpretation: {required}"
        );
    }
    let map_install = function_body(cas_fs, "map_persistent_locator_install_decision_v1");
    for required in [
        "PersistentLocatorInstallDecisionV1::Installed",
        "PersistentLocatorInstallDecisionV1::EqualReuse",
        "PersistentLocatorInstallDecisionV1::BindingCollision",
        "PersistentLocatorInstallDecisionV1::UnequalObject",
        "map_persistent_locator_install_error_v1",
    ] {
        assert!(
            map_install.contains(required),
            "fs adapter does not map the full locator-owned install decision: {required}"
        );
    }
    let rollback = function_body(cas_fs, "rollback_unpublished_admission");
    for required in [
        "PersistentLocatorRollbackEvidenceV1::new",
        "decide_persistent_locator_rollback_v1",
        "PersistentLocatorRollbackDecisionV1::Authorized",
        "revalidate_immutable_file_snapshot_v1",
        "fs::remove_file(&path)",
    ] {
        assert!(
            rollback.contains(required),
            "rollback does not authenticate exact locator publication custody: {required}"
        );
    }
    for substantive_locator_policy in [
        "validate_and_compare_object_locator",
        "classify_persistent_locator_binding_v1",
        "persistent_locator_matches_catalog_v1",
        "persistent_locator_matches_index_entry_v1",
        "if locator.sealed == catalog",
        "if locator.entry == indexed",
        "if locator.entry == candidate_entry",
        "if !object_bytes_equal",
    ] {
        assert!(
            !cas_fs.contains(substantive_locator_policy),
            "cas/fs.rs still reimplements locator-owned policy: {substantive_locator_policy}"
        );
    }
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
fn historical_c3_source_is_immutable_and_not_a_current_target() {
    let manifest = include_str!("../Cargo.toml");
    let lib = include_str!("../src/lib.rs");
    let content = include_str!("../src/content/mod.rs");
    let cas = include_str!("../src/cas/mod.rs");
    let cow = include_str!("../src/cow/mod.rs");
    let create = include_str!("../src/content/create.rs");
    let historical_source = include_bytes!("../src/bin/c3_qualification.rs");

    assert!(manifest.contains("autobins = false"));
    assert!(manifest.contains("autotests = false"));
    assert!(!manifest.contains("name = \"c3-qualification\""));
    assert_eq!(historical_source.len(), 49_821);
    let digest = support::sha256(historical_source)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        digest,
        "0f6f731e366a4802cac801ceacf8cb75d75296494f49c036ac368fcf31ca7da6"
    );
    for module in ["cas", "content", "cow", "limits", "pack"] {
        assert!(lib.contains(&format!("pub(crate) mod {module};")));
        assert!(!lib.contains(&format!("pub mod {module};")));
    }
    assert!(!content.contains("pub use create::*"));
    assert!(!cas.contains("pub use port::*"));
    assert!(!cow.contains("pub use tree::*"));
    for leaked_surface in [
        "pub struct CreateOperationGrantV1",
        "pub fn request_create_operation_v1",
        "pub fn run_create_v1",
    ] {
        assert!(
            !create.contains(leaked_surface),
            "public C3 surface leaked: {leaked_surface}"
        );
    }
}
