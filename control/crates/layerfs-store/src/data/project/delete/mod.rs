//! `project.delete`: the caller's entry ([`run`]), the worker's handlers
//! ([`worker`]) and the worker-side opens a remote Store takes.
mod remote;
pub mod run;
pub mod worker;
