//! Where a bucket keeps a Store: its provider ([`Provider`], from the
//! bucket's record) and the remote file of each Store under its name
//! ([`store_object`]): Store `NAME` is the file `NAME/store.sqlite` in the
//! bucket's prefix (plus its pieces when split).
use crate::backup::providers::s3::{self, MinioSettings};
use crate::backup::providers::r2::{self, R2Settings};
use crate::backup::bucket::{Driver, RemoteError};

/// The file name of every Store on a remote, under the Store's name.
pub const STORE_FILE: &str = "store.sqlite";

/// The remote file of the Store named `store` (one segment of
/// `[A-Za-z0-9._-]`, not starting with a dot).
pub fn store_object(store: &str) -> Result<String, String> {
    let valid = !store.is_empty()
        && store.len() <= 128
        && !store.starts_with('.')
        && store
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    if !valid {
        return Err(format!(
            "Store name {store:?}: use [A-Za-z0-9._-], at most 128, not starting with a dot"
        ));
    }
    Ok(format!("{store}/{STORE_FILE}"))
}

/// Where one remote keeps the Store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Provider {
    Minio(MinioSettings),
    /// Cloudflare R2, over HTTPS (a build with the `tls` feature).
    R2(R2Settings),
}

impl Provider {
    /// The provider's driver. Resolves credentials; sends nothing.
    pub fn driver(&self) -> Result<Box<dyn Driver>, RemoteError> {
        Ok(match self {
            Self::Minio(settings) => Box::new(s3::connect(settings)?),
            Self::R2(settings) => Box::new(r2::connect(settings)?),
        })
    }
}
