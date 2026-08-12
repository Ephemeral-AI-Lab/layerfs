//! One-pass Create, explicit Replace, and bounded Update construction.
//!
//! File-object preparation is owned by `file`; complete root orchestration,
//! read verification, Replace, and Update retain their separate semantic owners.

#[cfg(feature = "operation-polymorphism")]
mod create;
mod file;
mod read;
mod replace;
pub(crate) mod update;

pub use file::*;
pub(crate) use read::{
    stream_verified_file_range_v1, VerifiedFileBytesConsumerV1, VerifiedFileRangePortV1,
    VerifiedFileSegmentV1,
};

#[cfg(all(test, feature = "operation-polymorphism"))]
pub(crate) use crate::lifecycle::{request_create_operation_v1, request_tree_operation_v1};

#[cfg(all(test, feature = "operation-polymorphism"))]
pub(crate) use crate::lifecycle::{run_create_tree_v1, run_create_v1};
#[cfg(all(test, feature = "operation-polymorphism"))]
pub(crate) use crate::lifecycle::{OperationBuffersV1, OperationErrorV1};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use create::{SourceSupplierV1, TreeFileV1};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use replace::replace_file_borrowed_v1;
#[cfg(test)]
pub(crate) use replace::replace_file_v1;
