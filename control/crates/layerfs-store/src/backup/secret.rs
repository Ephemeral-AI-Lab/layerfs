//! A bucket key's secret, kept in the home's secret folder
//! (`layerfs_home::home::secrets`, kind `bucket`: `secret/bucket/<bucket>.json`,
//! a 0600 file in a 0700 folder), never answered and never logged. Only the
//! backup family reads or writes one.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::secrets::{read_secret, remove_secret, save_secret};
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The secret folder's kind of a bucket key's secret.
const KIND: &str = "bucket";

/// Bucket `bucket`'s secret; a missing or empty one is `Failed`, naming the
/// call that keeps it again.
pub fn read(home: &Path, bucket: &str) -> Result<String> {
    let kept = read_secret(home, KIND, bucket)?;
    match kept.as_ref().and_then(|value| value.get_text("secret")) {
        Some(text) if !text.is_empty() => Ok(text.to_owned()),
        Some(_) => Err(refused(
            "Failed",
            format!("the home's secret {KIND}/{bucket} is empty"),
        )),
        None => Err(refused(
            "Failed",
            format!("the home keeps no secret {KIND}/{bucket}; connect it again at its URL with its key"),
        )),
    }
}

/// Keeps `secret` as bucket `bucket`'s secret, replacing one kept before.
pub fn write(home: &Path, bucket: &str, secret: &str) -> Result<()> {
    save_secret(
        home,
        KIND,
        bucket,
        &SdkValue::map([("secret", SdkValue::text(secret))]),
    )
}

/// Bucket `bucket`'s secret as kept, whole, to put back with
/// [`put_back`]; `None` when there is none.
pub fn kept(home: &Path, bucket: &str) -> Result<Option<SdkValue>> {
    read_secret(home, KIND, bucket)
}

/// Puts back what [`kept`] read: that secret, or none.
pub fn put_back(home: &Path, bucket: &str, kept: Option<&SdkValue>) -> Result<()> {
    match kept {
        Some(value) => save_secret(home, KIND, bucket, value),
        None => remove(home, bucket),
    }
}

/// Forgets bucket `bucket`'s secret; one already gone is no error.
pub fn remove(home: &Path, bucket: &str) -> Result<()> {
    remove_secret(home, KIND, bucket).map(|_| ())
}
