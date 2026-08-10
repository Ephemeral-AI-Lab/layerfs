//! Exact authenticated file-range operation.
//!
//! Range reads remain a distinct root-owned operation kind. The shared
//! extraction coordinator acquires the opaque root grant before inspecting
//! this request and retains it through validation, streaming, cleanup, and
//! return.

use super::extraction::{
    read_c3_file_range_impl_v1, C3ReadBuffersV1, C3ReadOperationErrorV1, C3ReadResultV1,
    C3ReadSinkV1,
};
use crate::cas::{FsCasControlV1, FsCasV1};
use crate::identity::{PhysicalTreeIdV1, PhysicalVersionRecordIdV1};
use crate::limits::OperationCountersV1;

#[allow(clippy::too_many_arguments)]
pub(super) fn read_c3_file_range_v1<S, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    requested_root: PhysicalTreeIdV1,
    path: &[u8],
    offset: u64,
    len: u64,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    buffers: C3ReadBuffersV1<'_>,
    control: &mut C,
) -> Result<C3ReadResultV1, C3ReadOperationErrorV1>
where
    S: C3ReadSinkV1 + ?Sized,
    C: FsCasControlV1 + ?Sized,
{
    read_c3_file_range_impl_v1(
        cas,
        cancellation_key,
        version_record,
        requested_root,
        path,
        offset,
        len,
        sink,
        counters,
        buffers,
        control,
    )
}
