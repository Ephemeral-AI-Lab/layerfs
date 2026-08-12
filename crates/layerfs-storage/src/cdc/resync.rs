//! Shared bounded update/resynchronization limits.

use crate::limits::{CounterFieldV1, OperationCountersV1};
use crate::profile::ChunkerSpecV1;
use crate::{CoreError, CoreResult};

use super::BorrowedChunkV1;
use super::MAXIMUM_CHUNK_BYTES;

pub const MAX_UPDATE_ANCHOR_SCAN_BYTES: u64 = 65_536;
pub const MAX_UPDATE_REJOIN_VERIFICATION_BYTES: u64 = MAXIMUM_CHUNK_BYTES as u64;
pub const MAX_UPDATE_RESYNCHRONIZATION_BYTES: u64 =
    MAX_UPDATE_ANCHOR_SCAN_BYTES + MAX_UPDATE_REJOIN_VERIFICATION_BYTES;

/// Binding for one bounded CDC rejoin proof. The address of this value is
/// part of the proof contract, so a proof cannot be moved to another update
/// invocation even when the algorithm and profile bytes match.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RejoinOperationBindingV1 {
    algorithm: [u8; 8],
    chunker_profile: [u8; 32],
}

impl RejoinOperationBindingV1 {
    pub(crate) fn frozen_fast() -> Self {
        Self {
            algorithm: super::FASTCDC_ALGORITHM_TAG_V1,
            chunker_profile: *ChunkerSpecV1::frozen().id().as_bytes(),
        }
    }
}

/// Opaque proof minted only after bounded exact comparison. The evidence is
/// supplied by the semantic caller, while CDC owns proof minting, binding,
/// and one-shot consumption before suffix reuse.
#[derive(Debug)]
pub(crate) struct VerifiedRejoinV1<'operation, E> {
    evidence: E,
    algorithm: [u8; 8],
    chunker_profile: [u8; 32],
    operation: &'operation RejoinOperationBindingV1,
}

impl<'operation, E> VerifiedRejoinV1<'operation, E> {
    fn new(evidence: E, operation: &'operation RejoinOperationBindingV1) -> Self {
        Self {
            evidence,
            algorithm: operation.algorithm,
            chunker_profile: operation.chunker_profile,
            operation,
        }
    }

    pub(crate) fn consume(self, operation: &RejoinOperationBindingV1) -> CoreResult<E> {
        if !core::ptr::eq(self.operation, operation)
            || self.algorithm != operation.algorithm
            || self.chunker_profile != operation.chunker_profile
        {
            return Err(CoreError::RangeResyncFailed);
        }
        Ok(self.evidence)
    }
}

/// Drive the bounded old-suffix scan used to find a verified CDC rejoin.
///
/// The content layer supplies evidence lookup and the authenticated read/CDC
/// step through `process`; this owner retains the single scan window, checked
/// cursor accounting, and stop-at-rejoin law.
pub(crate) fn resynchronize_update_v1<F>(
    base_len: u64,
    start: u64,
    source: &mut [u8],
    mut process: F,
) -> CoreResult<bool>
where
    F: FnMut(u64, u64, &mut [u8]) -> CoreResult<(u64, bool)>,
{
    let mut base_cursor = start;
    let mut resynchronization_bytes = 0_u64;
    while base_cursor < base_len {
        let remaining_window = MAX_UPDATE_ANCHOR_SCAN_BYTES
            .checked_sub(resynchronization_bytes)
            .ok_or(CoreError::RangeResyncFailed)?;
        if remaining_window == 0 {
            return Err(CoreError::RangeResyncFailed);
        }
        let (read_len, rejoined) = process(base_cursor, remaining_window, source)?;
        if read_len == 0 || read_len > remaining_window {
            return Err(CoreError::RangeResyncFailed);
        }
        base_cursor = base_cursor
            .checked_add(read_len)
            .ok_or(CoreError::RangeResyncFailed)?;
        resynchronization_bytes = resynchronization_bytes
            .checked_add(read_len)
            .ok_or(CoreError::RangeResyncFailed)?;
        if rejoined {
            return Ok(false);
        }
    }
    Ok(base_cursor == base_len)
}

/// Authenticate one candidate rejoin with the bounded exact-byte comparison
/// required before structural suffix reuse.
pub(crate) fn verify_rejoin_bytes_v1<'operation, E, F>(
    operation: &'operation RejoinOperationBindingV1,
    evidence: E,
    base_start: u64,
    expected_len: u32,
    candidate: BorrowedChunkV1<'_>,
    counters: &mut OperationCountersV1,
    compare: F,
) -> CoreResult<Option<VerifiedRejoinV1<'operation, E>>>
where
    F: FnOnce(u64, &[u8], &[u8]) -> CoreResult<bool>,
{
    let len = usize::try_from(expected_len).map_err(|_| CoreError::IntegerOverflow)?;
    if candidate.len() != len {
        return Ok(None);
    }
    let len_u64 = u64::try_from(len).map_err(|_| CoreError::IntegerOverflow)?;
    if len_u64 > MAX_UPDATE_REJOIN_VERIFICATION_BYTES {
        return Err(CoreError::RangeResyncFailed);
    }
    let next_resynchronization_bytes = counters
        .update_resynchronization_bytes
        .checked_add(len_u64)
        .ok_or(CoreError::RangeResyncFailed)?;
    if next_resynchronization_bytes > MAX_UPDATE_RESYNCHRONIZATION_BYTES {
        return Err(CoreError::RangeResyncFailed);
    }
    let equal = compare(base_start, candidate.first(), candidate.second())?;
    counters.add(CounterFieldV1::BytesRead, len_u64)?;
    counters.record_update_base_payload(len_u64)?;
    counters.add(CounterFieldV1::UpdateResynchronizationBytes, len_u64)?;
    counters.record_exact_rejoin(len_u64, equal)?;
    Ok(equal.then(|| VerifiedRejoinV1::new(evidence, operation)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resynchronization_stops_at_the_first_verified_rejoin() {
        let mut source = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut calls = 0_u8;
        let reached_base_end =
            resynchronize_update_v1(128, 0, &mut source, |cursor, window, bytes| {
                calls += 1;
                assert_eq!(cursor, 0);
                assert_eq!(window, MAX_UPDATE_ANCHOR_SCAN_BYTES);
                bytes[..4].fill(0xA5);
                Ok((4, true))
            })
            .expect("verified rejoin should terminate the bounded scan");

        assert!(!reached_base_end);
        assert_eq!(calls, 1);
    }

    #[test]
    fn resynchronization_rejects_a_callback_that_exceeds_the_window() {
        let mut source = [0_u8; MAXIMUM_CHUNK_BYTES];
        let error = resynchronize_update_v1(128, 0, &mut source, |_cursor, window, _bytes| {
            Ok((window + 1, false))
        })
        .expect_err("the CDC owner must reject an over-window callback");

        assert_eq!(error, CoreError::RangeResyncFailed);
    }

    #[test]
    fn exact_rejoin_verification_charges_after_the_authenticated_compare() {
        let first = [1_u8, 2, 3];
        let candidate = BorrowedChunkV1 {
            first: &first,
            second: &[],
        };
        let mut counters = OperationCountersV1::default();
        let mut compared = false;
        let operation = RejoinOperationBindingV1::frozen_fast();
        let proof = verify_rejoin_bytes_v1(
            &operation,
            (),
            17,
            3,
            candidate,
            &mut counters,
            |offset, left, right| {
                compared = true;
                assert_eq!(offset, 17);
                assert_eq!(left, &[1, 2, 3]);
                assert!(right.is_empty());
                Ok(true)
            },
        )
        .expect("bounded exact verification should succeed");

        assert!(proof.is_some());
        assert!(compared);
        assert_eq!(counters.bytes_read, 3);
        assert_eq!(counters.update_resynchronization_bytes, 3);
        assert_eq!(counters.exact_rejoin_bytes, 3);
        assert_eq!(counters.rejoin_successes, 1);
        assert_eq!(proof.unwrap().consume(&operation), Ok(()));
    }

    #[test]
    fn exact_rejoin_proof_rejects_a_different_operation_binding() {
        let first = [1_u8, 2, 3];
        let candidate = BorrowedChunkV1 {
            first: &first,
            second: &[],
        };
        let mut counters = OperationCountersV1::default();
        let operation_a = RejoinOperationBindingV1::frozen_fast();
        let operation_b = RejoinOperationBindingV1::frozen_fast();
        let proof = verify_rejoin_bytes_v1(
            &operation_a,
            (),
            17,
            3,
            candidate,
            &mut counters,
            |_offset, left, right| Ok(left == [1, 2, 3] && right.is_empty()),
        )
        .expect("bounded exact verification should succeed")
        .expect("equal bytes should mint a proof");

        assert_eq!(
            proof.consume(&operation_b),
            Err(CoreError::RangeResyncFailed)
        );
    }
}
