//! Selected Linux coordinator adapter. Recipes and oracles remain in their owners.
use super::*;

const SDK: [&str; 3] = [
    "edit_length_preserving",
    "edit_length_changing",
    "edit_canonical_chunk_count",
];

#[allow(
    clippy::too_many_arguments,
    reason = "Arguments mirror the emitted registry columns"
)]
fn row(
    family: &str,
    id: &str,
    route: &str,
    fresh: bool,
    proof: bool,
    max_seed: u8,
    supported: bool,
    bytes: u64,
    files: u64,
    tier: usize,
    verification: bool,
) {
    let inherited = SDK.contains(&family) || family == "edit_length_changing_capped";
    let low_tier = matches!(family, "init_namespace" | "store_footprint") || tier <= 10;
    let smoke_supported = low_tier && bytes <= 50_000_000 && files <= 1_000 && !proof;
    let profile = if id.contains("compact-") || id.contains("low-v") {
        "compact-low-tier-v2"
    } else {
        "registered-fixture"
    };
    let bytes = bytes.to_string();
    println!("{{\"family_id\":\"{family}\",\"scenario_id\":\"{id}\",\"route\":\"{route}\",\"setup_policy\":\"{}\",\"proof_only\":{proof},\"seed_min\":1,\"seed_max\":{max_seed},\"supported\":{supported},\"verification_supported\":{verification},\"inherited\":{inherited},\"tier\":{tier},\"fixture_bytes\":{bytes},\"fixture_files\":{files},\"fixture_profile\":\"{profile}\",\"smoke_supported\":{smoke_supported},\"full_workload\":true}}", if fresh {"fresh-output"} else {"post-init"});
}

fn list(family_filter: Option<&str>, case_filter: Option<&str>) -> AnyResult<()> {
    let selected = |family: &str, id: &str| {
        family_filter.is_none_or(|value| value == family)
            && case_filter.is_none_or(|value| value == id)
    };
    for case in workload_source::NAMESPACE_SCENARIOS {
        if !selected("init_namespace", case.id) {
            continue;
        }
        row(
            "init_namespace",
            case.id,
            "namespace",
            true,
            false,
            3,
            true,
            case.logical_bytes,
            case.regular_files,
            case.regular_files as usize,
            true,
        );
    }
    for family in SDK {
        for case in sdk_edit_registry(family)? {
            if !selected(family, &case.id) {
                continue;
            }
            row(
                family,
                &case.id,
                "sdk",
                false,
                false,
                5,
                case.final_bytes <= 524_288_000,
                case.fixture_bytes,
                1,
                (case.fixture_bytes / 1_048_576) as usize,
                true,
            );
        }
    }
    for case in workload_source::store_footprint::CONTROLS {
        if !selected("store_footprint", case.id) {
            continue;
        }
        row(
            "store_footprint",
            case.id,
            "store-footprint",
            true,
            false,
            3,
            true,
            case.logical_bytes,
            case.regular_files,
            case.infra_tier as usize,
            true,
        );
    }
    let registry = workload_source::workspace_registry::cases()
        .into_iter()
        .chain(workload_source::workspace_registry::proofs())
        .chain(workload_source::workspace_registry::inherited());
    for case in registry {
        if !selected(case.family, &case.id) {
            continue;
        }
        let reliability = case.family == "workspace_reliability";
        let proof = reliability || case.kind == "boundaries";
        let entries = workload_source::workspace_registry::fixture(&case, 1)?;
        let fixture_bytes = workload_source::workspace_common::validate_entries(&entries)?;
        let fixture_files = entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.kind,
                    workload_source::workspace_common::EntryKind::File(_)
                        | workload_source::workspace_common::EntryKind::Hardlink(_)
                )
            })
            .count() as u64;
        row(
            case.family,
            &case.id,
            if reliability {
                "reliability"
            } else {
                "workspace"
            },
            workload_source::workspace_registry::is_import(&case),
            proof,
            if reliability {
                1
            } else if case.family == "edit_length_changing_capped" {
                5
            } else {
                3
            },
            true,
            fixture_bytes,
            fixture_files,
            case.tier,
            case.kind != "sustained-600s",
        );
    }
    Ok(())
}

fn validate(family: &str, id: &str, seed: u8, performance: bool) -> AnyResult<()> {
    if family == "init_namespace" {
        let case = namespace_scenario(id)?;
        if case.id != id {
            return Err("canonical case ID required".into());
        }
    } else if SDK.contains(&family) {
        let case = sdk_edit_scenario(family, id)?;
        if case.final_bytes > 524_288_000 {
            return Err("historical oversized SDK case: select its capped replacement".into());
        }
    } else if family == "store_footprint" {
        workload_source::store_footprint::control(id)?;
    } else {
        let case = workload_source::workspace_registry::resolve(id)?;
        if case.family != family {
            return Err("family/case mismatch".into());
        }
        if case.kind == "sustained-600s" {
            return Err(
                "INCOMPLETE: sustained-600s cannot fit the selected verification deadline".into(),
            );
        }
        if performance && (family == "workspace_reliability" || case.kind == "boundaries") {
            return Err("proof-only case does not support performance".into());
        }
    }
    let maximum = if SDK.contains(&family) || family == "edit_length_changing_capped" {
        5
    } else if family == "workspace_reliability" {
        1
    } else {
        3
    };
    if seed == 0 || seed > maximum {
        return Err("invalid selected seed/repetition".into());
    }
    Ok(())
}

fn capture(args: &[OsString]) -> AnyResult<String> {
    let output = Command::new(std::env::current_exe()?).args(args).output()?;
    if output.stdout.len() > 1_048_576 || output.stderr.len() > 1_048_576 {
        return Err("preparation receipt exceeds bound".into());
    }
    if !output.status.success() {
        return Err(format!(
            "preparation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

// The existing preparation emitters produce flat scalar JSON with unescaped
// canonical IDs, hashes and relative paths. Reject any other scalar encoding.
fn field(json: &str, key: &str) -> AnyResult<String> {
    let needle = format!("\"{key}\":");
    let tail = json
        .split_once(&needle)
        .ok_or_else(|| format!("missing preparation field {key}"))?
        .1
        .trim_start();
    let value = if let Some(tail) = tail.strip_prefix('"') {
        tail.split_once('"')
            .ok_or("unterminated preparation scalar")?
            .0
    } else {
        tail.split([',', '}', '\n'])
            .next()
            .ok_or("empty preparation scalar")?
            .trim()
    };
    if value.is_empty() || value.contains(['\\', '\n', '\r', '\t']) {
        return Err("unsupported preparation scalar".into());
    }
    Ok(value.to_owned())
}

fn prepare(family: &str, id: &str, seed: u8, root: &Path) -> AnyResult<()> {
    validate(family, id, seed, false)?;
    if root.exists() {
        return Err("preparation output must be absent".into());
    }
    std::fs::create_dir_all(root)?;
    let payload = root.join("payload");
    let seed_text = seed.to_string();
    let fixture = if family == "init_namespace" {
        capture(&[
            "namespace-fixture".into(),
            payload.as_os_str().into(),
            id.into(),
        ])?
    } else if SDK.contains(&family) {
        let case = sdk_edit_scenario(family, id)?;
        let fixture = capture(&[
            "sdk-edit-prepare".into(),
            payload.as_os_str().into(),
            case.fixture_bytes.to_string().into(),
        ])?;
        let branch = field(&fixture, "branch_id")?;
        std::fs::write(payload.join("branch-id"), &branch)?;
        let qualification = capture(&[
            "sdk-edit-qualify".into(),
            payload.as_os_str().into(),
            branch.into(),
            family.into(),
            id.into(),
        ])?;
        std::fs::write(root.join("qualification.tsv"),format!("family\tcase\tplan\tinitial\texpected\tfile\tmap\tinitial_count\tfinal_count\tdigest\n{qualification}"))?;
        fixture
    } else if family == "store_footprint" {
        let control = workload_source::store_footprint::control(id)?;
        let output = Command::new("/usr/local/bin/fs-benchmark-workload")
            .args([
                OsString::from("store-footprint-fixture"),
                payload.as_os_str().into(),
                id.into(),
                control.infra_tier.to_string().into(),
            ])
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "footprint preparation: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        String::from_utf8(output.stdout)?
    } else {
        let fixture = capture(&[
            "workspace-prepare".into(),
            payload.as_os_str().into(),
            id.into(),
            seed_text.into(),
        ])?;
        if field(&fixture, "input_mode")? == "store" {
            std::fs::write(payload.join("branch-id"), field(&fixture, "branch_id")?)?;
        }
        if family == "git_tool_workflow" {
            capture(&[
                "workspace-reference-prepare".into(),
                root.join("reference").as_os_str().into(),
                id.into(),
                seed.to_string().into(),
            ])?;
        }
        fixture
    };
    std::fs::write(root.join("fixture.json"), &fixture)?;
    let mut qualification_hash = "not-applicable".to_owned();
    if matches!(family, "dedup_cross_file" | "dedup_cdc_locality") {
        let case = workload_source::workspace_registry::resolve(id)?;
        let receipt =
            super::dedup_verify::qualify_import_input(&case, seed, &payload.join("input"))?;
        let encoded = receipt
            .iter()
            .map(|(key, value)| format!("{key}\t{value}\n"))
            .collect::<String>();
        qualification_hash = workload_source::sdk_edit_common::sha256_hex(encoded.as_bytes());
        std::fs::write(root.join("input-qualification.tsv"), encoded)?;
    }
    std::fs::write(
        root.join("selection.tsv"),
        format!("{family}\t{id}\t{seed}\n"),
    )?;
    let digest = workload_source::sdk_edit_common::sha256_hex(fixture.as_bytes());
    let manifest=format!("{{\"schema\":\"fs-bench-infra-prepared-v1\",\"family_id\":\"{family}\",\"scenario_id\":\"{id}\",\"seed\":{seed},\"input_identity\":\"{digest}\",\"fixture_receipt_sha256\":\"{digest}\",\"input_qualification_sha256\":\"{qualification_hash}\",\"full_workload\":true}}\n");
    std::fs::write(root.join("manifest.json"), &manifest)?;
    print!("{manifest}");
    Ok(())
}

fn run_selected(
    family: &str,
    id: &str,
    seed: u8,
    mode: &str,
    root: &Path,
    container: &str,
) -> AnyResult<()> {
    if !matches!(mode, "performance" | "verify") {
        return Err("invalid selected mode".into());
    }
    validate(family, id, seed, mode == "performance")?;
    if std::fs::read_to_string(root.join("selection.tsv"))? != format!("{family}\t{id}\t{seed}\n") {
        return Err("prepared selection mismatch".into());
    }
    let fixture = std::fs::read_to_string(root.join("fixture.json"))?;
    let manifest = std::fs::read_to_string(root.join("manifest.json"))?;
    if field(&manifest, "fixture_receipt_sha256")?
        != workload_source::sdk_edit_common::sha256_hex(fixture.as_bytes())
    {
        return Err("prepared receipt mismatch".into());
    }
    let payload = root.join("payload");
    let work = root.join("work");
    let profile = "reused-first-sample-uncontrolled";
    let source = std::env::var("LAYERFS_BENCH_SOURCE_ARM").unwrap_or_else(|_| "candidate".into());
    if !matches!(source.as_str(), "baseline" | "candidate") {
        return Err("invalid source arm".into());
    }
    if matches!(family, "dedup_cross_file" | "dedup_cdc_locality") {
        let qualification = std::fs::read(root.join("input-qualification.tsv"))?;
        let digest = workload_source::sdk_edit_common::sha256_hex(&qualification);
        if digest != field(&manifest, "input_qualification_sha256")? {
            return Err("missing or changed pre-admission input qualification".into());
        }
        println!("{{\"kind\":\"input-qualification\",\"status\":\"pass\",\"receipt_sha256\":\"{digest}\",\"scope\":\"prepared-source-transcripts-before-product-admission\"}}");
    }
    if family == "init_namespace" {
        let scenario = namespace_scenario(id)?;
        if mode == "performance" {
            namespace_init_diagnostic(
                &work,
                &payload,
                scenario,
                seed.into(),
                &field(&fixture, "fixture_digest")?,
                profile,
            )
        } else {
            namespace_verify_case(
                &work,
                &payload,
                ContainerId(container.into()),
                scenario,
                seed,
                &source,
                &field(&fixture, "fixture_digest")?,
                &field(&fixture, "edited_fixture_digest")?,
                &field(&fixture, "edit_path")?,
                field(&fixture, "edit_size")?.parse()?,
                profile,
            )
        }
    } else if family == "store_footprint" {
        store_footprint_case(
            &work,
            &payload,
            ContainerId(container.into()),
            id,
            seed,
            &source,
            field(&fixture, "regular_files")?.parse()?,
            field(&fixture, "logical_bytes")?.parse()?,
            &field(&fixture, "edit_path")?,
            field(&fixture, "edit_size")?.parse()?,
            &field(&fixture, "fixture_digest")?,
            &field(&fixture, "edited_fixture_digest")?,
            mode == "verify",
        )
    } else if SDK.contains(&family) {
        let branch = std::fs::read_to_string(payload.join("branch-id"))?;
        let qualification = root.join("qualification.tsv");
        let qualification_hash =
            workload_source::sdk_edit_common::sha256_hex(&std::fs::read(&qualification)?);
        let mut command = Command::new(std::env::current_exe()?);
        if mode == "performance" {
            command.args([
                OsString::from("sdk-edit-run"),
                payload.as_os_str().into(),
                branch.trim().into(),
                family.into(),
                id.into(),
                source.as_str().into(),
                seed.to_string().into(),
                container.into(),
            ]);
        } else {
            command.args([
                OsString::from("sdk-edit-verify"),
                payload.as_os_str().into(),
                branch.trim().into(),
                family.into(),
                id.into(),
                source.as_str().into(),
                container.into(),
                "-".into(),
            ]);
        }
        let code_hash =
            workload_source::sdk_edit_common::sha256_hex(include_bytes!("sdk_file_edit.rs"));
        command
            .env("LAYERFS_SDK_EDIT_TIMED_MANIFEST_SHA256", &code_hash)
            .env(
                "LAYERFS_SDK_EDIT_ROUTE_MANIFEST_SHA256",
                workload_source::sdk_edit_common::sha256_hex(b"infra-sdk-single-range-edit-v1"),
            )
            .env("LAYERFS_SDK_EDIT_QUALIFICATION_FILE", qualification)
            .env("LAYERFS_SDK_EDIT_QUALIFICATION_SHA256", qualification_hash);
        if !command.status()?.success() {
            return Err("selected SDK operation failed".into());
        }
        Ok(())
    } else {
        let case = workload_source::workspace_registry::resolve(id)?;
        let fast = mode == "verify"
            && !matches!(
                family,
                "git_tool_workflow" | "edit_length_changing_capped" | "workspace_reliability"
            )
            && case.kind != "boundaries";
        let args = [
            OsString::from("workspace-run"),
            payload.as_os_str().into(),
            if family == "workspace_reliability" {
                OsString::from("/var/lib/fs-bench/prepared/payload/input")
            } else {
                payload.join("input").as_os_str().into()
            },
            id.into(),
            seed.to_string().into(),
            if fast {
                "fast-verify".into()
            } else {
                mode.into()
            },
            container.into(),
        ];
        let mut command = Command::new(std::env::current_exe()?);
        command.args(args);
        if fast {
            std::fs::create_dir_all("/verification")?;
            command
                .env_remove("LAYERFS_V013_FAST_CERTIFICATE")
                .env_remove("LAYERFS_V013_FAST_CERTIFICATE_SHA256")
                .env("LAYERFS_V013_FAST_NO_REUSE", "1")
                .env(
                    "LAYERFS_V013_FAST_INPUT_PLAN_SHA256",
                    field(&fixture, "input_plan_sha256")?,
                )
                .env("LAYERFS_V013_VERIFIER_EXCHANGE_HOST", "/verification");
        }
        if family == "git_tool_workflow" {
            std::fs::create_dir_all("/qualified")?;
            std::os::unix::fs::symlink(root.join("reference/input"), "/qualified/git-reference")?;
            std::fs::create_dir_all("/verification")?;
            command.env("LAYERFS_V013_VERIFIER_EXCHANGE_HOST", "/verification");
        }
        if !command.status()?.success() {
            return Err("selected Workspace operation failed".into());
        }
        Ok(())
    }
}

pub(crate) fn dispatch(args: &[OsString]) -> AnyResult<()> {
    let text = args
        .iter()
        .map(|s| s.to_str().ok_or("infra arguments must be UTF-8"))
        .collect::<Result<Vec<_>, _>>()?;
    match text.as_slice() {
        ["infra-list"]=>list(None, None),
        ["infra-list",family]=>list(Some(family), None),
        ["infra-list",family,case]=>list(Some(family), Some(case)),
        ["infra-prepare",family,id,seed,root]=>prepare(family,id,seed.parse()?,Path::new(root)),
        ["infra-run",family,id,seed,mode,root,container]=>run_selected(family,id,seed.parse()?,mode,Path::new(root),container),
        _=>Err("usage: infra-list | infra-prepare FAMILY CASE SEED DIR | infra-run FAMILY CASE SEED performance|verify DIR CONTAINER".into())
    }
}
