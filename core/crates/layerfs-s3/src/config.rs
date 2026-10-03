//! Explicit local HTTP profile, credentials, namespace and operation bounds.
use layerfs_storage::port::{ObjectError, ObjectKey};
use std::{fmt, time::Duration};
/// Connection and namespace inputs. This engine implements the local HTTP profile.
#[derive(Clone)]
pub struct S3Config {
    /// HTTP endpoint with an explicit or default port, without path or credentials.
    pub endpoint: String,
    /// Existing bucket; this client does not bootstrap services.
    pub bucket: String,
    /// Optional slash-separated ASCII namespace within the bucket.
    pub prefix: String,
    /// SigV4 region.
    pub region: String,
    /// Access-key identity, redacted by Debug.
    pub access_key: String,
    /// Secret signing input, redacted by Debug.
    pub secret_key: String,
    /// Bound for the one TCP connection attempt.
    pub connect_timeout: Duration,
    /// Absolute bound for one wire request, across all partial socket operations.
    pub request_timeout: Duration,
    /// Largest complete opaque body this client accepts.
    pub max_body_bytes: usize,
}
impl fmt::Debug for S3Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &"[configured]")
            .field("bucket", &self.bucket)
            .field("prefix", &self.prefix)
            .field("region", &self.region)
            .field("credentials", &"[redacted]")
            .field("connect_timeout", &self.connect_timeout)
            .field("request_timeout", &self.request_timeout)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish()
    }
}
impl S3Config {
    /// Reads the explicit service environment. Missing required inputs fail.
    pub fn from_env() -> Result<Self, ObjectError> {
        let required = |name| std::env::var(name).map_err(|_| ObjectError::Malformed);
        let config = Self {
            endpoint: required("LAYERFS_S3_ENDPOINT")?,
            bucket: required("LAYERFS_S3_BUCKET")?,
            prefix: std::env::var("LAYERFS_S3_PREFIX").unwrap_or_default(),
            region: required("LAYERFS_S3_REGION")?,
            access_key: required("LAYERFS_S3_ACCESS_KEY")?,
            secret_key: required("LAYERFS_S3_SECRET_KEY")?,
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(2),
            max_body_bytes: layerfs_storage::policy::SINGLETON_PACK_LIMIT,
        };
        config.validate()?;
        Ok(config)
    }
    pub(crate) fn validate(&self) -> Result<(), ObjectError> {
        self.address()?;
        let safe = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        if !(3..=63).contains(&self.bucket.len())
            || !self
                .bucket
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
            || !self.bucket.as_bytes()[0].is_ascii_alphanumeric()
            || !self.bucket.as_bytes()[self.bucket.len() - 1].is_ascii_alphanumeric()
            || (!self.prefix.is_empty() && !self.prefix.split('/').all(safe))
            || !safe(&self.region)
            || !safe(&self.access_key)
            || self.secret_key.is_empty()
            || self.secret_key.bytes().any(|b| b.is_ascii_control())
            || self.connect_timeout.is_zero()
            || self.request_timeout.is_zero()
            || self.max_body_bytes == 0
            || self.max_body_bytes > layerfs_storage::policy::SINGLETON_PACK_LIMIT
        {
            return Err(ObjectError::Malformed);
        }
        Ok(())
    }
    pub(crate) fn address(&self) -> Result<(String, u16, String), ObjectError> {
        let authority = self
            .endpoint
            .strip_prefix("http://")
            .ok_or(ObjectError::Malformed)?;
        if authority.is_empty()
            || authority
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !b".-:".contains(&b))
        {
            return Err(ObjectError::Malformed);
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (
                host,
                port.parse::<u16>().map_err(|_| ObjectError::Malformed)?,
            ),
            None => (authority, 80),
        };
        if host.is_empty() || host.contains(':') || port == 0 {
            return Err(ObjectError::Malformed);
        }
        Ok((host.to_owned(), port, authority.to_owned()))
    }
    pub(crate) fn path(&self, key: ObjectKey) -> String {
        let suffix = crate::sign::hex(key.as_bytes());
        if self.prefix.is_empty() {
            format!("/{}/{}", self.bucket, suffix)
        } else {
            format!("/{}/{}/{}", self.bucket, self.prefix, suffix)
        }
    }
}
