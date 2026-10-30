//! MinIO: one bucket on an S3-compatible server, path-style, plain HTTP in
//! this phase. Objects up to 50 TiB; one PUT up to 5 TiB; multipart parts of
//! 5 MiB–5 GiB, at most 10,000.
use crate::credentials::CredentialRef;
use crate::error::RemoteError;
use crate::limits::Limits;
use crate::s3_api::{S3Client, S3Config};
use std::time::Duration;

const TIB: u64 = 1024 * 1024 * 1024 * 1024;
/// Multipart part size unless settings choose another.
pub const DEFAULT_PART_SIZE: u64 = 64 * 1024 * 1024;

/// What MinIO allows. A file above 50 TiB goes up as 5 TiB pieces.
pub fn limits() -> Limits {
    Limits {
        max_file: 50 * TIB,
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

impl MinioSettings {
    /// Reads `LAYERFS_REMOTE_ENDPOINT`, `LAYERFS_REMOTE_BUCKET`, and
    /// optionally `LAYERFS_REMOTE_PREFIX`, `LAYERFS_REMOTE_REGION`
    /// (`us-east-1`), `LAYERFS_REMOTE_PART_SIZE` and
    /// `LAYERFS_REMOTE_CREDENTIALS` (a reference; default
    /// `env:LAYERFS_REMOTE`, that is `LAYERFS_REMOTE_ACCESS_KEY` and
    /// `LAYERFS_REMOTE_SECRET_KEY`).
    pub fn from_env() -> Result<Self, RemoteError> {
        let read = |name: &str| std::env::var(format!("LAYERFS_REMOTE_{name}")).ok();
        let required = |name: &str| {
            read(name)
                .ok_or_else(|| RemoteError::Invalid(format!("LAYERFS_REMOTE_{name} is not set")))
        };
        let part_size = match read("PART_SIZE") {
            Some(text) => text
                .parse()
                .map_err(|_| RemoteError::Invalid("LAYERFS_REMOTE_PART_SIZE".to_owned()))?,
            None => DEFAULT_PART_SIZE,
        };
        Ok(Self {
            endpoint: required("ENDPOINT")?,
            bucket: required("BUCKET")?,
            prefix: read("PREFIX").unwrap_or_default(),
            region: read("REGION").unwrap_or_else(|| "us-east-1".to_owned()),
            credentials: CredentialRef::parse(
                &read("CREDENTIALS").unwrap_or_else(|| "env:LAYERFS_REMOTE".to_owned()),
            )?,
            part_size,
        })
    }
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
