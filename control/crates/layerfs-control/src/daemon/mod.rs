//! The daemon channel (#536): the authenticated control connection, its
//! pool, its calls and their failures, and the Workspace calls over it.
pub(crate) mod call;
pub(crate) mod channel;
pub(crate) mod failure;
#[doc(hidden)]
pub mod internal;
#[doc(hidden)]
pub mod operator;
pub(crate) mod pool;
pub(crate) mod trace;
pub mod values;
pub mod workspace;
