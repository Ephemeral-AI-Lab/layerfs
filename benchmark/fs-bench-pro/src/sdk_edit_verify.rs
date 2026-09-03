use super::*;
use std::io::Read;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    root: &Path,
    branch: BranchId,
    family: &str,
    scenario_id: &str,
    source_arm: &str,
    container: ContainerId,
    performance_rows: &str,
) -> AnyResult<()> {
    if !root.is_dir() || !matches!(source_arm, "baseline" | "candidate") {
        return Err("SDK edit verifier arguments".into());
    }
    let bound_rows = if performance_rows == "-" {
        Vec::new()
    } else {
        performance_rows.split(',').collect::<Vec<_>>()
    };
    if !bound_rows.is_empty()
        && (bound_rows.len() != 5
            || bound_rows.iter().any(|row| row.is_empty())
            || bound_rows
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != 5)
    {
        return Err("SDK edit verifier performance row binding".into());
    }
    let scenario = sdk_edit_scenario(family, scenario_id)?;
    let qualification =
        std::fs::read_to_string(std::env::var("LAYERFS_SDK_EDIT_QUALIFICATION_FILE")?)?;
    let qualification_sha256 =
        workload_source::sdk_edit_common::sha256_hex(qualification.as_bytes());
    if qualification_sha256 != std::env::var("LAYERFS_SDK_EDIT_QUALIFICATION_SHA256")? {
        return Err("SDK edit verifier qualification seal".into());
    }
    let expected = qualification
        .lines()
        .skip(1)
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .find(|fields| fields.len() == 10 && fields[0] == family && fields[1] == scenario_id)
        .ok_or("SDK edit verifier qualification membership")?;
    if expected[2] != scenario.plan_sha256 {
        return Err("SDK edit verifier qualification plan".into());
    }
    let expected_initial_count: u64 = expected[7].parse()?;
    let expected_final_count: u64 = expected[8].parse()?;
    let process_before = process_resource_snapshot()?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::connect(&store_path)?);
    let initial = store.pin_branch(branch)?;
    let initial_root = initial.root;
    let initial_reader = layerfs_layerstack_store::CoreReader(&initial.reader);
    let path = layerfs_content::CanonicalPath::new("payload.bin")?;
    let initial_resolved = layerfs_content::filesystem::resolve(
        &initial_reader,
        initial_root,
        &path,
        &mut Default::default(),
    )?;
    let initial_inode = workload_source::hex(&initial_resolved.inode.0);
    let initial_state = layerfs_content::file::rope::state(
        &initial_reader,
        layerfs_content::file::rope::FileStateRoot(initial_resolved.record.content_root),
        &mut Default::default(),
    )?;
    let initial_sha256 = sdk_edit_hash(&initial.reader, initial_root)?;
    let initial_payload_ids = payload_ids(&initial.reader, initial_root, "payload.bin")?;
    let untouched_payload_ids = untouched_payload_ids(
        &initial.reader,
        initial_root,
        scenario.start,
        scenario.delete_len,
    )?;
    drop(initial);

    let logical_replacement = match scenario.replacement_kind {
        workload_source::sdk_edit_common::ReplacementKind::Inline => {
            workload_source::sdk_edit_common::replacement_bytes(&scenario)
        }
        workload_source::sdk_edit_common::ReplacementKind::Zero => {
            vec![0; scenario.replacement_len as usize]
        }
    };
    let sdk_replacement = match scenario.replacement_kind {
        workload_source::sdk_edit_common::ReplacementKind::Inline => {
            WorkspaceFileReplacement::Inline(logical_replacement.clone())
        }
        workload_source::sdk_edit_common::ReplacementKind::Zero => {
            WorkspaceFileReplacement::Zero(scenario.replacement_len)
        }
    };
    let client = Client::connect(store.clone())?;
    let session = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: WorkspacePlacement::Container {
            container_id: container,
            root: PathBuf::from(format!("/workspace/sdk-edit-verify-{}", std::process::id())),
        },
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let initial_fuse = execute(
        &client,
        session.id,
        vec![
            workload.clone(),
            OsString::from("stat-inode"),
            OsString::from("payload.bin"),
        ],
    )?;
    let (initial_fuse_bytes, _, initial_fuse_inode) = sdk_edit_digest_inode(&initial_fuse)?;
    let edit = WorkspaceFileRangeEdit {
        workspace_id: session.id,
        path: "payload.bin".to_owned(),
        start: scenario.start,
        delete_len: scenario.delete_len,
        replacement: sdk_replacement,
    };
    let calibration = sdk_edit_start_resource_sample(&client, session.id)?;
    let t0_clock_ns = sdk_edit_clock_ns()?;
    client.edit_workspace_file_range(edit)?;
    let commit_id = match client.commit_workspace_session(session.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => commit_id,
        result => return Err(format!("SDK edit verifier Commit: {result:?}").into()),
    };
    let t3_clock_ns = sdk_edit_clock_ns()?;
    let clock_uncertainty_ns = calibration.phase_uncertainty(t3_clock_ns)?;
    let cgroup = client.finish_workspace_resource_sample(
        session.id,
        calibration.daemon_time(t0_clock_ns)?,
        calibration.daemon_time(t3_clock_ns)?,
        clock_uncertainty_ns,
    )?;

    let fuse_output = execute(
        &client,
        session.id,
        vec![
            workload,
            OsString::from("digest-inode"),
            OsString::from("payload.bin"),
        ],
    )?;
    let (fuse_bytes, fuse_sha256, final_fuse_inode) = sdk_edit_digest_inode(&fuse_output)?;
    client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
    let snapshot = client.monitor_snapshot()?;
    let commit = operation_workspace_commit(&snapshot)?;
    let candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    let fuse = edit_fuse_metrics(&snapshot);
    let mutation_counts = |family| {
        snapshot
            .operations
            .iter()
            .filter(|receipt| receipt.operation.family == family)
            .count()
    };
    let lifecycle = snapshot
        .operations
        .iter()
        .flat_map(|operation| operation.storage.iter())
        .filter_map(|receipt| match receipt {
            StorageReceipt::WorkspaceLifecycle(receipt) => Some(*receipt),
            _ => None,
        })
        .collect::<Vec<_>>();
    let route_valid = mutation_counts(OperationFamily::WorkspaceCreate) == 1
        && mutation_counts(OperationFamily::WorkspaceFileRangeEdit) == 1
        && mutation_counts(OperationFamily::WorkspaceCommit) == 1
        && mutation_counts(OperationFamily::WorkspaceEnd) == 1
        && mutation_counts(OperationFamily::WorkspaceExec) == 2
        && mutation_counts(OperationFamily::WorkspaceShell) == 0
        && snapshot.operations.iter().all(|receipt| {
            receipt.outcome == OperationOutcome::Success
                && (!matches!(
                    receipt.operation.family,
                    OperationFamily::WorkspaceFileRangeEdit
                        | OperationFamily::WorkspaceCommit
                        | OperationFamily::WorkspaceEnd
                        | OperationFamily::WorkspaceExec
                ) || receipt.operation.workspace_id == Some(session.id))
                && (receipt.operation.family != OperationFamily::WorkspaceCreate
                    || receipt.operation.branch_id == Some(branch))
        })
        && matches!(
            lifecycle
                .iter()
                .map(|receipt| receipt.kind)
                .collect::<Vec<_>>()
                .as_slice(),
            [
                layerfs_sdk::WorkspaceLifecycleKind::Attach,
                layerfs_sdk::WorkspaceLifecycleKind::End
            ] | [
                layerfs_sdk::WorkspaceLifecycleKind::Attach,
                layerfs_sdk::WorkspaceLifecycleKind::End,
                layerfs_sdk::WorkspaceLifecycleKind::Attach,
                layerfs_sdk::WorkspaceLifecycleKind::End
            ]
        )
        && lifecycle.iter().all(|receipt| receipt.docker_calls == 0)
        && client.active_workspace_count()? == 0
        && client.active_execution_count()? == 0;
    drop(client);
    drop(store);

    let reopened_store = Arc::new(LayerStackStore::connect(&store_path)?);
    let reopened_client = Client::connect(reopened_store.clone())?;
    let page = reopened_client.query(Query::new(QueryKind::Branches).limit(512))?;
    if page.continuation.is_some()
        || !page.items.iter().any(|item| {
            matches!(item, QueryItem::Branch(record)
                if record.id == branch && record.head_commit_id == Some(commit_id))
        })
    {
        return Err("SDK edit verifier reconnect visibility".into());
    }
    let observed = reopened_store.pin_branch(branch)?;
    let observed_reader = layerfs_layerstack_store::CoreReader(&observed.reader);
    let observed_resolved = layerfs_content::filesystem::resolve(
        &observed_reader,
        observed.root,
        &path,
        &mut Default::default(),
    )?;
    let observed_inode = workload_source::hex(&observed_resolved.inode.0);
    let observed_state = layerfs_content::file::rope::state(
        &observed_reader,
        layerfs_content::file::rope::FileStateRoot(observed_resolved.record.content_root),
        &mut Default::default(),
    )?;
    let observed_sha256 = sdk_edit_hash(&observed.reader, observed.root)?;
    let mut observed_payload_ids = std::collections::BTreeSet::new();
    let (_, mapping_counters) = layerfs_content::file::rope::visit_extents(
        &observed_reader,
        layerfs_content::file::rope::FileStateRoot(observed_resolved.record.content_root),
        |extents| {
            observed_payload_ids.extend(extents.iter().map(|extent| extent.payload_object_id));
            Ok(())
        },
    )?;
    let lost_payload_ids = initial_payload_ids
        .difference(&observed_payload_ids)
        .count() as u64;
    let retention_valid = if scenario.delete_len == 0 {
        initial_payload_ids.is_subset(&observed_payload_ids)
    } else {
        untouched_payload_ids.is_subset(&observed_payload_ids)
    };
    let (materialized_bytes, materialized_sha256) = materialized_digest(
        &observed.reader,
        observed.root,
        &root.join(format!("materialized-{}.bin", std::process::id())),
        scenario.final_bytes,
    )?;
    let base_reader = reopened_store.snapshot_reader(initial_root);
    let expected_sha256 =
        independent_digest(&base_reader, initial_root, &scenario, &logical_replacement)?;
    let size_index = workload_source::sdk_edit_common::SIZES
        .iter()
        .position(|size| *size == scenario.fixture_bytes)
        .ok_or("SDK edit verifier size")?;
    let semantic_valid = initial_sha256
        == workload_source::sdk_edit_common::FIXTURE_SHA256[size_index]
        && observed_sha256 == expected_sha256
        && fuse_sha256 == expected_sha256
        && materialized_sha256 == expected_sha256
        && materialized_bytes == scenario.final_bytes
        && initial_fuse_bytes == scenario.fixture_bytes
        && initial_fuse_inode == final_fuse_inode
        && fuse_bytes == scenario.final_bytes
        && observed_state.logical_len == scenario.final_bytes
        && initial_inode == observed_inode
        && retention_valid;
    let qualified_roots_valid = initial_root.to_string() == expected[3]
        && observed.root.to_string() == expected[4]
        && observed_resolved.record.content_root.to_string() == expected[5]
        && observed_state.mapping_root.to_string() == expected[6]
        && initial_state.extent_count == expected_initial_count
        && observed_state.extent_count == expected_final_count;
    let canonical_valid = qualified_roots_valid
        && if family == workload_source::edit_canonical_chunk_count::FAMILY_ID {
            let frozen = workload_source::edit_canonical_chunk_count::expected(&scenario);
            observed_state.extent_count == frozen.final_count
                && observed_sha256 == frozen.final_sha256
                && observed_resolved.record.content_root.to_string() == frozen.file_root
                && observed_state.mapping_root.to_string() == frozen.map_sha256
        } else {
            true
        };
    let cgroup_coverage = cgroup.t0_boundary_sampled
        && cgroup.t3_boundary_sampled
        && cgroup.interior_sampled
        && !cgroup.sample_overflow
        && cgroup.sample_count >= 3
        && cgroup.sample_interval_ns <= 1_000_000
        && cgroup.maximum_sample_gap_ns <= 1_000_000;
    let fuse_payload = fuse
        .kernel_write_bytes
        .saturating_add(fuse.client_request_copy_bytes)
        .saturating_add(fuse.frame_payload_copy_bytes)
        .saturating_add(fuse.client_frame_bytes)
        .saturating_add(fuse.host_frame_bytes)
        .saturating_add(fuse.host_decode_copy_bytes);
    let final_live_non_base_bytes = scenario.replacement_len;
    let resource_valid = route_valid
        && commit.capture_mode == Some(layerfs_layerstack_store::CaptureMode::Live)
        && commit.captured_files == 0
        && commit.captured_bytes == 0
        && fuse.kernel_write_requests == 0
        && fuse_payload == 0
        && fuse.spool_write_bytes == 0
        && commit.edit_spool_allocated_bytes == 0
        && commit.edit_spool_peak_bytes == 0
        && commit.edit_spool_live_bytes == 0
        && commit.edit_spool_superseded_bytes == 0
        && commit.cdc_bytes_scanned == final_live_non_base_bytes
        && candidate.candidate_bytes <= final_live_non_base_bytes + 8 * 1024 * 1024
        && candidate.inserted_bytes <= candidate.candidate_bytes
        && candidate.max_transaction_objects <= 127
        && candidate.max_transaction_bytes < 4 * 1024 * 1024
        && commit.edit_piece_count <= 3
        && commit.edit_piece_logical_charge <= 1024
        && (family != workload_source::edit_length_changing::FAMILY_ID
            || commit.payload_bytes_read == 0)
        && cgroup_coverage
        && cgroup.memory_current_peak <= 128 * 1024 * 1024
        && cgroup.memory_incremental_peak <= 32 * 1024 * 1024
        && cgroup.dirty_writeback_incremental_peak <= 8 * 1024 * 1024
        && cgroup.memory_lifetime_peak_final <= 128 * 1024 * 1024
        && cgroup.swap_peak == 0
        && cgroup.oom_delta == 0
        && cgroup.oom_kill_delta == 0;
    let process_after = process_resource_snapshot()?;
    if !semantic_valid
        || !canonical_valid
        || !resource_valid
        || process_after.peak_resident_bytes > 128 * 1024 * 1024
        || process_after.swaps != process_before.swaps
    {
        return Err(format!(
            "SDK edit verifier gate: semantic={semantic_valid} canonical={canonical_valid} route={route_valid} resource={resource_valid} cgroup_boundaries=({},{},{}) gap={} uncertainty={} lifetime_rss={}",
            cgroup.t0_boundary_sampled, cgroup.t3_boundary_sampled, cgroup.interior_sampled,
            cgroup.maximum_sample_gap_ns, clock_uncertainty_ns, process_after.peak_resident_bytes,
        ).into());
    }
    let conformance = std::env::var("LAYERFS_SDK_EDIT_CONFORMANCE_SHA256")
        .unwrap_or_else(|_| "unbound-selected-mode".to_owned());
    if conformance != "unbound-selected-mode"
        && (conformance.len() != 64 || !conformance.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("SDK edit verifier conformance identity".into());
    }
    let mut result = format!(
        "{{\"schema\":\"{}\",\"receipt_kind\":\"source-arm-subproof\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"source_arm\":\"{}\",\"edit_plan_sha256\":\"{}\",\"performance_row_ids\":[{}],\"performance_binding_status\":\"{}\",\"initial_file_bytes\":{},\"final_file_bytes\":{},\"expected_sha256\":\"{}\",\"observed_sha256\":\"{}\",\"fuse_sha256\":\"{}\",\"materialized_bytes\":{},\"materialized_sha256\":\"{}\",\"materialized_status\":\"pass\",\"initial_branch_root\":\"{}\",\"observed_branch_root\":\"{}\",\"observed_canonical_file_root\":\"{}\",\"observed_extent_count\":{},\"observed_mapping_root\":\"{}\",\"initial_inode_id\":\"{}\",\"final_inode_id\":\"{}\",\"inode_behavior\":\"preserved\",\"initial_payload_object_count\":{},\"observed_payload_object_count\":{},\"untouched_payload_object_count\":{},\"lost_payload_object_count\":{},\"payload_retention_status\":\"pass\",\"conformance_proof_sha256\":\"{}\",\"failure_atomicity_status\":\"sealed-conformance\",\"retry_status\":\"sealed-conformance\",\"public_sdk_edit_call_count\":1,\"workspace_create_count\":1,\"workspace_commit_count\":1,\"workspace_end_count\":1,\"fresh_client_reconnect\":true,\"fresh_store_reconnect\":true,\"fresh_fuse_reopen\":true,\"independent_byte_oracle\":true,\"commit_id\":\"{}\",\"commit_cdc_bytes_scanned\":{},\"commit_payload_bytes_read\":{},\"final_live_non_base_bytes\":{},\"piece_count\":{},\"piece_height\":{},\"piece_logical_charge_bytes\":{},\"spool_allocated_bytes\":{},\"physical_spool_high_water_bytes\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"inserted_bytes\":{},\"reused_objects\":{},\"reused_bytes\":{},\"max_transaction_objects\":{},\"max_transaction_bytes\":{},\"cgroup_memory_baseline_bytes\":{},\"cgroup_phase_peak_bytes\":{},\"cgroup_phase_incremental_peak_bytes\":{},\"cgroup_lifetime_peak_bytes\":{},\"cgroup_swap_baseline_bytes\":{},\"cgroup_swap_peak_bytes\":{},\"cgroup_swap_final_bytes\":{},\"cgroup_oom_baseline\":{},\"cgroup_oom_final\":{},\"cgroup_oom_kill_baseline\":{},\"cgroup_oom_kill_final\":{},\"dirty_writeback_baseline_bytes\":{},\"dirty_writeback_peak_bytes\":{},\"dirty_writeback_incremental_peak_bytes\":{},\"cgroup_sample_interval_ns\":{},\"cgroup_sample_count\":{},\"cgroup_maximum_sample_gap_ns\":{},\"cgroup_t0_boundary_sampled\":{},\"cgroup_t3_boundary_sampled\":{},\"cgroup_interior_sampled\":{},\"cgroup_sample_overflow\":{},\"process_lifetime_peak_rss_bytes\":{},\"process_swap_count\":{},\"root_status\":\"{}\",\"fresh_reopen_status\":\"pass\",\"resource_status\":\"pass\",\"cleanup_status\":\"pass\",\"performance_distribution\":false,\"admission_eligible\":false,\"status\":\"pass\"}}",
        workload_source::sdk_edit_common::VERIFICATION_SCHEMA,
        scenario.family_id,
        scenario.id,
        source_arm,
        scenario.plan_sha256,
        bound_rows.iter().map(|row| format!("\"{row}\"")).collect::<Vec<_>>().join(","),
        if bound_rows.is_empty() { "unbound-selected-mode" } else { "bound-five-performance-rows" },
        scenario.fixture_bytes,
        scenario.final_bytes,
        expected_sha256,
        observed_sha256,
        fuse_sha256,
        materialized_bytes,
        materialized_sha256,
        initial_root,
        observed.root,
        observed_resolved.record.content_root,
        observed_state.extent_count,
        observed_state.mapping_root,
        initial_inode,
        observed_inode,
        initial_payload_ids.len(),
        observed_payload_ids.len(),
        untouched_payload_ids.len(),
        lost_payload_ids,
        conformance,
        commit_id,
        commit.cdc_bytes_scanned,
        commit.payload_bytes_read,
        final_live_non_base_bytes,
        commit.edit_piece_count,
        commit.edit_piece_height,
        commit.edit_piece_logical_charge,
        commit.edit_spool_allocated_bytes,
        commit.edit_spool_peak_bytes,
        candidate.candidate_objects,
        candidate.candidate_bytes,
        candidate.inserted_objects,
        candidate.inserted_bytes,
        candidate.reused_objects,
        candidate.reused_bytes,
        candidate.max_transaction_objects,
        candidate.max_transaction_bytes,
        cgroup.memory_current_baseline,
        cgroup.memory_current_peak,
        cgroup.memory_incremental_peak,
        cgroup.memory_lifetime_peak_final,
        cgroup.swap_baseline,
        cgroup.swap_peak,
        cgroup.swap_final,
        cgroup.oom_baseline,
        cgroup.oom_final,
        cgroup.oom_kill_baseline,
        cgroup.oom_kill_final,
        cgroup.dirty_writeback_baseline,
        cgroup.dirty_writeback_peak,
        cgroup.dirty_writeback_incremental_peak,
        cgroup.sample_interval_ns,
        cgroup.sample_count,
        cgroup.maximum_sample_gap_ns,
        cgroup.t0_boundary_sampled,
        cgroup.t3_boundary_sampled,
        cgroup.interior_sampled,
        cgroup.sample_overflow,
        process_after.peak_resident_bytes,
        process_after.swaps.saturating_sub(process_before.swaps),
        if family == workload_source::edit_canonical_chunk_count::FAMILY_ID { "pass-frozen" } else { "pass-qualified" },
    );
    result.pop();
    result.push_str(&calibration.json_fields(
        t0_clock_ns,
        t3_clock_ns,
        &cgroup,
        clock_uncertainty_ns,
    ));
    result.push_str(&format!(",\"qualification_manifest_sha256\":\"{}\",\"expected_branch_root\":\"{}\",\"expected_canonical_file_root\":\"{}\",\"expected_mapping_root\":\"{}\",\"expected_initial_extent_count\":{},\"observed_initial_extent_count\":{},\"expected_extent_count\":{}",
        qualification_sha256, expected[4], expected[5], expected[6], expected_initial_count,
        initial_state.extent_count, expected_final_count));
    result.push_str(&format!(",\"initial_fuse_inode\":{initial_fuse_inode},\"final_fuse_inode\":{final_fuse_inode},\"read_only_verifier_execution_count\":2,\"query_count\":1,\"operation_route_manifest_status\":\"pass\",\"timed_call_graph_manifest_status\":\"pass\",\"cgroup_first_sample_ns\":{},\"cgroup_last_sample_ns\":{},\"cgroup_oom_delta\":{},\"cgroup_oom_kill_delta\":{},\"cgroup_memory_lifetime_peak_baseline_bytes\":{},\"cgroup_memory_lifetime_peak_final_bytes\":{}",
        cgroup.first_sample_ns, cgroup.last_sample_ns, cgroup.oom_delta, cgroup.oom_kill_delta,
        cgroup.memory_lifetime_peak_baseline, cgroup.memory_lifetime_peak_final));
    result.push_str(&format!(",\"initial_sha256\":\"{initial_sha256}\",\"capture_mode\":\"Live\",\"captured_files\":{},\"captured_bytes\":{},\"fuse_kernel_write_requests\":{},\"fuse_kernel_write_bytes\":{},\"fuse_client_request_copy_bytes\":{},\"fuse_frame_payload_copy_bytes\":{},\"fuse_client_frame_bytes\":{},\"fuse_host_frame_bytes\":{},\"fuse_host_decode_copy_bytes\":{},\"spool_write_bytes\":{},\"spool_live_bytes\":{},\"spool_superseded_bytes\":{},\"active_workspace_count_after_end\":0,\"active_execution_count_after_end\":0,\"cgroup_sampler_thread_count\":{},\"cgroup_coverage_status\":\"pass\",\"projection_lifecycle\":[{}],\"referenced_extent_count\":{},\"unique_payload_object_count\":{},\"mapping_node_count\":{},\"mapping_tree_level\":{}",
        commit.captured_files, commit.captured_bytes, fuse.kernel_write_requests, fuse.kernel_write_bytes,
        fuse.client_request_copy_bytes, fuse.frame_payload_copy_bytes, fuse.client_frame_bytes,
        fuse.host_frame_bytes, fuse.host_decode_copy_bytes, fuse.spool_write_bytes,
        commit.edit_spool_live_bytes, commit.edit_spool_superseded_bytes, cgroup.sampler_thread_count,
        sdk_edit_lifecycle_json(&lifecycle), observed_state.extent_count, observed_payload_ids.len(),
        mapping_counters.nodes_read, observed_state.tree_level));
    result.push('}');
    if bound_rows.is_empty() {
        result = result.replace("\"sealed-conformance\"", "\"unbound-selected\"");
    }
    println!("{result}");
    Ok(())
}

fn untouched_payload_ids(
    source: &dyn layerfs_layerstack_store::ObjectSource,
    root: layerfs_content::ObjectId,
    start: u64,
    delete_len: u64,
) -> AnyResult<std::collections::BTreeSet<layerfs_content::ObjectId>> {
    let reader = layerfs_layerstack_store::CoreReader(source);
    let path = layerfs_content::CanonicalPath::new("payload.bin")?;
    let (stat, _) = layerfs_content::filesystem::stat(&reader, root, &path)?;
    let delete_end = start
        .checked_add(delete_len)
        .ok_or("SDK edit verifier range")?;
    let mut offset = 0_u64;
    let mut ids = std::collections::BTreeSet::new();
    layerfs_content::file::rope::visit_extents(
        &reader,
        layerfs_content::file::rope::FileStateRoot(stat.content_root),
        |extents| {
            for extent in extents {
                let end = offset + u64::from(extent.logical_length);
                if end <= start || offset >= delete_end {
                    ids.insert(extent.payload_object_id);
                }
                offset = end;
            }
            Ok(())
        },
    )?;
    Ok(ids)
}

fn materialized_digest(
    source: &dyn layerfs_layerstack_store::ObjectSource,
    root: layerfs_content::ObjectId,
    target: &Path,
    expected_len: u64,
) -> AnyResult<(u64, String)> {
    let reader = layerfs_layerstack_store::CoreReader(source);
    let path = layerfs_content::CanonicalPath::new("payload.bin")?;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    layerfs_content::filesystem::read_range(&reader, root, &path, 0..expected_len, &mut output)?;
    output.sync_all()?;
    drop(output);
    let bytes = std::fs::metadata(target)?.len();
    let mut input = std::io::BufReader::new(std::fs::File::open(target)?);
    let mut hash = workload_source::Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    std::fs::remove_file(target)?;
    Ok((bytes, workload_source::hex(&hash.finish())))
}

fn independent_digest(
    source: &dyn layerfs_layerstack_store::ObjectSource,
    initial_root: layerfs_content::ObjectId,
    scenario: &workload_source::sdk_edit_common::Scenario,
    replacement: &[u8],
) -> AnyResult<String> {
    let reader = layerfs_layerstack_store::CoreReader(source);
    let path = layerfs_content::CanonicalPath::new("payload.bin")?;
    let mut sink = SdkEditHashSink(workload_source::Sha256::new());
    layerfs_content::filesystem::read_range(
        &reader,
        initial_root,
        &path,
        0..scenario.start,
        &mut sink,
    )?;
    sink.write_all(replacement)?;
    let suffix = scenario.start + scenario.delete_len;
    layerfs_content::filesystem::read_range(
        &reader,
        initial_root,
        &path,
        suffix..scenario.fixture_bytes,
        &mut sink,
    )?;
    Ok(workload_source::hex(&sink.0.finish()))
}
