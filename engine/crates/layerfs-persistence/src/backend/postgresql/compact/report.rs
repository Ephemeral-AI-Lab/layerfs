//! §11 report for the PostgreSQL rebuild.

use crate::backend::sqlite::compact::CompactKeptWaste;
use crate::backend::sqlite::compact::CompactPath;
use crate::backend::sqlite::compact::CompactReceipt;
use crate::backend::sqlite::compact::CompactResidue;
use crate::backend::sqlite::compact::CompactZeroReason;

#[allow(clippy::too_many_arguments)]
pub(crate) fn report(
    path: CompactPath,
    generation_before: u64,
    generation_after: u64,
    dead_bytes: u64,
    dead_refined: u64,
    logical_discarded: u64,
    backend_reusable: u64,
    physical_shrink: u64,
    kept_waste: CompactKeptWaste,
    peak_temp_bytes: u64,
    residue: CompactResidue,
    total_stored: u64,
    history_rows: u64,
) -> CompactReceipt {
    let mut zero_reasons = Vec::new();
    if dead_bytes == 0 {
        let reason = if total_stored == 0 {
            "empty-store"
        } else {
            "no-waste"
        };
        zero_reasons.push(CompactZeroReason {
            field: "deadBytesBound",
            reason,
        });
        zero_reasons.push(CompactZeroReason {
            field: "deadBytesRefined",
            reason,
        });
    }
    if logical_discarded == 0 {
        let reason = if history_rows == 0 {
            "no-history"
        } else if path == CompactPath::None {
            "none-path"
        } else {
            "no-waste"
        };
        zero_reasons.push(CompactZeroReason {
            field: "logicalDiscarded",
            reason,
        });
    }
    if physical_shrink == 0 {
        zero_reasons.push(CompactZeroReason {
            field: "physicalShrink",
            reason: match path {
                CompactPath::None => "none-path",
                // A resumed `G + 1` run can legitimately measure no relation
                // delta even though it collected dead bytes; that is not
                // "no waste".
                CompactPath::Rebuild if dead_bytes == 0 => "no-waste",
                CompactPath::Rebuild => "no-shrink",
            },
        });
    }
    zero_reasons.push(CompactZeroReason {
        field: "backendReusable",
        reason: "postgresql-in-place",
    });
    if kept_waste.is_empty() {
        zero_reasons.push(CompactZeroReason {
            field: "keptWaste",
            reason: "no-waste",
        });
    }
    if peak_temp_bytes == 0 {
        zero_reasons.push(CompactZeroReason {
            field: "peakTempBytes",
            reason: "none-path",
        });
    }
    CompactReceipt {
        path,
        generation_before,
        generation_after,
        dead_bytes_bound: dead_bytes,
        dead_bytes_refined: dead_refined,
        logical_discarded,
        backend_reusable,
        physical_shrink,
        kept_waste,
        peak_temp_bytes,
        residue,
        estimate: true,
        zero_reasons,
    }
}

/// The wire version of [`encode_kept_waste`].
const KEPT_WASTE_VERSION: u8 = 1;

/// The fixed header of the encoded `keptWaste`: version, three `u64` byte
/// totals, the capped flag and the reference count.
const KEPT_WASTE_HEADER: usize = 1 + 8 + 8 + 8 + 1 + 4;

/// Encodes the §11 `keptWaste` into the `gc_op.kept_waste` bytea so a resumed
/// `G + 1` run reports the mark's accounting instead of a zero placeholder.
///
/// The layout is versioned and fixed: one version byte, the three big-endian
/// `u64` byte totals, the capped flag, a big-endian `u32` reference count and
/// the identity bytes. The caller's references are already capped by
/// [`CompactKeptWaste::from_waste`].
pub(crate) fn encode_kept_waste(waste: &CompactKeptWaste) -> Vec<u8> {
    let mut out = Vec::with_capacity(KEPT_WASTE_HEADER + waste.delta_base_references.len() * 32);
    out.push(KEPT_WASTE_VERSION);
    out.extend_from_slice(&waste.dead_in_retained_packs_bytes.to_be_bytes());
    out.extend_from_slice(&waste.delta_bases_bytes.to_be_bytes());
    out.extend_from_slice(&waste.inode_blocks_bytes.to_be_bytes());
    out.push(u8::from(waste.delta_base_references_capped));
    let count = u32::try_from(waste.delta_base_references.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&count.to_be_bytes());
    for reference in &waste.delta_base_references {
        out.extend_from_slice(reference);
    }
    out
}

/// Decodes [`encode_kept_waste`]; a malformed blob is `Integrity`, never a
/// silently zero report.
pub(crate) fn decode_kept_waste(
    bytes: &[u8],
) -> layerfs_history::error::HistoryResult<CompactKeptWaste> {
    use layerfs_history::error::HistoryError;
    let header = bytes
        .get(..KEPT_WASTE_HEADER)
        .ok_or(HistoryError::Integrity("compaction kept waste"))?;
    if header[0] != KEPT_WASTE_VERSION {
        return Err(HistoryError::Integrity("compaction kept waste version"));
    }
    let u64_at = |start: usize| -> u64 {
        let mut value = [0u8; 8];
        value.copy_from_slice(&header[start..start + 8]);
        u64::from_be_bytes(value)
    };
    let dead_in_retained_packs_bytes = u64_at(1);
    let delta_bases_bytes = u64_at(9);
    let inode_blocks_bytes = u64_at(17);
    let delta_base_references_capped = header[25] != 0;
    let mut count_bytes = [0u8; 4];
    count_bytes.copy_from_slice(&header[26..30]);
    let count = usize::try_from(u32::from_be_bytes(count_bytes))
        .map_err(|_| HistoryError::Integrity("compaction kept waste count"))?;
    let references = bytes
        .get(KEPT_WASTE_HEADER..)
        .ok_or(HistoryError::Integrity("compaction kept waste references"))?;
    if references.len() != count.saturating_mul(32) {
        return Err(HistoryError::Integrity("compaction kept waste references"));
    }
    let delta_base_references = references
        .chunks_exact(32)
        .map(|chunk| {
            let mut id = [0u8; 32];
            id.copy_from_slice(chunk);
            id
        })
        .collect();
    Ok(CompactKeptWaste {
        dead_in_retained_packs_bytes,
        delta_bases_bytes,
        delta_base_references,
        delta_base_references_capped,
        inode_blocks_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::decode_kept_waste;
    use super::encode_kept_waste;
    use super::report;
    use crate::backend::sqlite::compact::CompactKeptWaste;
    use crate::backend::sqlite::compact::CompactPath;
    use crate::backend::sqlite::compact::CompactResidue;

    /// The `gc_op.kept_waste` bytea round-trips the whole §11 part, references
    /// and capped flag included; a corrupt blob is refused, never zeroed.
    #[test]
    fn kept_waste_round_trips_through_the_op_blob() {
        let waste = CompactKeptWaste {
            dead_in_retained_packs_bytes: 5,
            delta_bases_bytes: 7,
            delta_base_references: vec![[9; 32], [10; 32]],
            delta_base_references_capped: true,
            inode_blocks_bytes: 11,
        };
        let bytes = encode_kept_waste(&waste);
        assert_eq!(decode_kept_waste(&bytes).unwrap(), waste);
        let mut broken = bytes;
        broken[0] = 2;
        assert!(decode_kept_waste(&broken).is_err());
    }

    /// A resumed `G + 1` run with dead bytes and no relation delta reports
    /// `no-shrink` rather than the misleading `no-waste`.
    #[test]
    fn a_resumed_rebuild_with_no_relation_delta_explains_no_shrink() {
        let receipt = report(
            CompactPath::Rebuild,
            1,
            2,
            60,
            60,
            46,
            0,
            0,
            CompactKeptWaste::default(),
            57_344,
            CompactResidue::default(),
            1_000,
            1,
        );
        assert!(receipt
            .zero_reasons
            .iter()
            .any(|zero| zero.field == "physicalShrink" && zero.reason == "no-shrink"));
        assert!(receipt
            .zero_reasons
            .iter()
            .any(|zero| zero.field == "backendReusable" && zero.reason == "postgresql-in-place"));
    }
}
