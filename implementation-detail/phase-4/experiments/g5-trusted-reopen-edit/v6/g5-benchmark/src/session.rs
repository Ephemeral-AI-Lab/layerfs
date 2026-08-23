const G5_REQUEST_BYTES_V6: usize = 4_096;
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
        total.checked_add(*value).ok_or_else(|| CoreError::LengthOverflow.into())
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
        let source = g5_rooted_path(
            root,
            separator,
            source_capacity,
            &[source_label, ".source"],
        )?;
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
        let database_capacity =
            g5_checked_sum(&[prefix, "g5-semantic-v6-".len(), label.len(), ".sqlite".len()])?;
        let authority_capacity = database_capacity
            .checked_add(".authority".len())
            .ok_or(CoreError::LengthOverflow)?;
        let expectations_capacity = database_capacity
            .checked_add(".expectations".len())
            .ok_or(CoreError::LengthOverflow)?;
        let combined = g5_checked_sum(&[
            database_capacity,
            authority_capacity,
            expectations_capacity,
        ])?;
        let charge = charge_capacity(metrics, combined)?;
        let database_parts = ["g5-semantic-v6-", label, ".sqlite"];
        let database = g5_rooted_path(root, separator, database_capacity, &database_parts)?;
        let authority = g5_rooted_path(
            root,
            separator,
            authority_capacity,
            &["g5-semantic-v6-", label, ".sqlite.authority"],
        )?;
        let expectations = g5_rooted_path(
            root,
            separator,
            expectations_capacity,
            &["g5-semantic-v6-", label, ".sqlite.expectations"],
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
            request: self.request.as_ref().map_or(0, G5OwnedRowPaths::owner_count),
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
    buffer: &'a mut [u8; G5_REQUEST_BYTES_V6],
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
    let mut buffer = [0_u8; G5_REQUEST_BYTES_V6];
    let mut rows = 0_u64;
    let mut owners = G5ChildOwners::default();

    writeln!(output, "{{\"status\":\"READY\",\"schema\":\"phase4-g5-trusted-child-ready-v6\",\"integrity_mode\":\"{mode_name}\",\"mode_provenance\":\"fixed-at-child-start\",\"size_bytes\":{size},\"operation\":\"{operation}\",\"expected_rows\":{expected_rows},\"full_wrapper_forecast_ns\":{forecast_ns},\"full_wrapper_limit_ns\":{limit_ns},\"custody\":\"runner-preverified-borrowed\",\"request_schema\":\"id\\troot\\titeration\\twarmup\\tvalidation\"}}")?;
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
        writeln!(output, "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-child-row-v6\",\"request_id\":\"{}\",\"integrity_mode\":\"{mode_name}\",\"mode_provenance\":\"fixed-at-child-start\",\"row\":{}}}", request.id, &**report)?;
        output.flush()?;
        drop(owners.report.take());
        drop(owners.request.take());
        g5_terminal_q()?;
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
    writeln!(output, "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-child-terminal-v6\",\"integrity_mode\":\"{mode_name}\",\"rows\":{rows},\"expected_rows\":{expected_rows},\"argument_owners\":{},\"request_owners\":{},\"schedule_owners\":{},\"timing_owners\":{},\"report_owners\":{},\"q_current\":{},\"rss\":\"external-high-water-only\"}}", terminal.argument, terminal.request, terminal.schedule, terminal.timing, terminal.report, q_current())?;
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
    let mut builder = FileBuilder::new(
        SELECTED_PROFILE,
        G5_SEMANTIC_REFERENCES,
        &mut metrics,
    )?;
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
    let head = store.current_head()?.ok_or(CoreError::InvalidValidationReceipt)?;
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
    let before = store.current_head()?.ok_or(CoreError::InvalidValidationReceipt)?;
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
        Store::finish_publication(
            started,
            result,
            Some(provenance),
            carry,
            &mut metrics,
        )
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
    let after = store.current_head()?.ok_or(CoreError::InvalidValidationReceipt)?;
    g5_require_live_q(g5_expected_live_q(outer_path_charge, store)?)?;
    metrics.q_current = 0;
    Ok(G5SemanticObservation {
        before,
        after,
        error,
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

struct G5JsonDebug<T>(Option<T>);

impl<T: std::fmt::Debug> std::fmt::Display for G5JsonDebug<T> {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(value) => write!(output, "\"{value:?}\""),
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
    let reconciliation = observation
        .provenance
        .map(|value| value.reconciliation);
    writeln!(output, "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-semantic-v6\",\"case\":\"{case}\",\"integrity_mode\":\"{mode}\",\"error\":{},\"later_snapshot_error\":{},\"publication_status\":{},\"reconciliation\":{},\"before_generation\":{},\"after_generation\":{},\"before_root\":\"{}\",\"after_root\":\"{}\",\"head_unchanged\":{},\"transactions\":{},\"commits\":{},\"edit_base_complete_scrub_calls\":{},\"edit_base_complete_scrub_canonical_bytes\":{},\"verified_reopen_complete_scrub_calls\":{},\"verified_reopen_complete_scrub_canonical_bytes\":{},\"trusted_assumed_equal_edges\":{},\"trusted_assumed_prior_references\":{},\"trusted_assumed_prior_raw_bytes\":{},\"verified_carry_forward\":{},\"cleanup_ok\":{},\"residue\":{},\"q_high_water\":{},\"q_current\":{}}}", G5JsonDebug(observation.error), G5JsonDebug(later_error), G5JsonDebug(observation.publication_status), G5JsonDebug(reconciliation), observation.before.0, observation.after.0, observation.before.1, observation.after.1, observation.before == observation.after, observation.transactions, observation.commits, observation.scrub_calls, observation.scrub_bytes, observation.verified_reopen_scrub_calls, observation.verified_reopen_scrub_bytes, observation.trusted_equal_edges, observation.trusted_prior_references, observation.trusted_prior_raw_bytes, observation.verified_carry_forward, observation.cleanup_ok, residue, observation.q_high_water, observation.q_current)?;
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
    Ok((paths, store, if label.contains("touched") { file } else { common }, unrelated))
}

fn g5_semantic(case: &str, root: &Path) -> AnyResult<()> {
    if !root.is_absolute() {
        return Err("semantic root must be absolute".into());
    }
    fs::create_dir_all(root)?;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    match case {
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
                g5_emit_semantic(
                    &mut output,
                    case,
                    name,
                    observation,
                    None,
                    residue,
                )?;
            }
        }
        "unrelated-corruption" => {
            let (verified_db, mut verified, _, unrelated) = g5_semantic_mode_database(
                root,
                "unrelated-verified",
                IntegrityMode::Verified,
            )?;
            g5_corrupt(&verified, unrelated)?;
            let verified_observation =
                g5_semantic_edit(&mut verified, verified_db.charge_bytes(), None, false)?;
            if verified_observation.error
                != Some(FailureCause::Core(CoreError::IdentityMismatch))
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
            let head = later.current_head()?.ok_or(CoreError::InvalidValidationReceipt)?;
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
                let (database, mut store, _, _) = g5_semantic_mode_database(
                    root,
                    label,
                    IntegrityMode::TrustedLocalDev,
                )?;
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
                g5_emit_semantic(
                    &mut output,
                    case,
                    label,
                    observation,
                    None,
                    residue,
                )?;
            }
        }
        _ => return Err("unknown G5 semantic case".into()),
    }
    g5_terminal_q()?;
    writeln!(output, "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-semantic-terminal-v6\",\"case\":\"{case}\",\"q_current\":0}}")?;
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
            println!("{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-fixture-v6\",\"size_bytes\":{size},\"q_current\":0}}" );
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
            println!("{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-prepare-v6\",\"size_bytes\":{size},\"operation\":\"{operation}\",\"iteration\":{iteration},\"q_current\":0}}" );
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
        let mut buffer = [0_u8; G5_REQUEST_BYTES_V6];
        let mut valid = &b"p01\t/tmp/g5\t1\tfalse\tcapture-only\n"[..];
        assert!(g5_read_line(&mut valid, &mut buffer).expect("line").is_some());
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
        assert!(g5_digest("0000000000000000000000000000000000000000000000000000000000000000").is_ok());
        assert!(g5_digest("0").is_err());
    }
}
