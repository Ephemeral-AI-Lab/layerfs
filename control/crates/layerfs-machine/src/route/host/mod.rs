//! The host worker's protocol (macOS and Linux): its endpoint custody,
//! client, frames, hello, diagnostics and streamed writes.
pub(crate) mod client;
pub(crate) mod diagnostic;
pub(crate) mod endpoint;
pub(crate) mod frame;
pub(crate) mod hello;
pub(crate) mod start;
pub(crate) mod stream;
