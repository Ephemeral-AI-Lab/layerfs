//! Explicit whole-file Replace operation.

#[cfg(test)]
use super::prepare_file_v1;
use super::{
    ChunkReferenceSpoolV1, ContentBuffersV1, ContentSourceV1, PreparedFileV1, PreparedObjectSinkV1,
};
use crate::cdc::CdcControlV1;
use crate::limits::OperationCountersV1;
#[cfg(feature = "operation-polymorphism")]
use crate::limits::OperationReservationV1;
#[cfg(test)]
use crate::limits::ResourceLedgerV1;
use crate::CoreResult;

/// Explicit Replace entry point. It shares the bounded constructor but is not
/// reachable from Update and records no fallback attempt.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub fn replace_file_v1<S, O, R, C>(
    path: &[u8],
    mode: u16,
    declared_len: u64,
    source: &mut S,
    objects: &mut O,
    references: &mut R,
    buffers: ContentBuffersV1<'_>,
    control: &mut C,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    prepare_file_v1(
        path,
        mode,
        declared_len,
        source,
        objects,
        references,
        buffers,
        control,
        ledger,
        counters,
    )
}

/// Complete-C3 Replace constructor borrowing the already-granted root
/// operation. Keeping this semantic entry distinct prevents Update from
/// redispatching to Replace while sharing the one canonical file encoder.
#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn replace_file_borrowed_v1<S, O, R, C>(
    path: &[u8],
    mode: u16,
    declared_len: u64,
    source: &mut S,
    objects: &mut O,
    references: &mut R,
    buffers: ContentBuffersV1<'_>,
    control: &mut C,
    reservation: &OperationReservationV1<'_>,
    algorithm: crate::cdc::CdcAlgorithmV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<PreparedFileV1>
where
    S: ContentSourceV1 + ?Sized,
    O: PreparedObjectSinkV1 + ?Sized,
    R: ChunkReferenceSpoolV1 + ?Sized,
    C: CdcControlV1 + ?Sized,
{
    super::create_file_borrowed_v1(
        path,
        mode,
        declared_len,
        source,
        objects,
        references,
        buffers,
        control,
        reservation,
        algorithm,
        counters,
    )
}
