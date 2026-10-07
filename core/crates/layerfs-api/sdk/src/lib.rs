//! Host Project Init and sealed Store provisioning, with no Store data service.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod init;
mod init_types;
pub use init::initialize;
pub use init_types::{InitError, InitFailure, InitRequest, SealedProject};
