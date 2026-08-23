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

pub(super) struct HistoryProjectionSession {
    worker: ProjectionWorker,
    mailbox: ProjectionMailbox,
    output_root: PathBuf,
    parent: Roots,
    parent_digest: [u8; 32],
    preparation_q_high_water: u64,
}

#[derive(Clone, Copy)]
pub(super) struct HistoryProjectionObservation {
    pub submitted: u64,
    pub started: u64,
    pub published: u64,
    pub coalesced: u64,
    pub exact_root: ObjectId,
    pub latest_root: ObjectId,
    pub clone_calls: u64,
    pub full_fallbacks: u64,
    pub range_fetches: u64,
    pub written_bytes: u64,
    pub seed_rotations: u64,
    pub max_buffer_bytes: u64,
    pub q_high_water: u64,
    pub q_current: u64,
    pub temp_residue: u64,
}

pub(super) fn history_projection_start(
    store: &mut Store,
    parent_head: &VisibleHead,
    parent_digest: [u8; 32],
    _parent_occurrence: [u8; 32],
    output_root: &Path,
) -> AnyResult<HistoryProjectionSession> {
    let mut roots_metrics = Metrics::default();
    let parent = g4_roots(store, parent_head.1, &mut roots_metrics)?;
    finish_q(&mut roots_metrics)?;
    fs::create_dir(output_root)?;
    fs::set_permissions(output_root, fs::Permissions::from_mode(0o700))?;
    let directory = open_dir(output_root)?;
    if stat_at(&directory, DESTINATION_NAME)?.is_some() {
        return Err(CoreError::PublicationConflict.into());
    }
    let mut preparation_metrics = Metrics::default();
    let (length, references, digest) = materialize_for_preparation(
        store,
        parent.namespace,
        &output_root.join(DESTINATION_NAME),
        &mut preparation_metrics,
    )?;
    finish_q(&mut preparation_metrics)?;
    if length != parent.length || references != parent.references || digest != parent_digest {
        return Err(CoreError::PublicationConflict.into());
    }
    let active = open_projection_seed(&directory, parent, parent_digest)?;
    let reader = Store::open_existing_read_only(&store.path, SELECTED_PROFILE)?;
    let token_authority = ProjectionTokenAuthority::from_store(&reader);
    let mut worker = ProjectionWorker {
        directory_path: output_root.to_path_buf(),
        directory,
        store: Some(reader),
        active,
        counters: ProjectionCounters::default(),
        apply_native: Counters::default(),
        apply_rename_acknowledged: false,
        shutdown_rendezvous: None,
    };
    worker.initialize_reader()?;
    let mut mailbox = ProjectionMailbox {
        in_flight: false,
        pending: None,
        shutdown: false,
        release_first: true,
        submitted: 0,
        coalesced: 0,
        started: 0,
        published: 0,
        cancelled: 0,
        failed: 0,
        stale: 0,
        sqlite_busy_errors: 0,
        sqlite_locked_errors: 0,
        worker_error: None,
        token_authority,
        exact_ordinal: None,
        same_size_ordinal: None,
        count_storm_ordinal: None,
        isolated_sparse_accepted: false,
    };
    mailbox.submit(ProjectionRequest {
        parent,
        parent_digest,
        target: parent,
        target_digest: parent_digest,
        plan: ProjectionPlan::Ranges(Vec::new()),
        contended: false,
        policy: RequestPolicy::ExactEveryRoot { ordinal: 0 },
        force_full_fallback: false,
        token: None,
        edge_authenticated: false,
        end_to_end: None,
        fault: ProjectionFault::None,
    })?;
    let exact = mailbox.take()?.ok_or(CoreError::MissingObject)?;
    worker.apply(exact, Instant::now(), None)?;
    mailbox.complete()?;
    Ok(HistoryProjectionSession {
        worker,
        mailbox,
        output_root: output_root.to_path_buf(),
        parent,
        parent_digest,
        preparation_q_high_water: roots_metrics
            .q_high_water
            .max(preparation_metrics.q_high_water),
    })
}

pub(super) fn history_projection_latest(
    mut session: HistoryProjectionSession,
    writer: &mut Store,
    target_head: &VisibleHead,
    target_digest: [u8; 32],
    parent_source: &Path,
    target_source: &Path,
    range: std::ops::Range<u64>,
) -> AnyResult<HistoryProjectionObservation> {
    let mut metrics = Metrics::default();
    let target = g4_roots(
        session
            .worker
            .store
            .as_ref()
            .ok_or(CoreError::ValidationAuthorityUnavailable)?,
        target_head.1,
        &mut metrics,
    )?;
    finish_q(&mut metrics)?;
    let token = prove_and_mint_projection_edge(
        writer,
        target_head,
        session.parent,
        session.parent_digest,
        parent_source,
        target,
        target_digest,
        target_source,
        &range,
        RequestPolicy::LatestFollowing {
            stream: LatestStream::SameSize,
            ordinal: 0,
        },
    )?;
    session.mailbox.submit(ProjectionRequest {
        parent: session.parent,
        parent_digest: session.parent_digest,
        target,
        target_digest,
        plan: projection_plan(std::iter::once(range), target.length)?,
        contended: false,
        policy: RequestPolicy::LatestFollowing {
            stream: LatestStream::SameSize,
            ordinal: 0,
        },
        force_full_fallback: false,
        token: Some(token),
        edge_authenticated: false,
        end_to_end: None,
        fault: ProjectionFault::None,
    })?;
    let latest = session.mailbox.take()?.ok_or(CoreError::MissingObject)?;
    session.worker.apply(latest, Instant::now(), None)?;
    session.mailbox.complete()?;
    session.worker.counters.q_terminal = q_current();
    let temp_residue = count_residue(&session.output_root, ".g3-tmp-")?;
    if !session.mailbox.equations_hold()
        || session.mailbox.submitted != 2
        || session.mailbox.started != 2
        || session.mailbox.published != 2
        || session.mailbox.coalesced != 0
        || session.mailbox.in_flight
        || session.mailbox.pending.is_some()
        || session.worker.active.identity.namespace_root != target.namespace
        || session.worker.counters.full_fallbacks != 0
        || session.worker.counters.range_fetches != 1
        || session.worker.counters.written_bytes == 0
        || session.worker.counters.q_terminal != 0
        || temp_residue != 0
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let observation = HistoryProjectionObservation {
        submitted: session.mailbox.submitted,
        started: session.mailbox.started,
        published: session.mailbox.published,
        coalesced: session.mailbox.coalesced,
        exact_root: session.parent.namespace,
        latest_root: target.namespace,
        clone_calls: session.worker.counters.clone_calls,
        full_fallbacks: session.worker.counters.full_fallbacks,
        range_fetches: session.worker.counters.range_fetches,
        written_bytes: session.worker.counters.written_bytes,
        seed_rotations: session.worker.counters.seed_rotations,
        max_buffer_bytes: session.worker.counters.max_buffer_bytes,
        q_high_water: session
            .worker
            .counters
            .q_high_water
            .max(session.preparation_q_high_water),
        q_current: session.worker.counters.q_terminal,
        temp_residue,
    };
    drop(session.worker);
    drop(session.mailbox);
    let directory = open_dir(&session.output_root)?;
    if !unlink_at(&directory, DESTINATION_NAME)? {
        return Err(CoreError::MissingObject.into());
    }
    sync_fd(&directory)?;
    drop(directory);
    fs::remove_dir(&session.output_root)?;
    Ok(observation)
}

#[cfg(test)]
#[test]
fn g5_history_projection_start_retains_visible_seed() {
    let root = env::temp_dir().join(format!(
        "g5-history-projection-seed-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir(&root).expect("test root");
    let source = root.join("source");
    write_fixture(&source, G5_PROJECTION_MECHANISM_BYTES).expect("source");
    let mut store = Store::open(&root.join("store.sqlite"), SELECTED_PROFILE).expect("store");
    let mut metrics = Metrics::default();
    let parent = build_and_publish_parent(&mut store, &source, &mut metrics).expect("parent");
    finish_q(&mut metrics).expect("parent Q");
    let head = store.current_head().expect("head read").expect("head");
    assert_eq!(head.1, parent.namespace);
    let (_, digest, _) = hash_file(&source).expect("digest");
    let (_, occurrence) = source_cdc_sequence(&source).expect("occurrence");
    let occurrence = occurrence
        .parse::<ObjectId>()
        .expect("occurrence id")
        .to_bytes();
    let projection = root.join("projection");
    let session = history_projection_start(
        &mut store,
        &head,
        digest,
        occurrence,
        &projection,
    )
    .expect("projection start");
    assert!(
        stat_at(&open_dir(&projection).expect("directory"), DESTINATION_NAME)
            .expect("stat")
            .is_some()
    );
    drop(session);
    drop(store);
    fs::remove_dir_all(root).expect("cleanup");
}
