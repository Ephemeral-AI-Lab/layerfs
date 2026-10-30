//! Cloudflare R2: one bucket of an account, path-style over HTTPS (the `tls`
//! feature), SigV4 region `auto`. Objects up to 4.995 TiB; one PUT up to
//! 4.995 GiB; multipart parts of 5 MiB–5 GiB, equal-sized but the last, at
//! most 10,000.
use crate::credentials::CredentialRef;
use crate::error::RemoteError;
use crate::limits::Limits;
use crate::s3_api::{S3Client, S3Config};
use std::time::Duration;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// What R2 allows. A file above 4.995 TiB goes up as pieces of that size.
pub fn limits() -> Limits {
    let max_file = 5 * 1024 * GIB - 5 * GIB;
    Limits {
        max_file,
        max_request: 5 * GIB - 5 * MIB,
        piece_size: max_file,
    }
}

/// One R2 remote. Holds a credential reference, never the secret.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct R2Settings {
    /// The Cloudflare account id: 32 lowercase hex digits.
    pub account: String,
    /// `eu` or `fedramp` for a bucket in that jurisdiction; empty otherwise.
    pub jurisdiction: String,
    pub bucket: String,
    /// Namespace within the bucket; may be empty.
    pub prefix: String,
    pub credentials: CredentialRef,
    pub part_size: u64,
}

impl R2Settings {
    /// Refuses an account id or jurisdiction R2 does not have.
    pub fn validate(&self) -> Result<(), RemoteError> {
        let account = self.account.as_bytes();
        if account.len() != 32
            || !account
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return Err(RemoteError::Invalid(
                "R2 account id: 32 lowercase hex digits".to_owned(),
            ));
        }
        if !matches!(self.jurisdiction.as_str(), "" | "eu" | "fedramp") {
            return Err(RemoteError::Invalid(format!(
                "R2 jurisdiction {:?}: eu, fedramp or none",
                self.jurisdiction
            )));
        }
        Ok(())
    }

    /// The account's S3 endpoint, in its jurisdiction.
    pub fn endpoint(&self) -> String {
        match self.jurisdiction.as_str() {
            "" => format!("https://{}.r2.cloudflarestorage.com", self.account),
            j => format!("https://{}.{j}.r2.cloudflarestorage.com", self.account),
        }
    }

    /// The S3 client inputs, with `credentials` resolved by the caller.
    pub fn config(
        &self,
        credentials: crate::credentials::Credentials,
    ) -> Result<S3Config, RemoteError> {
        self.validate()?;
        Ok(S3Config {
            endpoint: self.endpoint(),
            bucket: self.bucket.clone(),
            prefix: self.prefix.clone(),
            region: "auto".to_owned(),
            credentials,
            connect_timeout: Duration::from_secs(10),
            io_timeout: Duration::from_secs(60),
            part_size: self.part_size,
            limits: limits(),
        })
    }
}

/// A driver for `settings`, resolving its credentials. Sends nothing.
pub fn connect(settings: &R2Settings) -> Result<S3Client, RemoteError> {
    S3Client::new(settings.config(settings.credentials.resolve()?)?)
}
