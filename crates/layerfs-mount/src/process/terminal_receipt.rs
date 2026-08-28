{
    let status = if terminal_error.is_none() {
        "PASS"
    } else {
        "FAIL"
    };
    let terminal_error_json = terminal_error
        .as_deref()
        .map(|error| format!("\"{}\"", json(error)))
        .unwrap_or_else(|| "null".to_owned());
    let record_json = record
        .map(|record| {
            format!(
                "{{\"parent_branch_id\":\"{}\",\"operation_id\":\"{}\",\"operation_version_id\":\"{}\",\"root\":\"{}\"}}",
                hex(record.parent_branch_id.as_bytes()),
                hex(record.operation_id.as_bytes()),
                hex(record.operation_version_id.as_bytes()),
                record.root,
            )
        })
        .unwrap_or_else(|| "null".to_owned());
    let reconciled_json = reconciled
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_owned());
    let working_recorded_ns = quiescence_ns + candidate_ns + working_commit_ns;
    let complete_wall_ns = complete_started.elapsed().as_nanos();
    let body = format!(
    concat!(
        "{{\n",
        "  \"schema\": \"layerfs-mount-terminal-v2\",\n",
        "  \"status\": \"{}\",\n",
        "  \"signal\": {},\n",
        "  \"session_terminated\": {},\n",
        "  \"kernel_cache_released\": {},\n",
        "  \"terminal_snapshot_complete\": {},\n",
        "  \"error\": {},\n",
        "  \"backend\": \"layerfs-mount\",\n",
        "  \"integrity\": \"{}\",\n",
        "  \"working_storage_id\": \"{}\",\n",
        "  \"operation_id\": \"{}\",\n",
        "  \"operation_record_ref\": {},\n",
        "  \"working_commit_reconciled\": {},\n",
        "  \"working_receipt_acknowledged\": {},\n",
        "  \"branch_head_before\": {{\"branch_id\":\"{}\",\"generation\":{},\"root\":\"{}\"}},\n",
        "  \"candidate_root\": \"{}\",\n",
        "  \"ref\": \"{}\",\n",
        "  \"generation\": {},\n",
        "  \"root\": \"{}\",\n",
        "  \"executable_blake3\": \"{}\",\n",
        "  \"source_commit\": \"{}\",\n",
        "  \"source_tree\": \"{}\",\n",
        "  \"timers\": {{\"live_ns\":{},\"quiescence_ns\":{},\"candidate_ns\":{},\"working_commit_ns\":{},\"working_recorded_ns\":{},\"cleanup_ns\":{},\"complete_wall_ns\":{},\"working_recorded_equation_closed\":{}}},\n",
        "  \"callbacks\": {{\"init\":{},\"destroy\":{},\"lookup\":{},\"getattr\":{},\"create\":{},\"read\":{},\"write\":{},\"flush\":{},\"release\":{},\"fsync\":{},\"fsyncdir\":{},\"statfs\":{},\"readdir\":{},\"callback_wall_ns\":{},\"mount_lock_wait_ns\":{},\"invalidations_requested\":{},\"invalidations_succeeded\":{},\"invalidations_failed\":{},\"invalidations_unsupported\":{}}},\n",
        "  \"mounted\": {{\"checkpoints\":{},\"no_op_checkpoints\":{},\"created_then_deleted\":{},\"splices\":{},\"lookup_refs\":{},\"lookup_refs_high_water\":{},\"live_nodes\":{},\"live_nodes_high_water\":{},\"open_handles\":{},\"open_handles_high_water\":{},\"pending_nodes\":{},\"pending_nodes_high_water\":{},\"dirty_nodes\":{},\"dirty_nodes_high_water\":{},\"dirty_ranges\":{},\"dirty_ranges_high_water\":{},\"directory_cursors\":{},\"directory_changes\":{},\"directory_changes_high_water\":{},\"inode_mappings\":{},\"inode_mappings_high_water\":{},\"logical_workspace_bytes\":{},\"logical_workspace_high_water_bytes\":{},\"spool_appended_bytes\":{},\"spool_live_bytes\":{},\"spool_live_high_water_bytes\":{},\"spool_dead_bytes\":{},\"spool_physical_bytes\":{},\"spool_physical_high_water_bytes\":{},\"spool_resets\":{},\"spool_compactions\":{},\"largest_request_bytes\":{},\"operation_q_terminal_bytes\":{},\"operation_q_high_water_bytes\":{},\"materializations\":{},\"capture_scans\":{}}},\n",
        "  \"engine\": {{\"transactions_started\":{},\"transactions_committed\":{},\"transactions_rolled_back\":{},\"publication_commits\":{},\"objects_created\":{},\"objects_reused\":{},\"object_bytes_read\":{},\"object_bytes_written\":{},\"statements\":{},\"fetched_rows\":{},\"busy_events\":{},\"locked_events\":{},\"connection_mutex_wait_ns\":{},\"connections_high_water\":{},\"connections_before_drop\":{},\"connections_terminal\":{}}}\n",
        "}}\n"
    ),
    status,
    signal,
    session_terminated,
    kernel_cache_released,
    session_terminated,
    terminal_error_json,
    match integrity {
        IntegrityMode::Verified => "Verified",
        IntegrityMode::TrustedLocalDev => "TrustedLocalDev",
    },
    hex(&admission.working_storage_id),
    hex(admission.operation_id.as_bytes()),
    record_json,
    reconciled_json,
    record.is_some() && acknowledgement_error.is_none() && cleanup_error.is_none(),
    hex(admission.branch_head_before.branch_id.as_bytes()),
    admission.branch_head_before.generation,
    admission.branch_head_before.root,
    candidate_root,
        hex(accepted.branch_id.as_bytes()),
    accepted.generation,
    accepted.root,
    hex(&executable_hash),
    json(source_commit),
    json(source_tree),
    live_ns,
    quiescence_ns,
    candidate_ns,
    working_commit_ns,
    working_recorded_ns,
    cleanup_ns,
    complete_wall_ns,
    working_recorded_ns == quiescence_ns + candidate_ns + working_commit_ns,
    fuse.init,
    fuse.destroy,
    fuse.lookup,
    fuse.getattr,
    fuse.create,
    fuse.read,
    fuse.write,
    fuse.flush,
    fuse.release,
    fuse.fsync,
    fuse.fsyncdir,
    fuse.statfs,
    fuse.readdir,
    fuse.callback_wall_ns,
    fuse.mount_lock_wait_ns,
    fuse.invalidations_requested,
    fuse.invalidations_succeeded,
    fuse.invalidations_failed,
    fuse.invalidations_unsupported,
    mounted.checkpoints,
    mounted.no_op_checkpoints,
    mounted.created_then_deleted,
    mounted.splices,
    mounted.lookup_refs,
    mounted.lookup_refs_high_water,
    mounted.live_nodes,
    mounted.live_nodes_high_water,
    mounted.open_handles,
    mounted.open_handles_high_water,
    mounted.pending_nodes,
    mounted.pending_nodes_high_water,
    mounted.dirty_nodes,
    mounted.dirty_nodes_high_water,
    mounted.dirty_ranges,
    mounted.dirty_ranges_high_water,
    mounted.directory_cursors,
    mounted.directory_changes,
    mounted.directory_changes_high_water,
    mounted.inode_mappings,
    mounted.inode_mappings_high_water,
    mounted.logical_workspace_bytes,
    mounted.logical_workspace_high_water_bytes,
    mounted.spool_appended_bytes,
    mounted.spool_live_bytes,
    mounted.spool_live_high_water_bytes,
    mounted.spool_dead_bytes,
    mounted.spool_physical_bytes,
    mounted.spool_physical_high_water_bytes,
    mounted.spool_resets,
    mounted.spool_compactions,
    mounted.largest_request_bytes,
    mounted.operation_q_current_bytes,
    mounted.operation_q_high_water_bytes,
    mounted.materializations,
    mounted.capture_scans,
    engine.transactions_started,
    engine.transactions_committed,
    engine.transactions_rolled_back,
    engine.publication_commits,
    engine.objects_created,
    engine.objects_reused,
    engine.object_bytes_read,
    engine.object_bytes_written,
    engine.statements,
    engine.fetched_rows,
    engine.busy_events,
    engine.locked_events,
    engine.connection_mutex_wait_ns,
    connections_high_water,
    connections_before_drop,
    connections_terminal,
);
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(receipt)?;
    output.write_all(body.as_bytes())?;
    output.sync_all()?;
    match terminal_error {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}
