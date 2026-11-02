//! What every entity uses (#536): typed names, call outcomes, the call
//! trace, JSON text, a running call's context and the home.
pub mod context;
pub mod home;
pub mod json;
pub mod names;
#[doc(hidden)]
pub mod outcome;
#[doc(hidden)]
pub mod trace;
