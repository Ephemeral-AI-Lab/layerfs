//! Host-owned fair composition of provider service and independent native I/O.
mod attachment;
mod drive;
mod fences;
mod owner;
mod progress;
mod types;

pub use crate::runtime::wake::{SupervisorWake, SupervisorWakeHandle};
pub use owner::Supervisor;
pub use progress::{
    AttachmentTurn, ParkReasons, ProviderTurn, SupervisorPark, SupervisorStage, SupervisorTurn,
    SupervisorWait,
};
pub use types::{
    AttachFailure, AttachmentFence, AttachmentId, Delivery, RefusedInput, RequestCustody,
    SupervisorConfig, SupervisorEvent, SupervisorFailure, SupervisorStartError, SupervisorWork,
};
