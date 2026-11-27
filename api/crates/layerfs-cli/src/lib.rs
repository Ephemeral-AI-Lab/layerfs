//! Library exports for layerfs-cli.
#![forbid(unsafe_code)]

pub(crate) mod discovery;
pub mod help;
pub mod output;
pub mod parse;
pub mod run;
mod signals;
pub mod style;
pub(crate) mod usage_words;
pub mod view;
pub mod worker;

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub use crate::style as banner; // #828 compat
