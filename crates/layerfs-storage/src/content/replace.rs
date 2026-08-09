//! Explicit whole-file Replace operation.

use super::{
    prepare_file_v1, ChunkReferenceSpoolV1, ContentBuffersV1, ContentSourceV1, PreparedFileV1,
    PreparedObjectSinkV1,
};
use crate::cdc::CdcControlV1;
use crate::limits::{OperationCountersV1, ResourceLedgerV1};
use crate::CoreResult;

/// Explicit Replace entry point. It shares the bounded constructor but is not
/// reachable from Update and records no fallback attempt.
#[allow(clippy::too_many_arguments)]
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
