//! One S3 endpoint, bucket and namespace, credentials and bounds.
use crate::credentials::Credentials;
use crate::driver::check_name;
use crate::error::RemoteError;
use crate::limits::Limits;
use std::time::Duration;

/// Connection and namespace inputs for one S3-compatible remote.
#[derive(Clone, Debug)]
pub struct S3Config {
    /// `http://host[:port]`, without path or credentials.
    pub endpoint: String,
    /// Existing bucket; this client does not create buckets.
    pub bucket: String,
    /// Optional slash-separated namespace within the bucket.
    pub prefix: String,
    /// SigV4 region.
    pub region: String,
    pub credentials: Credentials,
    /// Bound for the one TCP connection attempt.
    pub connect_timeout: Duration,
    /// Bound for each socket read or write, so a stalled peer fails while a
    /// long transfer that keeps moving does not.
    pub io_timeout: Duration,
    /// Multipart part size; a file larger than this goes up in parts.
    pub part_size: u64,
    /// What the provider allows.
    pub limits: Limits,
}

impl S3Config {
    /// Refuses unusable inputs before anything is sent.
    pub fn validate(&self) -> Result<(), RemoteError> {
        self.address()?;
        self.limits.validate()?;
        let bucket = self.bucket.as_bytes();
        let invalid = |what: &str| Err(RemoteError::Invalid(format!("S3 {what}")));
        if !(3..=63).contains(&bucket.len())
            || !bucket
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.".contains(b))
            || !bucket[0].is_ascii_alphanumeric()
            || !bucket[bucket.len() - 1].is_ascii_alphanumeric()
        {
            return invalid("bucket");
        }
        if !self.prefix.is_empty() {
            check_name(&self.prefix)?;
        }
        if self.region.is_empty()
            || !self
                .region
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return invalid("region");
        }
        if self.connect_timeout.is_zero() || self.io_timeout.is_zero() {
            return invalid("timeouts");
        }
        crate::s3_api::multipart::check_part(self.part_size)?;
        Ok(())
    }

    /// Host, port and the `Host` header value.
    pub(crate) fn address(&self) -> Result<(String, u16, String), RemoteError> {
        if self.endpoint.starts_with("https://") {
            return Err(RemoteError::Unsupported(
                "TLS endpoints come in a later phase; MinIO runs plain HTTP locally".to_owned(),
            ));
        }
        let invalid = || RemoteError::Invalid(format!("S3 endpoint {:?}", self.endpoint));
        let authority = self.endpoint.strip_prefix("http://").ok_or_else(invalid)?;
        if authority.is_empty()
            || !authority
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
        {
            return Err(invalid());
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host, port.parse::<u16>().map_err(|_| invalid())?),
            None => (authority, 80),
        };
        if host.is_empty() || host.contains(':') || port == 0 {
            return Err(invalid());
        }
        Ok((host.to_owned(), port, authority.to_owned()))
    }

    /// The path-style object key path of `name`.
    pub(crate) fn object_path(&self, name: &str) -> Result<String, RemoteError> {
        check_name(name)?;
        Ok(format!("/{}/{}", self.bucket, self.key(name)))
    }

    /// The object key of `name` within the bucket.
    pub(crate) fn key(&self, name: &str) -> String {
        if self.prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{}/{name}", self.prefix)
        }
    }
}
