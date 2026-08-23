use std::collections::BTreeSet;

const H11_MAX_REVISION: u64 = 1_001;
const H11_HISTORY_POINTS: [u64; 4] = [1, 10, 100, 1_000];
const H11_SAMPLES: [u64; 2] = [1, 2];
const H11_RANGE_BYTES: u64 = 64 * 1024;
const H11_CACHE_PAGES: i64 = 1_500;

#[derive(Clone, Copy)]
struct H11Expected {
    revision: u64,
    root: ObjectId,
    transition: ObjectId,
    file: ObjectId,
    output_digest: [u8; 32],
    occurrence_digest: [u8; 32],
    closure_digest: [u8; 32],
    range_digest: [u8; 32],
}

#[derive(Clone, Copy, Default)]
struct H11Operation {
    wall_ns: u128,
    sql_queries: u64,
    sql_rows: u64,
    row_blob_reads: u64,
    row_blob_writes: u64,
    canonical_authenticated: u64,
    canonical_new_bytes: u64,
    mapping_rewritten: u64,
    objects_created: u64,
    objects_reused: u64,
    transactions: u64,
    commits: u64,
    q_high_water: u64,
    q_current: u64,
}

#[derive(Default)]
struct H11History {
    edit_ns: Vec<u128>,
    objects_created: u64,
    objects_reused: u64,
    canonical_new_bytes: u64,
    mapping_rewritten: u64,
    transactions: u64,
    commits: u64,
    q_high_water: u64,
}

#[derive(Default)]
struct H11ObjectStats {
    stored_objects: u64,
    stored_canonical_bytes: u64,
    stored_mapping_bytes: u64,
    current_live_objects: u64,
    current_live_canonical_bytes: u64,
    current_live_mapping_bytes: u64,
    current_unreachable_objects: u64,
    retained_live_objects: u64,
    retained_live_canonical_bytes: u64,
    retained_live_mapping_bytes: u64,
    retained_unreachable_objects: u64,
}

fn h11_digest(value: &str) -> AnyResult<[u8; 32]> {
    Ok(value.parse::<ObjectId>()?.to_bytes())
}

fn h11_expected(path: &Path) -> AnyResult<Vec<H11Expected>> {
    let input = fs::read_to_string(path)?;
    let mut rows = Vec::new();
    for (index, line) in input.lines().enumerate() {
        if index == 0 {
            if line != "revision\troot\ttransition\tfile\toutput_digest\toccurrence_digest\tclosure_digest\trange_digest" {
                return Err(CoreError::InvalidRecord("H11 expected manifest header").into());
            }
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 8 {
            return Err(CoreError::InvalidRecord("H11 expected manifest row").into());
        }
        let revision = fields[0].parse::<u64>()?;
        if revision != u64::try_from(index).map_err(|_| CoreError::LengthOverflow)? {
            return Err(CoreError::NonCanonicalOrdering.into());
        }
        rows.push(H11Expected {
            revision,
            root: fields[1].parse()?,
            transition: fields[2].parse()?,
            file: fields[3].parse()?,
            output_digest: h11_digest(fields[4])?,
            occurrence_digest: h11_digest(fields[5])?,
            closure_digest: h11_digest(fields[6])?,
            range_digest: h11_digest(fields[7])?,
        });
    }
    if rows.len() != usize::try_from(H11_MAX_REVISION).map_err(|_| CoreError::LengthOverflow)? {
        return Err(CoreError::LengthMismatch {
            expected: H11_MAX_REVISION,
            actual: u64::try_from(rows.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    Ok(rows)
}

fn h11_range(target: u64) -> CoreResult<std::ops::Range<u64>> {
    let start = target.saturating_sub(H11_RANGE_BYTES / 2);
    let end = start
        .checked_add(H11_RANGE_BYTES)
        .ok_or(CoreError::LengthOverflow)?
        .min(SOURCE_1);
    Ok(end.saturating_sub(H11_RANGE_BYTES)..end)
}

fn h11_operation(metrics: &Metrics, wall_ns: u128) -> H11Operation {
    H11Operation {
        wall_ns,
        sql_queries: metrics.sql_query_calls,
        sql_rows: metrics.sql_rows_returned,
        row_blob_reads: metrics.row_blob_reads,
        row_blob_writes: metrics.row_blob_writes,
        canonical_authenticated: metrics.canonical_bytes_authenticated,
        canonical_new_bytes: metrics.canonical_bytes_written,
        mapping_rewritten: metrics.mapping_bytes_rewritten,
        objects_created: metrics.objects_created,
        objects_reused: metrics.objects_reused,
        transactions: metrics.transactions,
        commits: metrics.commits,
        q_high_water: metrics.q_high_water,
        q_current: metrics.q_current,
    }
}

fn h11_observe_revision(
    store: &Store,
    revision: u64,
    root: ObjectId,
    transition: ObjectId,
    expected_parent: Option<ObjectId>,
    expected_operations: Option<&[delta_codec::TransitionOperation]>,
    range: std::ops::Range<u64>,
    metrics: &mut Metrics,
) -> AnyResult<H11Expected> {
    let transition_digest = verify_transition(
        store,
        transition,
        expected_parent,
        root,
        expected_operations,
        metrics,
    )?;
    let mut emit = |_bytes: &[u8]| Ok(());
    let reconstructed = reconstruct_file_to(
        store, root, None, None, true, true, metrics, &mut emit,
    )?;
    let file = resolve_namespace_file_root(store, root, metrics)?;
    let selected = read_file_range(store, file, SELECTED_PROFILE, range, metrics)?;
    let range_digest = *blake3::hash(&selected).as_bytes();
    drop(selected);
    Ok(H11Expected {
        revision,
        root,
        transition,
        file,
        output_digest: reconstructed.output_digest,
        occurrence_digest: reconstructed.occurrence_digest,
        closure_digest: combined_closure_digest(
            transition_digest,
            reconstructed.content_closure.ok_or(CoreError::Io)?,
        ),
        range_digest,
    })
}

fn h11_replace_revision(
    store: &mut Store,
    revision: u64,
    target: EditPoint,
    expected_prior: H11Expected,
    expected_prior_parent: Option<H11Expected>,
    expected: Option<H11Expected>,
    range: std::ops::Range<u64>,
) -> AnyResult<(H11Expected, H11Operation)> {
    let mut metrics = Metrics::default();
    let started = Instant::now();
    let observed = store.transaction_attempt(&mut metrics, |store, metrics| {
        let (prior_operations, prior_operations_charge) = if let Some(parent) = expected_prior_parent {
            if parent.revision.checked_add(1) != Some(expected_prior.revision) {
                return Err(CoreError::NonCanonicalOrdering.into());
            }
            let (operations, charge) = charged_replace_operation(
                b"file",
                parent.file,
                expected_prior.file,
                metrics,
            )?;
            (Some(operations), Some(charge))
        } else {
            (None, None)
        };
        let mut witness = establish_same_open_file_witness(
            store,
            SELECTED_PROFILE,
            expected_prior_parent.map(|parent| parent.root),
            prior_operations.as_deref(),
            metrics,
        )?;
        let permit = witness.consume(store, metrics)?;
        drop(prior_operations_charge);
        drop(prior_operations);
        let prior = store.current_head_accounted(metrics)?.ok_or(CoreError::InvalidValidationReceipt)?;
        if prior.0 != expected_prior.revision || prior.1 != expected_prior.root || prior.2 != expected_prior.transition {
            return Err(CoreError::PublicationConflict.into());
        }
        let before_file = resolve_namespace_file_root(store, prior.1, metrics)?;
        if before_file != expected_prior.file {
            return Err(CoreError::PublicationConflict.into());
        }
        let reference = file_reference_at_ordinal(
            store, before_file, SELECTED_PROFILE, target.position, metrics,
        )?;
        let mut replacement = store.with_borrowed_bytes(reference.object_id, metrics, |canonical, metrics| {
            let raw = layerfs_core::decode_bytes_object(canonical)?;
            let mut bytes = ChargedVec::with_capacity(raw.len(), metrics)?;
            bytes.extend_from_slice(raw);
            Ok(bytes)
        })?;
        if replacement.len() < 8 {
            return Err(CoreError::UnexpectedEof.into());
        }
        replacement[..8].copy_from_slice(&revision.to_be_bytes());
        let replacement_reference = make_reference(store, &replacement, metrics)?;
        drop(replacement);
        let (after_file, changed) = rewrite_same_root_by_offset(
            store, before_file, SELECTED_PROFILE, target.byte_offset, replacement_reference, metrics,
        )?;
        if !changed {
            return Err(CoreError::PublicationConflict.into());
        }
        let root = namespace_file_root(store, after_file, metrics)?;
        let (operations, operations_charge) = charged_replace_operation(b"file", before_file, after_file, metrics)?;
        let transition = publish_transition_with_operations(
            store, Some(prior.1), root, &operations, metrics,
        )?;
        let gate = expected.unwrap_or(H11Expected {
            revision,
            root,
            transition,
            file: after_file,
            output_digest: [0; 32],
            occurrence_digest: [0; 32],
            closure_digest: [0; 32],
            range_digest: [0; 32],
        });
        if gate.revision != revision || gate.root != root || gate.transition != transition || gate.file != after_file {
            return Err(CoreError::PublicationConflict.into());
        }
        qualify_same_middle_changed_spine(
            store,
            permit,
            prior.1,
            root,
            transition,
            &operations,
            ExpectedEditResult {
                before_file,
                after_file,
                root: gate.root,
                transition: gate.transition,
                closure: gate.closure_digest,
            },
            SELECTED_PROFILE,
            metrics,
        )?;
        let observed = if expected.is_none() {
            h11_observe_revision(
                store,
                revision,
                root,
                transition,
                Some(prior.1),
                Some(&operations),
                range,
                metrics,
            )?
        } else {
            gate
        };
        let authority = store.mint_publication_authority_after_qualification(
            Some(&prior), root, transition, metrics,
        )?;
        drop(operations_charge);
        drop(operations);
        store.publish_qualified(authority, metrics)?;
        Ok(observed)
    })?;
    let wall_ns = started.elapsed().as_nanos();
    finish_q(&mut metrics)?;
    if metrics.transactions != 1 || metrics.commits != 1 || metrics.q_current != 0 {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok((observed, h11_operation(&metrics, wall_ns)))
}

fn h11_create_base(
    store: &mut Store,
    source: &Path,
    range: std::ops::Range<u64>,
) -> AnyResult<(H11Expected, H11Operation)> {
    let mut metrics = Metrics::default();
    let started = Instant::now();
    let (root, transition) = build_file(store, source, SELECTED_PROFILE, &mut metrics)?;
    let observed = h11_observe_revision(store, 1, root, transition, None, None, range, &mut metrics)?;
    store.publish(None, root, transition, &mut metrics)?;
    let wall_ns = started.elapsed().as_nanos();
    finish_q(&mut metrics)?;
    if metrics.transactions != 1 || metrics.commits != 1 {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok((observed, h11_operation(&metrics, wall_ns)))
}

fn h11_oracle(source: &Path, database: &Path, manifest: &Path, operation_log: &Path) -> AnyResult<()> {
    if database.exists() || authority_path(database).exists() || manifest.exists() || operation_log.exists() {
        return Err("H11 oracle targets must be absent".into());
    }
    let target = prepared_edit_point(source, "one-byte-middle")?;
    if target.reference_count < 8 || target.replacement_length != 1 {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    let range = h11_range(target.byte_offset)?;
    let mut store = Store::open(database, SELECTED_PROFILE)?;
    store.connection.pragma_update(None, "cache_size", H11_CACHE_PAGES)?;
    let (base, _) = h11_create_base(&mut store, source, range.clone())?;
    let mut expected = Vec::with_capacity(usize::try_from(H11_MAX_REVISION).map_err(|_| CoreError::LengthOverflow)?);
    expected.push(base);
    for revision in 2..=H11_MAX_REVISION {
        let prior = *expected.last().ok_or(CoreError::MissingObject)?;
        let parent = expected
            .len()
            .checked_sub(2)
            .and_then(|index| expected.get(index).copied());
        let (next, _) =
            h11_replace_revision(&mut store, revision, target, prior, parent, None, range.clone())?;
        expected.push(next);
    }
    drop(store);

    let mut manifest_file = File::create_new(manifest)?;
    writeln!(manifest_file, "revision\troot\ttransition\tfile\toutput_digest\toccurrence_digest\tclosure_digest\trange_digest")?;
    for row in &expected {
        writeln!(
            manifest_file,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.revision,
            row.root,
            row.transition,
            row.file,
            hex_bytes(&row.output_digest),
            hex_bytes(&row.occurrence_digest),
            hex_bytes(&row.closure_digest),
            hex_bytes(&row.range_digest),
        )?;
    }
    manifest_file.sync_all()?;

    let mut log = File::create_new(operation_log)?;
    writeln!(log, "revision\tparent_revision\toperation\treference_ordinal\tbyte_offset\treplacement_prefix_be_u64")?;
    writeln!(log, "1\t0\tgenesis\t{}\t{}\t0000000000000001", target.position, target.byte_offset)?;
    for revision in 2..=H11_MAX_REVISION {
        writeln!(
            log,
            "{}\t{}\tfixed-reference-same-count-replace\t{}\t{}\t{:016x}",
            revision,
            revision - 1,
            target.position,
            target.byte_offset,
            revision,
        )?;
    }
    log.sync_all()?;
    remove_sqlite_image(database)?;
    println!(
        "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-h11-oracle-v1\",\"revisions\":{},\"source_bytes\":{},\"reference_count\":{},\"reference_ordinal\":{},\"byte_offset\":{},\"range_start\":{},\"range_end\":{}}}",
        expected.len(), SOURCE_1, target.reference_count, target.position, target.byte_offset, range.start, range.end
    );
    Ok(())
}

fn h11_collect_file_node(
    store: &Store,
    id: ObjectId,
    level: u8,
    final_node: bool,
    live: &mut BTreeSet<ObjectId>,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if !live.insert(id) {
        return Ok(());
    }
    let bytes = store.get_bytes(id, metrics)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let references = file_codec::parse_file_leaf(payload)?;
        file_codec::validate_file_leaf(&references, final_node)?;
        live.extend(references.into_iter().map(|reference| reference.object_id));
        return Ok(());
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let (actual_level, children) = file_codec::parse_file_children(payload, true)?;
    if actual_level != level {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    file_codec::validate_file_children(&children, final_node)?;
    let count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        h11_collect_file_node(
            store,
            child.object_id,
            level.checked_sub(1).ok_or(CoreError::MappingDepthExceeded)?,
            index + 1 == count,
            live,
            metrics,
        )?;
    }
    Ok(())
}

fn h11_collect_root(
    store: &Store,
    root: ObjectId,
    live: &mut BTreeSet<ObjectId>,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if !live.insert(root) {
        return Ok(());
    }
    let (_object_charge, object, _bytes) = store.get(root, metrics)?;
    let Object::Directory(entries) = object else {
        return Err(CoreError::WrongLogicalRole.into());
    };
    if entries.len() != 1 || entries[0].name().as_bytes() != b"file" {
        return Err(CoreError::WrongLogicalRole.into());
    }
    let file = entries[0].reference().id();
    if !live.insert(file) {
        return Ok(());
    }
    let file_bytes = store.get_bytes(file, metrics)?;
    let payload = file_codec::decode_mapping(&file_bytes, file_codec::FILE_ROOT_TAG)?;
    let (_, _, references, level, children) = file_codec::parse_file_root(payload)?;
    if level != file_codec::expected_file_level(references)? {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    file_codec::validate_file_children(&children, true)?;
    let count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        h11_collect_file_node(store, child.object_id, level, index + 1 == count, live, metrics)?;
    }
    Ok(())
}

fn h11_collect_transition(
    store: &Store,
    transition: ObjectId,
    live: &mut BTreeSet<ObjectId>,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if !live.insert(transition) {
        return Ok(());
    }
    let bytes = store.get_bytes(transition, metrics)?;
    let decoded = delta_codec::decode_mapping_transition(&bytes)?;
    live.extend(decoded.pages);
    Ok(())
}

fn h11_is_mapping(canonical: &[u8]) -> bool {
    layerfs_core::decode_bytes_object(canonical)
        .is_ok_and(|inner| inner.starts_with(&layerfs_core::content::persistence::MAPPING_MAGIC))
}

fn h11_sum_ids(store: &Store, ids: &BTreeSet<ObjectId>) -> AnyResult<(u64, u64)> {
    let mut canonical = 0_u64;
    let mut mapping = 0_u64;
    let mut statement = store
        .connection
        .prepare_cached("SELECT canonical_bytes FROM wp4m_objects WHERE object_id = ?1")?;
    for id in ids {
        let (len, is_mapping) = statement.query_row(params![id.as_bytes().as_slice()], |row| {
            let bytes = row.get_ref(0)?.as_blob()?;
            Ok((bytes.len(), h11_is_mapping(bytes)))
        })?;
        let len = u64::try_from(len).map_err(|_| CoreError::LengthOverflow)?;
        canonical = canonical.checked_add(len).ok_or(CoreError::LengthOverflow)?;
        if is_mapping {
            mapping = mapping.checked_add(len).ok_or(CoreError::LengthOverflow)?;
        }
    }
    Ok((canonical, mapping))
}

fn h11_object_stats(
    store: &Store,
    retained: &[H11Expected],
    current: H11Expected,
) -> AnyResult<H11ObjectStats> {
    let (stored_objects_i64, stored_canonical_bytes_i64): (i64, i64) = store.connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(length(canonical_bytes)), 0) FROM wp4m_objects",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let stored_objects = u64::try_from(stored_objects_i64).map_err(|_| CoreError::LengthOverflow)?;
    let stored_canonical_bytes =
        u64::try_from(stored_canonical_bytes_i64).map_err(|_| CoreError::LengthOverflow)?;
    let mut stored_mapping_bytes = 0_u64;
    {
        let mut statement = store.connection.prepare("SELECT canonical_bytes FROM wp4m_objects")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let bytes = row.get_ref(0)?.as_blob()?;
            if h11_is_mapping(bytes) {
                stored_mapping_bytes = stored_mapping_bytes
                    .checked_add(u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?)
                    .ok_or(CoreError::LengthOverflow)?;
            }
        }
    }
    let mut metrics = Metrics::default();
    let mut current_ids = BTreeSet::new();
    h11_collect_root(store, current.root, &mut current_ids, &mut metrics)?;
    h11_collect_transition(store, current.transition, &mut current_ids, &mut metrics)?;
    let mut retained_ids = BTreeSet::new();
    for row in retained {
        h11_collect_root(store, row.root, &mut retained_ids, &mut metrics)?;
        h11_collect_transition(store, row.transition, &mut retained_ids, &mut metrics)?;
    }
    finish_q(&mut metrics)?;
    let (current_live_canonical_bytes, current_live_mapping_bytes) = h11_sum_ids(store, &current_ids)?;
    let (retained_live_canonical_bytes, retained_live_mapping_bytes) = h11_sum_ids(store, &retained_ids)?;
    let current_live_objects = u64::try_from(current_ids.len()).map_err(|_| CoreError::LengthOverflow)?;
    let retained_live_objects = u64::try_from(retained_ids.len()).map_err(|_| CoreError::LengthOverflow)?;
    Ok(H11ObjectStats {
        stored_objects,
        stored_canonical_bytes,
        stored_mapping_bytes,
        current_live_objects,
        current_live_canonical_bytes,
        current_live_mapping_bytes,
        current_unreachable_objects: stored_objects
            .checked_sub(current_live_objects)
            .ok_or(CoreError::LengthOverflow)?,
        retained_live_objects,
        retained_live_canonical_bytes,
        retained_live_mapping_bytes,
        retained_unreachable_objects: stored_objects
            .checked_sub(retained_live_objects)
            .ok_or(CoreError::LengthOverflow)?,
    })
}

fn h11_verify_historical(
    store: &Store,
    expected: &[H11Expected],
    history: u64,
) -> AnyResult<(Vec<u64>, u64)> {
    let mut selected = BTreeSet::from([1, history, history / 2]);
    selected.extend(H11_HISTORY_POINTS.into_iter().filter(|revision| *revision <= history));
    selected.remove(&0);
    let mut max_q = 0_u64;
    for revision in &selected {
        let row = expected[usize::try_from(revision - 1).map_err(|_| CoreError::LengthOverflow)?];
        let prior = if row.revision > 1 {
            Some(
                expected
                    [usize::try_from(row.revision - 2).map_err(|_| CoreError::LengthOverflow)?],
            )
        } else {
            None
        };
        let operation = prior.map(|prior| delta_codec::TransitionOperation::Replace {
            path: b"file".to_vec(),
            before: prior.file,
            after: row.file,
        });
        let mut metrics = Metrics::default();
        verify_transition(
            store,
            row.transition,
            prior.map(|prior| prior.root),
            row.root,
            operation.as_ref().map(std::slice::from_ref),
            &mut metrics,
        )?;
        let mut emit = |_bytes: &[u8]| Ok(());
        let reconstructed = reconstruct_file_to(
            store,
            row.root,
            Some(&hex_bytes(&row.output_digest)),
            Some(&hex_bytes(&row.occurrence_digest)),
            true,
            true,
            &mut metrics,
            &mut emit,
        )?;
        if reconstructed.content_closure.is_none() || reconstructed.length != SOURCE_1 {
            return Err(CoreError::PublicationConflict.into());
        }
        finish_q(&mut metrics)?;
        max_q = max_q.max(metrics.q_high_water);
    }
    Ok((selected.into_iter().collect(), max_q))
}

fn h11_fd_count() -> AnyResult<u64> {
    Ok(u64::try_from(fs::read_dir("/dev/fd")?.count()).map_err(|_| CoreError::LengthOverflow)?)
}

fn h11_option(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn h11_operation_json(name: &str, value: H11Operation) -> String {
    format!(
        "\"{name}\":{{\"wall_ns\":{},\"sql_queries\":{},\"sql_rows\":{},\"row_blob_reads\":{},\"row_blob_writes\":{},\"canonical_authenticated\":{},\"canonical_new_bytes\":{},\"mapping_rewritten\":{},\"objects_created\":{},\"objects_reused\":{},\"transactions\":{},\"commits\":{},\"q_high_water\":{},\"q_current\":{}}}",
        value.wall_ns,
        value.sql_queries,
        value.sql_rows,
        value.row_blob_reads,
        value.row_blob_writes,
        value.canonical_authenticated,
        value.canonical_new_bytes,
        value.mapping_rewritten,
        value.objects_created,
        value.objects_reused,
        value.transactions,
        value.commits,
        value.q_high_water,
        value.q_current,
    )
}

fn h11_sample(
    source: &Path,
    manifest: &Path,
    work_root: &Path,
    history_count: u64,
    sample: u64,
) -> AnyResult<()> {
    if !H11_HISTORY_POINTS.contains(&history_count) || !H11_SAMPLES.contains(&sample) {
        return Err(CoreError::InvalidRecord("H11 schedule").into());
    }
    if work_root.exists() {
        return Err("H11 sample work root must be absent".into());
    }
    fs::create_dir(work_root)?;
    fs::set_permissions(work_root, fs::Permissions::from_mode(0o700))?;
    let database = work_root.join("store.sqlite");
    let materialization = work_root.join("materialization");
    let expected = h11_expected(manifest)?;
    let target = prepared_edit_point(source, "one-byte-middle")?;
    let range = h11_range(target.byte_offset)?;
    let source_hash = source_hash(source)?.1;
    let fd_before = h11_fd_count()?;

    let mut store = Store::open(&database, SELECTED_PROFILE)?;
    store.connection.pragma_update(None, "cache_size", H11_CACHE_PAGES)?;
    let observed_cache = store.connection.query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))?;
    if observed_cache != H11_CACHE_PAGES {
        return Err(CoreError::ProfileMismatch.into());
    }
    let (base, base_operation) = h11_create_base(&mut store, source, range.clone())?;
    if base.root != expected[0].root
        || base.transition != expected[0].transition
        || base.output_digest != expected[0].output_digest
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let mut history = H11History::default();
    for revision in 2..=history_count {
        let prior = expected[usize::try_from(revision - 2).map_err(|_| CoreError::LengthOverflow)?];
        let gate = expected[usize::try_from(revision - 1).map_err(|_| CoreError::LengthOverflow)?];
        let (_, operation) = h11_replace_revision(
            &mut store,
            revision,
            target,
            prior,
            revision
                .checked_sub(3)
                .and_then(|index| expected.get(usize::try_from(index).ok()?).copied()),
            Some(gate),
            range.clone(),
        )?;
        history.edit_ns.push(operation.wall_ns);
        history.objects_created = history.objects_created.checked_add(operation.objects_created).ok_or(CoreError::LengthOverflow)?;
        history.objects_reused = history.objects_reused.checked_add(operation.objects_reused).ok_or(CoreError::LengthOverflow)?;
        history.canonical_new_bytes = history.canonical_new_bytes.checked_add(operation.canonical_new_bytes).ok_or(CoreError::LengthOverflow)?;
        history.mapping_rewritten = history.mapping_rewritten.checked_add(operation.mapping_rewritten).ok_or(CoreError::LengthOverflow)?;
        history.transactions = history.transactions.checked_add(operation.transactions).ok_or(CoreError::LengthOverflow)?;
        history.commits = history.commits.checked_add(operation.commits).ok_or(CoreError::LengthOverflow)?;
        history.q_high_water = history.q_high_water.max(operation.q_high_water);
    }
    let history_snapshot = store.physical_snapshot();
    drop(store);

    let mut reopen_metrics = Metrics::default();
    let reopen_started = Instant::now();
    let mut store = Store::open_measured(&database, SELECTED_PROFILE, &mut reopen_metrics)?;
    store.connection.pragma_update(None, "cache_size", H11_CACHE_PAGES)?;
    let head = store.current_head_accounted(&mut reopen_metrics)?.ok_or(CoreError::InvalidValidationReceipt)?;
    let reopen_ns = reopen_started.elapsed().as_nanos();
    let current_before_edit = expected[usize::try_from(history_count - 1).map_err(|_| CoreError::LengthOverflow)?];
    if head.0 != history_count || head.1 != current_before_edit.root || head.2 != current_before_edit.transition {
        return Err(CoreError::PublicationConflict.into());
    }
    finish_q(&mut reopen_metrics)?;
    let reopen = h11_operation(&reopen_metrics, reopen_ns);

    let mut head_metrics = Metrics::default();
    let head_started = Instant::now();
    let repeated_head = store.current_head_accounted(&mut head_metrics)?.ok_or(CoreError::InvalidValidationReceipt)?;
    let head_ns = head_started.elapsed().as_nanos();
    if repeated_head != head {
        return Err(CoreError::PublicationConflict.into());
    }
    finish_q(&mut head_metrics)?;
    let head_lookup = h11_operation(&head_metrics, head_ns);

    let mut range_metrics = Metrics::default();
    let range_started = Instant::now();
    let range_output = read_file_range(
        &store,
        current_before_edit.file,
        SELECTED_PROFILE,
        range.clone(),
        &mut range_metrics,
    )?;
    let range_ns = range_started.elapsed().as_nanos();
    if *blake3::hash(&range_output).as_bytes() != current_before_edit.range_digest {
        return Err(CoreError::IdentityMismatch.into());
    }
    drop(range_output);
    finish_q(&mut range_metrics)?;
    let range_read = h11_operation(&range_metrics, range_ns);

    let mut reconstruction_metrics = Metrics::default();
    let reconstruction_started = Instant::now();
    let mut emit = |_bytes: &[u8]| Ok(());
    let reconstructed = reconstruct_file_to(
        &store,
        current_before_edit.root,
        Some(&hex_bytes(&current_before_edit.output_digest)),
        Some(&hex_bytes(&current_before_edit.occurrence_digest)),
        true,
        false,
        &mut reconstruction_metrics,
        &mut emit,
    )?;
    let reconstruction_ns = reconstruction_started.elapsed().as_nanos();
    if reconstructed.length != SOURCE_1 {
        return Err(CoreError::LengthMismatch { expected: SOURCE_1, actual: reconstructed.length }.into());
    }
    finish_q(&mut reconstruction_metrics)?;
    let reconstruction = h11_operation(&reconstruction_metrics, reconstruction_ns);

    let after_edit = expected[usize::try_from(history_count).map_err(|_| CoreError::LengthOverflow)?];
    let (edited, first_edit) = h11_replace_revision(
        &mut store,
        history_count + 1,
        target,
        current_before_edit,
        history_count
            .checked_sub(2)
            .and_then(|index| expected.get(usize::try_from(index).ok()?).copied()),
        Some(after_edit),
        range,
    )?;
    if edited.root != after_edit.root {
        return Err(CoreError::PublicationConflict.into());
    }
    let edited_head = store.current_head()?.ok_or(CoreError::InvalidValidationReceipt)?;
    let native = phase4_g3_materialization::h11_materialize_current(
        &mut store,
        &edited_head,
        after_edit.output_digest,
        after_edit.occurrence_digest,
        &materialization,
    )?;
    if native.output_digest != after_edit.output_digest
        || native.references != target.reference_count
        || native.q_current != 0
        || native.max_single_buffer_bytes > SOURCE_1
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let (historical_revisions, historical_q) = h11_verify_historical(&store, &expected, history_count)?;
    let retained = &expected[..=usize::try_from(history_count).map_err(|_| CoreError::LengthOverflow)?];
    let object_stats = h11_object_stats(&store, retained, after_edit)?;
    let terminal_snapshot = store.physical_snapshot();
    drop(store);
    let fd_after_store_close = h11_fd_count()?;
    remove_sqlite_image(&database)?;
    if materialization.exists() {
        fs::remove_dir(&materialization)?;
    }
    let residue_before_root_remove = fs::read_dir(work_root)?.count();
    fs::remove_dir(work_root)?;
    let fd_after_cleanup = h11_fd_count()?;
    if residue_before_root_remove != 0 || fd_after_cleanup != fd_before {
        return Err(CoreError::PublicationConflict.into());
    }

    let history_samples = history.edit_ns.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
    let historical = historical_revisions.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
    let q_max = [
        base_operation.q_high_water,
        history.q_high_water,
        reopen.q_high_water,
        head_lookup.q_high_water,
        range_read.q_high_water,
        reconstruction.q_high_water,
        first_edit.q_high_water,
        native.q_high_water,
        historical_q,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);

    let mut output = String::from("{");
    write!(
        output,
        "\"status\":\"PASS\",\"schema\":\"phase4-g5-h11-sample-v1\",\"history_revisions\":{history_count},\"sample\":{sample},\"source_bytes\":{SOURCE_1},\"source_blake3\":\"{source_hash}\",\"profile\":\"{}\",\"cache_size_pages\":{H11_CACHE_PAGES},",
        hex_bytes(&profile_id()),
    )?;
    write!(
        output,
        "\"history_edit_samples_ns\":[{history_samples}],\"history_objects_created\":{},\"history_objects_reused\":{},\"history_canonical_new_bytes\":{},\"history_mapping_rewritten\":{},\"history_transactions\":{},\"history_commits\":{},\"history_q_high_water\":{},",
        history.objects_created,
        history.objects_reused,
        history.canonical_new_bytes,
        history.mapping_rewritten,
        history.transactions,
        history.commits,
        history.q_high_water,
    )?;
    write!(
        output,
        "\"history_logical_store_bytes\":{},\"history_apparent_store_bytes\":{},\"history_allocated_store_bytes\":{},\"final_root\":\"{}\",\"final_transition\":\"{}\",\"final_output_digest\":\"{}\",\"historical_revisions_verified\":[{historical}],",
        h11_option(history_snapshot.logical_store()),
        h11_option(history_snapshot.apparent_store()),
        h11_option(history_snapshot.allocated_store()),
        after_edit.root,
        after_edit.transition,
        hex_bytes(&after_edit.output_digest),
    )?;
    for operation in [
        h11_operation_json("reopen_head", reopen),
        h11_operation_json("head_lookup", head_lookup),
        h11_operation_json("range_read", range_read),
        h11_operation_json("reconstruction", reconstruction),
        h11_operation_json("first_edit_after_reopen", first_edit),
    ] {
        output.push_str(&operation);
        output.push(',');
    }
    write!(
        output,
        "\"materialization\":{{\"wall_ns\":{},\"verification_ns\":{},\"cleanup_ns\":{},\"user_us\":{},\"system_us\":{},\"voluntary_switches\":{},\"involuntary_switches\":{},\"sql_queries\":{},\"sql_rows\":{},\"row_blob_reads\":{},\"row_blob_writes\":{},\"canonical_authenticated\":{},\"write_calls\":{},\"write_bytes\":{},\"data_sync_calls\":{},\"metadata_sync_calls\":{},\"rename_calls\":{},\"directory_sync_calls\":{},\"temp_files_created\":{},\"temp_files_removed\":{},\"q_high_water\":{},\"q_current\":{},\"max_single_buffer_bytes\":{}}},",
        native.wall_ns,
        native.verification_ns,
        native.cleanup_ns,
        native.user_us,
        native.system_us,
        native.voluntary_switches,
        native.involuntary_switches,
        native.sql_query_calls,
        native.sql_rows_returned,
        native.row_blob_reads,
        native.row_blob_writes,
        native.canonical_bytes_authenticated,
        native.write_calls,
        native.write_bytes,
        native.data_sync_calls,
        native.metadata_sync_calls,
        native.rename_calls,
        native.directory_sync_calls,
        native.temp_files_created,
        native.temp_files_removed,
        native.q_high_water,
        native.q_current,
        native.max_single_buffer_bytes,
    )?;
    write!(
        output,
        "\"stored_objects\":{},\"stored_canonical_bytes\":{},\"stored_mapping_bytes\":{},\"current_live_objects\":{},\"current_live_canonical_bytes\":{},\"current_live_mapping_bytes\":{},\"current_unreachable_objects\":{},\"retained_live_objects\":{},\"retained_live_canonical_bytes\":{},\"retained_live_mapping_bytes\":{},\"retained_unreachable_objects\":{},",
        object_stats.stored_objects,
        object_stats.stored_canonical_bytes,
        object_stats.stored_mapping_bytes,
        object_stats.current_live_objects,
        object_stats.current_live_canonical_bytes,
        object_stats.current_live_mapping_bytes,
        object_stats.current_unreachable_objects,
        object_stats.retained_live_objects,
        object_stats.retained_live_canonical_bytes,
        object_stats.retained_live_mapping_bytes,
        object_stats.retained_unreachable_objects,
    )?;
    write!(
        output,
        "\"terminal_logical_store_bytes\":{},\"terminal_apparent_store_bytes\":{},\"terminal_allocated_store_bytes\":{},\"q_high_water\":{q_max},\"q_current\":0,\"fd_before\":{fd_before},\"fd_after_store_close\":{fd_after_store_close},\"fd_after_cleanup\":{fd_after_cleanup},\"descriptor_leak\":false,\"permit_leak\":false,\"seed_residue\":0,\"temp_residue\":0,\"lock_residue_checked_by_runner\":true,\"physical_io_bytes\":\"Unavailable\",\"continuous_storage_peak\":\"Unavailable\",\"controlled_cold\":\"Unavailable\"}}",
        h11_option(terminal_snapshot.logical_store()),
        h11_option(terminal_snapshot.apparent_store()),
        h11_option(terminal_snapshot.allocated_store()),
    )?;
    println!("{output}");
    Ok(())
}

pub(super) fn h11_main() -> AnyResult<()> {
    let args = env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("--fixture") => {
            let source = Path::new(args.get(2).ok_or("missing H11 source path")?);
            if source.exists() {
                return Err("H11 source path must be absent".into());
            }
            fill_source(source, SOURCE_1, 0x41)?;
            let (length, digest) = source_hash(source)?;
            println!(
                "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-h11-fixture-v1\",\"length\":{length},\"blake3\":\"{digest}\"}}"
            );
            Ok(())
        }
        Some("--oracle") => h11_oracle(
            Path::new(args.get(2).ok_or("missing H11 source")?),
            Path::new(args.get(3).ok_or("missing H11 oracle database")?),
            Path::new(args.get(4).ok_or("missing H11 expected manifest")?),
            Path::new(args.get(5).ok_or("missing H11 operation log")?),
        ),
        Some("--sample") => h11_sample(
            Path::new(args.get(2).ok_or("missing H11 source")?),
            Path::new(args.get(3).ok_or("missing H11 expected manifest")?),
            Path::new(args.get(4).ok_or("missing H11 sample root")?),
            args.get(5).ok_or("missing H11 history count")?.parse()?,
            args.get(6).ok_or("missing H11 sample index")?.parse()?,
        ),
        _ => Err("usage: --fixture SOURCE | --oracle SOURCE DATABASE EXPECTED_MANIFEST OPERATION_LOG | --sample SOURCE EXPECTED_MANIFEST WORK_ROOT {1|10|100|1000} {1|2}".into()),
    }
}
