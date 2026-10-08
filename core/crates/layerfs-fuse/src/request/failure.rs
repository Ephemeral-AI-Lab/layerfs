//! Original kernel inputs retained even when request service creation fails.
use crate::{
    operations::{MutationInput, ReadDataInput},
    ports::ServiceError,
    RequestDisposition,
};
use layerfs_overlay::NativeMount;
use layerfs_workspace::NativeReadOperation;
use std::fmt;

#[derive(Debug)]
pub enum KernelInput {
    Read {
        protected: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
        data: Option<ReadDataInput>,
    },
    Directory {
        inode: u64,
        handle: u64,
        offset: u64,
    },
    Release {
        inode: u64,
        handle: u64,
        directory: bool,
    },
    Forget {
        inode: u64,
        count: u64,
    },
    /// A mutation that never reached the engine; nothing was published.
    Mutation {
        protected: u64,
        handle: Option<u64>,
        input: MutationInput,
    },
}
#[derive(Debug)]
pub struct RequestFailure {
    pub request: u64,
    pub mount: NativeMount,
    pub input: KernelInput,
    pub reason: ServiceError,
}
impl fmt::Display for RequestFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "native request {} on {:?}, input {:?}: {}",
            self.request, self.mount, self.input, self.reason
        )
    }
}
impl std::error::Error for RequestFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.reason.as_ref())
    }
}
pub(super) fn failed(
    request: u64,
    mount: NativeMount,
    input: KernelInput,
    reason: ServiceError,
) -> RequestDisposition {
    RequestDisposition::Retained(Box::new(RequestFailure {
        request,
        mount,
        input,
        reason,
    }))
}
