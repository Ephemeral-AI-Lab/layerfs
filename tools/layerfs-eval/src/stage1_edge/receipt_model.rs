use super::artifact::json_escape;
use super::engine_counters::{EngineDelta, PhaseCounterDelta};
use super::limits::INITIAL_BYTES;
use super::locality::ContentCounters;
use super::operation_json::{
    counters_json, native_json, oracle_json, resources_json, storage_json,
};
use super::receipt_json::{
    edit_json, history_probe_json, phase_counter_json, ref_json, sub_edit_json,
};
use super::schedule_model::{EditSpec, ScheduledRow};
use crate::legacy_full::{Diagnostics, RefState};
use crate::stage1_fixture::EvalResult;
#[derive(Clone, Debug)]
pub(crate) struct Phase {
    pub(crate) name: &'static str,
    pub(crate) wall_ns: u128,
}
#[derive(Clone, Debug)]
pub(crate) struct Unavailable {
    pub(crate) field: String,
    pub(crate) availability: &'static str,
    pub(crate) reason: String,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ResourceObservation {
    pub(crate) rss_current_bytes: Option<u64>,
    pub(crate) rss_peak_bytes: u64,
    pub(crate) fd_current: u64,
    pub(crate) active_store_connections: u64,
    pub(crate) child_processes: u64,
    pub(crate) owned_temp_entries: Option<u64>,
    pub(crate) residue_entries: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct OracleReceipt {
    pub(crate) logical_length: u64,
    pub(crate) content_digest: String,
    pub(crate) physical_bytes_exact: Option<bool>,
    pub(crate) canonical_bytes_exact: Option<bool>,
    pub(crate) metadata_exact: Option<bool>,
    pub(crate) historical_roots_exact: Option<bool>,
    pub(crate) route_exact: Option<bool>,
}
impl Default for OracleReceipt {
    fn default() -> Self {
        Self {
            logical_length: INITIAL_BYTES,
            content_digest: String::new(),
            physical_bytes_exact: None,
            canonical_bytes_exact: None,
            metadata_exact: None,
            historical_roots_exact: None,
            route_exact: None,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct SubEditReceipt {
    pub(crate) edit: EditSpec,
    pub(crate) native_wall_ns: u128,
    pub(crate) physical_oracle_wall_ns: u128,
    pub(crate) native_route: String,
    pub(crate) native_bytes_read: u64,
    pub(crate) native_bytes_written: u64,
    pub(crate) native_patch_bytes: u64,
    pub(crate) native_suffix_bytes_shifted: u64,
    pub(crate) native_clone_attempts: u64,
    pub(crate) native_clone_successes: u64,
    pub(crate) native_clone_fallbacks: u64,
    pub(crate) native_full_fallback_files: u64,
    pub(crate) tree_level_before: Option<u8>,
    pub(crate) locality: Option<ContentCounters>,
}
#[derive(Clone, Debug)]
pub(crate) struct HistoryProbeReceipt {
    pub(crate) root_index: usize,
    pub(crate) ordinal: u8,
    pub(crate) start: u64,
    pub(crate) length: u64,
    pub(crate) wall_ns: u128,
    pub(crate) engine: EngineDelta,
    pub(crate) operation: crate::legacy_full::OperationDiagnostics,
}
#[derive(Clone, Debug)]
pub(crate) struct RowReceipt {
    pub(crate) schedule: ScheduledRow,
    pub(crate) status: &'static str,
    pub(crate) before_bytes: u64,
    pub(crate) after_bytes: u64,
    pub(crate) edit: Option<EditSpec>,
    pub(crate) sub_edits: Vec<SubEditReceipt>,
    pub(crate) history_probes: Vec<HistoryProbeReceipt>,
    pub(crate) pre_ref: Option<RefState>,
    pub(crate) post_ref: Option<RefState>,
    pub(crate) native_route: String,
    pub(crate) tree_level_before: Option<u8>,
    pub(crate) phases: Vec<Phase>,
    pub(crate) phase_counters: Vec<PhaseCounterDelta>,
    pub(crate) row_wall_ns: u128,
    pub(crate) row_residual_ns: u128,
    pub(crate) engine: Option<EngineDelta>,
    pub(crate) operation: Option<crate::legacy_full::OperationDiagnostics>,
    pub(crate) storage_before: Option<Diagnostics>,
    pub(crate) storage_after: Option<Diagnostics>,
    pub(crate) resources: ResourceObservation,
    pub(crate) oracle: OracleReceipt,
    pub(crate) unavailable: Vec<Unavailable>,
    pub(crate) error: Option<(String, String, String, String, Option<String>)>,
    pub(crate) custody: Option<String>,
}
impl RowReceipt {
    pub(crate) fn json(&self) -> EvalResult<String> {
        let edit = self
            .edit
            .as_ref()
            .map(edit_json)
            .transpose()?
            .unwrap_or_else(|| "null".to_owned());
        let sub_edits = self
            .sub_edits
            .iter()
            .map(sub_edit_json)
            .collect::<EvalResult<Vec<_>>>()?
            .join(",");
        let history_probes = self
            .history_probes
            .iter()
            .map(history_probe_json)
            .collect::<EvalResult<Vec<_>>>()?
            .join(",");
        let phases = self
            .phases
            .iter()
            .map(|phase| {
                format!(
                    "{{\"name\":\"{}\",\"wall_ns\":{}}}",
                    phase.name, phase.wall_ns
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let phase_counters = self
            .phase_counters
            .iter()
            .map(phase_counter_json)
            .collect::<Vec<_>>()
            .join(",");
        let mut unavailable_values = self.unavailable.clone();
        if self.engine.is_none() && self.operation.is_none() {
            for field in [
                "counters.transactions_started",
                "counters.transactions_committed",
                "counters.transactions_rolled_back",
                "counters.statements",
                "counters.admission_transactions_started",
                "counters.admission_transactions_committed",
                "counters.admission_transactions_rolled_back",
                "counters.admission_statements",
                "counters.integrity_transactions_started",
                "counters.integrity_transactions_committed",
                "counters.integrity_transactions_rolled_back",
                "counters.integrity_statements",
                "counters.busy_events",
                "counters.locked_events",
                "counters.objects_validated",
                "counters.objects_created",
                "counters.objects_reused",
                "counters.object_bytes_read",
                "counters.object_bytes_written",
                "counters.fetched_rows",
                "counters.fetched_row_authentication_passes",
                "counters.fetched_row_role_decode_passes",
                "counters.new_object_authentication_passes",
                "counters.incumbent_authentication_passes",
                "counters.payload_batch_queries",
                "counters.payload_batch_references",
                "counters.payload_batch_maximum",
                "counters.put_lookup_statements",
                "counters.put_insert_statements",
                "counters.created_rows",
                "counters.reused_rows",
                "counters.publication_transactions_started",
                "counters.publication_transactions_rolled_back",
                "counters.publication_commits",
                "counters.publication_closure_passes",
                "counters.namespace_graph_verification_passes",
                "counters.scratch_tables",
                "counters.scratch_statements",
                "counters.scratch_rows",
                "counters.scratch_high_water_bytes",
                "counters.retained_roots_validated",
                "counters.cdc_bytes_scanned",
                "counters.payload_bytes_written",
                "counters.unaffected_payload_reads",
                "counters.unaffected_payload_writes",
                "counters.rope_nodes_read",
                "counters.rope_nodes_emitted",
                "counters.content_directory_nodes_emitted",
                "counters.workspace_materializations",
                "counters.workspace_reuses",
                "counters.rematerializations",
                "counters.descriptor_resets",
                "native.bytes_read",
                "native.bytes_written",
                "native.patch_bytes",
                "native.suffix_bytes_shifted",
                "native.clone_attempts",
                "native.clone_successes",
                "native.clone_fallbacks",
                "native.full_fallback_files",
                "native.files_created",
                "native.files_replaced",
                "native.files_removed",
                "resources.operation_q_current_bytes",
                "resources.operation_q_high_water_bytes",
                "resources.operation_q_terminal_bytes",
            ] {
                unavailable_values.push(Unavailable {
                    field: field.to_owned(),
                    availability: "NotApplicable",
                    reason: "row has no product operation".to_owned(),
                });
            }
        }
        if self.storage_after.is_none() {
            for field in [
                "storage.database_bytes",
                "storage.logical_engine_bytes",
                "storage.database_growth_bytes",
                "storage.canonical_object_bytes_written",
                "storage.physical_to_canonical_amplification",
            ] {
                unavailable_values.push(Unavailable {
                    field: field.to_owned(),
                    availability: "NotApplicable",
                    reason: "row has no Store storage observation".to_owned(),
                });
            }
        } else if self
            .storage_before
            .as_ref()
            .zip(self.storage_after.as_ref())
            .is_some_and(|(before, after)| {
                after.object_bytes_written == before.object_bytes_written
            })
        {
            unavailable_values.push(Unavailable {
                field: "storage.physical_to_canonical_amplification".to_owned(),
                availability: "NotApplicable",
                reason: "row wrote no canonical object bytes".to_owned(),
            });
        }
        for (field, value) in [
            (
                "oracle.physical_bytes_exact",
                self.oracle.physical_bytes_exact,
            ),
            (
                "oracle.canonical_bytes_exact",
                self.oracle.canonical_bytes_exact,
            ),
            ("oracle.metadata_exact", self.oracle.metadata_exact),
            (
                "oracle.historical_roots_exact",
                self.oracle.historical_roots_exact,
            ),
            ("oracle.route_exact", self.oracle.route_exact),
        ] {
            if value.is_none() {
                unavailable_values.push(Unavailable {
                    field: field.to_owned(),
                    availability: "NotApplicable",
                    reason: "oracle is not applicable to this scheduled row".to_owned(),
                });
            }
        }
        if self.tree_level_before.is_none() {
            unavailable_values.push(Unavailable {
                field: "tree_level_before".to_owned(),
                availability: "NotApplicable",
                reason: "row is not one individual canonical content edit".to_owned(),
            });
        }
        if self.resources.rss_current_bytes.is_none() {
            unavailable_values.push(Unavailable {
                field: "resources.rss_current_bytes".to_owned(),
                availability: "Unavailable",
                reason: "per-row observer uses getrusage peak; current RSS is sampled only by decisive external observers".to_owned(),
            });
        }
        if self.operation.is_none() && self.resources.owned_temp_entries.is_none() {
            unavailable_values.push(Unavailable {
                field: "resources.owned_temp_entries".to_owned(),
                availability: "NotApplicable",
                reason: "row has no product operation or terminal owned-residue observation"
                    .to_owned(),
            });
        }
        let unavailable = unavailable_values
            .iter()
            .map(|value| {
                format!(
                    "{{\"field\":\"{}\",\"availability\":\"{}\",\"reason\":\"{}\"}}",
                    json_escape(&value.field),
                    value.availability,
                    json_escape(&value.reason)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let error = self.error.as_ref().map_or_else(
            || "null".to_owned(),
            |(class, message, phase, equation, stderr_sha256)| {
                format!(
                    concat!(
                        "{{\"class\":\"{}\",\"message\":\"{}\",",
                        "\"phase\":\"{}\",\"first_failed_equation\":\"{}\",",
                        "\"stderr_sha256\":{}}}"
                    ),
                    json_escape(class),
                    json_escape(message),
                    json_escape(phase),
                    json_escape(equation),
                    stderr_sha256.as_ref().map_or_else(
                        || "null".to_owned(),
                        |value| format!("\"{}\"", json_escape(value)),
                    ),
                )
            },
        );
        let custody = self
            .custody
            .as_ref()
            .map_or_else(|| "".to_owned(), |value| format!(",\"custody\":{value}"));
        Ok(format!(
            concat!(
                "{{\"schema\":\"layerfs-stage1.1-row-v1\",\"row_index\":{},",
                "\"row_id\":\"{}\",\"row_group\":\"{}\",\"sequence\":{},",
                "\"epoch\":{},\"direction\":\"{}\",\"operation\":\"{}\",",
                "\"size_band\":\"{}\",\"status\":\"{}\",",
                "\"before_bytes\":{},\"after_bytes\":{},\"edit\":{},",
                "\"sub_edits\":[{}],\"history_probes\":[{}],",
                "\"pre_ref\":{},\"post_ref\":{},",
                "\"native_route\":\"{}\",\"tree_level_before\":{},\"phases\":[{}],",
                "\"phase_counters\":[{}],",
                "\"row_wall_ns\":{},\"row_residual_ns\":{},",
                "\"counters\":{},\"native\":{},\"storage\":{},",
                "\"resources\":{},\"oracle\":{},\"unavailable\":[{}],",
                "\"error\":{}{} }}\n"
            ),
            self.schedule.row_index,
            self.schedule.row_id,
            self.schedule.row_group,
            self.schedule.sequence,
            self.schedule.epoch,
            self.schedule.direction,
            self.schedule.operation,
            self.schedule.size_band,
            self.status,
            self.before_bytes,
            self.after_bytes,
            edit,
            sub_edits,
            history_probes,
            ref_json(self.pre_ref.as_ref()),
            ref_json(self.post_ref.as_ref()),
            self.native_route,
            self.tree_level_before
                .map_or_else(|| "null".to_owned(), |value| value.to_string()),
            phases,
            phase_counters,
            self.row_wall_ns,
            self.row_residual_ns,
            counters_json(self.engine, self.operation.as_ref())?,
            native_json(self.operation.as_ref()),
            storage_json(self.storage_before.as_ref(), self.storage_after.as_ref()),
            resources_json(&self.resources, self.operation.as_ref()),
            oracle_json(&self.oracle),
            unavailable,
            error,
            custody,
        ))
    }
}
