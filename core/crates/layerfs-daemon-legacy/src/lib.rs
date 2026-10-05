#![forbid(unsafe_code)]
//! Native process assembly for headless delivery and local Linux mounts.
mod config;
mod control;
mod control_commit;
mod control_view;
mod execution;
mod headless;
mod lifecycle;
mod run;
mod transport;

pub use run::run;
