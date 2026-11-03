//! A bucket's record, `backup/<bucket>/record.json` (#536 §9.2): an r2 or
//! MinIO bucket and prefix with a key reference, never a secret, made from
//! the URL `backup.connect` is given; and the driver it names.
use crate::common::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup::Provider;
use layerfs_store_backup::bucket::minio::{MinioSettings, DEFAULT_PART_SIZE};
use layerfs_store_backup::bucket::r2::R2Settings;
use layerfs_store_backup::bucket::{CredentialRef, Driver, RemoteError};

type Result<T> = std::result::Result<T, SdkRefusal>;

pub(super) fn remote_error(error: RemoteError) -> SdkRefusal {
    match error {
        RemoteError::Uncertain(why) => refused("Unknown", why),
        RemoteError::Invalid(why) => refused("InvalidInput", why),
        other => refused("Failed", other.to_string()),
    }
}

/// The driver of a bucket record. Resolves its key now; sends nothing.
pub(super) fn driver(record: &SdkValue) -> Result<Box<dyn Driver>> {
    let text = |name: &str| record.get_text(name).unwrap_or("").to_owned();
    let credentials = CredentialRef::parse(&text("key")).map_err(remote_error)?;
    let part_size = record
        .get_int("part_size")
        .map(|n| n as u64)
        .unwrap_or(DEFAULT_PART_SIZE);
    let provider = match record.get_text("provider") {
        Some("minio") => Provider::Minio(MinioSettings {
            endpoint: text("endpoint"),
            bucket: text("bucket"),
            prefix: text("prefix"),
            region: record.get_text("region").unwrap_or("us-east-1").to_owned(),
            credentials,
            part_size,
        }),
        Some("r2") => Provider::R2(R2Settings {
            account: text("account"),
            jurisdiction: text("jurisdiction"),
            bucket: text("bucket"),
            prefix: text("prefix"),
            credentials,
            part_size,
        }),
        _ => return Err(refused("Failed", "a bucket record names no known provider")),
    };
    provider.driver().map_err(remote_error)
}

/// `(bucket, prefix)` of a URL's path.
fn bucket_path(path: &str) -> Result<(String, String)> {
    let path = path.trim_matches('/');
    let (bucket, prefix) = path.split_once('/').unwrap_or((path, ""));
    if bucket.is_empty() {
        return Err(refused("InvalidInput", "the URL names no bucket"));
    }
    Ok((bucket.to_owned(), prefix.trim_end_matches('/').to_owned()))
}

/// A bucket record from `url`: `http(s)://host:port/<bucket>/<prefix>` for
/// MinIO, `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]` for r2.
pub(super) fn bucket_record(name: &str, url: &str, args: &SdkValue) -> Result<SdkValue> {
    let key = args.get_text("key").map(str::to_owned).unwrap_or_else(|| {
        format!(
            "env:LAYERFS_BUCKET_{}",
            name.to_uppercase().replace(['-', '.'], "_")
        )
    });
    CredentialRef::parse(&key).map_err(remote_error)?;
    let part_size = SdkValue::Int(
        args.get_int("part_size")
            .unwrap_or(DEFAULT_PART_SIZE as i64),
    );
    if let Some(rest) = url.strip_prefix("r2://") {
        let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
        let (account, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (bucket, prefix) = bucket_path(path)?;
        let jurisdiction = query.strip_prefix("jurisdiction=").unwrap_or("");
        return Ok(SdkValue::map([
            ("name", SdkValue::text(name)),
            ("provider", SdkValue::text("r2")),
            ("account", SdkValue::text(account)),
            ("jurisdiction", SdkValue::text(jurisdiction)),
            ("bucket", SdkValue::text(bucket)),
            ("prefix", SdkValue::text(prefix)),
            ("region", SdkValue::text("auto")),
            ("key", SdkValue::text(key)),
            ("part_size", part_size),
        ]));
    }
    let scheme = if url.starts_with("http://") {
        "http://"
    } else if url.starts_with("https://") {
        "https://"
    } else {
        return Err(refused("InvalidInput", "a bucket URL is r2://<account>/<bucket>/<prefix> or http(s)://<host>/<bucket>/<prefix>"));
    };
    let after = &url[scheme.len()..];
    let (host, path) = after.split_once('/').unwrap_or((after, ""));
    let (bucket, prefix) = bucket_path(path)?;
    Ok(SdkValue::map([
        ("name", SdkValue::text(name)),
        ("provider", SdkValue::text("minio")),
        ("endpoint", SdkValue::text(format!("{scheme}{host}"))),
        ("bucket", SdkValue::text(bucket)),
        ("prefix", SdkValue::text(prefix)),
        (
            "region",
            SdkValue::text(args.get_text("region").unwrap_or("us-east-1")),
        ),
        ("key", SdkValue::text(key)),
        ("part_size", part_size),
    ]))
}
