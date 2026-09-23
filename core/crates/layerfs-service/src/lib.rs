//! Authorized service operations over the local public C1/C2 interfaces.
#![forbid(unsafe_code)]
mod error;
mod input;
mod project;
mod read;
mod records;
mod save;
pub mod server;
mod service;
pub use service::{Grant, Service, StoreAccess};
