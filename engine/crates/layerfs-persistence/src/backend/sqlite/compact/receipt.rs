//! §11 receipt assembly from one run’s metrics.

use super::types::CompactKeptWaste;
use super::types::CompactPath;
use super::types::CompactReceipt;
use super::types::CompactResidue;
use super::types::CompactZeroReason;
use crate::compact_metrics;
use crate::compact_metrics::CompactOutcome;
use crate::compact_metrics::CompactPhase;
use crate::compact_metrics::CompactRefusal;
use crate::compact_metrics::CompactRunObservation;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use std::time::Instant;

/// §11: a zero field always carries one word of the closed vocabulary.
#[allow(clippy::too_many_arguments)]
pub(crate) fn receipt(
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

/// The phase clock and §11 values one measured `compact` run observes.
pub(crate) struct RunMetrics {
    pub(crate) started: Instant,
    pub(crate) phase: Option<(CompactPhase, Instant)>,
    pub(crate) phase_nanos: Vec<(CompactPhase, u64)>,
    pub(crate) generation_before: u64,
    pub(crate) generation_after: u64,
    pub(crate) dead_bytes_bound: u64,
    pub(crate) dead_bytes_refined: u64,
    pub(crate) kept_waste: CompactKeptWaste,
    pub(crate) residue_bytes: u64,
    pub(crate) leftover_settled: bool,
}

impl RunMetrics {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            phase: None,
            phase_nanos: Vec::new(),
            generation_before: 0,
            generation_after: 0,
            dead_bytes_bound: 0,
            dead_bytes_refined: 0,
            kept_waste: CompactKeptWaste::default(),
            residue_bytes: 0,
            leftover_settled: false,
        }
    }

    /// Ends the current phase and starts `phase`; a phase is inclusive wall
    /// time, never an exclusive CPU decomposition.
    pub(crate) fn begin_phase(&mut self, phase: CompactPhase) {
        self.end_phase();
        self.phase = Some((phase, Instant::now()));
    }

    pub(crate) fn end_phase(&mut self) {
        if let Some((phase, started)) = self.phase.take() {
            self.phase_nanos.push((
                phase,
                u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            ));
        }
    }

    /// Records this run once, whatever its result. The outcome and refusal
    /// come from the typed error, so a `Busy`/`Unresolved` refusal is counted
    /// without changing the error the caller receives.
    pub(crate) fn finish(mut self, result: &HistoryResult<CompactReceipt>) {
        self.end_phase();
        let total_nanos = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let (outcome, refusal) = match result {
            Ok(receipt) => {
                self.generation_after = receipt.generation_after;
                self.dead_bytes_bound = receipt.dead_bytes_bound;
                self.dead_bytes_refined = receipt.dead_bytes_refined;
                self.kept_waste = receipt.kept_waste.clone();
                self.residue_bytes = receipt.residue.bytes;
                let outcome = match receipt.path {
                    CompactPath::Rebuild => CompactOutcome::Rebuild,
                    CompactPath::None => CompactOutcome::None,
                };
                (outcome, None)
            }
            Err(error) => {
                if !error.unknown() {
                    self.generation_after = self.generation_before;
                }
                (
                    if error.unknown() {
                        CompactOutcome::Uncertain
                    } else {
                        CompactOutcome::Failed
                    },
                    match error {
                        HistoryError::Busy => Some(CompactRefusal::Busy),
                        HistoryError::Unresolved(_) => Some(CompactRefusal::Unresolved),
                        _ => None,
                    },
                )
            }
        };
        compact_metrics::record_run(&CompactRunObservation {
            outcome,
            generation_before: self.generation_before,
            generation_after: self.generation_after,
            dead_bytes_bound: self.dead_bytes_bound,
            dead_bytes_refined: self.dead_bytes_refined,
            kept_waste: self.kept_waste,
            residue_bytes: self.residue_bytes,
            phase_nanos: self.phase_nanos,
            total_nanos,
            leftover_settled: self.leftover_settled,
            refusal,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::receipt;
    use super::CompactKeptWaste;
    use super::CompactPath;
    use super::CompactResidue;

    /// A rebuild whose source Store had no free pages names that absence; it
    /// does not claim there was no waste (there were 100 dead bytes).
    #[test]
    fn a_rebuild_without_free_pages_explains_no_free_pages() {
        let receipt = receipt(
            CompactPath::Rebuild,
            1,
            2,
            100,
            10,
            0,
            64,
            CompactKeptWaste::default(),
            64,
            CompactResidue::default(),
            1_000,
            2,
        );
        assert!(receipt
            .zero_reasons
            .iter()
            .any(|zero| zero.field == "backendReusable" && zero.reason == "no-free-pages"));
        assert!(
            receipt
                .zero_reasons
                .iter()
                .all(|zero| zero.field != "physicalShrink"),
            "a nonzero shrink carries no zero reason"
        );
    }

    /// A rebuild can collect dead bytes and still measure no file delta; that
    /// is `no-shrink`, never the misleading `no-waste`.
    #[test]
    fn a_rebuild_with_waste_but_no_shrink_explains_no_shrink() {
        let receipt = receipt(
            CompactPath::Rebuild,
            1,
            2,
            100,
            10,
            4_096,
            0,
            CompactKeptWaste::default(),
            64,
            CompactResidue::default(),
            1_000,
            2,
        );
        assert!(receipt
            .zero_reasons
            .iter()
            .any(|zero| zero.field == "physicalShrink" && zero.reason == "no-shrink"));
        assert!(
            receipt
                .zero_reasons
                .iter()
                .all(|zero| !(zero.field == "physicalShrink" && zero.reason == "no-waste")),
            "a zero delta with dead bytes is not no-waste"
        );
    }
}
