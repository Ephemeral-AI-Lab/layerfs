//! The compaction report `store.compact` answers (call surface 2.0) and the
//! projection of a compaction's own record a live read shows (T10).
//!
//! One serializer renders the [`CompactReceipt`] both backends return, so the
//! SQLite and PostgreSQL paths cannot disagree on a key: the report is flat,
//! every byte total ends in `_bytes`, a zero field is named by its key in
//! this answer, and `residue_reason` is null unless something remains.
use layerfs_persistence::{CompactPath, CompactReceipt, CompactStatus};
use layerfs_protocol::sdk::SdkValue;

/// The key of this answer a §11 report field (`wire`, as the engine names
/// it) is answered under: what a `zero_reasons` entry names.
fn key_of(wire: &str) -> &str {
    match wire {
        "dead_bytes_bound" | "dead_bytes_refined" => "dead_bytes",
        "logical_discarded" => "discarded_bytes",
        "backend_reusable" => "reusable_bytes",
        "physical_shrink" => "freed_bytes",
        "residue" | "residue.bytes" => "residue_bytes",
        other => other,
    }
}

/// The report `store.compact` answers for Store `store`: `{ freed_bytes,
/// outcome, store, generation_before, generation_after, dead_bytes,
/// discarded_bytes, reusable_bytes, kept_waste, delta_base_references,
/// delta_base_references_count, peak_temp_bytes, residue_bytes,
/// residue_artifacts, residue_reason, estimate, zero_reasons }`.
/// `freed_bytes` is how much the Store's file or database shrank; `outcome`
/// is `compacted` for a rebuild and `unchanged` when the run stayed below
/// its threshold and wrote nothing; `dead_bytes` is the one total the mark
/// found (its bound and its refinement are equal on every path).
#[must_use]
pub fn compact_report(store: &str, report: &CompactReceipt) -> SdkValue {
    let int = |value: u64| SdkValue::Int(i64::try_from(value).unwrap_or(i64::MAX));
    let found = &report.kept_waste.delta_base_references;
    let references = found
        .iter()
        .map(|id| SdkValue::text(layerfs_protocol::sdk::hex(id)))
        .collect();
    let capped = report.kept_waste.delta_base_references_capped;
    let (references, references_count) = layerfs_home::reply::values::shown(references, 0, capped);
    let artifacts = report.residue.artifacts.iter().map(SdkValue::text);
    let mut zero_reasons: Vec<SdkValue> = Vec::new();
    for zero in &report.zero_reasons {
        let entry = SdkValue::map([
            ("field", SdkValue::text(key_of(zero.wire_field()))),
            ("reason", SdkValue::text(zero.reason)),
        ]);
        // the bound and the refinement are one key here: one entry for both
        if !zero_reasons.contains(&entry) {
            zero_reasons.push(entry);
        }
    }
    let outcome = match report.path {
        CompactPath::Rebuild => "compacted",
        CompactPath::None => "unchanged",
    };
    SdkValue::map([
        ("freed_bytes", int(report.physical_shrink)),
        ("outcome", SdkValue::text(outcome)),
        ("store", SdkValue::text(store)),
        ("generation_before", int(report.generation_before)),
        ("generation_after", int(report.generation_after)),
        ("dead_bytes", int(report.dead_bytes_refined)),
        ("discarded_bytes", int(report.logical_discarded)),
        ("reusable_bytes", int(report.backend_reusable)),
        (
            "kept_waste",
            SdkValue::map([
                (
                    "dead_in_retained_packs_bytes",
                    int(report.kept_waste.dead_in_retained_packs_bytes),
                ),
                (
                    "delta_bases_bytes",
                    int(report.kept_waste.delta_bases_bytes),
                ),
                (
                    "inode_blocks_bytes",
                    int(report.kept_waste.inode_blocks_bytes),
                ),
            ]),
        ),
        ("delta_base_references", references),
        ("delta_base_references_count", references_count),
        ("peak_temp_bytes", int(report.peak_temp_bytes)),
        ("residue_bytes", int(report.residue.bytes)),
        ("residue_artifacts", SdkValue::List(artifacts.collect())),
        (
            "residue_reason",
            SdkValue::opt_text(report.residue.reason.as_deref()),
        ),
        ("estimate", SdkValue::Bool(report.estimate)),
        ("zero_reasons", SdkValue::List(zero_reasons)),
    ])
}

/// The projection of a compaction's own record, as a live read takes it:
/// `{id, name, phase, started, uncertain, state, phases}` (§11).
#[must_use]
pub fn compact_status(status: &CompactStatus) -> SdkValue {
    SdkValue::map([
        ("id", SdkValue::text(&status.id)),
        ("name", SdkValue::text(status.name)),
        ("phase", SdkValue::text(&status.phase)),
        ("started", SdkValue::Int(status.started)),
        ("uncertain", SdkValue::Bool(status.uncertain)),
        ("state", SdkValue::text(status.state)),
        (
            "phases",
            SdkValue::List(
                status
                    .phases
                    .iter()
                    .map(|phase| SdkValue::text(*phase))
                    .collect(),
            ),
        ),
    ])
}
