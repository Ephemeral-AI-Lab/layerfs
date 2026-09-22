#![forbid(unsafe_code)]
//! Native process assembly for headless delivery and local Linux mounts.
mod config;
mod control;
mod control_commit;
mod headless;
mod lifecycle;
mod run;

pub use run::run;
