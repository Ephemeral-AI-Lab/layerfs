//! Explicit diagnostics outside the call table and home migration dispatcher.
mod options;
mod run;

pub use options::{doctor_request, duration};
pub use run::{doctor_options_json, doctor_with_options};
