const G5_REQUEST_BYTES_V10: usize = 4_096;
const G5_SEMANTIC_REFERENCES: u64 = 130;

#[derive(Clone, Copy)]
struct G5Custody {
    executable_sha256: &'static str,
    database_sha256: &'static str,
    authority_sha256: &'static str,
    expectations_sha256: &'static str,
}

#[derive(Debug)]
struct G5Request<'a> {
    id: &'a str,
    root: &'a Path,
    iteration: usize,
    warmup: bool,
    validation: RowValidation,
}

#[derive(Clone, Copy)]
struct G5SemanticObservation {
    before: VisibleHead,
    after: VisibleHead,
    error: Option<FailureCause>,
    fault_case: Option<&'static str>,
    error_class: Option<&'static str>,
    failure_boundary: Option<&'static str>,
    provenance: Option<FailureProvenance>,
    publication_status: Option<PublicationStatus>,
    transactions: u64,
    commits: u64,
    scrub_calls: u64,
    scrub_bytes: u64,
    verified_reopen_scrub_calls: u64,
    verified_reopen_scrub_bytes: u64,
    trusted_equal_edges: u64,
    trusted_prior_references: u64,
    trusted_prior_raw_bytes: u64,
    verified_carry_forward: bool,
    q_high_water: u64,
    q_current: u64,
    cleanup_ok: bool,
}

struct G5Decimal {
    bytes: [u8; 20],
    start: usize,
}

impl G5Decimal {
    fn new(mut value: u64) -> Self {
        let mut bytes = [0_u8; 20];
        let mut start = bytes.len();
        loop {
            start -= 1;
            bytes[start] = b'0' + u8::try_from(value % 10).expect("decimal digit");
            value /= 10;
            if value == 0 {
                break;
            }
        }
        Self { bytes, start }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[self.start..]).expect("ASCII decimal")
    }
}

fn g5_checked_sum(parts: &[usize]) -> AnyResult<usize> {
    parts.iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(*value)
            .ok_or_else(|| CoreError::LengthOverflow.into())
    })
}

fn g5_root_prefix_capacity(root: &Path) -> AnyResult<(usize, bool)> {
    let root_bytes = root.as_os_str().as_encoded_bytes();
    let separator = !root_bytes.ends_with(b"/");
    Ok((
        root_bytes
            .len()
            .checked_add(usize::from(separator))
            .ok_or(CoreError::LengthOverflow)?,
        separator,
    ))
}

fn g5_rooted_path(
    root: &Path,
    separator: bool,
    capacity: usize,
    parts: &[&str],
) -> AnyResult<PathBuf> {
    let mut value = OsString::with_capacity(capacity);
    value.push(root);
    if separator {
        value.push("/");
    }
    for part in parts {
        value.push(part);
    }
    if value.capacity() != capacity || value.as_encoded_bytes().len() != capacity {
        return Err(CoreError::LengthOverflow.into());
    }
    Ok(PathBuf::from(value))
}

fn g5_store_charge(store: &Store) -> u64 {
    store._path_charge.as_ref().map_or(0, |charge| charge.0)
}

fn g5_expected_live_q(outer: u64, store: &Store) -> AnyResult<u64> {
    outer
        .checked_add(g5_store_charge(store))
        .ok_or_else(|| CoreError::LengthOverflow.into())
}

fn g5_require_live_q(expected: u64) -> AnyResult<()> {
    if q_current() != expected {
        return Err(CoreError::LengthMismatch {
            expected,
            actual: q_current(),
        }
        .into());
    }
    Ok(())
}

struct G5OwnedRowPaths {
    source: PathBuf,
    database: PathBuf,
    authority: PathBuf,
    expectations: PathBuf,
    charge: CapacityCharge,
}

impl G5OwnedRowPaths {
    fn new(
        root: &Path,
        size: u64,
        operation: &str,
        iteration: usize,
        metrics: &mut Metrics,
    ) -> AnyResult<Self> {
        let source_label = borrowed_source_label(size)?;
        let size_text = G5Decimal::new(size);
        let iteration = u64::try_from(iteration).map_err(|_| CoreError::LengthOverflow)?;
        let iteration_text = G5Decimal::new(iteration);
        let (prefix, separator) = g5_root_prefix_capacity(root)?;
        let source_capacity = g5_checked_sum(&[prefix, source_label.len(), ".source".len()])?;
        let database_capacity = g5_checked_sum(&[
            prefix,
            "db-".len(),
            SELECTED_PROFILE.name.len(),
            1,
            size_text.as_str().len(),
            1,
            operation.len(),
            1,
            iteration_text.as_str().len(),
            ".sqlite".len(),
        ])?;
        let authority_capacity = database_capacity
            .checked_add(".authority".len())
            .ok_or(CoreError::LengthOverflow)?;
        let expectations_capacity = database_capacity
            .checked_add(".expectations".len())
            .ok_or(CoreError::LengthOverflow)?;
        let combined = g5_checked_sum(&[
            source_capacity,
            database_capacity,
            authority_capacity,
            expectations_capacity,
        ])?;
        let charge = charge_capacity(metrics, combined)?;
        let database_parts = [
            "db-",
            SELECTED_PROFILE.name,
            "-",
            size_text.as_str(),
            "-",
            operation,
            "-",
            iteration_text.as_str(),
            ".sqlite",
        ];
        let source = g5_rooted_path(root, separator, source_capacity, &[source_label, ".source"])?;
        let database = g5_rooted_path(root, separator, database_capacity, &database_parts)?;
        let authority = g5_rooted_path(
            root,
            separator,
            authority_capacity,
            &[
                "db-",
                SELECTED_PROFILE.name,
                "-",
                size_text.as_str(),
                "-",
                operation,
                "-",
                iteration_text.as_str(),
                ".sqlite.authority",
            ],
        )?;
        let expectations = g5_rooted_path(
            root,
            separator,
            expectations_capacity,
            &[
                "db-",
                SELECTED_PROFILE.name,
                "-",
                size_text.as_str(),
                "-",
                operation,
                "-",
                iteration_text.as_str(),
                ".sqlite.expectations",
            ],
        )?;
        Ok(Self {
            source,
            database,
            authority,
            expectations,
            charge,
        })
    }

    fn charge_bytes(&self) -> u64 {
        self.charge.0
    }

    fn owner_count(&self) -> u64 {
        4
    }
}

struct G5OwnedSemanticPaths {
    database: PathBuf,
    authority: PathBuf,
    expectations: PathBuf,
    charge: CapacityCharge,
}

impl G5OwnedSemanticPaths {
    fn new(root: &Path, label: &str, metrics: &mut Metrics) -> AnyResult<Self> {
        let (prefix, separator) = g5_root_prefix_capacity(root)?;
        let database_capacity = g5_checked_sum(&[
            prefix,
            "g5-semantic-v10-".len(),
            label.len(),
            ".sqlite".len(),
        ])?;
        let authority_capacity = database_capacity
            .checked_add(".authority".len())
            .ok_or(CoreError::LengthOverflow)?;
        let expectations_capacity = database_capacity
            .checked_add(".expectations".len())
            .ok_or(CoreError::LengthOverflow)?;
        let combined =
            g5_checked_sum(&[database_capacity, authority_capacity, expectations_capacity])?;
        let charge = charge_capacity(metrics, combined)?;
        let database_parts = ["g5-semantic-v10-", label, ".sqlite"];
        let database = g5_rooted_path(root, separator, database_capacity, &database_parts)?;
        let authority = g5_rooted_path(
            root,
            separator,
            authority_capacity,
            &["g5-semantic-v10-", label, ".sqlite.authority"],
        )?;
        let expectations = g5_rooted_path(
            root,
            separator,
            expectations_capacity,
            &["g5-semantic-v10-", label, ".sqlite.expectations"],
        )?;
        Ok(Self {
            database,
            authority,
            expectations,
            charge,
        })
    }

    fn charge_bytes(&self) -> u64 {
        self.charge.0
    }
}

#[derive(Default)]
struct G5ChildOwners {
    request: Option<G5OwnedRowPaths>,
    report: Option<ChargedString>,
}

struct G5TerminalOwnerCounts {
    argument: u64,
    request: u64,
    schedule: u64,
    timing: u64,
    report: u64,
}

impl G5ChildOwners {
    fn terminal_counts(&self) -> G5TerminalOwnerCounts {
        G5TerminalOwnerCounts {
            argument: u64::from(std::mem::needs_drop::<G5Custody>()),
            request: self
                .request
                .as_ref()
                .map_or(0, G5OwnedRowPaths::owner_count),
            schedule: u64::from(std::mem::needs_drop::<(u64, u128, u128)>()),
            timing: u64::from(std::mem::needs_drop::<u128>()),
            report: u64::from(self.report.is_some()),
        }
    }
}

fn g5_arg_count() -> AnyResult<usize> {
    // SAFETY: Darwin owns argc for the process lifetime. We only read it.
    let argc = unsafe { *libc::_NSGetArgc() };
    usize::try_from(argc).map_err(|_| CoreError::InvalidRecord("G5 argc").into())
}

fn g5_arg(index: usize) -> AnyResult<&'static str> {
    if index >= g5_arg_count()? {
        return Err(CoreError::InvalidRecord("missing G5 argument").into());
    }
    // SAFETY: Darwin owns the checked argv pointer and NUL-terminated bytes for
    // the process lifetime. CStr::to_str validates UTF-8 without allocating.
    let value = unsafe {
        let argv = *libc::_NSGetArgv();
        if argv.is_null() {
            return Err(CoreError::InvalidRecord("G5 argv").into());
        }
        let value = *argv.add(index);
        if value.is_null() {
            return Err(CoreError::InvalidRecord("G5 argv").into());
        }
        std::ffi::CStr::from_ptr(value)
    };
    value
        .to_str()
        .map_err(|_| CoreError::InvalidRecord("G5 UTF-8 argument").into())
}

fn g5_require_args(expected: usize) -> AnyResult<()> {
    let actual = g5_arg_count()?;
    if actual != expected {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(expected).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(actual).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    Ok(())
}

fn g5_mode(value: &str) -> AnyResult<(IntegrityMode, &'static str)> {
    match value {
        "verified" => Ok((IntegrityMode::Verified, "verified")),
        "trusted" => Ok((IntegrityMode::TrustedLocalDev, "trusted-local-dev")),
        _ => Err("G5 mode must be verified or trusted".into()),
    }
}

fn g5_operation(value: &str) -> AnyResult<&str> {
    match value {
        "first-edit-after-reopen"
        | "same-middle"
        | "one-byte-early"
        | "one-byte-middle"
        | "one-byte-late"
        | "plus1-early"
        | "plus1-middle" => Ok(value),
        _ => Err("unsupported G5 operation".into()),
    }
}

fn g5_digest(value: &'static str) -> AnyResult<&'static str> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid borrowed G5 SHA-256".into());
    }
    Ok(value)
}

fn g5_request_id(value: &str) -> AnyResult<&str> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("invalid G5 request id".into());
    }
    Ok(value)
}

fn g5_request(line: &str) -> AnyResult<G5Request<'_>> {
    let mut fields = line.split('\t');
    let id = g5_request_id(fields.next().ok_or("missing request id")?)?;
    let root = Path::new(fields.next().ok_or("missing request root")?);
    if !root.is_absolute() {
        return Err("request root must be absolute".into());
    }
    let iteration = fields
        .next()
        .ok_or("missing request iteration")?
        .parse::<usize>()?;
    let warmup = fields
        .next()
        .ok_or("missing request warmup")?
        .parse::<bool>()?;
    let validation = match fields.next() {
        Some("capture-only") => RowValidation::CaptureOnly,
        Some("complete-roundtrip") => RowValidation::CompleteRoundTrip,
        _ => return Err("invalid request validation".into()),
    };
    if fields.next().is_some() {
        return Err("too many request fields".into());
    }
    Ok(G5Request {
        id,
        root,
        iteration,
        warmup,
        validation,
    })
}

fn g5_read_line<'a>(
    input: &mut impl std::io::Read,
    buffer: &'a mut [u8; G5_REQUEST_BYTES_V10],
) -> AnyResult<Option<&'a str>> {
    let mut used = 0;
    loop {
        if used == buffer.len() {
            return Err("G5 request exceeds fixed input bound".into());
        }
        match input.read(&mut buffer[used..used + 1])? {
            0 if used == 0 => return Ok(None),
            0 => return Err("truncated G5 request".into()),
            1 if buffer[used] == b'\n' => {
                let line = std::str::from_utf8(&buffer[..used])?;
                if line.is_empty() || line.as_bytes().contains(&0) || line.ends_with('\r') {
                    return Err("invalid G5 request line".into());
                }
                return Ok(Some(line));
            }
            1 => used += 1,
            _ => unreachable!(),
        }
    }
}

fn g5_forecast(forecast_ns: u128, limit_ns: u128) -> AnyResult<()> {
    if limit_ns == 0 || forecast_ns > limit_ns {
        return Err("G5 full-wrapper forecast exceeds its frozen limit".into());
    }
    Ok(())
}

fn g5_terminal_q() -> AnyResult<()> {
    if q_current() != 0 {
        return Err(CoreError::LengthMismatch {
            expected: 0,
            actual: q_current(),
        }
        .into());
    }
    Ok(())
}

fn g5_release_closed_sqlite_memory() {
    // SAFETY: run_row_with_session has returned, so its Connection is closed;
    // SQLite documents this process-global call as releasing currently unused
    // allocator/cache memory without changing live database state.
    unsafe {
        rusqlite::ffi::sqlite3_release_memory(i32::MAX);
    }
}

fn g5_precondition_database_pages(path: &Path) -> AnyResult<()> {
    let expected = fs::metadata(path)?.len();
    let mut database = File::open(path)?;
    let observed = std::io::copy(&mut database, &mut std::io::sink())?;
    if observed != expected {
        return Err(CoreError::UnexpectedEof.into());
    }
    Ok(())
}

fn g5_child() -> AnyResult<()> {
    g5_require_args(12)?;
    let (mode, mode_name) = g5_mode(g5_arg(2)?)?;
    let size = g5_arg(3)?.parse::<u64>()?;
    require_fast_size(size)?;
    let operation = g5_operation(g5_arg(4)?)?;
    let expected_rows = g5_arg(5)?.parse::<u64>()?;
    let forecast_ns = g5_arg(6)?.parse::<u128>()?;
    let limit_ns = g5_arg(7)?.parse::<u128>()?;
    g5_forecast(forecast_ns, limit_ns)?;
    let custody = G5Custody {
        executable_sha256: g5_digest(g5_arg(8)?)?,
        database_sha256: g5_digest(g5_arg(9)?)?,
        authority_sha256: g5_digest(g5_arg(10)?)?,
        expectations_sha256: g5_digest(g5_arg(11)?)?,
    };
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let mut buffer = [0_u8; G5_REQUEST_BYTES_V10];
    let mut rows = 0_u64;
    let mut owners = G5ChildOwners::default();

    writeln!(output, "{{\"status\":\"READY\",\"schema\":\"phase4-g5-trusted-child-ready-v10\",\"integrity_mode\":\"{mode_name}\",\"mode_provenance\":\"fixed-at-child-start\",\"size_bytes\":{size},\"operation\":\"{operation}\",\"expected_rows\":{expected_rows},\"full_wrapper_forecast_ns\":{forecast_ns},\"full_wrapper_limit_ns\":{limit_ns},\"custody\":\"runner-preverified-borrowed\",\"request_schema\":\"id\\troot\\titeration\\twarmup\\tvalidation\"}}")?;
    output.flush()?;
    while let Some(line) = g5_read_line(&mut input, &mut buffer)? {
        let request = g5_request(line)?;
        let mut outer_metrics = Metrics::default();
        owners.request = Some(G5OwnedRowPaths::new(
            request.root,
            size,
            operation,
            request.iteration,
            &mut outer_metrics,
        )?);
        let paths = owners.request.as_ref().ok_or(CoreError::MissingObject)?;
        g5_require_live_q(paths.charge_bytes())?;
        g5_precondition_database_pages(&paths.database)?;
        let session = RunRowSession {
            source: &paths.source,
            database: &paths.database,
            authority: &paths.authority,
            expectations: &paths.expectations,
            executable_sha256: custody.executable_sha256,
            base_copy_method: "fast-lane-isolated-prepared-row",
            base_database_sha256: custody.database_sha256,
            base_authority_sha256: custody.authority_sha256,
            base_expectations_sha256: custody.expectations_sha256,
            qualification_mode: QualificationMode::ChangedSpine,
        };
        let row = run_row_with_session(
            SELECTED_PROFILE,
            mode,
            size,
            operation,
            request.iteration,
            request.warmup,
            request.validation,
            &session,
        )?;
        drop(session);
        owners.report = Some(row);
        let report = owners.report.as_ref().ok_or(CoreError::MissingObject)?;
        writeln!(output, "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-child-row-v10\",\"request_id\":\"{}\",\"integrity_mode\":\"{mode_name}\",\"mode_provenance\":\"fixed-at-child-start\",\"row\":{}}}", request.id, &**report)?;
        output.flush()?;
        drop(owners.report.take());
        drop(owners.request.take());
        g5_terminal_q()?;
        g5_release_closed_sqlite_memory();
        rows = rows.checked_add(1).ok_or(CoreError::LengthOverflow)?;
    }
    if rows != expected_rows {
        return Err(CoreError::LengthMismatch {
            expected: expected_rows,
            actual: rows,
        }
        .into());
    }
    g5_terminal_q()?;
    let terminal = owners.terminal_counts();
    if terminal.argument != 0
        || terminal.request != 0
        || terminal.schedule != 0
        || terminal.timing != 0
        || terminal.report != 0
    {
        return Err("G5 child terminal owner leak".into());
    }
    writeln!(output, "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-child-terminal-v10\",\"integrity_mode\":\"{mode_name}\",\"rows\":{rows},\"expected_rows\":{expected_rows},\"argument_owners\":{},\"request_owners\":{},\"schedule_owners\":{},\"timing_owners\":{},\"report_owners\":{},\"q_current\":{},\"rss\":\"external-high-water-only\"}}", terminal.argument, terminal.request, terminal.schedule, terminal.timing, terminal.report, q_current())?;
    output.flush()?;
    Ok(())
}

fn g5_semantic_base(
    paths: &G5OwnedSemanticPaths,
) -> AnyResult<(Store, VisibleHead, ObjectId, ObjectId, ObjectId)> {
    if paths.database.exists() || paths.authority.exists() || paths.expectations.exists() {
        return Err("semantic database must be absent".into());
    }
    let mut metrics = Metrics::default();
    let mut store = Store::open_measured_with_integrity_mode(
        &paths.database,
        SELECTED_PROFILE,
        &mut metrics,
        None,
        IntegrityMode::Verified,
    )?;
    store.begin(&mut metrics)?;
    let common = make_reference(&mut store, b"x", &mut metrics)?;
    let unrelated = make_reference(&mut store, b"y", &mut metrics)?;
    let mut builder = FileBuilder::new(SELECTED_PROFILE, G5_SEMANTIC_REFERENCES, &mut metrics)?;
    for ordinal in 0..G5_SEMANTIC_REFERENCES {
        builder.push_reference(
            &mut store,
            if ordinal + 1 == G5_SEMANTIC_REFERENCES {
                unrelated
            } else {
                common
            },
            &mut metrics,
        )?;
    }
    let file = builder.finish(&mut store, &mut metrics)?;
    let root = namespace_file_root(&mut store, file, &mut metrics)?;
    let transition = publish_transition(&mut store, None, root, &mut metrics)?;
    let publication = store.publish(None, root, transition, &mut metrics)?;
    if publication.status != PublicationStatus::Committed {
        return Err(CoreError::PublicationConflict.into());
    }
    let head = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    g5_require_live_q(g5_expected_live_q(paths.charge_bytes(), &store)?)?;
    Ok((store, head, file, common.object_id, unrelated.object_id))
}

fn g5_corrupt(store: &Store, id: ObjectId) -> AnyResult<()> {
    let changed = store.connection.execute(
        "UPDATE wp4m_objects SET canonical_bytes = zeroblob(length(canonical_bytes)) WHERE object_id=?1",
        params![id.as_bytes().as_slice()],
    )?;
    if changed != 1 {
        return Err(CoreError::MissingObject.into());
    }
    Ok(())
}

fn g5_semantic_edit(
    store: &mut Store,
    outer_path_charge: u64,
    fault: Option<PublishFault>,
    reject_commit: bool,
) -> AnyResult<G5SemanticObservation> {
    let before = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    let mut metrics = Metrics::default();
    if reject_commit {
        store.connection.commit_hook(Some(|| true))?;
    }
    let attempt = (|| -> AnyResult<PublicationOutcome> {
        store.begin(&mut metrics)?;
        let base_scope =
            establish_edit_base_scope(store, SELECTED_PROFILE, None, None, &mut metrics)?;
        let provenance = base_scope.provenance();
        let before_file = resolve_namespace_file_root(store, before.1, &mut metrics)?;
        let (root, transition) = edit_file(
            store,
            SELECTED_PROFILE,
            "same-middle",
            EditPoint {
                reference_count: G5_SEMANTIC_REFERENCES,
                position: G5_SEMANTIC_REFERENCES / 2,
                byte_offset: G5_SEMANTIC_REFERENCES / 2,
                replacement_length: 1,
            },
            true,
            &mut metrics,
        )?;
        let after_file = resolve_namespace_file_root(store, root, &mut metrics)?;
        let (operations, operations_charge) =
            charged_replace_operation(b"file", before_file, after_file, &mut metrics)?;
        qualify_same_middle_changed_spine(
            store,
            base_scope,
            before.1,
            root,
            transition,
            &operations,
            ExpectedEditResult {
                before_file,
                after_file,
                root,
                transition,
                closure: [0_u8; 32],
            },
            SELECTED_PROFILE,
            &mut metrics,
        )?;
        let authority = store.mint_publication_authority_after_qualification(
            Some(&before),
            root,
            transition,
            &mut metrics,
        )?;
        drop(operations);
        drop(operations_charge);
        let started = Instant::now();
        let result = store.publish_authorized_with_fault(authority, fault, &mut metrics);
        let carry = store.carried_same_open_authority.is_some();
        Store::finish_publication(started, result, Some(provenance), carry, &mut metrics)
    })();
    if reject_commit {
        store.connection.commit_hook(None::<fn() -> bool>)?;
    }
    let (publication, error, provenance) = match attempt {
        Ok(publication) => {
            let provenance = publication.diagnostic;
            (Some(publication), None, provenance)
        }
        Err(error) => {
            let provenance = error
                .downcast_ref::<PublicationFailure>()
                .map(|failure| failure.0);
            let cause = provenance
                .and_then(|value| value.dominant.or(value.first))
                .unwrap_or_else(|| failure_cause(error.as_ref()));
            (None, Some(cause), provenance)
        }
    };
    let mut cleanup_ok = true;
    if store.active_transaction.is_some() {
        cleanup_ok = store.rollback(&mut metrics).is_ok();
    }
    let after = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    g5_require_live_q(g5_expected_live_q(outer_path_charge, store)?)?;
    metrics.q_current = 0;
    Ok(G5SemanticObservation {
        before,
        after,
        error,
        fault_case: None,
        error_class: None,
        failure_boundary: None,
        provenance,
        publication_status: publication.map(|value| value.status),
        transactions: metrics.transactions,
        commits: metrics.commits,
        scrub_calls: metrics.edit_base_complete_scrub_calls,
        scrub_bytes: metrics.edit_base_complete_scrub_canonical_bytes,
        verified_reopen_scrub_calls: 0,
        verified_reopen_scrub_bytes: 0,
        trusted_equal_edges: metrics.trusted_assumed_equal_edges,
        trusted_prior_references: metrics.trusted_assumed_prior_references,
        trusted_prior_raw_bytes: metrics.trusted_assumed_prior_raw_bytes,
        verified_carry_forward: publication.is_some_and(|value| value.verified_carry_forward),
        q_high_water: metrics.q_high_water,
        q_current: metrics.q_current,
        cleanup_ok,
    })
}

fn g5_seed_touched_fault(
    store: &Store,
    fault_case: &str,
    file: ObjectId,
) -> AnyResult<(ObjectId, Option<Vec<u8>>)> {
    match fault_case {
        "missing-object" => {
            let changed = store.connection.execute(
                "DELETE FROM wp4m_objects WHERE object_id=?1",
                params![file.as_bytes().as_slice()],
            )?;
            if changed != 1 {
                return Err(CoreError::MissingObject.into());
            }
            Ok((file, None))
        }
        "identity-mismatch" => {
            g5_corrupt(store, file)?;
            Ok((file, None))
        }
        "wrong-logical-role" => {
            let canonical =
                encode_canonical_object(&Object::bytes(b"wrong-role-incumbent".to_vec())?)?;
            let id = ObjectId::for_bytes(&canonical);
            store.connection.execute(
                "INSERT INTO wp4m_objects (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id.as_bytes().as_slice(),
                    ObjectKind::Directory as u8,
                    i64::try_from(canonical.len()).map_err(|_| CoreError::LengthOverflow)?,
                    canonical.as_slice(),
                ],
            )?;
            Ok((id, Some(canonical)))
        }
        "malformed-logical-record" => {
            let (id, canonical) =
                canonical_bytes(file_codec::mapping_bytes(file_codec::FILE_ROOT_TAG, &[])?)?;
            store.connection.execute(
                "INSERT INTO wp4m_objects (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id.as_bytes().as_slice(),
                    ObjectKind::Bytes as u8,
                    i64::try_from(canonical.len()).map_err(|_| CoreError::LengthOverflow)?,
                    canonical.as_slice(),
                ],
            )?;
            Ok((id, None))
        }
        _ => Err("unknown touched fault".into()),
    }
}

fn g5_semantic_touched_error(
    store: &mut Store,
    outer_path_charge: u64,
    fault_case: &'static str,
    touched: ObjectId,
    wrong_canonical: Option<&[u8]>,
) -> AnyResult<G5SemanticObservation> {
    let before = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    let mut metrics = Metrics::default();
    let error = store
        .transaction_attempt(&mut metrics, |store, metrics| {
            let scope = establish_edit_base_scope(store, SELECTED_PROFILE, None, None, metrics)?;
            let result = match fault_case {
                "missing-object" | "identity-mismatch" => {
                    store.get_bytes(touched, metrics).map(|_| ())
                }
                "wrong-logical-role" => store.put(
                    touched,
                    wrong_canonical.ok_or(CoreError::MissingObject)?,
                    metrics,
                ),
                "malformed-logical-record" => {
                    let bytes = store.get_bytes(touched, metrics)?;
                    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_ROOT_TAG)?;
                    let _ = file_codec::parse_file_root(payload)?;
                    Ok(())
                }
                _ => Err("unknown touched fault".into()),
            };
            drop(scope);
            result
        })
        .expect_err("touched semantic fault must fail");
    let provenance = error
        .downcast_ref::<PublicationFailure>()
        .map(|failure| failure.0);
    let cause = provenance
        .and_then(|value| value.dominant.or(value.first))
        .unwrap_or_else(|| failure_cause(error.as_ref()));
    let mut cleanup_ok = store.active_transaction.is_none();
    if store.active_transaction.is_some() {
        cleanup_ok = store.rollback(&mut metrics).is_ok();
    }
    let after = store
        .current_head()?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    g5_require_live_q(g5_expected_live_q(outer_path_charge, store)?)?;
    metrics.q_current = 0;
    Ok(G5SemanticObservation {
        before,
        after,
        error: Some(cause),
        fault_case: Some(fault_case),
        error_class: Some(match cause {
            FailureCause::MissingObject(_) => "MissingObject",
            FailureCause::Core(CoreError::IdentityMismatch) => "IdentityMismatch",
            FailureCause::Core(CoreError::WrongLogicalRole) => "WrongLogicalRole",
            FailureCause::Core(CoreError::UnexpectedEof) => "UnexpectedEof",
            _ => "Unexpected",
        }),
        failure_boundary: Some("PreCommit"),
        provenance,
        publication_status: None,
        transactions: metrics.transactions,
        commits: metrics.commits,
        scrub_calls: metrics.edit_base_complete_scrub_calls,
        scrub_bytes: metrics.edit_base_complete_scrub_canonical_bytes,
        verified_reopen_scrub_calls: 0,
        verified_reopen_scrub_bytes: 0,
        trusted_equal_edges: metrics.trusted_assumed_equal_edges,
        trusted_prior_references: metrics.trusted_assumed_prior_references,
        trusted_prior_raw_bytes: metrics.trusted_assumed_prior_raw_bytes,
        verified_carry_forward: false,
        q_high_water: metrics.q_high_water,
        q_current: metrics.q_current,
        cleanup_ok,
    })
}

struct G5JsonDebug<T>(Option<T>);

impl<T: std::fmt::Debug> std::fmt::Display for G5JsonDebug<T> {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(value) => write!(output, "\"{value:?}\""),
            None => output.write_str("null"),
        }
    }
}

struct G5JsonString(Option<&'static str>);

impl std::fmt::Display for G5JsonString {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(value) => write!(output, "\"{value}\""),
            None => output.write_str("null"),
        }
    }
}

struct G5JsonFailure(Option<FailureCause>);

impl std::fmt::Display for G5JsonFailure {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(FailureCause::MissingObject(id)) => {
                write!(output, "\"MissingObject(ObjectId({id}))\"")
            }
            Some(FailureCause::Core(error)) => write!(output, "\"Core({error:?})\""),
            None => output.write_str("null"),
        }
    }
}

fn g5_emit_semantic(
    output: &mut impl std::io::Write,
    case: &str,
    mode: &str,
    observation: G5SemanticObservation,
    later_error: Option<FailureCause>,
    residue: bool,
) -> AnyResult<()> {
    let reconciliation = observation.provenance.map(|value| value.reconciliation);
    writeln!(output, "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-semantic-v10\",\"case\":\"{case}\",\"integrity_mode\":\"{mode}\",\"fault_case\":{},\"error_class\":{},\"failure_boundary\":{},\"error\":{},\"later_snapshot_error\":{},\"publication_status\":{},\"reconciliation\":{},\"before_generation\":{},\"after_generation\":{},\"before_root\":\"{}\",\"after_root\":\"{}\",\"head_unchanged\":{},\"transactions\":{},\"commits\":{},\"edit_base_complete_scrub_calls\":{},\"edit_base_complete_scrub_canonical_bytes\":{},\"verified_reopen_complete_scrub_calls\":{},\"verified_reopen_complete_scrub_canonical_bytes\":{},\"trusted_assumed_equal_edges\":{},\"trusted_assumed_prior_references\":{},\"trusted_assumed_prior_raw_bytes\":{},\"verified_carry_forward\":{},\"cleanup_ok\":{},\"residue\":{},\"q_high_water\":{},\"q_current\":{}}}", G5JsonString(observation.fault_case), G5JsonString(observation.error_class), G5JsonString(observation.failure_boundary), G5JsonFailure(observation.error), G5JsonFailure(later_error), G5JsonDebug(observation.publication_status), G5JsonDebug(reconciliation), observation.before.0, observation.after.0, observation.before.1, observation.after.1, observation.before == observation.after, observation.transactions, observation.commits, observation.scrub_calls, observation.scrub_bytes, observation.verified_reopen_scrub_calls, observation.verified_reopen_scrub_bytes, observation.trusted_equal_edges, observation.trusted_prior_references, observation.trusted_prior_raw_bytes, observation.verified_carry_forward, observation.cleanup_ok, residue, observation.q_high_water, observation.q_current)?;
    output.flush()?;
    Ok(())
}

fn g5_cleanup(paths: &G5OwnedSemanticPaths) -> AnyResult<bool> {
    remove_sqlite_image(&paths.database)?;
    Ok(paths.database.exists() || paths.authority.exists() || paths.expectations.exists())
}

fn g5_semantic_mode_database(
    root: &Path,
    label: &str,
    mode: IntegrityMode,
) -> AnyResult<(G5OwnedSemanticPaths, Store, ObjectId, ObjectId)> {
    let mut path_metrics = Metrics::default();
    let paths = G5OwnedSemanticPaths::new(root, label, &mut path_metrics)?;
    let (store, _, file, common, unrelated) = g5_semantic_base(&paths)?;
    drop(store);
    g5_require_live_q(paths.charge_bytes())?;
    let mut open_metrics = Metrics::default();
    let store = Store::open_measured_with_integrity_mode(
        &paths.database,
        SELECTED_PROFILE,
        &mut open_metrics,
        None,
        mode,
    )?;
    g5_require_live_q(g5_expected_live_q(paths.charge_bytes(), &store)?)?;
    Ok((
        paths,
        store,
        if label.contains("touched") {
            file
        } else {
            common
        },
        unrelated,
    ))
}

fn g5_semantic(case: &str, root: &Path) -> AnyResult<()> {
    if !root.is_absolute() {
        return Err("semantic root must be absolute".into());
    }
    fs::create_dir_all(root)?;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    match case {
        "touched-error-matrix" => {
            for fault_case in [
                "missing-object",
                "identity-mismatch",
                "wrong-logical-role",
                "malformed-logical-record",
            ] {
                for (mode, mode_name) in [
                    (IntegrityMode::Verified, "verified"),
                    (IntegrityMode::TrustedLocalDev, "trusted-local-dev"),
                ] {
                    let label = if mode == IntegrityMode::Verified {
                        match fault_case {
                            "missing-object" => "touched-matrix-missing-verified",
                            "identity-mismatch" => "touched-matrix-identity-verified",
                            "wrong-logical-role" => "touched-matrix-role-verified",
                            "malformed-logical-record" => "touched-matrix-malformed-verified",
                            _ => unreachable!(),
                        }
                    } else {
                        match fault_case {
                            "missing-object" => "touched-matrix-missing-trusted",
                            "identity-mismatch" => "touched-matrix-identity-trusted",
                            "wrong-logical-role" => "touched-matrix-role-trusted",
                            "malformed-logical-record" => "touched-matrix-malformed-trusted",
                            _ => unreachable!(),
                        }
                    };
                    let (database, mut store, file, _) =
                        g5_semantic_mode_database(root, label, mode)?;
                    let (touched, wrong_canonical) =
                        g5_seed_touched_fault(&store, fault_case, file)?;
                    let observation = g5_semantic_touched_error(
                        &mut store,
                        database.charge_bytes(),
                        fault_case,
                        touched,
                        wrong_canonical.as_deref(),
                    )?;
                    let expected = match fault_case {
                        "missing-object" => FailureCause::MissingObject(file),
                        "identity-mismatch" => FailureCause::Core(CoreError::IdentityMismatch),
                        "wrong-logical-role" => FailureCause::Core(CoreError::WrongLogicalRole),
                        "malformed-logical-record" => FailureCause::Core(CoreError::UnexpectedEof),
                        _ => unreachable!(),
                    };
                    if observation.error != Some(expected)
                        || observation.commits != 0
                        || observation.before != observation.after
                        || observation.error_class == Some("Unexpected")
                        || observation.failure_boundary != Some("PreCommit")
                        || !observation.cleanup_ok
                    {
                        return Err(CoreError::PublicationConflict.into());
                    }
                    drop(wrong_canonical);
                    drop(store);
                    let residue = g5_cleanup(&database)?;
                    drop(database);
                    g5_terminal_q()?;
                    g5_emit_semantic(&mut output, case, mode_name, observation, None, residue)?;
                }
            }
        }
        "touched-corruption" => {
            for (mode, name) in [
                (IntegrityMode::Verified, "verified"),
                (IntegrityMode::TrustedLocalDev, "trusted-local-dev"),
            ] {
                let label = if mode == IntegrityMode::Verified {
                    "touched-verified"
                } else {
                    "touched-trusted"
                };
                let (database, mut store, touched, _) =
                    g5_semantic_mode_database(root, label, mode)?;
                g5_corrupt(&store, touched)?;
                let observation =
                    g5_semantic_edit(&mut store, database.charge_bytes(), None, false)?;
                if observation.error != Some(FailureCause::Core(CoreError::IdentityMismatch))
                    || observation.commits != 0
                    || observation.before != observation.after
                {
                    return Err(CoreError::PublicationConflict.into());
                }
                drop(store);
                let residue = g5_cleanup(&database)?;
                drop(database);
                g5_terminal_q()?;
                g5_emit_semantic(&mut output, case, name, observation, None, residue)?;
            }
        }
        "unrelated-corruption" => {
            let (verified_db, mut verified, _, unrelated) =
                g5_semantic_mode_database(root, "unrelated-verified", IntegrityMode::Verified)?;
            g5_corrupt(&verified, unrelated)?;
            let verified_observation =
                g5_semantic_edit(&mut verified, verified_db.charge_bytes(), None, false)?;
            if verified_observation.error != Some(FailureCause::Core(CoreError::IdentityMismatch))
                || verified_observation.commits != 0
            {
                return Err(CoreError::PublicationConflict.into());
            }
            drop(verified);
            let verified_residue = g5_cleanup(&verified_db)?;
            drop(verified_db);
            g5_terminal_q()?;
            g5_emit_semantic(
                &mut output,
                case,
                "verified",
                verified_observation,
                None,
                verified_residue,
            )?;

            let (trusted_db, mut trusted, _, unrelated) = g5_semantic_mode_database(
                root,
                "unrelated-trusted",
                IntegrityMode::TrustedLocalDev,
            )?;
            g5_corrupt(&trusted, unrelated)?;
            let trusted_observation =
                g5_semantic_edit(&mut trusted, trusted_db.charge_bytes(), None, false)?;
            if trusted_observation.error.is_some() || trusted_observation.commits != 1 {
                return Err(CoreError::PublicationConflict.into());
            }
            drop(trusted);
            let mut later_metrics = Metrics::default();
            let mut later = Store::open_measured_with_integrity_mode(
                &trusted_db.database,
                SELECTED_PROFILE,
                &mut later_metrics,
                None,
                IntegrityMode::Verified,
            )?;
            let head = later
                .current_head()?
                .ok_or(CoreError::InvalidValidationReceipt)?;
            let later_error = verify_snapshot_closure(
                &mut later,
                &head,
                &[],
                SELECTED_PROFILE,
                &mut later_metrics,
            )
            .expect_err("unrelated corruption must fail later snapshot");
            let later_error = failure_cause(later_error.as_ref());
            if later_error != FailureCause::Core(CoreError::IdentityMismatch) {
                return Err(CoreError::PublicationConflict.into());
            }
            drop(later);
            g5_require_live_q(trusted_db.charge_bytes())?;
            let trusted_residue = g5_cleanup(&trusted_db)?;
            drop(trusted_db);
            g5_terminal_q()?;
            g5_emit_semantic(
                &mut output,
                case,
                "trusted-local-dev",
                trusted_observation,
                Some(later_error),
                trusted_residue,
            )?;
        }
        "trusted-verified-reopen" => {
            let (database, mut trusted, _, _) = g5_semantic_mode_database(
                root,
                "trusted-verified-reopen",
                IntegrityMode::TrustedLocalDev,
            )?;
            let mut observation =
                g5_semantic_edit(&mut trusted, database.charge_bytes(), None, false)?;
            if observation.error.is_some() || observation.commits != 1 {
                return Err(CoreError::PublicationConflict.into());
            }
            drop(trusted);
            let mut metrics = Metrics::default();
            let mut verified = Store::open_measured_with_integrity_mode(
                &database.database,
                SELECTED_PROFILE,
                &mut metrics,
                None,
                IntegrityMode::Verified,
            )?;
            verified.begin(&mut metrics)?;
            let scope = establish_edit_base_scope(
                &mut verified,
                SELECTED_PROFILE,
                None,
                None,
                &mut metrics,
            )?;
            if scope.provenance() != EditBaseProvenance::VerifiedCompleteClosure
                || metrics.edit_base_complete_scrub_calls == 0
                || metrics.edit_base_complete_scrub_canonical_bytes == 0
            {
                return Err(CoreError::PublicationConflict.into());
            }
            observation.verified_reopen_scrub_calls = metrics.edit_base_complete_scrub_calls;
            observation.verified_reopen_scrub_bytes =
                metrics.edit_base_complete_scrub_canonical_bytes;
            observation.q_high_water = observation.q_high_water.max(metrics.q_high_water);
            drop(scope);
            verified.rollback(&mut metrics)?;
            drop(verified);
            g5_require_live_q(database.charge_bytes())?;
            let residue = g5_cleanup(&database)?;
            drop(database);
            g5_terminal_q()?;
            g5_emit_semantic(
                &mut output,
                case,
                "trusted-local-dev",
                observation,
                None,
                residue,
            )?;
        }
        "reconciliation" => {
            for (label, fault, reject, expected) in [
                (
                    "rollback",
                    Some(PublishFault::BeforeCommit),
                    false,
                    Reconciliation::NotAttempted,
                ),
                ("prior", None, true, Reconciliation::PriorVisible),
                (
                    "requested",
                    Some(PublishFault::AfterCommitBeforeAck),
                    false,
                    Reconciliation::RequestedVisible,
                ),
                (
                    "different",
                    Some(PublishFault::AfterCommitDifferentHead),
                    false,
                    Reconciliation::DifferentHead,
                ),
                (
                    "ambiguous",
                    Some(PublishFault::AfterCommitUnavailable),
                    false,
                    Reconciliation::Ambiguous,
                ),
            ] {
                let (database, mut store, _, _) =
                    g5_semantic_mode_database(root, label, IntegrityMode::TrustedLocalDev)?;
                let observation =
                    g5_semantic_edit(&mut store, database.charge_bytes(), fault, reject)?;
                let actual = observation
                    .provenance
                    .map(|value| value.reconciliation)
                    .ok_or(CoreError::PublicationConflict)?;
                if actual != expected || !observation.cleanup_ok {
                    return Err(CoreError::PublicationConflict.into());
                }
                drop(store);
                let residue = g5_cleanup(&database)?;
                drop(database);
                g5_terminal_q()?;
                g5_emit_semantic(&mut output, case, label, observation, None, residue)?;
            }
        }
        _ => return Err("unknown G5 semantic case".into()),
    }
    g5_terminal_q()?;
    writeln!(output, "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-semantic-terminal-v10\",\"case\":\"{case}\",\"q_current\":0}}")?;
    output.flush()?;
    Ok(())
}

pub(super) fn g5_transport_main() -> AnyResult<()> {
    match g5_arg(1)? {
        "--g5-fixture" => {
            g5_require_args(4)?;
            let root = Path::new(g5_arg(2)?);
            let size = g5_arg(3)?.parse::<u64>()?;
            require_fast_size(size)?;
            prepare_fast_fixture(root, size)?;
            g5_terminal_q()?;
            println!("{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-fixture-v10\",\"size_bytes\":{size},\"q_current\":0}}" );
            Ok(())
        }
        "--g5-prepare" => {
            g5_require_args(6)?;
            let root = Path::new(g5_arg(2)?);
            let size = g5_arg(3)?.parse::<u64>()?;
            require_fast_size(size)?;
            let operation = g5_operation(g5_arg(4)?)?;
            let iteration = g5_arg(5)?.parse::<usize>()?;
            prepare_row_database(
                root,
                root,
                SELECTED_PROFILE,
                size,
                operation,
                iteration,
            )?;
            g5_terminal_q()?;
            println!("{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-prepare-v10\",\"size_bytes\":{size},\"operation\":\"{operation}\",\"iteration\":{iteration},\"q_current\":0}}" );
            Ok(())
        }
        "--g5-child" => g5_child(),
        "--g5-semantic" => {
            g5_require_args(4)?;
            g5_semantic(g5_arg(2)?, Path::new(g5_arg(3)?))
        }
        _ => Err("usage: --g5-fixture ROOT SIZE | --g5-prepare ROOT SIZE OP ITER | --g5-child MODE SIZE OP ROWS FORECAST LIMIT EXE_SHA DB_SHA AUTH_SHA EXPECT_SHA | --g5-semantic CASE ROOT".into()),
    }
}

#[cfg(test)]
mod g5_transport_tests {
    use super::*;

    fn source_section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        source
            .split_once(start)
            .expect("source start")
            .1
            .split_once(end)
            .expect("source end")
            .0
    }

    #[test]
    fn parser_and_forecast_are_fail_closed() {
        let request = g5_request("p01\t/tmp/g5\t7\tfalse\tcapture-only").expect("request");
        assert_eq!(request.id, "p01");
        assert_eq!(request.root, Path::new("/tmp/g5"));
        assert_eq!(request.iteration, 7);
        assert!(g5_request("bad id\t/tmp/g5\t7\tfalse\tcapture-only").is_err());
        assert!(g5_request("p01\trelative\t7\tfalse\tcapture-only").is_err());
        assert!(g5_forecast(119_999_999_999, 120_000_000_000).is_ok());
        assert!(g5_forecast(120_000_000_001, 120_000_000_000).is_err());
    }

    #[test]
    fn reader_and_argv_are_bounded() {
        let argc = g5_arg_count().expect("argc");
        assert!(argc > 0);
        assert!(g5_arg(argc).is_err());
        let mut buffer = [0_u8; G5_REQUEST_BYTES_V10];
        let mut valid = &b"p01\t/tmp/g5\t1\tfalse\tcapture-only\n"[..];
        assert!(g5_read_line(&mut valid, &mut buffer)
            .expect("line")
            .is_some());
        let mut truncated = &b"p01"[..];
        assert!(g5_read_line(&mut truncated, &mut buffer).is_err());
    }

    #[test]
    fn operations_and_digests_are_exact() {
        for operation in [
            "first-edit-after-reopen",
            "same-middle",
            "one-byte-early",
            "one-byte-middle",
            "one-byte-late",
            "plus1-early",
            "plus1-middle",
        ] {
            assert_eq!(g5_operation(operation).expect("operation"), operation);
        }
        assert!(g5_operation("plus1-late").is_err());
        assert!(
            g5_digest("0000000000000000000000000000000000000000000000000000000000000000").is_ok()
        );
        assert!(g5_digest("0").is_err());
    }

    #[test]
    fn row_and_semantic_paths_are_exactly_charged_and_dropped() {
        assert_eq!(q_current(), 0);
        let root = Path::new("/tmp/layerfs-g5-v10-owned-paths");
        let mut metrics = Metrics::default();
        let paths = G5OwnedRowPaths::new(
            root,
            SOURCE_100,
            "first-edit-after-reopen",
            19,
            &mut metrics,
        )
        .expect("row paths");
        assert_eq!(paths.source, source_path(root, SOURCE_100));
        assert_eq!(
            paths.database,
            row_database_path(
                root,
                SELECTED_PROFILE,
                SOURCE_100,
                "first-edit-after-reopen",
                19,
            )
        );
        assert_eq!(paths.authority, authority_path(&paths.database));
        assert_eq!(paths.expectations, expectations_path(&paths.database));
        let expected = [
            &paths.source,
            &paths.database,
            &paths.authority,
            &paths.expectations,
        ]
        .iter()
        .map(|path| path.as_os_str().as_encoded_bytes().len() as u64)
        .sum::<u64>();
        assert_eq!(paths.charge_bytes(), expected);
        assert_eq!(q_current(), expected);
        assert_eq!(metrics.q_high_water, expected);
        drop(paths);
        assert_eq!(q_current(), 0);

        let mut metrics = Metrics::default();
        let paths = G5OwnedSemanticPaths::new(root, "verified-reopen", &mut metrics)
            .expect("semantic paths");
        let expected = [&paths.database, &paths.authority, &paths.expectations]
            .iter()
            .map(|path| path.as_os_str().as_encoded_bytes().len() as u64)
            .sum::<u64>();
        assert_eq!(paths.charge_bytes(), expected);
        assert_eq!(q_current(), expected);
        assert_eq!(metrics.q_high_water, expected);
        drop(paths);
        assert_eq!(q_current(), 0);
    }

    #[test]
    fn row_path_error_unwinds_before_decharge() {
        assert_eq!(q_current(), 0);
        let result = (|| -> AnyResult<()> {
            let mut metrics = Metrics::default();
            let paths = G5OwnedRowPaths::new(
                Path::new("/tmp/layerfs-g5-v10-error-paths"),
                SOURCE_1,
                "plus1-middle",
                7,
                &mut metrics,
            )?;
            g5_require_live_q(paths.charge_bytes())?;
            Err(CoreError::Io.into())
        })();
        assert!(result.is_err());
        assert_eq!(q_current(), 0);
    }

    #[test]
    fn nested_path_and_report_q_and_terminal_owners_are_derived() {
        assert_eq!(q_current(), 0);
        let mut owners = G5ChildOwners::default();
        let mut outer_metrics = Metrics::default();
        owners.request = Some(
            G5OwnedRowPaths::new(
                Path::new("/tmp/layerfs-g5-v10-nested-q"),
                SOURCE_1,
                "same-middle",
                0,
                &mut outer_metrics,
            )
            .expect("outer paths"),
        );
        let outer = owners.request.as_ref().expect("paths").charge_bytes();
        let paths = owners.request.as_ref().expect("paths");
        let inner_bytes = paths
            .database
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_add(paths.authority.as_os_str().as_encoded_bytes().len())
            .expect("inner capacity");
        let mut row_metrics = Metrics::default();
        let inner = charge_capacity(&mut row_metrics, inner_bytes).expect("inner Store paths");
        let report = ChargedString::with_capacity(257, &mut row_metrics).expect("report");
        let expected_peak = outer
            .checked_add(u64::try_from(inner_bytes).expect("inner u64"))
            .and_then(|value| value.checked_add(257))
            .expect("peak");
        assert_eq!(q_current(), expected_peak);
        assert_eq!(row_metrics.q_high_water, expected_peak);
        drop(report);
        drop(inner);
        assert_eq!(q_current(), outer);

        let live = owners.terminal_counts();
        assert_eq!(live.request, 4);
        assert_eq!(live.report, 0);
        drop(owners.request.take());
        let terminal = owners.terminal_counts();
        assert_eq!(terminal.argument, 0);
        assert_eq!(terminal.request, 0);
        assert_eq!(terminal.schedule, 0);
        assert_eq!(terminal.timing, 0);
        assert_eq!(terminal.report, 0);
        assert_eq!(q_current(), 0);
    }

    #[test]
    fn generated_fast_fixture_helper_is_protocol_silent_only() {
        let retained = include_str!(concat!(env!("OUT_DIR"), "/retained_control.rs"));
        let fixture = source_section(
            retained,
            "fn prepare_fast_fixture(",
            "fn prepare_fixed_radix_acceptance_fixtures(",
        );
        assert!(!fixture.contains("println!("));
        assert!(!fixture.contains("fixture={}"));
        assert_eq!(fixture.matches("record.sync_all()?").count(), 1);
        assert_eq!(fixture.matches("Ok(())").count(), 1);

        let unrelated = source_section(
            retained,
            "fn prepare_count_change_scale_fixture(",
            "fn source_hash(",
        );
        assert_eq!(unrelated.matches("println!(").count(), 1);
        assert!(unrelated.contains("fixture={}"));
    }

    #[test]
    fn touched_error_matrix_is_two_mode_exact_and_terminal_q_zero() {
        let root = env::temp_dir().join(format!(
            "layerfs-g5-v10-touched-matrix-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        g5_semantic("touched-error-matrix", &root).expect("touched matrix");
        assert_eq!(q_current(), 0);
        fs::remove_dir_all(root).expect("matrix root cleanup");
    }

    #[test]
    fn fixture_and_prepare_wrappers_are_single_json_after_terminal_q() {
        let session = include_str!("session.rs");
        for (start, end, schema) in [
            (
                "        \"--g5-fixture\" => {",
                "        \"--g5-prepare\" => {",
                "phase4-g5-trusted-fixture-v10",
            ),
            (
                "        \"--g5-prepare\" => {",
                "        \"--g5-child\" => g5_child(),",
                "phase4-g5-trusted-prepare-v10",
            ),
        ] {
            let route = source_section(session, start, end);
            assert_eq!(route.matches("println!(").count(), 1);
            assert_eq!(route.matches(schema).count(), 1);
            assert_eq!(route.matches("g5_terminal_q()?").count(), 1);
            assert_eq!(route.matches("\\\"q_current\\\":0").count(), 1);
            assert!(
                route.find("g5_terminal_q()?").expect("terminal Q")
                    < route.find("println!(").expect("JSON print")
            );
        }
    }
}
