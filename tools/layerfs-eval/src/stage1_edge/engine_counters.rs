use crate::legacy_full::{Diagnostics, RootId};
use crate::stage1_fixture::EvalResult;
#[derive(Clone, Debug)]
pub(crate) struct FixtureMaster {
    pub(crate) raw_digest: String,
    pub(crate) root: RootId,
    pub(crate) generation: u64,
    pub(crate) store_id: String,
    pub(crate) profile: String,
    pub(crate) apfs_identity: String,
    pub(crate) fixture_blake3: String,
    pub(crate) preparation_wall_ns: u128,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct EngineDelta {
    pub(crate) transactions_started: u64,
    pub(crate) transactions_committed: u64,
    pub(crate) transactions_rolled_back: u64,
    pub(crate) statements: u64,
    pub(crate) admission_transactions_started: u64,
    pub(crate) admission_transactions_committed: u64,
    pub(crate) admission_transactions_rolled_back: u64,
    pub(crate) admission_statements: u64,
    pub(crate) integrity_transactions_started: u64,
    pub(crate) integrity_transactions_committed: u64,
    pub(crate) integrity_transactions_rolled_back: u64,
    pub(crate) integrity_statements: u64,
    pub(crate) busy_events: u64,
    pub(crate) locked_events: u64,
    pub(crate) objects_validated: u64,
    pub(crate) objects_created: u64,
    pub(crate) objects_reused: u64,
    pub(crate) object_bytes_read: u64,
    pub(crate) object_bytes_written: u64,
    pub(crate) range_bytes_requested: u64,
    pub(crate) range_bytes_returned: u64,
    pub(crate) logical_object_bytes: u64,
    pub(crate) logical_root_bytes: u64,
    pub(crate) logical_delta_bytes: u64,
    pub(crate) retained_union_scrubs: u64,
    pub(crate) root_verifications: u64,
    pub(crate) root_verification_objects: u64,
    pub(crate) root_verification_bytes: u64,
    pub(crate) fetched_rows: u64,
    pub(crate) fetched_row_authentication_passes: u64,
    pub(crate) fetched_row_role_decode_passes: u64,
    pub(crate) new_object_authentication_passes: u64,
    pub(crate) incumbent_authentication_passes: u64,
    pub(crate) payload_batch_queries: u64,
    pub(crate) payload_batch_references: u64,
    pub(crate) payload_batch_maximum: u64,
    pub(crate) put_lookup_statements: u64,
    pub(crate) put_insert_statements: u64,
    pub(crate) created_rows: u64,
    pub(crate) reused_rows: u64,
    pub(crate) publication_transactions_started: u64,
    pub(crate) publication_transactions_rolled_back: u64,
    pub(crate) publication_commits: u64,
    pub(crate) publication_closure_passes: u64,
    pub(crate) namespace_graph_verification_passes: u64,
    pub(crate) scratch_tables: u64,
    pub(crate) scratch_statements: u64,
    pub(crate) scratch_rows: u64,
    pub(crate) scratch_high_water_bytes: u64,
    pub(crate) retained_roots_validated: u64,
}
impl EngineDelta {
    pub(crate) fn between(before: &Diagnostics, after: &Diagnostics) -> EvalResult<Self> {
        macro_rules! delta {
            ($field:ident) => {
                after.$field.checked_sub(before.$field).ok_or_else(|| {
                    format!("engine counter {} moved backward", stringify!($field))
                })?
            };
        }
        Ok(Self {
            transactions_started: delta!(transactions_started),
            transactions_committed: delta!(transactions_committed),
            transactions_rolled_back: delta!(transactions_rolled_back),
            statements: delta!(statements),
            admission_transactions_started: delta!(admission_transactions_started),
            admission_transactions_committed: delta!(admission_transactions_committed),
            admission_transactions_rolled_back: delta!(admission_transactions_rolled_back),
            admission_statements: delta!(admission_statements),
            integrity_transactions_started: delta!(integrity_transactions_started),
            integrity_transactions_committed: delta!(integrity_transactions_committed),
            integrity_transactions_rolled_back: delta!(integrity_transactions_rolled_back),
            integrity_statements: delta!(integrity_statements),
            busy_events: delta!(busy_events),
            locked_events: delta!(locked_events),
            objects_validated: delta!(objects_validated),
            objects_created: delta!(objects_created),
            objects_reused: delta!(objects_reused),
            object_bytes_read: delta!(object_bytes_read),
            object_bytes_written: delta!(object_bytes_written),
            range_bytes_requested: delta!(range_bytes_requested),
            range_bytes_returned: delta!(range_bytes_returned),
            logical_object_bytes: delta!(logical_object_bytes),
            logical_root_bytes: delta!(logical_root_bytes),
            logical_delta_bytes: delta!(logical_delta_bytes),
            retained_union_scrubs: delta!(retained_union_scrubs),
            root_verifications: delta!(root_verifications),
            root_verification_objects: delta!(root_verification_objects),
            root_verification_bytes: delta!(root_verification_bytes),
            fetched_rows: delta!(fetched_rows),
            fetched_row_authentication_passes: delta!(fetched_row_authentication_passes),
            fetched_row_role_decode_passes: delta!(fetched_row_role_decode_passes),
            new_object_authentication_passes: delta!(new_object_authentication_passes),
            incumbent_authentication_passes: delta!(incumbent_authentication_passes),
            payload_batch_queries: delta!(payload_batch_queries),
            payload_batch_references: delta!(payload_batch_references),
            payload_batch_maximum: after
                .payload_batch_maximum
                .max(before.payload_batch_maximum),
            put_lookup_statements: delta!(put_lookup_statements),
            put_insert_statements: delta!(put_insert_statements),
            created_rows: delta!(created_rows),
            reused_rows: delta!(reused_rows),
            publication_transactions_started: delta!(publication_transactions_started),
            publication_transactions_rolled_back: delta!(publication_transactions_rolled_back),
            publication_commits: delta!(publication_commits),
            publication_closure_passes: delta!(publication_closure_passes),
            namespace_graph_verification_passes: delta!(namespace_graph_verification_passes),
            scratch_tables: delta!(scratch_tables),
            scratch_statements: delta!(scratch_statements),
            scratch_rows: delta!(scratch_rows),
            scratch_high_water_bytes: after
                .scratch_high_water_bytes
                .max(before.scratch_high_water_bytes),
            retained_roots_validated: delta!(retained_roots_validated),
        })
    }
    pub(crate) fn verify_common(self) -> EvalResult<()> {
        if self.fetched_row_authentication_passes > self.fetched_rows {
            return Err("fetched authentication passes <= fetched rows".to_owned());
        }
        if self.fetched_rows != self.fetched_row_role_decode_passes {
            return Err("fetched_rows = fetched_row_role_decode_passes".to_owned());
        }
        if self.new_object_authentication_passes
            != self
                .created_rows
                .checked_add(self.reused_rows)
                .ok_or_else(|| "new-object equation overflow".to_owned())?
            || self.new_object_authentication_passes != self.put_lookup_statements
        {
            return Err(
                "new_object_authentication_passes = created_rows + reused_rows = put_lookup_statements"
                    .to_owned(),
            );
        }
        if self.incumbent_authentication_passes != self.reused_rows {
            return Err("incumbent_authentication_passes = reused_rows".to_owned());
        }
        if self.put_insert_statements != self.created_rows
            || self.objects_created != self.created_rows
            || self.objects_reused != self.reused_rows
        {
            return Err("put/created/reused row equations".to_owned());
        }
        let expected_validated = self
            .fetched_row_role_decode_passes
            .checked_add(self.new_object_authentication_passes)
            .and_then(|value| value.checked_add(self.incumbent_authentication_passes))
            .ok_or_else(|| "objects_validated equation overflow".to_owned())?;
        if self.objects_validated != expected_validated {
            return Err(
                "objects_validated = fetched role decode + new auth + incumbent auth".to_owned(),
            );
        }
        if self.payload_batch_maximum > 64 {
            return Err("payload_batch_maximum <= 64".to_owned());
        }
        if self.admission_transactions_started
            != self
                .admission_transactions_committed
                .checked_add(self.admission_transactions_rolled_back)
                .ok_or_else(|| "admission transaction equation overflow".to_owned())?
            || self.integrity_transactions_started
                != self
                    .integrity_transactions_committed
                    .checked_add(self.integrity_transactions_rolled_back)
                    .ok_or_else(|| "integrity transaction equation overflow".to_owned())?
            || self.publication_transactions_started
                != self
                    .publication_commits
                    .checked_add(self.publication_transactions_rolled_back)
                    .ok_or_else(|| "publication transaction equation overflow".to_owned())?
        {
            return Err("admission/integrity/publication transaction closure".to_owned());
        }
        if self.object_bytes_written != self.logical_object_bytes
            || self.range_bytes_requested != self.range_bytes_returned
        {
            return Err("phase storage/range byte equations".to_owned());
        }
        Ok(())
    }
    pub(crate) fn verify_verified(self) -> EvalResult<()> {
        self.verify_common()?;
        if self.fetched_rows != self.fetched_row_authentication_passes {
            return Err("Verified fetched_rows = fetched_row_authentication_passes".to_owned());
        }
        Ok(())
    }
    pub(crate) fn verify_trusted(self) -> EvalResult<()> {
        self.verify_common()
    }
    pub(crate) fn verify_trusted_transition(self) -> EvalResult<()> {
        self.verify_trusted()?;
        self.verify_transition_work()
    }
    pub(crate) fn verify_transition_work(self) -> EvalResult<()> {
        if self.transactions_started != 1
            || self.transactions_committed != 1
            || self.transactions_rolled_back != 0
            || self.publication_transactions_started != 1
            || self.publication_transactions_rolled_back != 0
            || self.publication_commits != 1
        {
            return Err(
                "one writer transaction and one publication COMMIT per transition".to_owned(),
            );
        }
        Ok(())
    }
    pub(crate) fn verify_read_only(self) -> EvalResult<()> {
        self.verify_verified()?;
        self.verify_read_only_work()
    }
    pub(crate) fn verify_trusted_read_only(self) -> EvalResult<()> {
        self.verify_trusted()?;
        if self.fetched_row_authentication_passes != 0 {
            return Err("Trusted read-only fetched_row_authentication_passes = 0".to_owned());
        }
        self.verify_read_only_work()
    }
    pub(crate) fn verify_read_only_work(self) -> EvalResult<()> {
        if self.transactions_started != 0
            || self.transactions_committed != 0
            || self.transactions_rolled_back != 0
            || self.publication_transactions_started != 0
            || self.publication_transactions_rolled_back != 0
            || self.publication_commits != 0
            || self.object_bytes_written != 0
            || self.logical_object_bytes != 0
            || self.logical_root_bytes != 0
            || self.logical_delta_bytes != 0
        {
            return Err("historical read/reconstruction has zero writer work".to_owned());
        }
        Ok(())
    }
    pub(crate) fn combine(mut self, source: Self) -> EvalResult<Self> {
        macro_rules! add {
            ($field:ident) => {
                self.$field = self
                    .$field
                    .checked_add(source.$field)
                    .ok_or_else(|| format!("phase counter {} overflow", stringify!($field)))?;
            };
        }
        add!(transactions_started);
        add!(transactions_committed);
        add!(transactions_rolled_back);
        add!(statements);
        add!(admission_transactions_started);
        add!(admission_transactions_committed);
        add!(admission_transactions_rolled_back);
        add!(admission_statements);
        add!(integrity_transactions_started);
        add!(integrity_transactions_committed);
        add!(integrity_transactions_rolled_back);
        add!(integrity_statements);
        add!(busy_events);
        add!(locked_events);
        add!(objects_validated);
        add!(objects_created);
        add!(objects_reused);
        add!(object_bytes_read);
        add!(object_bytes_written);
        add!(range_bytes_requested);
        add!(range_bytes_returned);
        add!(logical_object_bytes);
        add!(logical_root_bytes);
        add!(logical_delta_bytes);
        add!(retained_union_scrubs);
        add!(root_verifications);
        add!(root_verification_objects);
        add!(root_verification_bytes);
        add!(fetched_rows);
        add!(fetched_row_authentication_passes);
        add!(fetched_row_role_decode_passes);
        add!(new_object_authentication_passes);
        add!(incumbent_authentication_passes);
        add!(payload_batch_queries);
        add!(payload_batch_references);
        self.payload_batch_maximum = self.payload_batch_maximum.max(source.payload_batch_maximum);
        add!(put_lookup_statements);
        add!(put_insert_statements);
        add!(created_rows);
        add!(reused_rows);
        add!(publication_transactions_started);
        add!(publication_transactions_rolled_back);
        add!(publication_commits);
        add!(publication_closure_passes);
        add!(namespace_graph_verification_passes);
        add!(scratch_tables);
        add!(scratch_statements);
        add!(scratch_rows);
        self.scratch_high_water_bytes = self
            .scratch_high_water_bytes
            .max(source.scratch_high_water_bytes);
        add!(retained_roots_validated);
        Ok(self)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PhaseCounterDelta {
    pub(crate) name: &'static str,
    pub(crate) engine: EngineDelta,
    pub(crate) q_before_bytes: u64,
    pub(crate) q_after_bytes: u64,
    pub(crate) q_high_water_bytes: u64,
    pub(crate) active_connections: u64,
    pub(crate) operation_scratch_tables: u64,
    pub(crate) operation_scratch_statements: u64,
    pub(crate) operation_scratch_rows: u64,
    pub(crate) operation_scratch_high_water_bytes: u64,
}
impl PhaseCounterDelta {
    pub(crate) fn between(
        name: &'static str,
        before: &Diagnostics,
        after: &Diagnostics,
    ) -> EvalResult<Self> {
        let engine = EngineDelta::between(before, after)?;
        engine.verify_common()?;
        if before.operation_q_current_bytes != 0
            || after.operation_q_current_bytes != 0
            || after.operation_q_high_water_bytes > 8_388_608
        {
            return Err(format!("{name} phase Q closure"));
        }
        Ok(Self {
            name,
            engine,
            q_before_bytes: before.operation_q_current_bytes,
            q_after_bytes: after.operation_q_current_bytes,
            q_high_water_bytes: after.operation_q_high_water_bytes,
            active_connections: after.active_connections,
            operation_scratch_tables: 0,
            operation_scratch_statements: 0,
            operation_scratch_rows: 0,
            operation_scratch_high_water_bytes: 0,
        })
    }
    pub(crate) fn with_operation_scratch(
        mut self,
        operation: &crate::legacy_full::OperationDiagnostics,
    ) -> Self {
        self.operation_scratch_tables = operation.scratch_tables;
        self.operation_scratch_statements = operation.scratch_statements;
        self.operation_scratch_rows = operation.scratch_rows;
        self.operation_scratch_high_water_bytes = operation.scratch_high_water_bytes;
        self
    }
    pub(crate) fn operation_only(
        name: &'static str,
        operation: &crate::legacy_full::OperationDiagnostics,
        active_connections: u64,
    ) -> Self {
        Self {
            name,
            engine: EngineDelta::default(),
            q_before_bytes: 0,
            q_after_bytes: operation.operation_q_terminal_bytes,
            q_high_water_bytes: operation.operation_q_high_water_bytes,
            active_connections,
            operation_scratch_tables: operation.scratch_tables,
            operation_scratch_statements: operation.scratch_statements,
            operation_scratch_rows: operation.scratch_rows,
            operation_scratch_high_water_bytes: operation.scratch_high_water_bytes,
        }
    }
}
pub(crate) fn verify_phase_partition(
    phases: &[PhaseCounterDelta],
    aggregate: EngineDelta,
) -> EvalResult<()> {
    let combined = phases
        .iter()
        .try_fold(EngineDelta::default(), |total, phase| {
            total.combine(phase.engine)
        })?;
    if combined != aggregate {
        return Err(format!(
            "phase engine deltas do not sum to retained aggregate: phases={combined:?} aggregate={aggregate:?}"
        ));
    }
    Ok(())
}
