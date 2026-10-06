//! Provisioned host attachment and per-operation runtime ownership.
mod binding;
mod operation;
mod owner;
mod types;

pub use operation::{OperationFailure, OperationRefusal, OperationSuccess, UpstreamOperation};
pub use owner::Upstream;
pub use types::{
    AttachPhase, AttachRefusal, BindingMismatch, ExpectedBinding, PersistenceBootstrap,
    UpstreamError,
};
