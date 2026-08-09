//! Scalar SeqCDC boundary scanner.

use super::super::engine::{RingSpansV1, ScanStepV1, ScannerV1};
use super::super::{SeqCdcCountersV1, MAXIMUM_CHUNK_BYTES, MINIMUM_CHUNK_BYTES};
use crate::{CoreError, CoreResult};

const SEQUENCE_THRESHOLD: u16 = 5;
const OPPOSING_SLOPE_JUMP_TRIGGER: u16 = 50;
const JUMP_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SeqCdcScannerV1 {
    next_ordinal: usize,
    opposing_slopes: u16,
    current_sequence: u16,
    pub(super) counters: SeqCdcCountersV1,
}

impl SeqCdcScannerV1 {
    pub(super) const fn new() -> Self {
        Self {
            next_ordinal: MINIMUM_CHUNK_BYTES,
            opposing_slopes: 0,
            current_sequence: 0,
            counters: SeqCdcCountersV1 {
                comparisons: 0,
                equal_absorptions: 0,
                opposing_slopes: 0,
                jumps: 0,
                jump_bytes: 0,
            },
        }
    }
}

impl ScannerV1 for SeqCdcScannerV1 {
    fn reset(&mut self) {
        let counters = self.counters;
        *self = Self {
            counters,
            ..Self::new()
        };
    }

    fn scan(&mut self, spans: RingSpansV1<'_>) -> CoreResult<ScanStepV1> {
        let mut step = ScanStepV1::default();
        while self.next_ordinal < MAXIMUM_CHUNK_BYTES && self.next_ordinal < spans.len() {
            let current = spans.byte(self.next_ordinal)?;
            let previous_ordinal = self
                .next_ordinal
                .checked_sub(1)
                .ok_or(CoreError::IntegerOverflow)?;
            let previous = spans.byte(previous_ordinal)?;
            self.next_ordinal = self
                .next_ordinal
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            step.inspected_bytes = step
                .inspected_bytes
                .checked_add(2)
                .ok_or(CoreError::IntegerOverflow)?;
            self.counters.comparisons = self
                .counters
                .comparisons
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;

            if current == previous {
                self.counters.equal_absorptions = self
                    .counters
                    .equal_absorptions
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                continue;
            }

            if current < previous {
                self.opposing_slopes = self
                    .opposing_slopes
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                self.current_sequence = 0;
                self.counters.opposing_slopes = self
                    .counters
                    .opposing_slopes
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
            } else {
                self.current_sequence = self
                    .current_sequence
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
            }

            if self.current_sequence == SEQUENCE_THRESHOLD {
                return Ok(ScanStepV1 {
                    cut: Some(
                        previous_ordinal
                            .checked_add(1)
                            .ok_or(CoreError::IntegerOverflow)?,
                    ),
                    ..step
                });
            }
            if self.opposing_slopes == OPPOSING_SLOPE_JUMP_TRIGGER {
                self.next_ordinal = self
                    .next_ordinal
                    .checked_add(JUMP_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?;
                self.opposing_slopes = 0;
                self.counters.jumps = self
                    .counters
                    .jumps
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                self.counters.jump_bytes = self
                    .counters
                    .jump_bytes
                    .checked_add(u64::try_from(JUMP_BYTES).map_err(|_| CoreError::IntegerOverflow)?)
                    .ok_or(CoreError::IntegerOverflow)?;
            }
        }
        step.cut = (spans.len() == MAXIMUM_CHUNK_BYTES).then_some(MAXIMUM_CHUNK_BYTES);
        Ok(step)
    }

    fn pausable_fill_limit(&self, retained: usize) -> CoreResult<usize> {
        self.next_ordinal
            .checked_add(1)
            .and_then(|needed| needed.checked_sub(retained))
            .ok_or(CoreError::IntegerOverflow)
    }

    fn maximum_pause_lookahead(&self) -> usize {
        1
    }
}
