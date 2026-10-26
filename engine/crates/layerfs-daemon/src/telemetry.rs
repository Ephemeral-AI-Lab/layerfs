//! The daemon's half of a traced reply: Commit storage and construction
//! receipts and kernel opcode counts. Phase timing and SQL work are the Store
//! service's ([`layerfs_store_service::telemetry`]).
use crate::control::{Failure, Success};
use layerfs_bridge::control::SdkValue;
pub(crate) use layerfs_store_service::telemetry::{delta, sql, sql_values};
use layerfs_store_service::telemetry::{named, timing};
pub use layerfs_store_service::telemetry::{phase, recording, timed, Recording};
use layerfs_telemetry::timer::TimingReport;

/// The telemetry of one traced request: its timing, its Store SQL work, the
/// Workspace's kernel opcode counts when given, the Commit's storage
/// diagnostics and construction receipt when it committed, and the failed
/// diagnostic write, if any.
pub(crate) fn value(
    reports: Vec<TimingReport>,
    outcome: &Result<Success, Failure>,
    diagnostic: Option<&std::io::Error>,
    [sql, delta, fuse]: [Option<SdkValue>; 3],
) -> SdkValue {
    let mut fields = vec![("timing".to_owned(), timing(&reports))];
    for (name, value) in [("sql", sql), ("delta", delta), ("fuse", fuse)] {
        fields.extend(value.map(|value| (name.to_owned(), value)));
    }
    let (namespace, storage) = crate::application::commit_parts(outcome);
    if let Some(storage) = storage {
        fields.push((
            "storage".to_owned(),
            named(
                &STORAGE_FIELDS,
                &crate::application::storage_values(storage),
            ),
        ));
    }
    if let Some(work) = namespace.and_then(|namespace| namespace.work) {
        fields.push((
            "construction".to_owned(),
            named(
                &NAMESPACE_FIELDS,
                &crate::application::namespace_values(&work),
            ),
        ));
    }
    if let Some(error) = diagnostic {
        fields.push(("diagnosticError".to_owned(), error_value(error)));
    }
    SdkValue::Map(fields)
}

/// The Workspace's kernel opcode counts, frame dispositions, largest request
/// sizes and per-opcode callback nanoseconds (`ns`) since attach.
#[cfg(target_os = "linux")]
pub(crate) fn fuse(w: &layerfs_fuse::request::OpcodeWork) -> SdkValue {
    use layerfs_fuse::request::Opcode;
    let extra = [
        ("served", w.served),
        ("inline", w.inline),
        ("refused", w.refused),
        ("terminal", w.terminal),
        ("forget_units", w.forget_units),
        ("store_units", w.store_units),
        ("largest_read", w.largest_read),
        ("largest_write", w.largest_write),
        ("lookup_missing", w.lookup_missing),
        ("served_ns", w.served_ns),
    ];
    let int = |(name, value): (&str, u64)| (name.to_owned(), SdkValue::Int(value as i64));
    // Callback wall time per opcode (#352 F-032), only for opcodes that ran.
    let nanos = Opcode::NAMES
        .iter()
        .copied()
        .zip(w.nanos)
        .filter(|(_, value)| *value > 0)
        .map(int)
        .collect();
    let mut fields: Vec<_> = Opcode::NAMES
        .iter()
        .copied()
        .zip(w.opcodes)
        .chain(extra)
        .map(int)
        .collect();
    fields.push(("ns".to_owned(), SdkValue::Map(nanos)));
    // Steps inside the callbacks: [count, ns] for the steps that ran.
    use layerfs_fuse::request::Phase;
    let phases = Phase::NAMES
        .iter()
        .zip(w.phase_counts.iter().zip(w.phase_nanos))
        .filter(|(_, (count, _))| **count > 0)
        .map(|(name, (count, nanos))| {
            (
                (*name).to_owned(),
                SdkValue::List(vec![
                    SdkValue::Int(*count as i64),
                    SdkValue::Int(nanos as i64),
                ]),
            )
        })
        .collect();
    fields.push(("phases".to_owned(), SdkValue::Map(phases)));
    SdkValue::Map(fields)
}

/// Telemetry that did not fit the reply frame beside the reply.
pub(crate) fn oversized() -> SdkValue {
    SdkValue::map([(
        "diagnosticError",
        SdkValue::map([
            ("kind", SdkValue::text("Oversized")),
            (
                "message",
                SdkValue::text("telemetry does not fit the reply frame"),
            ),
        ]),
    )])
}

fn error_value(error: &std::io::Error) -> SdkValue {
    SdkValue::map([
        ("kind", SdkValue::text(format!("{:?}", error.kind()))),
        ("message", SdkValue::text(error.to_string())),
    ])
}

/// The Commit storage diagnostics, in observation section 16 order.
pub(crate) const STORAGE_FIELDS: [&str; 42] = [
    "policy",
    "locate",
    "read_packs",
    "read_pack_selections",
    "whole_selected",
    "whole_due_density",
    "whole_due_singleton",
    "whole_due_small",
    "whole_due_reuse",
    "range_selected",
    "range_scan_bytes",
    "range_acquired_bytes",
    "range_materialized_bytes",
    "value_groups",
    "signatures",
    "reserve",
    "initial_reservations",
    "reservation_refills",
    "publish",
    "payload_reads",
    "payload_read_bytes",
    "pack_read_bytes",
    "pack_write_bytes",
    "pooled_packs",
    "pooled_groups",
    "reserved_directory_bytes",
    "ordinal_reservations",
    "locator_hits",
    "locator_misses",
    "locator_evictions",
    "locator_eviction_probes",
    "locator_second_chances",
    "locator_bookkeeping_live_peak_bytes",
    "pack_hits",
    "pack_misses",
    "pack_evictions",
    "pack_evicted_bytes",
    "directory_validations",
    "directory_unshared_walks",
    "directory_entry_bounds",
    "prefetch_group_decodes",
    "forced_seals",
];

/// The Commit captured namespace work, in observation section 15 order.
pub(crate) const NAMESPACE_FIELDS: [&str; 47] = [
    "inode_rows",
    "entry_rows",
    "inode_pages",
    "entry_pages",
    "change_pages",
    "largest_page",
    "parent_points",
    "base_lookups",
    "headers_written",
    "headers_dropped",
    "values_written",
    "fresh_serials",
    "tombstones_skipped",
    "files_constructed",
    "files_unchanged",
    "symlinks",
    "metadata_built",
    "metadata_patched",
    "record_jobs",
    "header_opens",
    "header_rows",
    "header_points",
    "change_opens",
    "change_rows",
    "change_points",
    "value_opens",
    "value_rows",
    "value_points",
    "fresh_opens",
    "fresh_rows",
    "fresh_points",
    "window_jobs",
    "normalize_ns",
    "values_ns",
    "update_ns",
    "inode_page_ns",
    "file_ns",
    "file_bind_ns",
    "file_source_ns",
    "values_emit_ns",
    "values_record_ns",
    "update_record_ns",
    "update_capture_ns",
    "update_emit_ns",
    "backing_calls",
    "backing_keys",
    "backing_ns",
];
