//! Exact authenticated file-range planning and execution.
//!
//! A range request is deliberately represented in two phases. The raw input
//! is cheap to carry before root admission, while the validated plan is only
//! created after the read operation owns its slot. This keeps path, length,
//! overflow, digest framing, and range execution policy in the read-range
//! owner without moving the filesystem-backed object traversal into it.

use super::extraction::{
    read_file_range_impl_v1, ReadBuffersV1, ReadOperationErrorV1, ReadResultV1, ReadSinkV1,
};
use crate::cas::{FsCasControlV1, FsCasV1};
use crate::format::ValidatedPath;
use crate::identity::{PhysicalTreeIdV1, PhysicalVersionRecordIdV1};
use crate::limits::OperationCountersV1;
use crate::{CoreError, CoreResult};

const RANGE_DIGEST_DOMAIN: &[u8; 8] = b"L155RNG1";

#[derive(Clone, Copy)]
pub(super) struct ExactRangeRequestV1<'a> {
    path: &'a [u8],
    offset: u64,
    len: u64,
}

impl<'a> ExactRangeRequestV1<'a> {
    pub(super) const fn new(path: &'a [u8], offset: u64, len: u64) -> Self {
        Self { path, offset, len }
    }

    pub(super) fn validate(self) -> CoreResult<ExactRangePlanV1<'a>> {
        let path = ValidatedPath::new(self.path)?;
        if self.len == 0 {
            return Err(CoreError::LogicalLength);
        }
        let end = self
            .offset
            .checked_add(self.len)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(ExactRangePlanV1 {
            path: path.as_bytes(),
            offset: self.offset,
            len: self.len,
            end,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ExactRangePlanV1<'a> {
    path: &'a [u8],
    offset: u64,
    len: u64,
    end: u64,
}

impl<'a> ExactRangePlanV1<'a> {
    pub(super) const fn path(self) -> &'a [u8] {
        self.path
    }

    pub(super) const fn offset(self) -> u64 {
        self.offset
    }

    pub(super) const fn len(self) -> u64 {
        self.len
    }

    pub(super) const fn end(self) -> u64 {
        self.end
    }
}

/// The concrete extraction reader implements this narrow execution port. It
/// keeps directory lookup, object authentication, and payload delivery in the
/// extraction owner while the range module owns the exact-range operation
/// boundary and its plan representation.
pub(super) trait ExactRangeExecutorV1 {
    fn execute_exact_range_v1(
        &mut self,
        root: PhysicalTreeIdV1,
        plan: ExactRangePlanV1<'_>,
        hasher: &mut blake3::Hasher,
    ) -> CoreResult<()>;
}

pub(super) fn execute_exact_range_v1<E: ExactRangeExecutorV1 + ?Sized>(
    executor: &mut E,
    root: PhysicalTreeIdV1,
    plan: ExactRangePlanV1<'_>,
    hasher: &mut blake3::Hasher,
) -> CoreResult<()> {
    executor.execute_exact_range_v1(root, plan, hasher)
}

pub(super) fn begin_exact_range_digest_v1(
    version: PhysicalVersionRecordIdV1,
    root: PhysicalTreeIdV1,
    plan: ExactRangePlanV1<'_>,
) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(RANGE_DIGEST_DOMAIN);
    digest_frame(&mut hasher, 0x01, version.as_bytes());
    digest_frame(&mut hasher, 0x02, root.as_bytes());
    digest_frame(&mut hasher, 0x21, plan.path());
    digest_frame(&mut hasher, 0x22, &plan.offset().to_be_bytes());
    digest_frame(&mut hasher, 0x23, &plan.len().to_be_bytes());
    hasher
}

fn digest_frame(hasher: &mut blake3::Hasher, tag: u8, bytes: &[u8]) {
    hasher.update(&[tag]);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_file_range_v1<S, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    requested_root: PhysicalTreeIdV1,
    path: &[u8],
    offset: u64,
    len: u64,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    buffers: ReadBuffersV1<'_>,
    control: &mut C,
) -> Result<ReadResultV1, ReadOperationErrorV1>
where
    S: ReadSinkV1 + ?Sized,
    C: FsCasControlV1 + ?Sized,
{
    read_file_range_impl_v1(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_range_plan_validates_path_length_and_end_once() {
        let plan = ExactRangeRequestV1::new(b"dir/file", 9, 17)
            .validate()
            .expect("valid exact range");
        assert_eq!(plan.path(), b"dir/file");
        assert_eq!(plan.offset(), 9);
        assert_eq!(plan.len(), 17);
        assert_eq!(plan.end(), 26);
    }

    #[test]
    fn exact_range_plan_rejects_empty_zero_and_overflowing_requests() {
        assert_eq!(
            ExactRangeRequestV1::new(b"file", 0, 0).validate(),
            Err(CoreError::LogicalLength)
        );
        assert_eq!(
            ExactRangeRequestV1::new(b".", 0, 1).validate(),
            Err(CoreError::Path)
        );
        assert_eq!(
            ExactRangeRequestV1::new(b"file", u64::MAX, 1).validate(),
            Err(CoreError::IntegerOverflow)
        );
    }
}
