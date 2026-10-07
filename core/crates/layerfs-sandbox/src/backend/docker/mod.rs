//! Local Docker Engine API1.54, with exact IDs and no command replay.
mod body;
mod headers;
mod http;
mod inspect;
mod json;
mod request;
mod socket;
mod streams;
mod strings;
pub use http::Docker;
pub use request::{ExecCreateFailure, ExecCreated, ExecRequest};
pub use streams::{
    Attached, ExecHandle, InputCloseFailure, InputEnd, OutputFailure, OutputProgress, RuntimeInput,
    RuntimeOutput, SplitFailure, StartFailure,
};
