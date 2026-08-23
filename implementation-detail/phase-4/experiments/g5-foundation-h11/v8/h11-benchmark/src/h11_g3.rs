pub(super) struct H11NativeObservation {
    pub wall_ns: u128,
    pub verification_ns: u128,
    pub cleanup_ns: u128,
    pub user_us: i128,
    pub system_us: i128,
    pub voluntary_switches: i128,
    pub involuntary_switches: i128,
    pub references: u64,
    pub output_digest: [u8; 32],
    pub sql_query_calls: u64,
    pub sql_rows_returned: u64,
    pub row_blob_reads: u64,
    pub row_blob_writes: u64,
    pub canonical_bytes_authenticated: u64,
    pub q_high_water: u64,
    pub q_current: u64,
    pub write_calls: u64,
    pub write_bytes: u64,
    pub data_sync_calls: u64,
    pub metadata_sync_calls: u64,
    pub rename_calls: u64,
    pub directory_sync_calls: u64,
    pub temp_files_created: u64,
    pub temp_files_removed: u64,
    pub max_single_buffer_bytes: u64,
}

pub(super) fn h11_materialize_current(
    store: &mut Store,
    head: &VisibleHead,
    expected_digest: [u8; 32],
    expected_sequence: [u8; 32],
    output_root: &Path,
) -> AnyResult<H11NativeObservation> {
    let mut roots_metrics = Metrics::default();
    let roots = g4_roots(store, head.1, &mut roots_metrics)?;
    finish_q(&mut roots_metrics)?;
    let result = g4_materialize(
        store,
        head,
        roots,
        expected_digest,
        expected_sequence,
        output_root,
        G4NativeAlgorithm::BatchedCandidate,
        G4NativeFault::default(),
    )?;
    if result.writer.short_writes != 0
        || result.writer.errors != 0
        || result.metrics.q_current != 0
        || result.temp_files_created != result.temp_files_removed
    {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok(H11NativeObservation {
        wall_ns: result.wall_ns,
        verification_ns: result.verification_ns,
        cleanup_ns: result.cleanup_ns,
        user_us: result.usage.user_us,
        system_us: result.usage.system_us,
        voluntary_switches: result.usage.voluntary_switches,
        involuntary_switches: result.usage.involuntary_switches,
        references: result.reconstructed.references,
        output_digest: result.reconstructed.output_digest,
        sql_query_calls: result.metrics.sql_query_calls,
        sql_rows_returned: result.metrics.sql_rows_returned,
        row_blob_reads: result.metrics.row_blob_reads,
        row_blob_writes: result.metrics.row_blob_writes,
        canonical_bytes_authenticated: result.metrics.canonical_bytes_authenticated,
        q_high_water: result.metrics.q_high_water,
        q_current: result.metrics.q_current,
        write_calls: result.writer.calls,
        write_bytes: result.writer.bytes,
        data_sync_calls: result.data_sync_calls,
        metadata_sync_calls: result.metadata_sync_calls,
        rename_calls: result.rename_calls,
        directory_sync_calls: result.directory_sync_calls,
        temp_files_created: result.temp_files_created,
        temp_files_removed: result.temp_files_removed,
        max_single_buffer_bytes: result.max_single_buffer_bytes,
    })
}





