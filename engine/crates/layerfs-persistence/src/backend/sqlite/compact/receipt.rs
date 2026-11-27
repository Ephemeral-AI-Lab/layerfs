//! §11 receipt assembly from one run’s metrics.

use crate::gc::compact::CompactKeptWaste;
use crate::gc::compact::CompactPath;
use crate::gc::compact::CompactReceipt;
use crate::gc::compact::CompactResidue;
use crate::gc::compact::CompactZeroReason;

/// §11: a zero field always carries one word of the closed vocabulary.
#[allow(clippy::too_many_arguments)]
pub fn receipt(
    path: CompactPath,
    generation_before: u64,
    generation_after: u64,
    dead_bytes: u64,
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
                // A rebuild with dead bytes but a zero file delta is a real
                // measurement, not "no waste": name the missing shrink.
                CompactPath::Rebuild if dead_bytes == 0 => "no-waste",
                CompactPath::Rebuild => "no-shrink",
            },
        });
    }
    if backend_reusable == 0 {
        zero_reasons.push(CompactZeroReason {
            field: "backendReusable",
            reason: match path {
                CompactPath::None => "none-path",
                // A rebuild reclaims the source Store's free pages into the new
                // artifact; a zero names the absent reusable space, never the
                // dead bytes it just collected.
                CompactPath::Rebuild => "no-free-pages",
            },
        });
    }
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
        dead_bytes_refined: dead_bytes,
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
