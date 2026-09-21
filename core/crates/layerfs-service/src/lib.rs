//! Authorized service operations over the local public C1/C2 interfaces.
#![forbid(unsafe_code)]
mod input;
mod operation;
mod owner;
pub use owner::{Grant, Service, StoreAccess};
pub mod native;
