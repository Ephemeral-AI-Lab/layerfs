//! One-pass Create, explicit Replace, and bounded Update construction.
//!
//! File-object preparation is owned by `file`; complete root orchestration,
//! read verification, Replace, and Update retain their separate semantic owners.

#[cfg(feature = "c3-polymorphism")]
mod create;
mod file;
#[cfg(feature = "c3-polymorphism")]
mod read;
mod replace;
pub(crate) mod update;

pub use file::*;

#[cfg(feature = "c3-polymorphism")]
pub(crate) use read::{
    stream_verified_file_range_v1, VerifiedFileBytesConsumerV1, VerifiedFileRangePortV1,
    VerifiedFileSegmentV1,
};

#[cfg(all(test, feature = "c3-polymorphism"))]
pub(crate) use crate::lifecycle::{
    request_c3_create_qualification_v1, request_c3_tree_operation_v1,
};

#[cfg(all(test, feature = "c3-polymorphism"))]
pub(crate) use crate::lifecycle::{C3OperationBuffersV1, C3OperationErrorV1};
#[cfg(all(test, feature = "c3-polymorphism"))]
pub(crate) use create::{
    run_c3_create_tree_v1, run_c3_create_v1, C3SourceSupplierV1, C3TreeFileV1,
};
#[cfg(feature = "c3-polymorphism")]
pub(crate) use replace::replace_file_c3_borrowed_v1;
#[cfg(test)]
pub(crate) use replace::replace_file_v1;
