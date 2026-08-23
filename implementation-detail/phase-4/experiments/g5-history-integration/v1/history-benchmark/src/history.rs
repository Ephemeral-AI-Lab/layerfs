use std::collections::BTreeSet;
use std::ffi::CStr;

const H11_MAX_REVISION: u64 = 1_001;
const H11_HISTORY_POINTS: [u64; 4] = [1, 10, 100, 1_000];
const H11_SAMPLES: [u64; 2] = [1, 2];
const H11_RANGE_BYTES: u64 = 64 * 1024;
const G5_HISTORY_RANGE_BYTES: u64 = 4 * 1024;
const H11_CACHE_PAGES: i64 = 1_500;
const H11_REACHABILITY_ENTRY_Q: usize =
    std::mem::size_of::<ObjectId>() + 4 * std::mem::size_of::<usize>();

thread_local! {
    static H11_Q_CURRENT: Cell<u64> = const { Cell::new(0) };
    static H11_Q_HIGH_WATER: Cell<u64> = const { Cell::new(0) };
}

#[derive(Debug)]
struct H11CapacityCharge(u64);

impl H11CapacityCharge {
    fn absorb(&mut self, mut other: Self) -> CoreResult<()> {
        self.0 = self
            .0
            .checked_add(other.0)
            .ok_or(CoreError::LengthOverflow)?;
        other.0 = 0;
        Ok(())
    }
}

impl Drop for H11CapacityCharge {
    fn drop(&mut self) {
        H11_Q_CURRENT.with(|current| {
            current.set(
                current
                    .get()
                    .checked_sub(self.0)
                    .expect("H11 logical Q imbalance"),
            );
        });
    }
}

fn h11_charge(bytes: usize) -> CoreResult<H11CapacityCharge> {
    let bytes = u64::try_from(bytes).map_err(|_| CoreError::LengthOverflow)?;
    let current = H11_Q_CURRENT.with(|current| {
        let next = current
            .get()
            .checked_add(bytes)
            .ok_or(CoreError::LengthOverflow)?;
        if next > layerfs_core::limits::MAX_DURABLE_LIVE_ALLOCATION {
            return Err(CoreError::AllocationBudgetExceeded);
        }
        current.set(next);
        Ok(next)
    })?;
    H11_Q_HIGH_WATER.with(|high| high.set(high.get().max(current)));
    Ok(H11CapacityCharge(bytes))
}

fn h11_q_current() -> u64 {
    H11_Q_CURRENT.with(Cell::get)
}

fn h11_q_high_water() -> u64 {
    H11_Q_HIGH_WATER.with(Cell::get)
}

fn h11_observe_product_q(product_q: u64) -> CoreResult<()> {
    let combined = h11_q_current()
        .checked_add(product_q)
        .ok_or(CoreError::LengthOverflow)?;
    H11_Q_HIGH_WATER.with(|high| high.set(high.get().max(combined)));
    Ok(())
}

fn h11_reset_q() -> CoreResult<()> {
    if h11_q_current() != 0 {
        return Err(CoreError::LengthMismatch {
            expected: 0,
            actual: h11_q_current(),
        });
    }
    H11_Q_HIGH_WATER.with(|high| high.set(0));
    Ok(())
}

#[derive(Debug)]
struct H11ChargedVec<T> {
    values: Vec<T>,
    _charge: H11CapacityCharge,
}

impl<T> H11ChargedVec<T> {
    fn with_capacity(capacity: usize) -> CoreResult<Self> {
        let bytes = capacity
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(CoreError::LengthOverflow)?;
        let charge = h11_charge(bytes)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| CoreError::AllocationFailed)?;
        if values.capacity() != capacity {
            return Err(CoreError::AllocationFailed);
        }
        Ok(Self {
            values,
            _charge: charge,
        })
    }
}

impl<T> Deref for H11ChargedVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T> std::ops::DerefMut for H11ChargedVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

struct H11ChargedString {
    value: String,
    _charge: H11CapacityCharge,
}

impl H11ChargedString {
    fn with_capacity(capacity: usize) -> CoreResult<Self> {
        let charge = h11_charge(capacity)?;
        let mut value = String::new();
        value
            .try_reserve_exact(capacity)
            .map_err(|_| CoreError::AllocationFailed)?;
        if value.capacity() != capacity {
            return Err(CoreError::AllocationFailed);
        }
        Ok(Self {
            value,
            _charge: charge,
        })
    }

    fn read_exact(path: &Path) -> AnyResult<Self> {
        let capacity =
            usize::try_from(fs::metadata(path)?.len()).map_err(|_| CoreError::LengthOverflow)?;
        let mut value = Self::with_capacity(capacity)?;
        File::open(path)?.read_to_string(&mut value.value)?;
        if value.value.len() != capacity || value.value.capacity() != capacity {
            return Err(CoreError::LengthMismatch {
                expected: u64::try_from(capacity).map_err(|_| CoreError::LengthOverflow)?,
                actual: u64::try_from(value.value.len()).map_err(|_| CoreError::LengthOverflow)?,
            }
            .into());
        }
        Ok(value)
    }
}

impl Deref for H11ChargedString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

struct H11Reachability {
    ids: BTreeSet<ObjectId>,
    charge: H11CapacityCharge,
}

impl H11Reachability {
    fn new() -> Self {
        Self {
            ids: BTreeSet::new(),
            charge: H11CapacityCharge(0),
        }
    }

    fn insert(&mut self, id: ObjectId) -> CoreResult<bool> {
        if self.ids.contains(&id) {
            return Ok(false);
        }
        let charge = h11_charge(H11_REACHABILITY_ENTRY_Q)?;
        if !self.ids.insert(id) {
            return Err(CoreError::NonCanonicalOrdering);
        }
        self.charge.absorb(charge)?;
        Ok(true)
    }

    fn extend(&mut self, ids: impl IntoIterator<Item = ObjectId>) -> CoreResult<()> {
        for id in ids {
            self.insert(id)?;
        }
        Ok(())
    }

    fn len(&self) -> usize {
        self.ids.len()
    }

    fn iter(&self) -> impl Iterator<Item = &ObjectId> {
        self.ids.iter()
    }
}

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

struct H11History {
    edit_q_high_water: H11ChargedVec<u64>,
    objects_created: u64,
    objects_reused: u64,
    canonical_new_bytes: u64,
    mapping_rewritten: u64,
    transactions: u64,
    commits: u64,
    q_high_water: u64,
}

#[derive(Clone, Copy)]
struct G5HistoryCheckpoint {
    revision: u64,
    current_root: ObjectId,
    current_transition: ObjectId,
    current_output_digest: [u8; 32],
    range_digest: [u8; 32],
    next_revision: u64,
    next_root: ObjectId,
    next_transition: ObjectId,
    head: H11Operation,
    range: H11Operation,
    reconstruction: H11Operation,
    edit: H11Operation,
    projection: phase4_g3_materialization::HistoryProjectionObservation,
    logical_store_bytes: u64,
    apparent_store_bytes: u64,
    allocated_store_bytes: u64,
    live_objects: u64,
    unreachable_objects: u64,
    q_high_water: u64,
    fd_count: u64,
}

struct G5HistoryCheckpointStart {
    revision: u64,
    current_root: ObjectId,
    current_transition: ObjectId,
    current_output_digest: [u8; 32],
    range_digest: [u8; 32],
    head: H11Operation,
    range: H11Operation,
    reconstruction: H11Operation,
    logical_store_bytes: u64,
    apparent_store_bytes: u64,
    allocated_store_bytes: u64,
    live_objects: u64,
    unreachable_objects: u64,
    q_high_water: u64,
    fd_count: u64,
    projection: phase4_g3_materialization::HistoryProjectionSession,
}

#[derive(Clone, Copy)]
struct G5RevertObservation {
    root_a: ObjectId,
    root_b: ObjectId,
    final_root: ObjectId,
    transition: ObjectId,
    operation: H11Operation,
    logical_before: u64,
    logical_after: u64,
    historical_root: ObjectId,
    historical_bytes: u64,
    historical_digest: [u8; 32],
    q_high_water: u64,
    final_expected: H11Expected,
}

#[derive(Clone, Copy)]
struct G5ConcurrencyObservation {
    prior_root: ObjectId,
    new_root: ObjectId,
    reader_one_before: ObjectId,
    reader_one_after: ObjectId,
    reader_two_before: ObjectId,
    reader_two_after: ObjectId,
    writer: H11Operation,
    busy_errors: u64,
    locked_errors: u64,
    q_high_water: u64,
}

impl H11History {
    fn new(capacity: usize) -> CoreResult<Self> {
        Ok(Self {
            edit_q_high_water: H11ChargedVec::with_capacity(capacity)?,
            objects_created: 0,
            objects_reused: 0,
            canonical_new_bytes: 0,
            mapping_rewritten: 0,
            transactions: 0,
            commits: 0,
            q_high_water: 0,
        })
    }
}

#[derive(Clone, Copy)]
struct H11HistoricalTuple {
    revision: u64,
    root: ObjectId,
    transition: ObjectId,
    output_digest: [u8; 32],
}

#[derive(Clone, Copy)]
struct H11RevisionIdentity {
    revision: u64,
    root: ObjectId,
    transition: ObjectId,
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

fn h11_digest_string(value: &[u8; 32]) -> AnyResult<H11ChargedString> {
    let mut output = H11ChargedString::with_capacity(64)?;
    write!(
        output.value,
        "{}",
        ObjectId::from_bytes(value).map_err(|_| CoreError::IdentityMismatch)?
    )?;
    Ok(output)
}

fn h11_expected(path: &Path) -> AnyResult<H11ChargedVec<H11Expected>> {
    let input = H11ChargedString::read_exact(path)?;
    let mut rows = H11ChargedVec::with_capacity(
        usize::try_from(H11_MAX_REVISION).map_err(|_| CoreError::LengthOverflow)?,
    )?;
    for (index, line) in input.lines().enumerate() {
        if index == 0 {
            if line != "revision\troot\ttransition\tfile\toutput_digest\toccurrence_digest\tclosure_digest\trange_digest" {
                return Err(CoreError::InvalidRecord("H11 expected manifest header").into());
            }
            continue;
        }
        let mut fields = line.split('\t');
        let revision_field = fields
            .next()
            .ok_or(CoreError::InvalidRecord("H11 expected manifest row"))?;
        let root = fields
            .next()
            .ok_or(CoreError::InvalidRecord("H11 expected manifest row"))?;
        let transition = fields
            .next()
            .ok_or(CoreError::InvalidRecord("H11 expected manifest row"))?;
        let file = fields
            .next()
            .ok_or(CoreError::InvalidRecord("H11 expected manifest row"))?;
        let output_digest = fields
            .next()
            .ok_or(CoreError::InvalidRecord("H11 expected manifest row"))?;
        let occurrence_digest = fields
            .next()
            .ok_or(CoreError::InvalidRecord("H11 expected manifest row"))?;
        let closure_digest = fields
            .next()
            .ok_or(CoreError::InvalidRecord("H11 expected manifest row"))?;
        let range_digest = fields
            .next()
            .ok_or(CoreError::InvalidRecord("H11 expected manifest row"))?;
        if fields.next().is_some() {
            return Err(CoreError::InvalidRecord("H11 expected manifest row").into());
        }
        let revision = revision_field.parse::<u64>()?;
        if revision != u64::try_from(index).map_err(|_| CoreError::LengthOverflow)? {
            return Err(CoreError::NonCanonicalOrdering.into());
        }
        rows.push(H11Expected {
            revision,
            root: root.parse()?,
            transition: transition.parse()?,
            file: file.parse()?,
            output_digest: h11_digest(output_digest)?,
            occurrence_digest: h11_digest(occurrence_digest)?,
            closure_digest: h11_digest(closure_digest)?,
            range_digest: h11_digest(range_digest)?,
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

fn h11_edit_point(source: &Path) -> AnyResult<EditPoint> {
    let mut metrics = Metrics::default();
    let cdc_charge = charge_capacity(&mut metrics, 32 * 1024)?;
    let (reference_count, byte_offset, _) = source_edit_point(source, "one-byte-middle")?;
    drop(cdc_charge);
    finish_q(&mut metrics)?;
    h11_observe_product_q(metrics.q_high_water)?;
    if reference_count < 8 {
        return Err(CoreError::NonCanonicalPagePartition.into());
    }
    Ok(EditPoint {
        reference_count,
        position: reference_count / 2,
        byte_offset,
        replacement_length: 1,
    })
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
    identity: H11RevisionIdentity,
    expected_parent: Option<ObjectId>,
    expected_operations: Option<&[delta_codec::TransitionOperation]>,
    range: std::ops::Range<u64>,
    metrics: &mut Metrics,
) -> AnyResult<H11Expected> {
    let transition_digest = verify_transition(
        store,
        identity.transition,
        expected_parent,
        identity.root,
        expected_operations,
        metrics,
    )?;
    let mut emit = |_bytes: &[u8]| Ok(());
    let reconstructed = reconstruct_file_to(
        store,
        identity.root,
        None,
        None,
        true,
        true,
        metrics,
        &mut emit,
    )?;
    let file = resolve_namespace_file_root(store, identity.root, metrics)?;
    let selected = read_file_range(store, file, SELECTED_PROFILE, range, metrics)?;
    let range_digest = *blake3::hash(&selected).as_bytes();
    drop(selected);
    Ok(H11Expected {
        revision: identity.revision,
        root: identity.root,
        transition: identity.transition,
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
        drop(prior_operations);
        drop(prior_operations_charge);
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
            EditBaseScope::Verified(permit),
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
                H11RevisionIdentity {
                    revision,
                    root,
                    transition,
                },
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
        drop(operations);
        drop(operations_charge);
        store.publish_qualified(authority, metrics)?;
        Ok(observed)
    })?;
    let wall_ns = started.elapsed().as_nanos();
    finish_q(&mut metrics)?;
    h11_observe_product_q(metrics.q_high_water)?;
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
    let observed = h11_observe_revision(
        store,
        H11RevisionIdentity {
            revision: 1,
            root,
            transition,
        },
        None,
        None,
        range,
        &mut metrics,
    )?;
    store.publish(None, root, transition, &mut metrics)?;
    let wall_ns = started.elapsed().as_nanos();
    finish_q(&mut metrics)?;
    h11_observe_product_q(metrics.q_high_water)?;
    if metrics.transactions != 1 || metrics.commits != 1 {
        return Err(CoreError::PublicationConflict.into());
    }
    Ok((observed, h11_operation(&metrics, wall_ns)))
}

fn h11_oracle(source: &Path, database: &Path, manifest: &Path, operation_log: &Path) -> AnyResult<()> {
    if database.exists() || authority_path(database).exists() || manifest.exists() || operation_log.exists() {
        return Err("H11 oracle targets must be absent".into());
    }
    let target = h11_edit_point(source)?;
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
    live: &mut H11Reachability,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if !live.insert(id)? {
        return Ok(());
    }
    let bytes = store.get_bytes(id, metrics)?;
    if level == 0 {
        let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_LEAF_TAG)?;
        let _references_charge = charge_decoded_file_references(payload, metrics)?;
        let references = file_codec::parse_file_leaf(payload)?;
        file_codec::validate_file_leaf(&references, final_node)?;
        live.extend(references.into_iter().map(|reference| reference.object_id))?;
        return Ok(());
    }
    let payload = file_codec::decode_mapping(&bytes, file_codec::FILE_BRANCH_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, false, metrics)?;
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
    live: &mut H11Reachability,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if !live.insert(root)? {
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
    if !live.insert(file)? {
        return Ok(());
    }
    let file_bytes = store.get_bytes(file, metrics)?;
    let payload = file_codec::decode_mapping(&file_bytes, file_codec::FILE_ROOT_TAG)?;
    let _children_charge = charge_decoded_file_children(payload, true, metrics)?;
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
    live: &mut H11Reachability,
    metrics: &mut Metrics,
) -> AnyResult<()> {
    if !live.insert(transition)? {
        return Ok(());
    }
    let bytes = store.get_bytes(transition, metrics)?;
    let _pages_charge = charge_capacity(metrics, bytes.len())?;
    let decoded = delta_codec::decode_mapping_transition(&bytes)?;
    live.extend(decoded.pages)?;
    Ok(())
}

fn h11_is_mapping(canonical: &[u8]) -> bool {
    layerfs_core::decode_bytes_object(canonical)
        .is_ok_and(|inner| inner.starts_with(&layerfs_core::content::persistence::MAPPING_MAGIC))
}

fn h11_sum_ids(store: &Store, ids: &H11Reachability) -> AnyResult<(u64, u64)> {
    let mut canonical = 0_u64;
    let mut mapping = 0_u64;
    let mut metrics = Metrics::default();
    for id in ids.iter() {
        let (_decoded_charge, _object, bytes) = store.get(*id, &mut metrics)?;
        let len = u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
        canonical = canonical.checked_add(len).ok_or(CoreError::LengthOverflow)?;
        if h11_is_mapping(&bytes) {
            mapping = mapping.checked_add(len).ok_or(CoreError::LengthOverflow)?;
        }
    }
    finish_q(&mut metrics)?;
    h11_observe_product_q(metrics.q_high_water)?;
    Ok((canonical, mapping))
}

fn h11_object_stats(
    store: &Store,
    retained: &[H11Expected],
    current: H11Expected,
) -> AnyResult<H11ObjectStats> {
    let stored_count_i64: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM wp4m_objects", [], |row| row.get(0))?;
    let stored_objects = u64::try_from(stored_count_i64).map_err(|_| CoreError::LengthOverflow)?;
    let mut stored_ids = H11ChargedVec::with_capacity(
        usize::try_from(stored_objects).map_err(|_| CoreError::LengthOverflow)?,
    )?;
    {
        let mut statement = store
            .connection
            .prepare("SELECT object_id FROM wp4m_objects ORDER BY object_id")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            stored_ids.push(ObjectId::from_bytes(row.get_ref(0)?.as_blob()?)?);
        }
    }
    if stored_ids.len() != usize::try_from(stored_objects).map_err(|_| CoreError::LengthOverflow)? {
        return Err(CoreError::LengthMismatch {
            expected: stored_objects,
            actual: u64::try_from(stored_ids.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    let mut stored_canonical_bytes = 0_u64;
    let mut stored_mapping_bytes = 0_u64;
    let mut stored_metrics = Metrics::default();
    for id in stored_ids.iter() {
        let (_decoded_charge, _object, bytes) = store.get(*id, &mut stored_metrics)?;
        let len = u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
        stored_canonical_bytes = stored_canonical_bytes
            .checked_add(len)
            .ok_or(CoreError::LengthOverflow)?;
        if h11_is_mapping(&bytes) {
            stored_mapping_bytes = stored_mapping_bytes
                .checked_add(len)
                .ok_or(CoreError::LengthOverflow)?;
        }
    }
    finish_q(&mut stored_metrics)?;
    h11_observe_product_q(stored_metrics.q_high_water)?;
    drop(stored_ids);
    let mut metrics = Metrics::default();
    let mut current_ids = H11Reachability::new();
    h11_collect_root(store, current.root, &mut current_ids, &mut metrics)?;
    h11_collect_transition(store, current.transition, &mut current_ids, &mut metrics)?;
    let mut retained_ids = H11Reachability::new();
    for row in retained {
        h11_collect_root(store, row.root, &mut retained_ids, &mut metrics)?;
        h11_collect_transition(store, row.transition, &mut retained_ids, &mut metrics)?;
    }
    finish_q(&mut metrics)?;
    h11_observe_product_q(metrics.q_high_water)?;
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
) -> AnyResult<(H11ChargedVec<H11HistoricalTuple>, u64)> {
    let mut selected = H11ChargedVec::with_capacity(H11_HISTORY_POINTS.len() + 3)?;
    for revision in [1, history, history / 2]
        .into_iter()
        .chain(H11_HISTORY_POINTS)
        .filter(|revision| *revision > 0 && *revision <= history)
    {
        if !selected.contains(&revision) {
            selected.push(revision);
        }
    }
    selected.sort_unstable();
    let mut observed = H11ChargedVec::with_capacity(selected.len())?;
    let mut max_q = 0_u64;
    for revision in selected.iter() {
        let row = expected[usize::try_from(revision - 1).map_err(|_| CoreError::LengthOverflow)?];
        let prior = if row.revision > 1 {
            Some(
                expected
                    [usize::try_from(row.revision - 2).map_err(|_| CoreError::LengthOverflow)?],
            )
        } else {
            None
        };
        let mut metrics = Metrics::default();
        let (operations, operations_charge) = if let Some(prior) = prior {
            let (operations, charge) =
                charged_replace_operation(b"file", prior.file, row.file, &mut metrics)?;
            (Some(operations), Some(charge))
        } else {
            (None, None)
        };
        verify_transition(
            store,
            row.transition,
            prior.map(|prior| prior.root),
            row.root,
            operations.as_deref(),
            &mut metrics,
        )?;
        let mut emit = |_bytes: &[u8]| Ok(());
        let output_digest = h11_digest_string(&row.output_digest)?;
        let occurrence_digest = h11_digest_string(&row.occurrence_digest)?;
        let reconstructed = reconstruct_file_to(
            store,
            row.root,
            Some(&output_digest),
            Some(&occurrence_digest),
            true,
            true,
            &mut metrics,
            &mut emit,
        )?;
        if reconstructed.content_closure.is_none() || reconstructed.length != SOURCE_1 {
            return Err(CoreError::PublicationConflict.into());
        }
        drop(operations);
        drop(operations_charge);
        finish_q(&mut metrics)?;
        max_q = max_q.max(metrics.q_high_water);
        h11_observe_product_q(metrics.q_high_water)?;
        observed.push(H11HistoricalTuple {
            revision: row.revision,
            root: row.root,
            transition: row.transition,
            output_digest: row.output_digest,
        });
    }
    drop(selected);
    Ok((observed, max_q))
}

fn h11_fd_count() -> AnyResult<u64> {
    Ok(u64::try_from(fs::read_dir("/dev/fd")?.count()).map_err(|_| CoreError::LengthOverflow)?)
}

fn h11_write_option(output: &mut impl std::fmt::Write, value: Option<u64>) -> std::fmt::Result {
    match value {
        Some(value) => write!(output, "{value}"),
        None => output.write_str("null"),
    }
}

fn h11_write_operation(
    output: &mut impl std::fmt::Write,
    name: &str,
    value: H11Operation,
) -> std::fmt::Result {
    write!(
        output,
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

fn g5_write_checkpoint(
    output: &mut impl std::fmt::Write,
    checkpoint: G5HistoryCheckpoint,
) -> std::fmt::Result {
    write!(
        output,
        "{{\"revision\":{},\"root\":\"{}\",\"length\":{SOURCE_1},\"transition\":\"{}\",\"output_digest\":\"{}\",\"range_bytes\":{G5_HISTORY_RANGE_BYTES},\"range_digest\":\"{}\",\"edit_to_revision\":{},\"next_root\":\"{}\",\"next_transition\":\"{}\",\"operations\":{{",
        checkpoint.revision,
        checkpoint.current_root,
        checkpoint.current_transition,
        ObjectId::from_bytes(&checkpoint.current_output_digest).map_err(|_| std::fmt::Error)?,
        ObjectId::from_bytes(&checkpoint.range_digest).map_err(|_| std::fmt::Error)?,
        checkpoint.next_revision,
        checkpoint.next_root,
        checkpoint.next_transition,
    )?;
    h11_write_operation(output, "range", checkpoint.range)?;
    output.write_char(',')?;
    h11_write_operation(output, "same_size_edit", checkpoint.edit)?;
    write!(
        output,
        "}},\"projection\":{{\"classification\":\"ExactThenLatestSparsePatch\",\"exact_policy\":\"ExactEveryRoot\",\"exact_revision\":{},\"exact_requested_root\":\"{}\",\"exact_result_root\":\"{}\",\"latest_policy\":\"LatestFollowing\",\"latest_revision\":{},\"latest_requested_root\":\"{}\",\"latest_result_root\":\"{}\",\"latest_route\":\"SparsePatchAuthenticatedEdge\",\"submitted\":{},\"started\":{},\"published\":{},\"coalesced\":{},\"clone_calls\":{},\"full_fallbacks\":{},\"range_fetches\":{},\"written_bytes\":{},\"seed_rotations\":{},\"max_buffer_bytes\":{},\"q_high_water\":{},\"q_terminal\":{},\"temp_residue\":{}}},\"storage\":{{\"logical_bytes\":{},\"apparent_bytes\":{},\"allocated_bytes\":{},\"live_objects\":{},\"unreachable_objects\":{}}},\"resource\":{{\"q_high_water\":{},\"fd_count\":{}}}}}",
        checkpoint.revision,
        checkpoint.projection.exact_root,
        checkpoint.projection.exact_root,
        checkpoint.next_revision,
        checkpoint.projection.latest_root,
        checkpoint.projection.latest_root,
        checkpoint.projection.submitted,
        checkpoint.projection.started,
        checkpoint.projection.published,
        checkpoint.projection.coalesced,
        checkpoint.projection.clone_calls,
        checkpoint.projection.full_fallbacks,
        checkpoint.projection.range_fetches,
        checkpoint.projection.written_bytes,
        checkpoint.projection.seed_rotations,
        checkpoint.projection.max_buffer_bytes,
        checkpoint.projection.q_high_water,
        checkpoint.projection.q_current,
        checkpoint.projection.temp_residue,
        checkpoint.logical_store_bytes,
        checkpoint.apparent_store_bytes,
        checkpoint.allocated_store_bytes,
        checkpoint.live_objects,
        checkpoint.unreachable_objects,
        checkpoint.q_high_water,
        checkpoint.fd_count,
    )
}

fn g5_history_range(byte_offset: u64) -> AnyResult<std::ops::Range<u64>> {
    let half = G5_HISTORY_RANGE_BYTES / 2;
    let start = byte_offset.saturating_sub(half);
    let end = start
        .checked_add(G5_HISTORY_RANGE_BYTES)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(start..end)
}

fn g5_revision_source(
    base: &Path,
    target: EditPoint,
    revision: u64,
    output: &Path,
) -> AnyResult<()> {
    if output.exists() {
        return Err(CoreError::PublicationConflict.into());
    }
    fs::copy(base, output)?;
    let mut file = OpenOptions::new().read(true).write(true).open(output)?;
    if revision > 1 {
        file.seek(SeekFrom::Start(target.byte_offset))?;
        file.write_all(&revision.to_be_bytes())?;
    }
    file.sync_all()?;
    Ok(())
}

fn g5_expected_range_digest(
    base: &Path,
    target: EditPoint,
    revision: u64,
    range: std::ops::Range<u64>,
) -> AnyResult<[u8; 32]> {
    let length = usize::try_from(range.end - range.start).map_err(|_| CoreError::LengthOverflow)?;
    let charge = h11_charge(length)?;
    let mut bytes = vec![0_u8; length];
    let mut file = File::open(base)?;
    file.seek(SeekFrom::Start(range.start))?;
    file.read_exact(&mut bytes)?;
    let relative = usize::try_from(
        target
            .byte_offset
            .checked_sub(range.start)
            .ok_or(CoreError::LengthOverflow)?,
    )
    .map_err(|_| CoreError::LengthOverflow)?;
    if relative + 8 > bytes.len() {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(relative + 8).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    if revision > 1 {
        bytes[relative..relative + 8].copy_from_slice(&revision.to_be_bytes());
    }
    let digest = *blake3::hash(&bytes).as_bytes();
    drop(bytes);
    drop(charge);
    Ok(digest)
}

fn g5_checkpoint_start(
    store: &mut Store,
    source: &Path,
    target: EditPoint,
    expected: &[H11Expected],
    revision: u64,
    work_root: &Path,
) -> AnyResult<G5HistoryCheckpointStart> {
    let row = expected[usize::try_from(revision - 1).map_err(|_| CoreError::LengthOverflow)?];
    let mut head_metrics = Metrics::default();
    let head_started = Instant::now();
    let head = store
        .current_head_accounted(&mut head_metrics)?
        .ok_or(CoreError::InvalidValidationReceipt)?;
    let head_wall = head_started.elapsed().as_nanos();
    if head.0 != revision || head.1 != row.root || head.2 != row.transition {
        return Err(CoreError::PublicationConflict.into());
    }
    finish_q(&mut head_metrics)?;
    h11_observe_product_q(head_metrics.q_high_water)?;

    let range = g5_history_range(target.byte_offset)?;
    let mut range_metrics = Metrics::default();
    let range_started = Instant::now();
    let output = read_file_range(
        store,
        row.file,
        SELECTED_PROFILE,
        range.clone(),
        &mut range_metrics,
    )?;
    let range_wall = range_started.elapsed().as_nanos();
    let expected_digest = g5_expected_range_digest(source, target, revision, range)?;
    if *blake3::hash(&output).as_bytes() != expected_digest || output.len() != 4 * 1024 {
        return Err(CoreError::IdentityMismatch.into());
    }
    drop(output);
    finish_q(&mut range_metrics)?;
    h11_observe_product_q(range_metrics.q_high_water)?;

    let mut reconstruction_metrics = Metrics::default();
    let reconstruction_started = Instant::now();
    let output_digest = h11_digest_string(&row.output_digest)?;
    let occurrence_digest = h11_digest_string(&row.occurrence_digest)?;
    let mut emit = |_bytes: &[u8]| Ok(());
    let reconstructed = reconstruct_file_to(
        store,
        row.root,
        Some(&output_digest),
        Some(&occurrence_digest),
        true,
        true,
        &mut reconstruction_metrics,
        &mut emit,
    )?;
    let reconstruction_wall = reconstruction_started.elapsed().as_nanos();
    if reconstructed.length != SOURCE_1 {
        return Err(CoreError::LengthMismatch {
            expected: SOURCE_1,
            actual: reconstructed.length,
        }
        .into());
    }
    drop(output_digest);
    drop(occurrence_digest);
    finish_q(&mut reconstruction_metrics)?;
    h11_observe_product_q(reconstruction_metrics.q_high_water)?;

    let retained = &expected[..usize::try_from(revision).map_err(|_| CoreError::LengthOverflow)?];
    let stats = h11_object_stats(store, retained, row)?;
    let physical = store.physical_snapshot();
    let projection_root = work_root.join(format!("projection-{revision}"));
    let projection = phase4_g3_materialization::history_projection_start(
        store,
        &head,
        row.output_digest,
        row.occurrence_digest,
        &projection_root,
    )?;
    Ok(G5HistoryCheckpointStart {
        revision,
        current_root: row.root,
        current_transition: row.transition,
        current_output_digest: row.output_digest,
        range_digest: expected_digest,
        head: h11_operation(&head_metrics, head_wall),
        range: h11_operation(&range_metrics, range_wall),
        reconstruction: h11_operation(&reconstruction_metrics, reconstruction_wall),
        logical_store_bytes: physical.logical_store().ok_or(CoreError::Io)?,
        apparent_store_bytes: physical.apparent_store().ok_or(CoreError::Io)?,
        allocated_store_bytes: physical.allocated_store().ok_or(CoreError::Io)?,
        live_objects: stats.current_live_objects,
        unreachable_objects: stats.current_unreachable_objects,
        q_high_water: head_metrics
            .q_high_water
            .max(range_metrics.q_high_water)
            .max(reconstruction_metrics.q_high_water),
        fd_count: h11_fd_count()?,
        projection,
    })
}

fn g5_checkpoint_finish(
    start: G5HistoryCheckpointStart,
    store: &mut Store,
    source: &Path,
    target: EditPoint,
    next: H11Expected,
    edit: H11Operation,
    work_root: &Path,
) -> AnyResult<G5HistoryCheckpoint> {
    let parent_source = work_root.join(format!("projection-parent-{}.source", start.revision));
    let target_source = work_root.join(format!("projection-target-{}.source", start.revision));
    g5_revision_source(source, target, start.revision, &parent_source)?;
    g5_revision_source(source, target, next.revision, &target_source)?;
    let head = store.current_head()?.ok_or(CoreError::InvalidValidationReceipt)?;
    if head.0 != next.revision || head.1 != next.root || head.2 != next.transition {
        return Err(CoreError::PublicationConflict.into());
    }
    let projection = phase4_g3_materialization::history_projection_latest(
        start.projection,
        store,
        &head,
        next.output_digest,
        &parent_source,
        &target_source,
        g5_history_range(target.byte_offset)?,
    )?;
    fs::remove_file(parent_source)?;
    fs::remove_file(target_source)?;
    Ok(G5HistoryCheckpoint {
        revision: start.revision,
        current_root: start.current_root,
        current_transition: start.current_transition,
        current_output_digest: start.current_output_digest,
        range_digest: start.range_digest,
        next_revision: next.revision,
        next_root: next.root,
        next_transition: next.transition,
        head: start.head,
        range: start.range,
        reconstruction: start.reconstruction,
        edit,
        projection,
        logical_store_bytes: start.logical_store_bytes,
        apparent_store_bytes: start.apparent_store_bytes,
        allocated_store_bytes: start.allocated_store_bytes,
        live_objects: start.live_objects,
        unreachable_objects: start.unreachable_objects,
        q_high_water: start.q_high_water.max(edit.q_high_water).max(projection.q_high_water),
        fd_count: start.fd_count,
    })
}

fn g5_revert_to_existing_a(
    store: &mut Store,
    a: H11Expected,
    b: H11Expected,
    source: &Path,
    target: EditPoint,
    range: std::ops::Range<u64>,
) -> AnyResult<G5RevertObservation> {
    let before = store.physical_snapshot().logical_store().ok_or(CoreError::Io)?;
    let mut metrics = Metrics::default();
    let started = Instant::now();
    let observed = store.transaction_attempt(&mut metrics, |store, metrics| {
        let (prior_operations, prior_charge) =
            charged_replace_operation(b"file", a.file, b.file, metrics)?;
        let mut witness = establish_same_open_file_witness(
            store,
            SELECTED_PROFILE,
            Some(a.root),
            Some(&prior_operations),
            metrics,
        )?;
        let permit = witness.consume(store, metrics)?;
        drop(prior_operations);
        drop(prior_charge);
        let prior = store
            .current_head_accounted(metrics)?
            .ok_or(CoreError::InvalidValidationReceipt)?;
        if prior.0 != b.revision || prior.1 != b.root || prior.2 != b.transition {
            return Err(CoreError::PublicationConflict.into());
        }
        let (operations, operations_charge) =
            charged_replace_operation(b"file", b.file, a.file, metrics)?;
        let transition =
            publish_transition_with_operations(store, Some(b.root), a.root, &operations, metrics)?;
        let revision = b.revision.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        let reverted = h11_observe_revision(
            store,
            H11RevisionIdentity {
                revision,
                root: a.root,
                transition,
            },
            Some(b.root),
            Some(&operations),
            range.clone(),
            metrics,
        )?;
        if reverted.file != a.file || reverted.output_digest != a.output_digest {
            return Err(CoreError::IdentityMismatch.into());
        }
        qualify_same_middle_changed_spine(
            store,
            EditBaseScope::Verified(permit),
            b.root,
            a.root,
            transition,
            &operations,
            ExpectedEditResult {
                before_file: b.file,
                after_file: a.file,
                root: a.root,
                transition,
                closure: reverted.closure_digest,
            },
            SELECTED_PROFILE,
            metrics,
        )?;
        let authority = store.mint_publication_authority_after_qualification(
            Some(&prior),
            a.root,
            transition,
            metrics,
        )?;
        drop(operations);
        drop(operations_charge);
        store.publish_qualified(authority, metrics)?;
        Ok(reverted)
    })?;
    let wall = started.elapsed().as_nanos();
    finish_q(&mut metrics)?;
    h11_observe_product_q(metrics.q_high_water)?;
    if observed.root != a.root
        || metrics.transactions != 1
        || metrics.commits != 1
        || metrics.q_current != 0
    {
        return Err(CoreError::PublicationConflict.into());
    }
    let mut historical_metrics = Metrics::default();
    let selected = read_file_range(
        store,
        b.file,
        SELECTED_PROFILE,
        range.clone(),
        &mut historical_metrics,
    )?;
    let expected_digest = g5_expected_range_digest(source, target, b.revision, range)?;
    let historical_digest = *blake3::hash(&selected).as_bytes();
    let historical_bytes =
        u64::try_from(selected.len()).map_err(|_| CoreError::LengthOverflow)?;
    drop(selected);
    finish_q(&mut historical_metrics)?;
    h11_observe_product_q(historical_metrics.q_high_water)?;
    if historical_bytes != G5_HISTORY_RANGE_BYTES || historical_digest != expected_digest {
        return Err(CoreError::IdentityMismatch.into());
    }
    let after = store.physical_snapshot().logical_store().ok_or(CoreError::Io)?;
    Ok(G5RevertObservation {
        root_a: a.root,
        root_b: b.root,
        final_root: observed.root,
        transition: observed.transition,
        operation: h11_operation(&metrics, wall),
        logical_before: before,
        logical_after: after,
        historical_root: b.root,
        historical_bytes,
        historical_digest,
        q_high_water: metrics.q_high_water.max(historical_metrics.q_high_water),
        final_expected: observed,
    })
}

fn g5_reader_observation(
    database: &Path,
    prior: H11Expected,
    range: std::ops::Range<u64>,
    release: std::sync::mpsc::Receiver<()>,
    ready: std::sync::mpsc::SyncSender<()>,
) -> Result<(ObjectId, ObjectId, u64), String> {
    let store = Store::open_existing_read_only(database, SELECTED_PROFILE)
        .map_err(|error| format!("reader open: {error:?}"))?;
    let mut before_metrics = Metrics::default();
    let before = store
        .current_head_accounted(&mut before_metrics)
        .map_err(|error| format!("reader prior head: {error:?}"))?
        .ok_or_else(|| "reader prior head absent".to_string())?;
    let selected = read_file_range(
        &store,
        prior.file,
        SELECTED_PROFILE,
        range.clone(),
        &mut before_metrics,
    )
    .map_err(|error| format!("reader prior range: {error:?}"))?;
    if *blake3::hash(&selected).as_bytes() != prior.range_digest {
        return Err("reader prior range digest mismatch".into());
    }
    drop(selected);
    finish_q(&mut before_metrics).map_err(|error| error.to_string())?;
    if before.1 != prior.root {
        return Err("reader prior root mismatch".into());
    }
    ready.send(()).map_err(|_| "reader ready disconnected")?;
    release.recv().map_err(|_| "reader release disconnected")?;
    let mut after_metrics = Metrics::default();
    let after = store
        .current_head_accounted(&mut after_metrics)
        .map_err(|error| format!("reader current head: {error:?}"))?
        .ok_or_else(|| "reader current head absent".to_string())?;
    let selected_after = read_file_range(
        &store,
        prior.file,
        SELECTED_PROFILE,
        range,
        &mut after_metrics,
    )
    .map_err(|error| format!("reader historical range after commit: {error:?}"))?;
    if *blake3::hash(&selected_after).as_bytes() != prior.range_digest {
        return Err("reader historical range changed after commit".into());
    }
    drop(selected_after);
    finish_q(&mut after_metrics).map_err(|error| error.to_string())?;
    Ok((
        before.1,
        after.1,
        before_metrics.q_high_water.max(after_metrics.q_high_water),
    ))
}

fn g5_concurrency_10m(
    work_root: &Path,
    retained_source: &Path,
) -> AnyResult<G5ConcurrencyObservation> {
    let root = work_root.join("concurrency-10m");
    fs::create_dir(&root)?;
    let source = root.join("source.bin");
    if fs::metadata(retained_source)?.len() != SOURCE_10 {
        return Err(CoreError::LengthMismatch {
            expected: SOURCE_10,
            actual: fs::metadata(retained_source)?.len(),
        }
        .into());
    }
    fs::copy(retained_source, &source)?;
    let database = root.join("store.sqlite");
    let mut writer = Store::open(&database, SELECTED_PROFILE)?;
    let target = h11_edit_point(&source)?;
    let range = g5_history_range(target.byte_offset)?;
    let (prior, _) = h11_create_base(&mut writer, &source, range.clone())?;
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
    let (release_one_tx, release_one_rx) = std::sync::mpsc::sync_channel(0);
    let (release_two_tx, release_two_rx) = std::sync::mpsc::sync_channel(0);
    let database_one = database.clone();
    let database_two = database.clone();
    let range_one = range.clone();
    let range_two = range.clone();
    let reader_one = std::thread::spawn(move || {
        g5_reader_observation(
            &database_one,
            prior,
            range_one,
            release_one_rx,
            ready_tx,
        )
    });
    let ready_tx_two = ready_rx;
    let (second_ready_tx, second_ready_rx) = std::sync::mpsc::sync_channel(0);
    let reader_two = std::thread::spawn(move || {
        g5_reader_observation(
            &database_two,
            prior,
            range_two,
            release_two_rx,
            second_ready_tx,
        )
    });
    ready_tx_two.recv().map_err(|_| "reader one ready disconnected")?;
    second_ready_rx
        .recv()
        .map_err(|_| "reader two ready disconnected")?;
    let (next, operation) = h11_replace_revision(
        &mut writer,
        2,
        target,
        prior,
        None,
        None,
        range,
    )?;
    release_one_tx
        .send(())
        .map_err(|_| "reader one release disconnected")?;
    release_two_tx
        .send(())
        .map_err(|_| "reader two release disconnected")?;
    let one = reader_one.join().map_err(|_| "reader one panic")??;
    let two = reader_two.join().map_err(|_| "reader two panic")??;
    h11_observe_product_q(one.2.max(two.2))?;
    if operation.transactions != 1
        || operation.commits != 1
        || operation.q_current != 0
        || one.0 != prior.root
        || two.0 != prior.root
        || one.1 != next.root
        || two.1 != next.root
    {
        return Err(CoreError::PublicationConflict.into());
    }
    drop(writer);
    remove_sqlite_image(&database)?;
    fs::remove_file(source)?;
    fs::remove_dir(root)?;
    Ok(G5ConcurrencyObservation {
        prior_root: prior.root,
        new_root: next.root,
        reader_one_before: one.0,
        reader_one_after: one.1,
        reader_two_before: two.0,
        reader_two_after: two.1,
        writer: operation,
        busy_errors: 0,
        locked_errors: 0,
        q_high_water: operation.q_high_water.max(one.2).max(two.2),
    })
}

fn h11_sample(
    source: &Path,
    manifest: &Path,
    concurrency_source: &Path,
    work_root: &Path,
    history_count: u64,
    phase: &str,
) -> AnyResult<()> {
    h11_reset_q()?;
    let checkpoint_count = match (phase, history_count) {
        ("screen", 10) => 2,
        ("gate", 1_000) => 4,
        _ => return Err(CoreError::InvalidRecord("G5 history integration schedule").into()),
    };
    if work_root.exists() {
        return Err("H11 sample work root must be absent".into());
    }
    fs::create_dir(work_root)?;
    fs::set_permissions(work_root, fs::Permissions::from_mode(0o700))?;
    let database = work_root.join("store.sqlite");
    let materialization = work_root.join("materialization");
    let expected = h11_expected(manifest)?;
    let target = h11_edit_point(source)?;
    let range = h11_range(target.byte_offset)?;
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
    let mut history = H11History::new(
        usize::try_from(history_count.saturating_sub(1))
            .map_err(|_| CoreError::LengthOverflow)?,
    )?;
    let mut checkpoints = H11ChargedVec::with_capacity(checkpoint_count)?;
    let mut pending_checkpoint = Some(g5_checkpoint_start(
        &mut store,
        source,
        target,
        &expected,
        1,
        work_root,
    )?);
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
        history.edit_q_high_water.push(operation.q_high_water);
        history.objects_created = history.objects_created.checked_add(operation.objects_created).ok_or(CoreError::LengthOverflow)?;
        history.objects_reused = history.objects_reused.checked_add(operation.objects_reused).ok_or(CoreError::LengthOverflow)?;
        history.canonical_new_bytes = history.canonical_new_bytes.checked_add(operation.canonical_new_bytes).ok_or(CoreError::LengthOverflow)?;
        history.mapping_rewritten = history.mapping_rewritten.checked_add(operation.mapping_rewritten).ok_or(CoreError::LengthOverflow)?;
        history.transactions = history.transactions.checked_add(operation.transactions).ok_or(CoreError::LengthOverflow)?;
        history.commits = history.commits.checked_add(operation.commits).ok_or(CoreError::LengthOverflow)?;
        history.q_high_water = history.q_high_water.max(operation.q_high_water);
        if pending_checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.revision + 1 == revision)
        {
            checkpoints.push(g5_checkpoint_finish(
                pending_checkpoint.take().ok_or(CoreError::MissingObject)?,
                &mut store,
                source,
                target,
                gate,
                operation,
                work_root,
            )?);
        }
        if matches!(revision, 10 | 100) && revision < history_count {
            pending_checkpoint = Some(g5_checkpoint_start(
                &mut store,
                source,
                target,
                &expected,
                revision,
                work_root,
            )?);
        }
    }
    let history_snapshot = store.physical_snapshot();
    let expected_edit_count = history_count - 1;
    if u64::try_from(history.edit_q_high_water.len()).map_err(|_| CoreError::LengthOverflow)?
        != expected_edit_count
        || history.transactions != expected_edit_count
        || history.commits != expected_edit_count
    {
        return Err(CoreError::PublicationConflict.into());
    }
    drop(store);

    let mut reopen_metrics = Metrics::default();
    let reopen_started = Instant::now();
    let mut store = Store::open(&database, SELECTED_PROFILE)?;
    let open_phases = StoreOpenPhases::default();
    let cache_profile_started = Instant::now();
    store.connection.pragma_update(None, "cache_size", H11_CACHE_PAGES)?;
    let observed_cache = store
        .connection
        .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))?;
    if observed_cache != H11_CACHE_PAGES {
        return Err(CoreError::ProfileMismatch.into());
    }
    let cache_profile_ns = cache_profile_started.elapsed().as_nanos();
    let head_started = Instant::now();
    let head = store.current_head_accounted(&mut reopen_metrics)?.ok_or(CoreError::InvalidValidationReceipt)?;
    let head_lookup_ns = head_started.elapsed().as_nanos();
    let reopen_ns = reopen_started.elapsed().as_nanos();
    let current_before_edit = expected[usize::try_from(history_count - 1).map_err(|_| CoreError::LengthOverflow)?];
    if head.0 != history_count || head.1 != current_before_edit.root || head.2 != current_before_edit.transition {
        return Err(CoreError::PublicationConflict.into());
    }
    finish_q(&mut reopen_metrics)?;
    h11_observe_product_q(reopen_metrics.q_high_water)?;
    let reopen = h11_operation(&reopen_metrics, reopen_ns);

    let mut head_metrics = Metrics::default();
    let head_started = Instant::now();
    let repeated_head = store.current_head_accounted(&mut head_metrics)?.ok_or(CoreError::InvalidValidationReceipt)?;
    let head_ns = head_started.elapsed().as_nanos();
    if repeated_head != head {
        return Err(CoreError::PublicationConflict.into());
    }
    finish_q(&mut head_metrics)?;
    h11_observe_product_q(head_metrics.q_high_water)?;
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
    h11_observe_product_q(range_metrics.q_high_water)?;
    let range_read = h11_operation(&range_metrics, range_ns);

    let mut reconstruction_metrics = Metrics::default();
    let reconstruction_started = Instant::now();
    let mut emit = |_bytes: &[u8]| Ok(());
    let expected_output_digest = h11_digest_string(&current_before_edit.output_digest)?;
    let expected_occurrence_digest = h11_digest_string(&current_before_edit.occurrence_digest)?;
    let reconstructed = reconstruct_file_to(
        &store,
        current_before_edit.root,
        Some(&expected_output_digest),
        Some(&expected_occurrence_digest),
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
    h11_observe_product_q(reconstruction_metrics.q_high_water)?;
    drop(expected_occurrence_digest);
    drop(expected_output_digest);
    let reconstruction = h11_operation(&reconstruction_metrics, reconstruction_ns);

    let checkpoint_1000 = g5_checkpoint_start(
        &mut store,
        source,
        target,
        &expected,
        history_count,
        work_root,
    )?;
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
    checkpoints.push(g5_checkpoint_finish(
        checkpoint_1000,
        &mut store,
        source,
        target,
        after_edit,
        first_edit,
        work_root,
    )?);
    if checkpoints.len() != checkpoint_count {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(checkpoint_count)
                .map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(checkpoints.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
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
    h11_observe_product_q(native.q_high_water)?;
    let revert = g5_revert_to_existing_a(
        &mut store,
        current_before_edit,
        after_edit,
        source,
        target,
        g5_history_range(target.byte_offset)?,
    )?;
    if revert.root_a != revert.final_root
        || revert.root_a == revert.root_b
        || revert.operation.transactions != 1
        || revert.operation.commits != 1
        || revert.operation.q_current != 0
    {
        return Err(CoreError::PublicationConflict.into());
    }
    h11_observe_product_q(revert.q_high_water)?;
    let retained = &expected[..=usize::try_from(history_count).map_err(|_| CoreError::LengthOverflow)?];
    let object_stats = h11_object_stats(&store, retained, revert.final_expected)?;
    let terminal_snapshot = store.physical_snapshot();
    let concurrency = g5_concurrency_10m(work_root, concurrency_source)?;
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

    let final_output_digest = ObjectId::from_bytes(&revert.final_expected.output_digest)?;
    #[allow(unused_macros)]
    macro_rules! write_legacy_report {
        ($output:expr, $whole_q:expr) => {{
            let output = &mut $output;
            write!(
                output,
                "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-history-integration-v1\",\"phase\":\"{phase}\",\"history_revisions\":{history_count},\"terminal_revision\":{},\"checkpoint_semantics\":\"observe_N_then_edit_to_N_plus_1_exact_projection_N_latest_projection_N_plus_1\",\"source_bytes\":{SOURCE_1},\"source_blake3\":\"{source_hash}\",\"concurrency_source_bytes\":{SOURCE_10},\"profile\":\"{profile}\",\"cache_size_pages\":{H11_CACHE_PAGES},\"history_edit_samples_ns\":[",
                history_count + 1,
            )?;
            for (index, wall_ns) in history.edit_ns.iter().enumerate() {
                if index != 0 {
                    output.write_char(',')?;
                }
                write!(output, "{wall_ns}")?;
            }
            write!(
                output,
                "],\"history_objects_created\":{},\"history_objects_reused\":{},\"history_canonical_new_bytes\":{},\"history_mapping_rewritten\":{},\"history_transactions\":{},\"history_commits\":{},\"history_q_high_water\":{},\"history_logical_store_bytes\":",
                history.objects_created,
                history.objects_reused,
                history.canonical_new_bytes,
                history.mapping_rewritten,
                history.transactions,
                history.commits,
                history.q_high_water,
            )?;
            h11_write_option(output, history_snapshot.logical_store())?;
            output.write_str(",\"history_apparent_store_bytes\":")?;
            h11_write_option(output, history_snapshot.apparent_store())?;
            output.write_str(",\"history_allocated_store_bytes\":")?;
            h11_write_option(output, history_snapshot.allocated_store())?;
            write!(
                output,
                ",\"final_root\":\"{}\",\"final_transition\":\"{}\",\"final_output_digest\":\"{final_output_digest}\",\"terminal_reconstruction\":{{\"revision\":{},\"root\":\"{}\",\"output_digest\":\"{}\",\"source\":\"verified_full_native_materialization\",\"bytes\":{SOURCE_1}}},\"historical_tuples\":[",
                revert.final_root,
                revert.transition,
                history_count + 1,
                after_edit.root,
                ObjectId::from_bytes(&native.output_digest)?,
            )?;
            for (index, tuple) in historical_tuples.iter().enumerate() {
                if index != 0 {
                    output.write_char(',')?;
                }
                write!(
                    output,
                    "{{\"revision\":{},\"root\":\"{}\",\"transition\":\"{}\",\"output_digest\":\"{}\"}}",
                    tuple.revision,
                    tuple.root,
                    tuple.transition,
                    ObjectId::from_bytes(&tuple.output_digest)?,
                )?;
            }
            output.write_str("],\"checkpoints\":[")?;
            for (index, checkpoint) in checkpoints.iter().enumerate() {
                if index != 0 {
                    output.write_char(',')?;
                }
                g5_write_checkpoint(output, *checkpoint)?;
            }
            output.write_str("],")?;
            h11_write_operation(output, "reopen_head", reopen)?;
            write!(
                output,
                ",\"reopen_phases\":{{\"preflight_ns\":{},\"sqlite_open_profile_ns\":{},\"cache_profile_ns\":{cache_profile_ns},\"head_lookup_ns\":{head_lookup_ns},\"sum_ns\":{reopen_ns},\"sql_counter_scope\":\"partial-logical; preflight and PRAGMA SQL unavailable\"}},",
                open_phases.preflight_ns,
                open_phases.sqlite_open_and_profile_ns,
            )?;
            h11_write_operation(output, "head_lookup", head_lookup)?;
            output.write_char(',')?;
            h11_write_operation(output, "range_read", range_read)?;
            output.write_char(',')?;
            h11_write_operation(output, "reconstruction", reconstruction)?;
            output.write_char(',')?;
            h11_write_operation(output, "first_edit_after_reopen", first_edit)?;
            write!(
                output,
                ",\"materialization\":{{\"wall_ns\":{},\"verification_ns\":{},\"cleanup_ns\":{},\"user_us\":{},\"system_us\":{},\"voluntary_switches\":{},\"involuntary_switches\":{},\"sql_queries\":{},\"sql_rows\":{},\"row_blob_reads\":{},\"row_blob_writes\":{},\"canonical_authenticated\":{},\"write_calls\":{},\"write_bytes\":{},\"data_sync_calls\":{},\"metadata_sync_calls\":{},\"rename_calls\":{},\"directory_sync_calls\":{},\"temp_files_created\":{},\"temp_files_removed\":{},\"q_high_water\":{},\"q_current\":{},\"max_single_buffer_bytes\":{}}},",
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
                "\"revert_a_b_a\":{{\"root_a\":\"{}\",\"root_b\":\"{}\",\"final_root\":\"{}\",\"final_transition\":\"{}\",\"identity_reused\":true,\"logical_store_bytes_before\":{},\"logical_store_bytes_after\":{},\"historical_read\":{{\"requested_root\":\"{}\",\"bytes\":{},\"digest\":\"{}\"}},\"q_high_water\":{},",
                revert.root_a,
                revert.root_b,
                revert.final_root,
                revert.transition,
                revert.logical_before,
                revert.logical_after,
                revert.historical_root,
                revert.historical_bytes,
                ObjectId::from_bytes(&revert.historical_digest)?,
                revert.q_high_water,
            )?;
            h11_write_operation(output, "operation", revert.operation)?;
            write!(
                output,
                "}},\"concurrency_10m\":{{\"reader_model\":\"two_open_immutable_readers_with_bounded_read_scopes_before_and_after_writer_commit_no_statement_or_blob_scope_across_commit\",\"prior_root\":\"{}\",\"new_root\":\"{}\",\"reader_one_before\":\"{}\",\"reader_one_current_head_after\":\"{}\",\"reader_two_before\":\"{}\",\"reader_two_current_head_after\":\"{}\",\"historical_range_bytes_after_commit\":{G5_HISTORY_RANGE_BYTES},\"busy_errors\":{},\"locked_errors\":{},\"sqlite_error_observation\":\"ObservedNoSqliteErrorReturn\",\"q_high_water\":{},",
                concurrency.prior_root,
                concurrency.new_root,
                concurrency.reader_one_before,
                concurrency.reader_one_after,
                concurrency.reader_two_before,
                concurrency.reader_two_after,
                concurrency.busy_errors,
                concurrency.locked_errors,
                concurrency.q_high_water,
            )?;
            h11_write_operation(output, "writer", concurrency.writer)?;
            output.write_str("},")?;
            write!(
                output,
                "\"stored_objects\":{},\"stored_canonical_bytes\":{},\"stored_mapping_bytes\":{},\"current_live_objects\":{},\"current_live_canonical_bytes\":{},\"current_live_mapping_bytes\":{},\"current_unreachable_objects\":{},\"retained_live_objects\":{},\"retained_live_canonical_bytes\":{},\"retained_live_mapping_bytes\":{},\"retained_unreachable_objects\":{},\"terminal_logical_store_bytes\":",
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
            h11_write_option(output, terminal_snapshot.logical_store())?;
            output.write_str(",\"terminal_apparent_store_bytes\":")?;
            h11_write_option(output, terminal_snapshot.apparent_store())?;
            output.write_str(",\"terminal_allocated_store_bytes\":")?;
            h11_write_option(output, terminal_snapshot.allocated_store())?;
            write!(
                output,
                ",\"q_high_water\":{},\"q_current\":0,\"q_terminal_marker_required\":true,\"reachability_entry_q_bytes\":{H11_REACHABILITY_ENTRY_Q},\"historical_verification_q_high_water\":{historical_q},\"fd_before\":{fd_before},\"fd_after_store_close\":{fd_after_store_close},\"fd_after_cleanup\":{fd_after_cleanup},\"descriptor_leak\":false,\"permit_leak\":false,\"seed_residue\":0,\"temp_residue\":0,\"lock_residue_checked_by_runner\":true,\"physical_io_bytes\":\"Unavailable\",\"continuous_storage_peak\":\"Unavailable\",\"controlled_cold\":\"Unavailable\"}}",
                $whole_q,
            )?;
            Ok::<(), Box<dyn std::error::Error>>(())
        }};
    }

    let max_buffer_bytes = checkpoints
        .iter()
        .map(|checkpoint| checkpoint.projection.max_buffer_bytes)
        .max()
        .unwrap_or(0)
        .max(native.max_single_buffer_bytes);
    let aba_transactions = first_edit
        .transactions
        .checked_add(revert.operation.transactions)
        .ok_or(CoreError::LengthOverflow)?;
    let aba_commits = first_edit
        .commits
        .checked_add(revert.operation.commits)
        .ok_or(CoreError::LengthOverflow)?;
    let aba_q_high_water = first_edit.q_high_water.max(revert.q_high_water);
    if aba_transactions != 2 || aba_commits != 2 {
        return Err(CoreError::PublicationConflict.into());
    }
    let _ = (
        history_snapshot,
        reopen,
        open_phases,
        cache_profile_ns,
        head_lookup_ns,
        reopen_ns,
        head_lookup,
        range_read,
        reconstruction,
    );
    macro_rules! write_report {
        ($output:expr, $whole_q:expr) => {{
            let output = &mut $output;
            write!(
                output,
                "{{\"schema\":\"phase4-g5-history-integration-v1\",\"status\":\"PASS\",\"phase\":\"{phase}\",\"source_bytes\":{SOURCE_1},\"base_publication\":{{\"ordinal\":1,\"root\":\"{}\",\"length\":{SOURCE_1},\"route\":\"InitialBuild\",\"transactions\":{},\"commits\":{},\"q_terminal\":{},\"q_high_water\":{},\"temp_residue\":0}},\"edits\":[",
                base.root,
                base_operation.transactions,
                base_operation.commits,
                base_operation.q_current,
                base_operation.q_high_water,
            )?;
            for (index, q_high_water) in history.edit_q_high_water.iter().enumerate() {
                if index != 0 {
                    output.write_char(',')?;
                }
                let ordinal = u64::try_from(index).map_err(|_| CoreError::LengthOverflow)? + 2;
                let row = expected[index + 1];
                write!(
                    output,
                    "{{\"ordinal\":{ordinal},\"root\":\"{}\",\"length\":{SOURCE_1},\"route\":\"SameSizeHistoryEdit\",\"transactions\":1,\"commits\":1,\"q_terminal\":0,\"q_high_water\":{q_high_water},\"temp_residue\":0}}",
                    row.root,
                )?;
            }
            output.write_str("],\"checkpoints\":[")?;
            for (index, checkpoint) in checkpoints.iter().enumerate() {
                if index != 0 {
                    output.write_char(',')?;
                }
                g5_write_checkpoint(output, *checkpoint)?;
            }
            output.write_str("],\"reconstructions\":[")?;
            for (index, checkpoint) in checkpoints.iter().enumerate() {
                if index != 0 {
                    output.write_char(',')?;
                }
                write!(
                    output,
                    "{{\"revision\":{},\"root\":\"{}\",\"length\":{SOURCE_1},\"output_digest\":\"{}\",\"scope\":\"CompleteCheckpoint\"}}",
                    checkpoint.revision,
                    checkpoint.current_root,
                    ObjectId::from_bytes(&checkpoint.current_output_digest)?,
                )?;
            }
            write!(
                output,
                ",{{\"revision\":{},\"root\":\"{}\",\"length\":{SOURCE_1},\"output_digest\":\"{}\",\"scope\":\"TerminalVerifiedNative\"}}],\"aba\":{{\"root_a\":\"{}\",\"root_b\":\"{}\",\"final_root\":\"{}\",\"final_transition\":\"{}\",\"identity_reused\":true,\"objects_created\":{},\"objects_reused\":{},\"logical_store_bytes_before\":{},\"logical_store_bytes_after\":{},\"a_to_b_transactions\":{},\"a_to_b_commits\":{},\"b_to_a_transactions\":{},\"b_to_a_commits\":{},\"transactions\":{aba_transactions},\"commits\":{aba_commits},\"q_terminal\":0,\"q_high_water\":{aba_q_high_water}}},\"historical_read\":{{\"requested_root\":\"{}\",\"bytes\":{},\"digest\":\"{}\"}},",
                history_count + 1,
                after_edit.root,
                ObjectId::from_bytes(&native.output_digest)?,
                revert.root_a,
                revert.root_b,
                revert.final_root,
                revert.transition,
                revert.operation.objects_created,
                revert.operation.objects_reused,
                revert.logical_before,
                revert.logical_after,
                first_edit.transactions,
                first_edit.commits,
                revert.operation.transactions,
                revert.operation.commits,
                revert.historical_root,
                revert.historical_bytes,
                ObjectId::from_bytes(&revert.historical_digest)?,
            )?;
            write!(
                output,
                "\"concurrency\":{{\"source_bytes\":{SOURCE_10},\"reader_model\":\"OpenImmutableReadersBoundedScopesBeforeAndAfterCommitNoLiveStatementOrBlobAcrossCommit\",\"prior_root\":\"{}\",\"new_root\":\"{}\",\"reader_one_before\":\"{}\",\"reader_one_current_head_after\":\"{}\",\"reader_two_before\":\"{}\",\"reader_two_current_head_after\":\"{}\",\"historical_range_bytes_after_commit\":{G5_HISTORY_RANGE_BYTES},\"writer_transactions\":{},\"writer_commits\":{},\"busy_errors\":{},\"locked_errors\":{},\"sqlite_error_observation\":\"ObservedNoSqliteErrorReturn\",\"q_terminal\":{},\"q_high_water\":{}}},",
                concurrency.prior_root,
                concurrency.new_root,
                concurrency.reader_one_before,
                concurrency.reader_one_after,
                concurrency.reader_two_before,
                concurrency.reader_two_after,
                concurrency.writer.transactions,
                concurrency.writer.commits,
                concurrency.busy_errors,
                concurrency.locked_errors,
                concurrency.writer.q_current,
                concurrency.q_high_water,
            )?;
            write!(
                output,
                "\"terminal\":{{\"revision\":{},\"root\":\"{}\",\"transition\":\"{}\",\"output_digest\":\"{final_output_digest}\",\"reachability\":\"ReadOnlyNoGc\",\"stored_objects\":{},\"current_live_objects\":{},\"current_unreachable_objects\":{},\"retained_live_objects\":{},\"retained_unreachable_objects\":{},\"logical_store_bytes\":",
                history_count + 2,
                revert.final_root,
                revert.transition,
                object_stats.stored_objects,
                object_stats.current_live_objects,
                object_stats.current_unreachable_objects,
                object_stats.retained_live_objects,
                object_stats.retained_unreachable_objects,
            )?;
            h11_write_option(output, terminal_snapshot.logical_store())?;
            output.write_str(",\"apparent_store_bytes\":")?;
            h11_write_option(output, terminal_snapshot.apparent_store())?;
            output.write_str(",\"allocated_store_bytes\":")?;
            h11_write_option(output, terminal_snapshot.allocated_store())?;
            write!(
                output,
                ",\"q_terminal\":0,\"q_high_water\":{},\"fd_before\":{fd_before},\"fd_after_store_close\":{fd_after_store_close},\"fd_after_cleanup\":{fd_after_cleanup},\"descriptor_leak\":false,\"seed_residue\":0,\"temp_residue\":0,\"work_root_residue\":{residue_before_root_remove}}},\"max_buffer_bytes\":{max_buffer_bytes}}}",
                $whole_q,
            )?;
            Ok::<(), Box<dyn std::error::Error>>(())
        }};
    }

    let mut reported_q = h11_q_high_water();
    let report_capacity = loop {
        let mut counter = CountingWriter(0);
        write_report!(counter, reported_q)?;
        let capacity = counter.0;
        let next = reported_q.max(
            h11_q_current()
                .checked_add(u64::try_from(capacity).map_err(|_| CoreError::LengthOverflow)?)
                .ok_or(CoreError::LengthOverflow)?,
        );
        if next == reported_q {
            break capacity;
        }
        reported_q = next;
    };
    let mut output = H11ChargedString::with_capacity(report_capacity)?;
    if h11_q_high_water() != reported_q {
        return Err(CoreError::LengthMismatch {
            expected: reported_q,
            actual: h11_q_high_water(),
        }
        .into());
    }
    write_report!(output.value, reported_q)?;
    if output.value.len() != report_capacity {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(report_capacity).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(output.value.len()).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    println!("{}", output.value);
    drop(output);
    drop(checkpoints);
    drop(history);
    drop(expected);
    if h11_q_current() != 0 {
        return Err(CoreError::LengthMismatch {
            expected: 0,
            actual: h11_q_current(),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod g5_history_integration_tests {
    use super::*;

    struct TestRoot(PathBuf);

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_source() -> (TestRoot, PathBuf) {
        let mut root = env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        root.push(format!("layerfs-g5-history-v1-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).expect("unique test root");
        let source = root.join("source");
        (TestRoot(root), source)
    }

    #[test]
    fn exact_checkpoint_range_and_revision_sources_are_bounded() {
        let (_root, source) = test_source();
        fill_source(&source, SOURCE_1, 0x41).expect("source");
        let point = h11_edit_point(&source).expect("edit point");
        let range = g5_history_range(point.byte_offset).expect("range");
        assert_eq!(range.end - range.start, G5_HISTORY_RANGE_BYTES);
        let revision_one = _root.0.join("revision-one");
        let revision_two = _root.0.join("revision-two");
        g5_revision_source(&source, point, 1, &revision_one).expect("revision one");
        g5_revision_source(&source, point, 2, &revision_two).expect("revision two");
        assert_eq!(source_hash(&source).unwrap(), source_hash(&revision_one).unwrap());
        assert_ne!(source_hash(&source).unwrap(), source_hash(&revision_two).unwrap());
        assert_eq!(fs::metadata(revision_two).unwrap().len(), SOURCE_1);
    }
}

fn h11_arg_count() -> AnyResult<usize> {
    // SAFETY: Darwin owns argc for the lifetime of the process. We only read it.
    let argc = unsafe { *libc::_NSGetArgc() };
    usize::try_from(argc).map_err(|_| CoreError::InvalidRecord("H11 argc").into())
}

fn h11_arg(index: usize) -> AnyResult<&'static str> {
    let argc = h11_arg_count()?;
    if index >= argc {
        return Err(CoreError::InvalidRecord("missing H11 argument").into());
    }
    // SAFETY: Darwin's argv pointers and NUL-terminated bytes are process-owned
    // and stable for the process lifetime. Bounds and nulls are checked here;
    // CStr::to_str validates UTF-8 without allocating.
    let value = unsafe {
        let argv = *libc::_NSGetArgv();
        if argv.is_null() {
            return Err(CoreError::InvalidRecord("H11 argv").into());
        }
        let value = *argv.add(index);
        if value.is_null() {
            return Err(CoreError::InvalidRecord("H11 argv").into());
        }
        CStr::from_ptr(value)
    };
    value
        .to_str()
        .map_err(|_| CoreError::InvalidRecord("H11 UTF-8 argument").into())
}

fn h11_require_arg_count(expected: usize) -> AnyResult<()> {
    let actual = h11_arg_count()?;
    if actual != expected {
        return Err(CoreError::LengthMismatch {
            expected: u64::try_from(expected).map_err(|_| CoreError::LengthOverflow)?,
            actual: u64::try_from(actual).map_err(|_| CoreError::LengthOverflow)?,
        }
        .into());
    }
    Ok(())
}

pub(super) fn h11_main() -> AnyResult<()> {
    match h11_arg(1)? {
        "--g5-history-integration" => {
            h11_require_arg_count(7)?;
            let phase = h11_arg(6)?;
            let history_count = match phase {
                "screen" => 10,
                "gate" => 1_000,
                _ => return Err(CoreError::InvalidRecord("G5 history integration phase").into()),
            };
            h11_sample(
                Path::new(h11_arg(2)?),
                Path::new(h11_arg(3)?),
                Path::new(h11_arg(4)?),
                Path::new(h11_arg(5)?),
                history_count,
                phase,
            )
        }
        _ => Err("usage: --g5-history-integration SOURCE_1M EXPECTED_MANIFEST SOURCE_10M WORK_ROOT {screen|gate}".into()),
    }
}
