//! The Cloudflare R2 provider over the S3 API.
pub(crate) mod settings;
pub(crate) mod url;

pub use settings::{connect, limits, R2Settings};
