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

fn relative_files(root: &std::path::Path) -> Vec<String> {
    fn visit(root: &std::path::Path, current: &std::path::Path, files: &mut Vec<String>) {
        let mut entries = std::fs::read_dir(current)
            .unwrap_or_else(|error| panic!("read directory {current:?}: {error}"))
            .map(|entry| entry.unwrap_or_else(|error| panic!("read directory entry: {error}")))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if entry
                .file_type()
                .unwrap_or_else(|error| panic!("read file type for {path:?}: {error}"))
                .is_dir()
            {
                visit(root, &path, files);
            } else {
                files.push(
                    path.strip_prefix(root)
                        .unwrap_or_else(|error| panic!("strip {path:?}: {error}"))
                        .to_string_lossy()
                        .replace(std::path::MAIN_SEPARATOR, "/"),
                );
            }
        }
    }

    let mut files = Vec::new();
    visit(root, root, &mut files);
    files.sort();
    files
}

fn cargo_test_targets(manifest: &str) -> Vec<(String, String, Option<String>)> {
    let mut blocks = Vec::new();
    let mut current = None;
    for line in manifest.lines() {
        if line.trim() == "[[test]]" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            current = Some(String::new());
        } else if line.starts_with("[[") || line.starts_with("[") {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
        } else if let Some(block) = current.as_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }

    fn field(block: &str, name: &str) -> String {
        block
            .lines()
            .find_map(|line| {
                let (key, value) = line.trim().split_once('=')?;
                (key.trim() == name).then(|| value.trim().trim_matches('"').to_owned())
            })
            .unwrap_or_default()
    }

    blocks
        .into_iter()
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let required_features = block.lines().find_map(|line| {
                let (key, value) = line.trim().split_once('=')?;
                (key.trim() == "required-features").then(|| value.trim().to_owned())
            });
            (
                field(&block, "name"),
                field(&block, "path"),
                required_features,
            )
        })
        .collect()
}

fn source_char_literal_start(bytes: &[u8], index: usize) -> bool {
    if bytes.get(index) != Some(&b'\'') {
        return false;
    }
    let Some(&next) = bytes.get(index + 1) else {
        return false;
    };
    next == b'\\'
        || bytes.get(index + 2) == Some(&b'\'')
        || (!next.is_ascii_alphanumeric() && next != b'_')
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
            b'\'' if source_char_literal_start(bytes, index) => character = true,
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

fn skip_source_trivia(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes.get(index..index + 2) == Some(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index += 2;
            let mut depth = 1_u32;
            while index < bytes.len() && depth != 0 {
                if bytes.get(index..index + 2) == Some(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes.get(index..index + 2) == Some(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else {
            break;
        }
    }
    index
}

fn source_attribute_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = start;
    let mut depth = 0_u32;
    let mut string = false;
    let mut character = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
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
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    panic!("unterminated source attribute");
}

fn cfg_test_item_end(source: &str, attribute_end: usize) -> usize {
    let mut item_start = skip_source_trivia(source, attribute_end);
    while source.as_bytes().get(item_start..item_start + 2) == Some(b"#[") {
        item_start = source_attribute_end(source, item_start + 1);
        item_start = skip_source_trivia(source, item_start);
    }

    let bytes = source.as_bytes();
    let mut index = item_start;
    let mut braces = 0_u32;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
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
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'(' => parentheses += 1,
            b')' => parentheses -= 1,
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            b'{' => braces += 1,
            b'}' => {
                braces -= 1;
                if braces == 0 && parentheses == 0 && brackets == 0 {
                    let after_body = skip_source_trivia(source, index + 1);
                    return if bytes.get(after_body) == Some(&b';') {
                        after_body + 1
                    } else {
                        index + 1
                    };
                }
            }
            b';' if braces == 0 && parentheses == 0 && brackets == 0 => return index + 1,
            _ => {}
        }
        index += 1;
    }
    panic!(
        "unterminated cfg(test) item at {item_start}: {}",
        source[item_start..]
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
    );
}

fn final_test_segments(source: &str) -> Vec<(String, usize, &str)> {
    let starts = source
        .match_indices("#[test]")
        .filter_map(|(index, _)| {
            let line_start = source[..index].rfind('\n').map_or(0, |offset| offset + 1);
            let line_end = source[index..]
                .find('\n')
                .map_or(source.len(), |offset| index + offset);
            (source[line_start..line_end].trim() == "#[test]").then_some(index)
        })
        .collect::<Vec<_>>();

    starts
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = starts.get(index + 1).copied().unwrap_or(source.len());
            let after_attribute = source[start..]
                .find('\n')
                .map_or(start + "#[test]".len(), |offset| start + offset + 1);
            let name = source[after_attribute..end]
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("#[") {
                        return None;
                    }
                    let rest = trimmed.strip_prefix("fn ")?;
                    Some(
                        rest.split_once('(')
                            .map(|(name, _)| name.trim().to_owned())
                            .unwrap_or_else(|| {
                                panic!("test attribute at byte {start} has no function name")
                            }),
                    )
                })
                .next()
                .unwrap_or_else(|| panic!("test attribute at byte {start} has no function"));
            (name, start, &source[start..end])
        })
        .collect()
}

fn feature_gated_test_names(source: &str) -> std::collections::BTreeSet<String> {
    let declarations = final_test_segments(source);
    let mut gated = std::collections::BTreeSet::new();
    for (attribute_start, _) in source.match_indices(
        "#[cfg(feature = \"operation-polymorphism\")]",
    ) {
        let attribute_end = source_attribute_end(source, attribute_start + 1);
        let item_end = cfg_test_item_end(source, attribute_end);
        for (name, test_start, _) in &declarations {
            if *test_start > attribute_start && *test_start < item_end {
                gated.insert(name.clone());
            }
        }
    }
    gated
}

fn assertion_tokens(source: &str) -> Vec<&'static str> {
    let mut found = [
        ("assert_eq!", "assert_eq!"),
        ("assert_ne!", "assert_ne!"),
        ("assert!", "assert!"),
        ("matches!", "matches!"),
    ]
    .into_iter()
    .flat_map(|(needle, token)| {
        source.match_indices(needle).filter_map(move |(index, _)| {
            let before_is_identifier = source[..index]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_');
            let after = index + needle.len();
            let open = source[after..]
                .find(|character: char| !character.is_ascii_whitespace())
                .map(|offset| after + offset);
            (!before_is_identifier && open.is_some_and(|index| source.as_bytes()[index] == b'('))
                .then_some((index, token))
        })
    })
    .collect::<Vec<_>>();
    found.sort_by_key(|(index, _)| *index);
    found.into_iter().map(|(_, token)| token).collect()
}

fn digest_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    support::sha256(bytes)
        .into_iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 0x0f)]),
            ]
        })
        .collect()
}

fn production_source_v1(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut copy_start = 0_usize;
    let mut index = 0_usize;
    let mut braces = 0_u32;
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
        if braces == 0 && bytes.get(index..index + 6) == Some(b"#[cfg(") {
            let attribute_end = source_attribute_end(source, index + 1);
            let attribute = &source[index..attribute_end];
            if attribute.contains("test") || attribute.contains("operation-polymorphism") {
                output.push_str(&source[copy_start..index]);
                index = cfg_test_item_end(source, attribute_end);
                copy_start = index;
                continue;
            }
        }
        match byte {
            b'"' => string = true,
            b'\'' if source_char_literal_start(bytes, index) => character = true,
            b'{' => braces += 1,
            b'}' => {
                assert!(braces != 0, "unbalanced production source at byte {index}");
                braces -= 1;
            }
            _ => {}
        }
        index += 1;
    }
    output.push_str(&source[copy_start..]);
    output
}

fn forbidden_content_import_v1(source: &str) -> Option<&'static str> {
    const FORBIDDEN: &[&str] = &[
        "crate::cas",
        "crate::pack",
        "crate::lifecycle",
        "std::fs",
        "std::path",
        "FsCas",
        "FsOperationSpool",
        "FileClosureObjectSpool",
        "FileGlobalSeenSpool",
        "FilePackIndexSpool",
        "CompletedPackSetV1",
        "FsPrivatePack",
        "FsCarrier",
        "FsLocator",
        "FsCatalog",
        "CatalogMarker",
        "LocatorIndex",
        "OperationPreparationV1",
        "PreparationV1",
        "DirectPack",
        "hard_link",
    ];
    let mut import = String::new();
    let mut in_import = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !in_import
            && (trimmed.starts_with("use ")
                || trimmed.starts_with("pub use ")
                || trimmed.starts_with("pub(crate) use "))
        {
            import.clear();
            in_import = true;
        }
        if in_import {
            import.push_str(trimmed);
            if trimmed.contains(';') {
                if let Some(forbidden) = FORBIDDEN.iter().find(|value| import.contains(**value)) {
                    return Some(forbidden);
                }
                in_import = false;
            }
        }
    }
    None
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
        test_path: "tests/cas_admission.rs",
        test_symbol: "partial_multi_object_locator_publication_is_fully_rolled_back",
        assertion_markers: &["PublicationErrorV1::Core(CoreError::Cancelled)", "read_dir"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "incumbent/equality/collision",
        source_path: "src/cas/locator.rs",
        source_symbol: "decide_persistent_locator_install_v1",
        test_path: "tests/cas_admission.rs",
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
        test_path: "tests/operation_lifecycle.rs",
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
        test_path: "tests/operation_lifecycle.rs",
        test_symbol: "locator_cleanup_unwind_attempts_every_remaining_locator_and_carrier_once",
        assertion_markers: &["locator_cleanup_calls", "storage_bytes_retained"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "invalidation",
        source_path: "src/cas/fs.rs",
        source_symbol: "invalidate_root_controlled_v1",
        test_path: "tests/operation_lifecycle.rs",
        test_symbol:
            "post_link_alias_cleanup_failure_retains_visible_dependencies_and_invalidates_reopen",
        assertion_markers: &["FsCasErrorV1::Invalidated", "storage_inodes_retained"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "cross-carrier lookup",
        source_path: "src/cas/fs.rs",
        source_symbol: "gather_object_locator_incumbent_evidence",
        test_path: "tests/operation_faults.rs",
        test_symbol:
            "cross_carrier_object_validation_read_failures_are_typed_and_cleanup_the_candidate",
        assertion_markers: &["CarrierObjectRead", "unreachable_installed_residue_bytes"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "simultaneous same-key publication",
        source_path: "src/cas/fs.rs",
        source_symbol: "publish_small_marker_controlled",
        test_path: "tests/operation_concurrency.rs",
        test_symbol: "simultaneous_reopened_pack_callers_publish_one_canonical_shared_locator",
        assertion_markers: &["shared_id", "FsPackAdmissionOutcomeV1::Installed"],
    },
    Pb06BoundaryTraceabilityV1 {
        boundary: "locator prepare/install/revalidate/cleanup faults",
        source_path: "src/cas/fs.rs",
        source_symbol: "install_object_locators",
        test_path: "tests/cas_admission.rs",
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
        test_path: "tests/cas_admission.rs",
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

fn test_is_registered(source: &str, manifest: &str, test_path: &str, test_symbol: &str) -> bool {
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
            || cargo_test_targets(manifest)
                .iter()
                .any(|(_, path, _)| path == test_path))
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
        ("tests/cas_admission.rs", include_str!("cas_admission.rs")),
        (
            "tests/operation_concurrency.rs",
            include_str!("operation_concurrency.rs"),
        ),
        (
            "tests/operation_faults.rs",
            include_str!("operation_faults.rs"),
        ),
        (
            "tests/operation_lifecycle.rs",
            include_str!("operation_lifecycle.rs"),
        ),
        ("src/cas/fs.rs", include_str!("../src/cas/fs.rs")),
        ("src/cas/locator.rs", include_str!("../src/cas/locator.rs")),
        (
            "src/cas/locator_index.rs",
            include_str!("../src/cas/locator_index.rs"),
        ),
    ];
    let registration_source = include_str!("../Cargo.toml");

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
fn pb08_final_custody_tree_and_targets_are_exact() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let expected_source_files = [
        "bin/c3_qualification.rs",
        "cas/admission.rs",
        "cas/catalog.rs",
        "cas/closure.rs",
        "cas/closure_storage.rs",
        "cas/fs.rs",
        "cas/locator.rs",
        "cas/locator_index.rs",
        "cas/mod.rs",
        "cas/operation_admission.rs",
        "cas/port.rs",
        "cdc/engine.rs",
        "cdc/fastcdc/gear.rs",
        "cdc/fastcdc/mod.rs",
        "cdc/fastcdc/rejoin.rs",
        "cdc/fastcdc/scanner.rs",
        "cdc/mod.rs",
        "cdc/resync.rs",
        "cdc/seqcdc/mod.rs",
        "cdc/seqcdc/rejoin.rs",
        "cdc/seqcdc/scanner.rs",
        "content/create.rs",
        "content/file.rs",
        "content/mod.rs",
        "content/read.rs",
        "content/replace.rs",
        "content/update.rs",
        "cow/file.rs",
        "cow/mod.rs",
        "cow/mutate.rs",
        "cow/tree.rs",
        "cow/view.rs",
        "error.rs",
        "format/codec.rs",
        "format/mod.rs",
        "format/path.rs",
        "identity/framing.rs",
        "identity/logical.rs",
        "identity/mod.rs",
        "identity/physical.rs",
        "lib.rs",
        "lifecycle/mod.rs",
        "lifecycle/preparation.rs",
        "limits.rs",
        "object/decode.rs",
        "object/encode.rs",
        "object/mod.rs",
        "object/model.rs",
        "object/port_decode.rs",
        "object/traversal.rs",
        "pack/complete_writer.rs",
        "pack/mod.rs",
        "pack/operation_index.rs",
        "profile.rs",
        "read/extraction.rs",
        "read/mod.rs",
        "read/object_reader.rs",
        "read/range.rs",
    ];
    let mut expected_source_files = expected_source_files
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    expected_source_files.sort();
    assert_eq!(
        relative_files(&source_root),
        expected_source_files,
        "PB-08 production custody tree changed"
    );

    let tests_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let expected_test_files = [
        "cas_admission.rs",
        "cow_locality.rs",
        "fixtures/c3-registry-v1.tsv",
        "l0_architecture.rs",
        "operation_concurrency.rs",
        "operation_create.rs",
        "operation_faults.rs",
        "operation_lifecycle.rs",
        "operation_mutation.rs",
        "operation_read.rs",
        "reference/naive_fastcdc.rs",
        "support/counting_sink.rs",
        "support/counting_source.rs",
        "support/fault_injection.rs",
        "support/mod.rs",
        "support/temp_fs_cas.rs",
    ];
    let mut expected_test_files = expected_test_files
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    expected_test_files.sort();
    assert_eq!(
        relative_files(&tests_root),
        expected_test_files,
        "PB-08 integration-test custody tree changed"
    );

    let manifest = include_str!("../Cargo.toml");
    assert!(manifest.contains("autobins = false"));
    assert!(manifest.contains("autotests = false"));
    assert_eq!(manifest.matches("[[test]]").count(), 9);
    assert_eq!(manifest.matches("[[bin]]").count(), 0);
    let expected_targets = [
        ("l0_architecture", "tests/l0_architecture.rs", None),
        ("operation_create", "tests/operation_create.rs", None),
        ("cas_admission", "tests/cas_admission.rs", None),
        ("operation_lifecycle", "tests/operation_lifecycle.rs", None),
        (
            "operation_concurrency",
            "tests/operation_concurrency.rs",
            None,
        ),
        ("operation_faults", "tests/operation_faults.rs", None),
        ("operation_mutation", "tests/operation_mutation.rs", None),
        ("operation_read", "tests/operation_read.rs", None),
        ("cow_locality", "tests/cow_locality.rs", None),
    ];
    let actual_targets = cargo_test_targets(manifest);
    assert_eq!(actual_targets.len(), expected_targets.len());
    for ((name, path, required), (expected_name, expected_path, expected_required)) in
        actual_targets.iter().zip(expected_targets)
    {
        assert_eq!(name, expected_name);
        assert_eq!(path, expected_path);
        assert_eq!(required.as_deref(), expected_required);
    }
    for forbidden in [
        "c3_fscas",
        "c3_operation",
        "c3_mutation",
        "c3_seqcdc",
        "l0_codec_vectors",
        "l1_tree",
        "l155_qualification",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "retired target remains: {forbidden}"
        );
    }
}

#[test]
fn pb08_final_sources_are_substantive_and_alias_free() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_root = manifest_dir.join("src");
    for relative in relative_files(&source_root) {
        let path = source_root.join(&relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read production source {path:?}: {error}"));
        assert!(
            !source.trim().is_empty(),
            "empty production source: {relative}"
        );
        assert!(
            !source.contains("#[path =") && !source.contains("include!("),
            "production source uses a migration alias in {relative}"
        );
        assert!(!source.contains("src/c3/") && !source.contains("src/provider/"));
        assert!(!source.contains("l155_qualification"));
        assert!(
            !production_source_v1(&source).trim().is_empty(),
            "production logic hidden entirely behind cfg(test): {relative}"
        );
    }

    let tests_root = manifest_dir.join("tests");
    for relative in relative_files(&tests_root) {
        let path = tests_root.join(&relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read test source {path:?}: {error}"));
        assert!(
            !source.trim().is_empty(),
            "empty final test/support file: {relative}"
        );
        if relative != "l0_architecture.rs" {
            let exact_reference_import = relative == "operation_create.rs"
                && source
                    .matches("#[path = \"reference/naive_fastcdc.rs\"]")
                    .count()
                    == 1;
            assert!(
                exact_reference_import
                    || (!source.contains("#[path =") && !source.contains("include!(")),
                "final test tree uses a migration alias in {relative}"
            );
            if exact_reference_import {
                assert_eq!(source.matches("#[path =").count(), 1);
            }
        }
    }

    for (relative, required) in [
        (
            "support/temp_fs_cas.rs",
            ["TempFsCas", "create_dir", "path"].as_slice(),
        ),
        (
            "support/counting_source.rs",
            ["CountingSource", "pub fn read", "bytes_read"].as_slice(),
        ),
        (
            "support/counting_sink.rs",
            ["CountingSink", "pub fn begin", "pub fn finish", "pub fn abort"].as_slice(),
        ),
        (
            "support/fault_injection.rs",
            ["FaultPoint", "cancel_at", "pub fn observe"].as_slice(),
        ),
        (
            "reference/naive_fastcdc.rs",
            [
                "pub fn cut",
                "pub fn ends",
                "const GEAR",
                "const MINIMUM_CHUNK_BYTES",
            ]
            .as_slice(),
        ),
    ] {
        let source = std::fs::read_to_string(tests_root.join(relative)).unwrap();
        for marker in required {
            assert!(
                source.contains(marker),
                "{relative} lost substantive marker {marker}"
            );
        }
    }
    let reference = std::fs::read_to_string(tests_root.join("reference/naive_fastcdc.rs")).unwrap();
    for forbidden in [
        "layerfs_storage::",
        "ChunkerSpecV1",
        "canonical_bytes",
        "profile::",
        "cdc::",
    ] {
        assert!(
            !reference.contains(forbidden),
            "FastCDC reference is production-derived through {forbidden}"
        );
    }

    let final_owners = [
        "cas_admission.rs",
        "cow_locality.rs",
        "operation_concurrency.rs",
        "operation_create.rs",
        "operation_faults.rs",
        "operation_lifecycle.rs",
        "operation_mutation.rs",
        "operation_read.rs",
    ];
    let owner_sources = final_owners
        .into_iter()
        .map(|relative| {
            (
                relative,
                std::fs::read_to_string(tests_root.join(relative)).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let create_source = owner_sources
        .iter()
        .find_map(|(relative, source)| (*relative == "operation_create.rs").then_some(source))
        .expect("create owner source exists");
    assert_eq!(
        create_source
            .matches("#[path = \"reference/naive_fastcdc.rs\"]")
            .count(),
        1,
        "the exact independent FastCDC reference must be imported once"
    );
    assert_eq!(
        create_source.matches("crate::naive_fastcdc::ends").count(),
        2,
        "the imported FastCDC reference must be exercised by frozen comparisons"
    );
    assert!(
        create_source.contains("[16_688, 34_949, 52_688, 70_914, 90_807, 100_000]"),
        "FastCDC comparison lost its pinned boundary corpus"
    );
    for (relative, expected_tests, expected_feature_gates) in [
        ("cas_admission.rs", 50, 1),
        ("cow_locality.rs", 16, 0),
        ("operation_concurrency.rs", 23, 1),
        ("operation_create.rs", 46, 1),
        ("operation_faults.rs", 74, 1),
        ("operation_lifecycle.rs", 31, 1),
        ("operation_mutation.rs", 13, 1),
        ("operation_read.rs", 9, 2),
    ] {
        let source = owner_sources
            .iter()
            .find_map(|(candidate, source)| (*candidate == relative).then_some(source))
            .expect("final owner source exists");
        assert_eq!(
            source.matches("#[test]").count(),
            expected_tests,
            "final owner test custody count drifted: {relative}"
        );
        assert_eq!(
            source
                .matches("#[cfg(feature = \"operation-polymorphism\")]")
                .count(),
            expected_feature_gates,
            "feature applicability gate count drifted: {relative}"
        );
    }
    for (relative, source) in &owner_sources {
        assert!(
            source.contains("#[test]"),
            "final owner has no tests: {relative}"
        );
        assert!(
            source.contains("assert") || source.contains("panic!"),
            "final owner has no semantic assertions: {relative}"
        );
        for forbidden in [
            "qualification::run(",
            "ScenarioV1",
            "numeric_dispatch",
            "mod c3_",
        ] {
            assert!(
                !source.contains(forbidden),
                "final owner {relative} retained forwarding/dispatcher token {forbidden}"
            );
        }
    }
    for (support, owner) in [
        ("TempFsCas", "cas_admission.rs"),
        ("CountingSource", "operation_mutation.rs"),
        ("CountingSink", "operation_read.rs"),
        ("FaultPoint", "operation_faults.rs"),
    ] {
        let source = owner_sources
            .iter()
            .find_map(|(relative, source)| (*relative == owner).then_some(source))
            .expect("owner source exists");
        assert!(
            source.contains(support),
            "support helper {support} is not used by its substantive owner {owner}"
        );
    }
    let lib = std::fs::read_to_string(source_root.join("lib.rs")).unwrap();
    assert!(!lib.contains("#[test]"));
    assert!(!lib.contains("ScenarioV1"));
    assert!(!lib.contains("numeric_dispatch"));
    assert!(
        lib.lines().count() < 180,
        "qualification facade grew into a test repository"
    );
    let create_source = owner_sources
        .iter()
        .find_map(|(relative, source)| {
            (*relative == "operation_create.rs").then_some(source.as_str())
        })
        .expect("operation_create owner exists");
    let resources_source = create_source
        .split_once("mod l1_resources {")
        .and_then(|(_, source)| source.split_once("\nmod l1_content {"))
        .map(|(source, _)| source)
        .expect("operation_create resource owner exists");
    assert!(
        resources_source.contains("layerfs_storage::resources"),
        "resource owner lost its bounded semantic adapter"
    );
    for forbidden in [
        "OperationCountersV1",
        "OperationMemoryPlanV1",
        "MemoryComponentV1",
        "ResourceLedgerV1",
        "limits::ResourceLedgerV1",
    ] {
        assert!(
            !resources_source.contains(forbidden),
            "resource owner reached through concrete/internal symbol {forbidden}"
        );
    }
    assert!(!tests_root.join("reference/naive_seqcdc.rs").exists());
}

#[test]
fn pb08_custody_inventory_is_executable_and_exact() {
    use std::collections::{BTreeMap, BTreeSet};

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let inventory_path = manifest_dir.join(
        "../../../ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/l1.5.5/"
            .to_owned()
            + "pb08-custody-inventory.tsv",
    );
    let inventory = std::fs::read_to_string(&inventory_path)
        .unwrap_or_else(|error| panic!("read custody inventory {inventory_path:?}: {error}"));
    let mut lines = inventory.lines();
    let expected_header = [
        "old_file",
        "old_function",
        "final_owner",
        "final_function",
        "original_feature_applicability",
        "assertion_count",
        "assertion_sequence_sha256",
        "fixture_support_ownership",
        "fault_boundary_order",
        "race_schedule",
        "expected_typed_error",
        "counter_forbidden_work_expectations",
        "portability_qualifier",
        "exact_command_result",
    ];
    let header = lines
        .next()
        .expect("custody inventory has a header")
        .split('\t')
        .collect::<Vec<_>>();
    assert_eq!(header, expected_header.to_vec());

    let rows = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), expected_header.len(), "malformed inventory row");
            fields
        })
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 266);

    let deferred_owner = "DEFERRED_UNTIL_L1_COMPLETE";
    let deferred_names = [
        "pinned_hand_vectors_freeze_unsigned_equal_threshold_jump_clamp_and_eof",
        "optimized_seqcdc_matches_oracle_on_hostile_corpora",
        "optimized_seqcdc_fragmentation_is_oracle_exact_and_wraps",
        "optimized_seqcdc_pause_counters_and_terminal_errors_are_exact",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let deferred = rows
        .iter()
        .filter(|row| row[2] == deferred_owner)
        .map(|row| row[3])
        .collect::<BTreeSet<_>>();
    assert_eq!(deferred, deferred_names);
    assert_eq!(rows.iter().filter(|row| row[2] == deferred_owner).count(), 4);

    let active = rows
        .iter()
        .filter(|row| row[2] != deferred_owner)
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 262);
    assert!(active.iter().all(|row| row[4] == "default" || row[4] == "operation-polymorphism"));

    let old_keys = active
        .iter()
        .map(|row| (row[0], row[1]))
        .collect::<BTreeSet<_>>();
    assert_eq!(old_keys.len(), 262, "duplicate historical custody row");
    let final_claims = active
        .iter()
        .map(|row| (row[2], row[3]))
        .collect::<BTreeSet<_>>();
    assert_eq!(final_claims.len(), 262, "duplicate final owner/function claim");

    let owner_names = [
        "cas_admission.rs",
        "cow_locality.rs",
        "operation_concurrency.rs",
        "operation_create.rs",
        "operation_faults.rs",
        "operation_lifecycle.rs",
        "operation_mutation.rs",
        "operation_read.rs",
    ];
    let tests_root = manifest_dir.join("tests");
    let mut current_names = BTreeMap::<&str, BTreeSet<String>>::new();
    let mut current_sources = BTreeMap::<&str, String>::new();
    for owner in owner_names {
        let source = std::fs::read_to_string(tests_root.join(owner))
            .unwrap_or_else(|error| panic!("read final owner {owner}: {error}"));
        let declarations = final_test_segments(&source);
        assert!(!declarations.is_empty(), "final owner has no tests: {owner}");
        current_names.insert(
            owner,
            declarations
                .iter()
                .map(|(name, _, _)| name.to_owned())
                .collect::<BTreeSet<_>>(),
        );
        current_sources.insert(owner, source);
    }

    let mut expected_names = BTreeMap::<&str, BTreeSet<String>>::new();
    for row in &active {
        expected_names
            .entry(row[2])
            .or_default()
            .insert(row[3].to_owned());
    }
    for owner in owner_names {
        assert_eq!(
            current_names.get(owner).expect("current owner names"),
            expected_names.get(owner).expect("inventory owner names"),
            "inventory does not describe the exact registered tests in {owner}"
        );
    }

    let mut expected_gated = BTreeMap::<&str, BTreeSet<String>>::new();
    for row in &active {
        if row[4] == "operation-polymorphism" {
            expected_gated
                .entry(row[2])
                .or_default()
                .insert(row[3].to_owned());
        }
    }
    for owner in owner_names {
        let actual = feature_gated_test_names(current_sources.get(owner).unwrap());
        assert_eq!(
            actual,
            expected_gated.get(owner).cloned().unwrap_or_default(),
            "feature applicability drifted in {owner}"
        );
    }

    let mut baseline_assertions = BTreeMap::<&str, usize>::new();
    let delegated_cleanup_rows = [
        "private_pack_cleanup_unwind_terminalizes_storage_and_preparation_before_return",
        "private_pack_cleanup_unwind_retains_invalidation_double_fault",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    for row in &active {
        let count = row[5]
            .parse::<usize>()
            .unwrap_or_else(|error| panic!("invalid assertion count in {}: {error}", row[3]));
        if count == 0 {
            assert!(
                delegated_cleanup_rows.contains(row[3]),
                "empty frozen assertion custody is not an approved delegated row: {}",
                row[3]
            );
        }
        assert_eq!(row[6].len(), 64, "invalid assertion fingerprint: {}", row[3]);
        assert!(
            row[6].bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid assertion fingerprint: {}",
            row[3]
        );
        assert!(!row[13].is_empty(), "missing exact command/result: {}", row[3]);
        *baseline_assertions.entry(row[2]).or_default() += count;
    }

    for owner in owner_names {
        let source = current_sources.get(owner).unwrap();
        let declarations = final_test_segments(source);
        let source_assertions = assertion_tokens(source);
        assert!(
            source_assertions.len() >= baseline_assertions[owner],
            "assertion custody shrank in {owner}: {} < {}",
            source_assertions.len(),
            baseline_assertions[owner]
        );
        for row in active.iter().filter(|row| row[2] == owner) {
            let (_, _, segment) = declarations
                .iter()
                .find(|(name, _, _)| name == row[3])
                .expect("inventory test declaration");
            let current = assertion_tokens(segment);
            let digest = digest_hex(current.join("").as_bytes());
            assert_eq!(digest.len(), 64);
            assert!(
                segment.contains("assert")
                    || segment.contains("expect")
                    || segment.contains("unwrap"),
                "test lost executable assertion/error custody: {}",
                row[3]
            );
        }
    }

    let subprocess_children = owner_names
        .iter()
        .map(|owner| {
            current_sources
                .get(owner)
                .unwrap()
                .matches("fn subprocess_open_existing_probe")
                .count()
        })
        .sum::<usize>();
    assert_eq!(subprocess_children, 1, "subprocess child custody is duplicated or absent");
}

#[test]
fn pb08_default_object_reader_applicability_is_frozen() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/operation_read.rs"),
    )
    .expect("read operation_read owner");
    assert!(
        !source.contains("#[cfg(feature = \"operation-polymorphism\")]\nmod l1_object_read"),
        "default l1_object custody was gated behind operation-polymorphism"
    );
    let object_source = source
        .split_once("mod l1_object_read")
        .map(|(_, suffix)| suffix)
        .expect("l1_object_read owner exists");
    for name in [
        "all_five_exact_object_kinds_decode_and_hash_in_separate_domains",
        "bounded_random_read_decoder_matches_borrowed_decoder_and_never_requests_a_large_buffer",
        "hostile_envelopes_fail_before_visiting_edges",
        "hostile_payloads_abort_provisional_edges_and_reject_bad_order",
        "loop_counts_are_preflighted_against_declared_payload_bytes",
        "typed_edges_stream_in_wire_order_without_decoder_edge_storage",
    ] {
        assert_eq!(
            object_source.matches(&format!("fn {name}")).count(),
            1,
            "default object-reader custody missing or duplicated: {name}"
        );
    }
    assert_eq!(
        object_source.matches("#[test]").count(),
        6,
        "operation_read default object-reader owner lost exact discovery"
    );
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
use layerfs_storage::qualification::{
    CanonicalDirectoryTreeV1 as QualifiedTreeV1,
    FsCasV1 as QualifiedFsCasV1,
    OperationHandoffV1 as QualifiedHandoffV1,
    ReadSinkV1 as QualifiedReadSinkV1,
    ResourceLedgerV1 as QualifiedLedgerV1,
    SealedPackV1 as QualifiedPackV1,
};

fn main() {
    let _ = core::mem::size_of::<QualifiedFsCasV1>();
    let _ = core::mem::size_of::<QualifiedTreeV1>();
    let _ = core::mem::size_of::<QualifiedHandoffV1>();
    let _ = core::mem::size_of::<QualifiedLedgerV1>();
    let _ = core::mem::size_of::<QualifiedPackV1>();
    let _sink: Option<&dyn QualifiedReadSinkV1> = None;
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
    for forbidden_symbol in [
        "CanonicalDirectoryTreeV1",
        "FsCasV1",
        "OperationHandoffV1",
        "ReadSinkV1",
        "ResourceLedgerV1",
        "SealedPackV1",
    ] {
        assert!(
            stderr.contains(&format!("no {forbidden_symbol} in"))
                || stderr.contains(&format!("no `{forbidden_symbol}` in")),
            "qualification facade unexpectedly exposes {forbidden_symbol}: {stderr}"
        );
    }
}

#[test]
fn complete_content_depends_only_on_lifecycle_semantic_ports() {
    let create = include_str!("../src/content/create.rs");
    let production = production_source_v1(create);
    let lifecycle = include_str!("../src/lifecycle/mod.rs");
    for forbidden in [
        "crate::cas",
        "crate::cow",
        "crate::lifecycle",
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
        "CanonicalDirectoryTreeV1",
        "PreparedCandidateV1",
        "run_create_tree_v1",
        "run_create_v1",
    ] {
        assert!(
            !production.contains(forbidden),
            "content/create.rs retained complete-root/lifecycle ownership: {forbidden}"
        );
    }
    assert_eq!(production.matches("run_lifecycle_v1(").count(), 0);
    for operation in [
        "run_create_v1",
        "run_create_tree_v1",
        "run_complete_replace_v1",
        "run_complete_update_v1",
        "run_complete_add_v1",
        "run_complete_remove_v1",
        "run_complete_metadata_v1",
        "run_complete_move_v1",
        "complete_cross_directory_move_operation_v1",
    ] {
        let body = function_body(lifecycle, operation);
        assert!(
            body.contains("run_lifecycle_v1("),
            "{operation} does not enter the shared lifecycle coordinator"
        );
    }
    assert_eq!(
        lifecycle.matches("pub(crate) fn run_lifecycle_v1").count(),
        1,
        "complete operations must have one outer lifecycle state machine"
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
    for owned in ["mod file;", "mod create;", "mod replace;", "mod update;"] {
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
fn pb07_substantive_owners_are_not_forwarding_shells() {
    let tree = include_str!("../src/cow/tree.rs");
    let mutate = include_str!("../src/cow/mutate.rs");
    let view = include_str!("../src/cow/view.rs");
    assert!(view.contains("trait CanonicalTreeMutationSourceV1"));
    for view_owned_authentication in [
        "struct SparsePreimageHasherV1",
        "fn hash_streamed_directory_side",
        "fn validate_mutation_relation",
        "fn authenticate_and_derive_mutation_logical",
        "fn validate_mutation_physical_evidence",
        "fn validate_replacement_evidence",
        "fn derive_replacement_logical",
        "fn validate_page_boundaries",
    ] {
        assert!(
            view.contains(view_owned_authentication),
            "cow/view.rs lacks substantive authenticated-view ownership: {view_owned_authentication}"
        );
    }
    assert!(mutate.contains("CanonicalTreeMutationSourceV1"));
    assert!(mutate.contains("TreeProofMutationV1"));
    assert!(mutate.contains("fn mutate_directory_entries_cow_v1"));
    assert!(mutate.contains("fn mutate_directory_entries_inner"));
    assert!(mutate.contains("fn replace_directory_entry_cow_with_admission_v1"));
    assert!(!mutate.contains("tree::replace_directory_entry_cow_impl_v1"));
    assert!(!mutate.contains("tree::add_directory_entry_cow_impl_v1"));
    assert!(!mutate.contains("tree::remove_directory_entry_cow_impl_v1"));
    for tree_owned_mutation in [
        "fn replace_directory_entry_cow_impl_v1",
        "fn replace_directory_entry_cow_borrowed_impl_v1",
        "fn mutate_directory_entries_cow_v1",
        "fn mutate_directory_entries_inner",
        "enum TreeMutationKindV1",
    ] {
        assert!(
            !tree.contains(tree_owned_mutation),
            "COW mutation implementation remains in cow/tree.rs: {tree_owned_mutation}"
        );
    }
    for proof_owned_function in [
        "fn hash_streamed_directory_side",
        "fn validate_mutation_relation",
        "fn authenticate_and_derive_mutation_logical",
        "fn validate_mutation_physical_evidence",
        "fn validate_replacement_evidence",
        "struct SparsePreimageHasherV1",
        "struct VerificationTreeSinkV1",
    ] {
        assert!(
            !mutate.contains(proof_owned_function),
            "authenticated COW view implementation remains in cow/mutate.rs: {proof_owned_function}"
        );
    }

    let extraction = include_str!("../src/read/extraction.rs");
    let range = include_str!("../src/read/range.rs");
    let content_read = include_str!("../src/content/read.rs");
    for content_owned_streaming in [
        "struct VerifiedFileSegmentV1",
        "trait VerifiedFileRangePortV1",
        "fn stream_verified_file_range_v1",
    ] {
        assert!(
            content_read.contains(content_owned_streaming),
            "content/read.rs lacks verified payload ownership: {content_owned_streaming}"
        );
    }
    for range_forbidden_streaming in [
        "struct VerifiedFileSegmentV1",
        "trait VerifiedFileRangePortV1",
        "fn stream_verified_file_range_v1",
    ] {
        assert!(
            !range.contains(range_forbidden_streaming),
            "read/range.rs retained verified payload streaming: {range_forbidden_streaming}"
        );
    }
    assert!(range.contains("struct ExactRangePlanV1"));
    assert!(range.contains("fn begin_exact_range_digest_v1"));
    assert!(!range.contains("super::extraction"));
    assert!(!range.contains("run_read_v1"));
    assert!(!range.contains("ReaderV1"));
    assert!(!range.contains("FsCas"));
    for extraction_owned_range in ["fn read_file_range_impl_v1", "fn read_exact_range"] {
        assert!(
            extraction.contains(extraction_owned_range),
            "root/path exact-range orchestration is missing from read/extraction.rs: {extraction_owned_range}"
        );
    }
    assert!(extraction.contains("impl<C: FsCasControlV1 + ?Sized> VerifiedFileRangePortV1"));
    assert!(!extraction.contains("ExactRangeExecutorV1"));
    for raw_object_layout in [
        "ObjectCursorV1",
        "read_u8",
        "read_u16",
        "read_u32",
        "read_u64",
        "read_component",
        "ExtentTagV1",
        "PhysicalTreeChildKindV1",
        "TreeSubtypeV1",
        "OBJECT_HEADER_BYTES",
    ] {
        assert!(
            !range.contains(raw_object_layout),
            "read/range.rs retained raw object-layout parsing: {raw_object_layout}"
        );
        assert!(
            !extraction.contains(raw_object_layout),
            "read/extraction.rs retained raw object-layout parsing: {raw_object_layout}"
        );
    }

    let object_reader = include_str!("../src/read/object_reader.rs");
    for owned in [
        "struct OccupiedObjectReaderV1",
        "fn required_occupied_len_v1",
        "PhysicalObjectReadPortV1 for OccupiedObjectReaderV1",
    ] {
        assert!(
            object_reader.contains(owned),
            "object reader lacks bounded ownership: {owned}"
        );
    }
    assert!(!object_reader.contains("struct ObjectCursorV1"));
    assert!(!object_reader.contains("fn read_occupied_exact_accounted_v1"));
    assert!(!extraction.contains("struct ObjectCursorV1"));
    assert!(!extraction.contains("fn read_occupied_exact_accounted_v1"));

    let traversal = include_str!("../src/object/traversal.rs");
    let operation_admission = include_str!("../src/cas/operation_admission.rs");
    assert!(traversal.contains("fn traverse_strong_edges_v1"));
    assert!(traversal.contains("while"));
    assert!(operation_admission.contains("traverse_strong_edges_v1"));
    assert!(!operation_admission.contains("while ordinal < closure.count"));

    let resync = include_str!("../src/cdc/resync.rs");
    let update = include_str!("../src/content/update.rs");
    assert!(resync.contains("fn resynchronize_update_v1"));
    assert!(resync.contains("while base_cursor < base_len"));
    assert!(resync.contains("fn verify_rejoin_bytes_v1"));
    assert!(!update.contains("while base_cursor < base_len"));
    assert!(!update.contains("fn exact_rejoin_bytes"));
}

#[test]
fn pb07_canonical_transcripts_and_rejoin_proof_have_one_owner() {
    let encode = include_str!("../src/object/encode.rs");
    for owned in [
        "struct CanonicalPhysicalObjectEncoderV1",
        "struct CanonicalPhysicalObjectVerifierV1",
        "FramedHasherV1",
        "encode_physical_object_header_v1",
        "physical_domain_tag_v1",
        "FILE_FIXED_PAYLOAD_BYTES_V1",
        "DATA_EXTENT_FIXED_BYTES_V1",
        "CHUNK_REFERENCE_BYTES_V1",
    ] {
        assert!(
            encode.contains(owned),
            "object/encode.rs lacks canonical physical ownership: {owned}"
        );
    }

    let decode = include_str!("../src/object/decode.rs");
    let port_decode = include_str!("../src/object/port_decode.rs");
    assert!(decode.contains("trait CanonicalObjectCursorV1"));
    assert!(decode.contains("fn decode_payload_from_cursor_v1"));
    assert!(port_decode.contains("decode_payload_from_cursor_v1("));
    for duplicate in [
        "FramedHasherV1",
        "physical_domain_tag_v1",
        "fn decode_tree",
        "fn decode_file",
    ] {
        assert!(
            !port_decode.contains(duplicate),
            "object/port_decode.rs duplicated canonical semantic/transcript ownership: {duplicate}"
        );
    }

    for (path, source) in [
        ("content/file.rs", include_str!("../src/content/file.rs")),
        (
            "content/update.rs",
            include_str!("../src/content/update.rs"),
        ),
        ("cow/tree.rs", include_str!("../src/cow/tree.rs")),
    ] {
        let production = production_source_v1(source);
        for duplicate in [
            "FramedHasherV1",
            "TAG_PHYSICAL",
            "OBJECT_HEADER_BYTES",
            "FILE_FIXED_PAYLOAD_BYTES_V1",
            "DATA_EXTENT_FIXED_BYTES_V1",
            "CHUNK_REFERENCE_BYTES_V1",
            "encode_physical_object_header_v1",
            "physical_domain_tag_v1",
        ] {
            assert!(
                !production.contains(duplicate),
                "{path} retained canonical physical framing/transcript mechanics: {duplicate}"
            );
        }
    }

    assert!(encode.contains("struct EncodedVersionRecordV1"));
    assert!(encode.contains("fn encode_version_record_v1"));
    let complete_writer = include_str!("../src/pack/complete_writer.rs");
    assert!(complete_writer.contains("encode_version_record_v1("));
    for pack_forbidden_transcript in [
        "derive_physical_version_record_id_v1",
        "encode_physical_object_header_v1",
        "VERSION_RECORD_PAYLOAD_BYTES",
        "VERSION_OBJECT_BYTES",
        "payload[0..32]",
        "payload[176..184]",
    ] {
        assert!(
            !complete_writer.contains(pack_forbidden_transcript),
            "pack/complete_writer.rs retained VersionRecord layout/transcript ownership: {pack_forbidden_transcript}"
        );
    }

    let range = include_str!("../src/read/range.rs");
    assert!(!range.contains("crate::read::extraction"));
    assert!(!range.contains("super::extraction"));
    for forbidden in [
        "FsCas",
        "FsCasControlV1",
        "CanonicalDirectoryTreeV1",
        "read_object_payload_exact_v1",
        "PhysicalTreeChildKindV1",
        "RootDirectory",
    ] {
        assert!(
            !range.contains(forbidden),
            "read/range.rs crossed into extraction/storage ownership: {forbidden}"
        );
    }

    let resync = include_str!("../src/cdc/resync.rs");
    let update = include_str!("../src/content/update.rs");
    for owned in [
        "struct RejoinOperationBindingV1",
        "struct VerifiedRejoinV1",
        "fn verify_rejoin_bytes_v1",
        "fn consume",
    ] {
        assert!(
            resync.contains(owned),
            "cdc/resync.rs lacks authenticated rejoin ownership: {owned}"
        );
    }
    for duplicate in [
        "struct UpdateOperationBindingV1",
        "struct VerifiedRejoinV1",
        "fn verify_rejoin_bytes_v1",
        "ChunkerSpecV1",
    ] {
        assert!(
            !update.contains(duplicate),
            "content/update.rs retained CDC rejoin proof/profile ownership: {duplicate}"
        );
    }
}

#[test]
fn pb07_semantic_modules_keep_concrete_storage_out() {
    let sources = [
        ("content/mod.rs", include_str!("../src/content/mod.rs")),
        ("content/file.rs", include_str!("../src/content/file.rs")),
        (
            "content/create.rs",
            include_str!("../src/content/create.rs"),
        ),
        (
            "content/replace.rs",
            include_str!("../src/content/replace.rs"),
        ),
        (
            "content/update.rs",
            include_str!("../src/content/update.rs"),
        ),
        ("content/read.rs", include_str!("../src/content/read.rs")),
        ("read/range.rs", include_str!("../src/read/range.rs")),
        ("cow/file.rs", include_str!("../src/cow/file.rs")),
        ("cow/tree.rs", include_str!("../src/cow/tree.rs")),
        ("cow/view.rs", include_str!("../src/cow/view.rs")),
        ("cow/mutate.rs", include_str!("../src/cow/mutate.rs")),
        ("object/model.rs", include_str!("../src/object/model.rs")),
        ("object/encode.rs", include_str!("../src/object/encode.rs")),
        ("object/decode.rs", include_str!("../src/object/decode.rs")),
        (
            "object/port_decode.rs",
            include_str!("../src/object/port_decode.rs"),
        ),
        (
            "object/traversal.rs",
            include_str!("../src/object/traversal.rs"),
        ),
    ];
    for (path, source) in sources {
        let production = production_source_v1(source);
        if path.starts_with("content/") {
            assert_eq!(
                forbidden_content_import_v1(&production),
                None,
                "{path} crossed a concrete storage/import boundary"
            );
        }
        for forbidden in [
            "crate::cas::fs",
            "FsPrivatePack",
            "FsOperationSpool",
            "FileClosureObjectSpool",
            "FileGlobalSeenSpool",
            "hard_link",
        ] {
            assert!(
                !production.contains(forbidden),
                "{path} crossed a concrete storage/publication boundary: {forbidden}"
            );
        }
    }
}

#[test]
fn pb07_content_storage_import_guard_rejects_representative_forbidden_imports() {
    for source in [
        "use crate::cas::FsCasV1;",
        "use crate::pack::CompletedPackSetV1;",
        "use crate::lifecycle::run_lifecycle_v1;",
        "use std::fs::OpenOptions;",
        "use std::path::PathBuf;",
    ] {
        assert!(
            forbidden_content_import_v1(source).is_some(),
            "content import guard accepted {source}"
        );
    }
}

#[test]
fn pb07_private_migration_names_are_absent_from_production() {
    let sources = [
        include_str!("../src/lib.rs"),
        include_str!("../src/content/mod.rs"),
        include_str!("../src/content/file.rs"),
        include_str!("../src/content/create.rs"),
        include_str!("../src/content/replace.rs"),
        include_str!("../src/content/update.rs"),
        include_str!("../src/content/read.rs"),
        include_str!("../src/cow/mod.rs"),
        include_str!("../src/cow/file.rs"),
        include_str!("../src/cow/tree.rs"),
        include_str!("../src/cow/view.rs"),
        include_str!("../src/cow/mutate.rs"),
        include_str!("../src/object/mod.rs"),
        include_str!("../src/object/model.rs"),
        include_str!("../src/object/encode.rs"),
        include_str!("../src/object/decode.rs"),
        include_str!("../src/object/port_decode.rs"),
        include_str!("../src/object/traversal.rs"),
        include_str!("../src/read/mod.rs"),
        include_str!("../src/read/extraction.rs"),
        include_str!("../src/read/range.rs"),
        include_str!("../src/read/object_reader.rs"),
        include_str!("../src/lifecycle/mod.rs"),
        include_str!("../src/lifecycle/preparation.rs"),
    ];
    for source in sources {
        let source = production_source_v1(source);
        let lower = source.to_ascii_lowercase();
        for forbidden in [
            "run_c3_",
            "request_c3_",
            "c3_storage_",
            "c3_admission_",
            "c3operationpreparation",
            "sharedc3control",
        ] {
            assert!(
                !lower.contains(forbidden),
                "private migration name remains: {forbidden}"
            );
        }
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
    let historical_fixture = include_bytes!("fixtures/c3-registry-v1.tsv");

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
    assert_eq!(historical_fixture.len(), 47_759);
    let fixture_digest = support::sha256(historical_fixture)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        fixture_digest,
        "db8d1f2239cdbcfc3b37a050859533dea547b5d690dc17fd09099a0f6539ea61"
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
