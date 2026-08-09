//! Canonical optimized FastCDC boundary scanner.

use super::super::engine::{RingSpansV1, ScanStepV1, ScannerV1};
use super::super::{
    LARGE_MASK, MAXIMUM_CHUNK_BYTES, MINIMUM_CHUNK_BYTES, NORMALIZATION_SHIFT, PROFILE_SEED,
    SHIFTED_LARGE_MASK, SHIFTED_SMALL_MASK, SMALL_MASK, TARGET_CHUNK_BYTES,
};
use super::gear::GEAR_LS;
use crate::profile::GEAR;
use crate::{CoreError, CoreResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FastCdcScannerV1 {
    hash: u64,
    next_even: usize,
}

impl FastCdcScannerV1 {
    pub(super) const fn new() -> Self {
        Self {
            hash: PROFILE_SEED,
            next_even: MINIMUM_CHUNK_BYTES,
        }
    }
}

impl ScannerV1 for FastCdcScannerV1 {
    fn reset(&mut self) {
        *self = Self::new();
    }

    fn scan(&mut self, spans: RingSpansV1<'_>) -> CoreResult<ScanStepV1> {
        let mut inspected = 0_u64;
        while self.next_even < MAXIMUM_CHUNK_BYTES
            && self
                .next_even
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?
                < spans.len()
        {
            let even = spans.byte(self.next_even)?;
            let odd_ordinal = self
                .next_even
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            let odd = spans.byte(odd_ordinal)?;
            inspected = inspected.checked_add(2).ok_or(CoreError::IntegerOverflow)?;
            self.hash = self
                .hash
                .wrapping_shl(NORMALIZATION_SHIFT)
                .wrapping_add(GEAR_LS[usize::from(even)]);
            let shifted_mask = if self.next_even < TARGET_CHUNK_BYTES {
                SHIFTED_SMALL_MASK
            } else {
                SHIFTED_LARGE_MASK
            };
            if self.hash & shifted_mask == 0 {
                return Ok(ScanStepV1 {
                    cut: Some(self.next_even),
                    inspected_bytes: inspected,
                });
            }
            self.hash = self.hash.wrapping_add(GEAR[usize::from(odd)]);
            let mask = if self.next_even < TARGET_CHUNK_BYTES {
                SMALL_MASK
            } else {
                LARGE_MASK
            };
            if self.hash & mask == 0 {
                return Ok(ScanStepV1 {
                    cut: Some(odd_ordinal),
                    inspected_bytes: inspected,
                });
            }
            self.next_even = self
                .next_even
                .checked_add(2)
                .ok_or(CoreError::IntegerOverflow)?;
        }
        Ok(ScanStepV1 {
            cut: (spans.len() == MAXIMUM_CHUNK_BYTES).then_some(MAXIMUM_CHUNK_BYTES),
            inspected_bytes: inspected,
        })
    }

    fn pausable_fill_limit(&self, retained: usize) -> CoreResult<usize> {
        self.next_even
            .checked_add(2)
            .and_then(|needed| needed.checked_sub(retained))
            .ok_or(CoreError::IntegerOverflow)
    }

    fn maximum_pause_lookahead(&self) -> usize {
        2
    }
}
