//! sqlite Stores: create, the `store` family's dispatch, remove, the
//! engine diagnostic entry points, and `store.move` (call surface 2.0; it
//! was `store.push` and `store.pull`, #536 §7.3), which moves a Store's file
//! between the machine whose calls run in this process and another machine
//! (`move_phases`, `move_leftover`, `step_request`, `step_run`). Never
//! imports `postgresql/`.
pub mod create;
pub mod diagnostic;
#[doc(hidden)]
pub mod family;
pub mod move_leftover;
pub mod move_phases;
pub mod remove;
pub mod step_request;
pub mod step_run;
