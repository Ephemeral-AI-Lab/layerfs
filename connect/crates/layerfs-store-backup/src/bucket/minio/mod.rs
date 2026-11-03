//! The MinIO provider over the S3 API.
mod provider;

pub use provider::{connect, limits, MinioSettings, DEFAULT_PART_SIZE};
