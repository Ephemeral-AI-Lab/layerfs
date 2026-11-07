//! A Sandbox's Workspaces: the `workspace` calls, the executing side of
//! them and the unmount guard.
#[doc(hidden)]
pub mod calls;
#[doc(hidden)]
pub mod direct;
#[doc(hidden)]
pub mod dispatch;
#[doc(hidden)]
pub mod exec;
#[doc(hidden)]
pub mod serve;
