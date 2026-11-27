//! A bucket's key: its caller reads it (a bucket's key id from its record,
//! its secret from the home's secret folder) and hands it over.
use crate::backup::bucket::error::RemoteError;
use std::fmt;

/// An access key and secret, redacted by `Debug`.
#[derive(Clone, Eq, PartialEq)]
pub struct Credentials {
    pub access_key: String,
    pub secret_key: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}

impl Credentials {
    /// The key as a request may send it: refused when either part is empty
    /// or holds a byte that is not printable ASCII.
    pub fn checked(&self) -> Result<Credentials, RemoteError> {
        let (access_key, secret_key) = (&self.access_key, &self.secret_key);
        if access_key.is_empty()
            || secret_key.is_empty()
            || access_key
                .bytes()
                .chain(secret_key.bytes())
                .any(|b| !(0x21..0x7f).contains(&b))
        {
            return Err(RemoteError::Invalid(
                "empty or unprintable credentials".to_owned(),
            ));
        }
        Ok(self.clone())
    }
}
