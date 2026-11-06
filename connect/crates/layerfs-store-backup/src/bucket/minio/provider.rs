//! MinIO: one bucket on an S3-compatible server, path-style, over HTTP or,
//! with the `tls` feature, HTTPS. Objects up to 50 TiB; one PUT up to 5 TiB;
//! multipart parts of 5 MiB–5 GiB, at most 10,000, so one upload carries at
//! most 48.83 TiB.
use crate::bucket::credentials::CredentialRef;
use crate::bucket::error::RemoteError;
use crate::bucket::limits::Limits;
use crate::bucket::s3_api::{S3Client, S3Config, MAX_PART, MAX_PARTS};
use std::time::Duration;

const TIB: u64 = 1024 * 1024 * 1024 * 1024;
/// Multipart part size unless settings choose another.
pub const DEFAULT_PART_SIZE: u64 = 64 * 1024 * 1024;

/// What MinIO allows. A file above what one multipart upload carries
/// (10,000 parts of 5 GiB, below MinIO's 50 TiB object limit) goes up as
/// 5 TiB pieces.
pub fn limits() -> Limits {
    Limits {
        max_file: (50 * TIB).min(MAX_PARTS * MAX_PART),
        max_request: 5 * TIB,
        piece_size: 5 * TIB,
    }
}

/// One MinIO remote. Holds a credential reference, never the secret.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinioSettings {
    /// `http://host:port`.
    pub endpoint: String,
    pub bucket: String,
    /// Namespace within the bucket; may be empty.
    pub prefix: String,
    pub region: String,
    pub credentials: CredentialRef,
    pub part_size: u64,
}

/// A driver for `settings`, resolving its credentials. Sends nothing.
pub fn connect(settings: &MinioSettings) -> Result<S3Client, RemoteError> {
    S3Client::new(S3Config {
        endpoint: settings.endpoint.clone(),
        bucket: settings.bucket.clone(),
        prefix: settings.prefix.clone(),
        region: settings.region.clone(),
        credentials: settings.credentials.resolve()?,
        connect_timeout: Duration::from_secs(5),
        io_timeout: Duration::from_secs(60),
        part_size: settings.part_size,
        limits: limits(),
    })
}
