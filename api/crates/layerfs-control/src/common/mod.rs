//! What every entity uses (#536): typed names, call outcomes, the call
//! trace, JSON text, a running call's context and the home.
pub mod context;
pub mod home;
pub mod json;
pub mod names;
#[doc(hidden)]
pub mod outcome;
pub(crate) mod place;
#[doc(hidden)]
pub mod trace;
