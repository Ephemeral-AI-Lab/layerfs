//! An S3-endpoint bucket URL, `http(s)://<host>/<bucket>/<prefix>`, as the
//! fields of its bucket record (provider `minio`): region `us-east-1`.
use crate::backup::bucket::registry::bucket_path;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

/// Bucket `name`'s record fields from the URL after its `scheme`
/// (`http://` or `https://`), `after`, with the key id `key`.
pub(crate) fn record(
    name: &str,
    scheme: &str,
    after: &str,
    key: SdkValue,
) -> Result<SdkValue, SdkRefusal> {
    let (host, path) = after.split_once('/').unwrap_or((after, ""));
    let (bucket, prefix) = bucket_path(path)?;
    Ok(SdkValue::map([
        ("name", SdkValue::text(name)),
        ("provider", SdkValue::text("minio")),
        ("endpoint", SdkValue::text(format!("{scheme}{host}"))),
        ("bucket", SdkValue::text(bucket)),
        ("prefix", SdkValue::text(prefix)),
        ("region", SdkValue::text("us-east-1")),
        ("access_key", key),
    ]))
}
