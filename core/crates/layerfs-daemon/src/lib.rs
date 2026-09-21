#![forbid(unsafe_code)]
//! Native process assembly for headless delivery and local Linux mounts.
mod config;
mod headless;
mod run;

pub use run::run;
