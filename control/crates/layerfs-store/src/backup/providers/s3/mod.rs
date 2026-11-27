//! The MinIO provider over the S3 API.
pub(crate) mod settings;
pub(crate) mod url;

pub use settings::{connect, limits, MinioSettings, DEFAULT_PART_SIZE};
