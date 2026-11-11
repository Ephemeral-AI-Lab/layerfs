//! The daemon channel (#536): the authenticated control connection, its
//! pool, its calls and their failures, and the Workspace calls over it.
#[doc(hidden)]
pub mod call;
#[doc(hidden)]
pub mod capacity;
#[doc(hidden)]
pub mod channel;
#[doc(hidden)]
pub mod failure;
#[doc(hidden)]
pub mod fence;
#[doc(hidden)]
pub mod internal;
#[doc(hidden)]
pub mod operator;
#[doc(hidden)]
pub mod pool;
#[doc(hidden)]
pub mod trace;
#[doc(hidden)]
pub mod values;
#[doc(hidden)]
pub mod workspace;
