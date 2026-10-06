//! Explicit host serving-scope completion and original application-owned custody.
mod knowledge;
mod owner;
mod types;

pub use knowledge::{failure_knowledge, FailureKnowledge};
pub use types::{
    CustodyDisposition, ScopeFenceError, ScopeFenceRefusal, ServingCustody, SlotCustody,
};
