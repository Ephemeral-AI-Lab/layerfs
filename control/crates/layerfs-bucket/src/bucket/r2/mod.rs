//! The Cloudflare R2 provider over the S3 API.
mod provider;

pub use provider::{connect, limits, R2Settings};
