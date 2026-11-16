//! The wire rendering of the §11 compaction report and the `store.status`
//! compaction projection (T10).
//!
//! One serializer renders the shared [`CompactReport`], so the SQLite path and
//! T9's PostgreSQL path cannot disagree on a key: every key is snake_case (§10;
//! §11 names are logical), a zero field is named by its wire key, and
//! `residue.reason` appears only when something remains.
use layerfs_bridge::control::SdkValue;
use layerfs_persistence::{CompactReport, CompactStatus};

/// The §11 report as `store.compact` returns it:
/// `{kind: "Compacted", path, generation {before, after}, ...}`.
#[must_use]
pub fn compact_report(report: &CompactReport) -> SdkValue {
    let int = |value: u64| SdkValue::Int(i64::try_from(value).unwrap_or(i64::MAX));
    let references: Vec<SdkValue> = report
        .kept_waste
        .delta_base_references
        .iter()
        .map(|id| SdkValue::text(layerfs_bridge::sdk_values::hex(id)))
        .collect();
    // §11/S7: a capped field carries its cap. A list cut at the cap renders as
    // `{value, cap}`; every other report keeps the plain identity list.
    let references = if report.kept_waste.delta_base_references_capped {
        SdkValue::map([
            ("value", SdkValue::List(references)),
            (
                "cap",
                int(layerfs_persistence::DELTA_BASE_REFERENCES_CAP as u64),
            ),
        ])
    } else {
        SdkValue::List(references)
    };
    let mut residue = vec![
        ("bytes".to_owned(), int(report.residue.bytes)),
        (
            "artifacts".to_owned(),
            SdkValue::List(
                report
                    .residue
                    .artifacts
                    .iter()
                    .map(SdkValue::text)
                    .collect(),
            ),
        ),
    ];
    if let Some(reason) = &report.residue.reason {
        residue.push(("reason".to_owned(), SdkValue::text(reason)));
    }
    let zero_reasons = report
        .zero_reasons
        .iter()
        .map(|zero| {
            SdkValue::map([
                ("field", SdkValue::text(zero.wire_field())),
                ("reason", SdkValue::text(zero.reason)),
            ])
        })
        .collect();
    SdkValue::Map(
        [
            ("kind", SdkValue::text("Compacted")),
            ("path", SdkValue::text(report.path.as_str())),
            (
                "generation",
                SdkValue::map([
                    ("before", int(report.generation.before)),
                    ("after", int(report.generation.after)),
                ]),
            ),
            ("dead_bytes_bound", int(report.dead_bytes_bound)),
            ("dead_bytes_refined", int(report.dead_bytes_refined)),
            ("logical_discarded", int(report.logical_discarded)),
            ("backend_reusable", int(report.backend_reusable)),
            ("physical_shrink", int(report.physical_shrink)),
            (
                "kept_waste",
                SdkValue::map([
                    (
                        "dead_in_retained_packs_bytes",
                        int(report.kept_waste.dead_in_retained_packs_bytes),
                    ),
                    (
                        "delta_bases",
                        SdkValue::map([
                            ("bytes", int(report.kept_waste.delta_bases_bytes)),
                            ("references", references),
                        ]),
                    ),
                    ("inode_blocks_bytes", int(report.kept_waste.inode_blocks_bytes)),
                ]),
            ),
            ("peak_temp_bytes", int(report.peak_temp_bytes)),
            ("residue", SdkValue::Map(residue.into_iter().collect())),
            ("estimate", SdkValue::Bool(report.estimate)),
            ("zero_reasons", SdkValue::List(zero_reasons)),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect(),
    )
}

/// The `store.status` projection of a compaction: `{id, name, phase, started,
/// uncertain, state, phases}`. The caller splits the op fields into its `op`
/// and leaves `state`/`phases` beside them (§11).
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
