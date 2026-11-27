//! An R2 bucket URL, `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]`,
//! as the fields of its bucket record: region `auto`.
use crate::backup::bucket::registry::bucket_path;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

/// Bucket `name`'s record fields from the URL after `r2://` (`rest`), with
/// the key id `key`.
pub(crate) fn record(name: &str, rest: &str, key: SdkValue) -> Result<SdkValue, SdkRefusal> {
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (account, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (bucket, prefix) = bucket_path(path)?;
    let jurisdiction = query.strip_prefix("jurisdiction=").unwrap_or("");
    Ok(SdkValue::map([
        ("name", SdkValue::text(name)),
        ("provider", SdkValue::text("r2")),
        ("account", SdkValue::text(account)),
        ("jurisdiction", SdkValue::text(jurisdiction)),
        ("bucket", SdkValue::text(bucket)),
        ("prefix", SdkValue::text(prefix)),
        ("region", SdkValue::text("auto")),
        ("access_key", key),
    ]))
}
