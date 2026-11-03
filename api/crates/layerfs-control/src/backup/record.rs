//! A bucket's record, `backup/<bucket>/record.json` (#536 §9.2): an r2 or
//! MinIO bucket and prefix with a key reference, never a secret, made from
//! the URL `backup.connect` is given; and the driver it names. Also each
//! Store's link to a bucket, which the Store's record holds.
use crate::common::home::{self, paths, read_record, write_record};
use crate::common::outcome::{refused, View};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup::Provider;
use layerfs_store_backup::bucket::minio::{MinioSettings, DEFAULT_PART_SIZE};
use layerfs_store_backup::bucket::r2::R2Settings;
use layerfs_store_backup::bucket::{pieces, CredentialRef, Driver, RemoteError};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// One bucket's record as read (fields kept whole and in order). Its
/// provider is read only by [`BucketRecord::driver`], so a record naming
/// none can still be disconnected, as before.
#[derive(Clone, Debug)]
pub struct BucketRecord {
    name: String,
    fields: SdkValue,
}

pub(super) fn remote_error(error: RemoteError) -> SdkRefusal {
    match error {
        RemoteError::Uncertain(why) => refused("Unknown", why),
        RemoteError::Invalid(why) => refused("InvalidInput", why),
        other => refused("Failed", other.to_string()),
    }
}

impl BucketRecord {
    /// Bucket `name`'s record `value`, as read.
    pub fn from_record(name: &str, value: SdkValue) -> Self {
        Self {
            name: name.to_owned(),
            fields: value,
        }
    }

    /// Bucket `name`'s record; a bad name is `InvalidInput`, none `NotFound`.
    pub(crate) fn of(home: &Path, name: &str) -> Result<Self> {
        crate::common::names::BucketName::parse(name)?;
        read_record(&paths::bucket_record(home, name))?
            .map(|value| Self::from_record(name, value))
            .ok_or_else(|| refused("NotFound", format!("no bucket named {name}")))
    }

    /// Writes the record to `backup/<name>/record.json` under `home`.
    pub(crate) fn write(&self, home: &Path) -> Result<()> {
        write_record(&paths::bucket_record(home, &self.name), &self.fields)
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The bucket as `backup.connect` answers it: its record (never a `v`).
    pub fn to_sdk(&self, _view: View) -> SdkValue {
        match &self.fields {
            SdkValue::Map(fields) => {
                SdkValue::Map(fields.iter().filter(|(n, _)| n != "v").cloned().collect())
            }
            other => other.clone(),
        }
    }

    /// The driver of this bucket. Resolves its key now; sends nothing.
    pub(crate) fn driver(&self) -> Result<Box<dyn Driver>> {
        driver(&self.fields)
    }

    /// A bucket record from `url`: `http(s)://host:port/<bucket>/<prefix>`
    /// for MinIO, `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]`
    /// for r2.
    pub(crate) fn from_url(name: &str, url: &str, args: &SdkValue) -> Result<Self> {
        Ok(Self::from_record(name, bucket_record(name, url, args)?))
    }
}

/// The driver of a bucket record.
fn driver(record: &SdkValue) -> Result<Box<dyn Driver>> {
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

/// The fields of a bucket record from `url`.
fn bucket_record(name: &str, url: &str, args: &SdkValue) -> Result<SdkValue> {
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

/// One Store's link to a bucket, its record's `backups.<bucket>`: the file
/// it backs up to, the heads it had and what its last transfer moved.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreLink {
    fields: SdkValue,
}

impl StoreLink {
    /// A link `value` as a Store record holds it.
    pub fn from_record(value: SdkValue) -> Self {
        Self { fields: value }
    }

    /// The link a transfer `op` (`create` or `restore`) of file `file` leaves.
    pub(crate) fn new(
        file: &str,
        op: &str,
        bytes: u64,
        sha256: &[u8; 32],
        heads: Option<&[u8; 32]>,
    ) -> Self {
        let hex = |bytes: &[u8; 32]| SdkValue::text(pieces::hex(bytes));
        Self::from_record(SdkValue::map([
            ("file", SdkValue::text(file)),
            ("heads", heads.map(hex).unwrap_or_default()),
            (
                "last",
                SdkValue::map([
                    ("op", SdkValue::text(op)),
                    ("at", SdkValue::Int(home::now_ms())),
                    ("bytes", SdkValue::Int(bytes as i64)),
                    ("sha256", hex(sha256)),
                ]),
            ),
        ]))
    }

    /// The bucket file it backs up to.
    pub(crate) fn file(&self) -> Option<&str> {
        self.fields.get_text("file")
    }

    /// The heads digest it records, as hex.
    pub(crate) fn heads(&self) -> Option<String> {
        match self.fields.get("heads") {
            Some(SdkValue::Text(text)) => Some(text.clone()),
            Some(SdkValue::Bytes(bytes)) => Some(pieces::hex(bytes)),
            _ => None,
        }
    }

    /// The sha256 of the copy its last transfer moved.
    pub(crate) fn sha256(&self) -> Option<[u8; 32]> {
        self.fields.get("last").and_then(|l| match l.get("sha256") {
            Some(SdkValue::Text(t)) => pieces::parse_hex32(t),
            Some(SdkValue::Bytes(b)) => b.as_slice().try_into().ok(),
            _ => None,
        })
    }

    /// The link as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }
}
