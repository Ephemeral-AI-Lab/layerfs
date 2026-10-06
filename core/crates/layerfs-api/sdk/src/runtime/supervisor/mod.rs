//! Host-owned fair composition of provider service and independent native I/O.
mod attachment;
mod drive;
mod fences;
mod owner;
mod types;

pub use owner::Supervisor;
pub use types::{
    AttachFailure, AttachmentFence, AttachmentId, Delivery, RefusedInput, RequestCustody,
    SupervisorConfig, SupervisorEvent, SupervisorFailure, SupervisorStartError, SupervisorWork,
};
