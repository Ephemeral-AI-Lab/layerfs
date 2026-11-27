//! sqlite Stores: create, the `store` family's dispatch, remove, and the
//! push and pull that move a Store's file between machines. Never imports
//! `postgresql/`.
pub mod create;
pub mod diagnostic;
#[doc(hidden)]
pub mod family;
pub mod move_leftover;
pub mod move_phases;
pub mod remove;
pub mod step_request;
pub mod step_run;

// A sqlite Store between the machine whose calls run in this process and
// another machine: `store.move` (call
// surface 2.0; it was `store.push` and `store.pull`, #536 §7.3).
