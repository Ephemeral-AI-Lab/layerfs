//! Stores in the 2.0 home (#533): their records, the route each Store call
//! takes, chosen from records, the Stores this process holds open, and the
//! steps a move or remove takes on the file.
pub(crate) mod call;
#[doc(hidden)]
pub mod calls;
#[doc(hidden)]
pub mod connect;
#[doc(hidden)]
pub mod family;
pub(crate) mod identity;
pub(crate) mod open;
pub(crate) mod postgres;
#[doc(hidden)]
pub mod record;
pub(crate) mod resolve;
#[doc(hidden)]
pub mod service;
pub(crate) mod transfer;
