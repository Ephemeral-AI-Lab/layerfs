//! The daemon channel (#536): the authenticated control connection, its
//! pool, its calls and their failures, and the Workspace calls over it.
#[doc(hidden)]
pub mod call;
pub mod channel;
#[doc(hidden)]
pub mod failure;
#[doc(hidden)]
pub mod refusal;
#[doc(hidden)]
pub mod workspace;
