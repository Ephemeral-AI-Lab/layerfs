use super::super::contract::EvalResult;
use super::projection::delta;
use crate::legacy_full::{Diagnostics, OperationDiagnostics, ProjectionFacts};

pub(in crate::stage1_materialize) struct Row {
    pub(in crate::stage1_materialize) product_wall_ns: u128,
    pub(in crate::stage1_materialize) row_wall_ns: u128,
    pub(in crate::stage1_materialize) oracle_wall_ns: u128,
    pub(in crate::stage1_materialize) cleanup_wall_ns: u128,
    pub(in crate::stage1_materialize) output_digest: String,
    pub(in crate::stage1_materialize) engine: EngineDelta,
    pub(in crate::stage1_materialize) operation: OperationDiagnostics,
    pub(in crate::stage1_materialize) user_cpu_ns: u64,
    pub(in crate::stage1_materialize) system_cpu_ns: u64,
    pub(in crate::stage1_materialize) rss_peak_bytes: u64,
    pub(in crate::stage1_materialize) rss_current_bytes: u64,
    pub(in crate::stage1_materialize) fd_before: u64,
    pub(in crate::stage1_materialize) fd_after: u64,
    pub(in crate::stage1_materialize) active_connections: u64,
    pub(in crate::stage1_materialize) scratch_connections_current: u64,
    pub(in crate::stage1_materialize) scratch_connections_peak: u64,
    pub(in crate::stage1_materialize) total_connections_current: u64,
    pub(in crate::stage1_materialize) total_connections_peak: u64,
    pub(in crate::stage1_materialize) projection_total: ProjectionFacts,
    pub(in crate::stage1_materialize) fd_terminal: Option<u64>,
    pub(in crate::stage1_materialize) connections_terminal: Option<u64>,
    pub(in crate::stage1_materialize) scratch_connections_terminal: Option<u64>,
    pub(in crate::stage1_materialize) total_connections_terminal: Option<u64>,
    pub(in crate::stage1_materialize) process_fd_baseline: u64,
}

#[allow(clippy::too_many_arguments)]
#[derive(Clone, Copy, Default)]
pub(in crate::stage1_materialize) struct EngineDelta {
    pub(in crate::stage1_materialize) statements: u64,
    pub(in crate::stage1_materialize) integrity_statements: u64,
    pub(in crate::stage1_materialize) busy_events: u64,
    pub(in crate::stage1_materialize) locked_events: u64,
    pub(in crate::stage1_materialize) fetched_rows: u64,
    pub(in crate::stage1_materialize) authentication_passes: u64,
    pub(in crate::stage1_materialize) role_decode_passes: u64,
    pub(in crate::stage1_materialize) object_bytes_read: u64,
    pub(in crate::stage1_materialize) payload_batch_queries: u64,
    pub(in crate::stage1_materialize) payload_batch_references: u64,
    pub(in crate::stage1_materialize) payload_batch_maximum: u64,
    pub(in crate::stage1_materialize) publication_commits: u64,
    pub(in crate::stage1_materialize) publication_statements: u64,
    pub(in crate::stage1_materialize) live_verified_integrity_statements: u64,
    pub(in crate::stage1_materialize) primary_read_statements: u64,
    pub(in crate::stage1_materialize) reconciliation_statements: u64,
    pub(in crate::stage1_materialize) compaction_statements: u64,
    pub(in crate::stage1_materialize) connection_mutex_wait_ns: u64,
    pub(in crate::stage1_materialize) trust_guard_ns: u64,
    pub(in crate::stage1_materialize) nonpayload_query_ns: u64,
    pub(in crate::stage1_materialize) payload_query_ns: u64,
    pub(in crate::stage1_materialize) identity_authentication_ns: u64,
    pub(in crate::stage1_materialize) role_decode_ns: u64,
    pub(in crate::stage1_materialize) payload_callback_inclusive_ns: u64,
    pub(in crate::stage1_materialize) counter_merge_ns: u64,
    pub(in crate::stage1_materialize) store_id_queries: u64,
}

impl EngineDelta {
    pub(in crate::stage1_materialize) fn between(
        before: &Diagnostics,
        after: &Diagnostics,
    ) -> EvalResult<Self> {
        let payload_batch_queries = delta(
            after.payload_batch_queries,
            before.payload_batch_queries,
            "payload_batch_queries",
        )?;
        Ok(Self {
            statements: delta(after.statements, before.statements, "statements")?,
            integrity_statements: delta(
                after.integrity_statements,
                before.integrity_statements,
                "integrity_statements",
            )?,
            busy_events: delta(after.busy_events, before.busy_events, "busy_events")?,
            locked_events: delta(after.locked_events, before.locked_events, "locked_events")?,
            fetched_rows: delta(after.fetched_rows, before.fetched_rows, "fetched_rows")?,
            authentication_passes: delta(
                after.fetched_row_authentication_passes,
                before.fetched_row_authentication_passes,
                "authentication_passes",
            )?,
            role_decode_passes: delta(
                after.fetched_row_role_decode_passes,
                before.fetched_row_role_decode_passes,
                "role_decode_passes",
            )?,
            object_bytes_read: delta(
                after.object_bytes_read,
                before.object_bytes_read,
                "object_bytes_read",
            )?,
            payload_batch_queries,
            payload_batch_references: delta(
                after.payload_batch_references,
                before.payload_batch_references,
                "payload_batch_references",
            )?,
            payload_batch_maximum: if payload_batch_queries == 0 {
                0
            } else {
                after.payload_batch_maximum
            },
            publication_commits: delta(
                after.publication_commits,
                before.publication_commits,
                "publication_commits",
            )?,
            publication_statements: delta(
                after.publication_statements,
                before.publication_statements,
                "publication_statements",
            )?,
            live_verified_integrity_statements: delta(
                after.live_verified_integrity_statements,
                before.live_verified_integrity_statements,
                "live_verified_integrity_statements",
            )?,
            primary_read_statements: delta(
                after.primary_read_statements,
                before.primary_read_statements,
                "primary_read_statements",
            )?,
            reconciliation_statements: delta(
                after.reconciliation_statements,
                before.reconciliation_statements,
                "reconciliation_statements",
            )?,
            compaction_statements: delta(
                after.compaction_statements,
                before.compaction_statements,
                "compaction_statements",
            )?,
            connection_mutex_wait_ns: delta(
                after.connection_mutex_wait_ns,
                before.connection_mutex_wait_ns,
                "connection_mutex_wait_ns",
            )?,
            trust_guard_ns: delta(
                after.trust_guard_ns,
                before.trust_guard_ns,
                "trust_guard_ns",
            )?,
            nonpayload_query_ns: delta(
                after.nonpayload_query_ns,
                before.nonpayload_query_ns,
                "nonpayload_query_ns",
            )?,
            payload_query_ns: delta(
                after.payload_query_ns,
                before.payload_query_ns,
                "payload_query_ns",
            )?,
            identity_authentication_ns: delta(
                after.identity_authentication_ns,
                before.identity_authentication_ns,
                "identity_authentication_ns",
            )?,
            role_decode_ns: delta(
                after.role_decode_ns,
                before.role_decode_ns,
                "role_decode_ns",
            )?,
            payload_callback_inclusive_ns: delta(
                after.payload_callback_inclusive_ns,
                before.payload_callback_inclusive_ns,
                "payload_callback_inclusive_ns",
            )?,
            counter_merge_ns: delta(
                after.counter_merge_ns,
                before.counter_merge_ns,
                "counter_merge_ns",
            )?,
            store_id_queries: delta(
                after.store_id_queries,
                before.store_id_queries,
                "store_id_queries",
            )?,
        })
    }
}
