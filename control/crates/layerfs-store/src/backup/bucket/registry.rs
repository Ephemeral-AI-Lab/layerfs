//! Where a bucket keeps a Store: its provider ([`Provider`], from the
//! bucket's record, or from the URL `backup.connect` is given by its
//! scheme, [`record_of_url`]) and the remote file of each Store under its name
//! ([`store_object`]): Store `NAME` is the file `NAME/store.sqlite` in the
//! bucket's prefix (plus its pieces when split).
use crate::backup::bucket::record::BucketConnect;
use crate::backup::bucket::{Credentials, Driver, RemoteError};
use crate::backup::providers::r2::{self, R2Settings};
use crate::backup::providers::s3::{self, MinioSettings, DEFAULT_PART_SIZE};
use layerfs_home::reply::outcome::refused;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

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

impl Provider {
    /// The provider bucket record `record` names, holding `credentials`.
    pub fn from_record(record: &SdkValue, credentials: Credentials) -> Result<Self, SdkRefusal> {
        let text = |name: &str| record.get_text(name).unwrap_or("").to_owned();
        let part_size = DEFAULT_PART_SIZE;
        let provider = match record.get_text("provider") {
            Some("minio") => Self::Minio(MinioSettings {
                endpoint: text("endpoint"),
                bucket: text("bucket"),
                prefix: text("prefix"),
                region: record.get_text("region").unwrap_or("us-east-1").to_owned(),
                credentials,
                part_size,
            }),
            Some("r2") => Self::R2(R2Settings {
                account: text("account"),
                jurisdiction: text("jurisdiction"),
                bucket: text("bucket"),
                prefix: text("prefix"),
                credentials,
                part_size,
            }),
            _ => return Err(refused("Failed", "a bucket record names no known provider")),
        };
        Ok(provider)
    }
}

/// The fields of a bucket record from `request`'s URL, by its scheme: r2 is
/// always region `auto` and an S3 endpoint `us-east-1`; the piece size is
/// the default.
pub fn record_of_url(request: &BucketConnect) -> Result<SdkValue, SdkRefusal> {
    let (name, url) = (request.name.as_str(), request.url.as_str());
    let key = SdkValue::text(&request.access_key);
    if let Some(rest) = url.strip_prefix("r2://") {
        return r2::url::record(name, rest, key);
    }
    let scheme = if url.starts_with("http://") {
        "http://"
    } else if url.starts_with("https://") {
        "https://"
    } else {
        return Err(refused("InvalidInput", "a bucket URL is r2://<account>/<bucket>/<prefix> or http(s)://<host>/<bucket>/<prefix>"));
    };
    s3::url::record(name, scheme, &url[scheme.len()..], key)
}

/// `(bucket, prefix)` of a bucket URL's path.
pub(crate) fn bucket_path(path: &str) -> Result<(String, String), SdkRefusal> {
    let path = path.trim_matches('/');
    let (bucket, prefix) = path.split_once('/').unwrap_or((path, ""));
    if bucket.is_empty() {
        return Err(refused("InvalidInput", "the URL names no bucket"));
    }
    Ok((bucket.to_owned(), prefix.trim_end_matches('/').to_owned()))
}
