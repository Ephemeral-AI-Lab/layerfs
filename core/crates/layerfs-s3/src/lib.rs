//! MinIO immutable object transport: one attempted request, no recovery after failed I/O.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod client;
mod config;
mod counters;
mod http;
mod sign;
pub use client::S3Objects;
pub use config::S3Config;
pub use counters::S3Diagnostics;
