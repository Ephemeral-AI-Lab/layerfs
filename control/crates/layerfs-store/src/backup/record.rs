//! A bucket's record, `backup/<bucket>/record.json` (#536 §9.2): an r2 or
//! S3-endpoint bucket and prefix with its key's id, made from the URL
//! `backup.connect` is given, and the driver it names. The key's secret is
//! in the home's secret folder ([`super::secret`]), never in the record.
//! Also each Store's link to a bucket, which the Store's record holds.
use super::secret;
use crate::get::hex_text;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_bucket::backup::Provider;
use layerfs_bucket::bucket::minio::{MinioSettings, DEFAULT_PART_SIZE};
use layerfs_bucket::bucket::r2::R2Settings;
use layerfs_bucket::bucket::{pieces, Credentials, Driver, RemoteError};
use layerfs_home::home::{self, paths, read_record, write_entity};
use layerfs_home::names::BucketName;
use layerfs_home::outcome::{refused, View};
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

pub fn remote_error(error: RemoteError) -> SdkRefusal {
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
    pub fn of(home: &Path, name: &str) -> Result<Self> {
        layerfs_home::names::BucketName::parse(name)?;
        read_record(&paths::bucket_record(home, name))?
            .map(|value| Self::from_record(name, value))
            .ok_or_else(|| refused("NotFound", format!("no bucket named {name}")))
    }

    /// Writes the record to `backup/<name>/record.json` under `home`.
    pub fn write(&self, home: &Path) -> Result<()> {
        write_entity(&paths::bucket_record(home, &self.name), &self.fields)
    }

    /// The bucket's name in this home.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The record's fields, never its `v`.
    pub fn to_sdk(&self, _view: View) -> SdkValue {
        match &self.fields {
            SdkValue::Map(fields) => {
                SdkValue::Map(fields.iter().filter(|(n, _)| n != "v").cloned().collect())
            }
            other => other.clone(),
        }
    }

    /// The bucket as `backup.connect` answers it (its long form): its
    /// name, provider, URL and key id, never the secret.
    pub fn answer(&self, outcome: &str) -> SdkValue {
        SdkValue::map([
            ("ok", SdkValue::Bool(true)),
            ("outcome", SdkValue::text(outcome)),
            ("bucket", SdkValue::text(&self.name)),
            (
                "provider",
                SdkValue::opt_text(self.fields.get_text("provider")),
            ),
            ("url", SdkValue::text(self.url())),
            (
                "access_key",
                SdkValue::opt_text(self.fields.get_text("access_key")),
            ),
        ])
    }

    /// The URL the record was made from: `r2://<account>/<bucket>/<prefix>`
    /// (with `?jurisdiction=`) or `<endpoint>/<bucket>/<prefix>`.
    pub fn url(&self) -> String {
        let text = |name: &str| self.fields.get_text(name).unwrap_or("");
        let mut path = text("bucket").to_owned();
        if !text("prefix").is_empty() {
            path = format!("{path}/{}", text("prefix"));
        }
        match self.fields.get_text("provider") {
            Some("r2") => {
                let url = format!("r2://{}/{path}", text("account"));
                match text("jurisdiction") {
                    "" => url,
                    j => format!("{url}?jurisdiction={j}"),
                }
            }
            _ => format!("{}/{path}", text("endpoint")),
        }
    }

    /// The bucket's key: its id from the record and its secret from the
    /// home's secret folder. A record with no key id (written before 2.0)
    /// is refused with the fix that connects it again.
    pub fn credentials(&self, home: &Path) -> Result<Credentials> {
        let Some(access_key) = self.fields.get_text("access_key") else {
            let name = &self.name;
            let refusal = refused(
                "Failed",
                format!("bucket {name}'s record names no access key; disconnect it and connect it again"),
            );
            return Err(layerfs_home::outcome::with_fix(
                refusal,
                format!(
                    "layerfs backup disconnect {name} && layerfs backup connect {name} {} --access-key <key> --key-stdin",
                    layerfs_home::outcome::shell_word(&self.url())
                ),
            ));
        };
        Ok(Credentials {
            access_key: access_key.to_owned(),
            secret_key: secret::read(home, &self.name)?,
        })
    }

    /// The driver of this bucket. Reads its key now; sends nothing.
    pub fn driver(&self, home: &Path) -> Result<Box<dyn Driver>> {
        self.driver_with(self.credentials(home)?)
    }

    /// The driver of this bucket with key `credentials`; sends nothing.
    pub fn driver_with(&self, credentials: Credentials) -> Result<Box<dyn Driver>> {
        provider(&self.fields, credentials)?
            .driver()
            .map_err(remote_error)
    }

    /// Diagnostic S3 client from the exact saved provider settings and key
    /// `credentials`; sends nothing.
    pub fn diagnostic_client(
        &self,
        credentials: Credentials,
    ) -> Result<layerfs_bucket::bucket::s3_api::S3Client> {
        match provider(&self.fields, credentials)? {
            Provider::Minio(settings) => {
                layerfs_bucket::bucket::minio::connect(&settings).map_err(remote_error)
            }
            Provider::R2(settings) => {
                layerfs_bucket::bucket::r2::connect(&settings).map_err(remote_error)
            }
        }
    }

    /// A bucket record from `request`'s URL:
    /// `http(s)://host:port/<bucket>/<prefix>` for an S3 endpoint,
    /// `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]` for r2.
    pub fn from_url(request: &BucketConnect) -> Result<Self> {
        Ok(Self::from_record(&request.name, bucket_record(request)?))
    }
}

/// `backup.connect(name, url, { access_key, secret_key })`. No `Debug`: it
/// holds a secret.
#[derive(Clone)]
pub struct BucketConnect {
    /// The bucket's name in this home.
    pub name: BucketName,
    /// Its URL.
    pub url: String,
    /// The key's id.
    pub access_key: String,
    /// The key's secret.
    pub secret_key: String,
}

impl BucketConnect {
    /// The key it names.
    pub fn credentials(&self) -> Credentials {
        Credentials {
            access_key: self.access_key.clone(),
            secret_key: self.secret_key.clone(),
        }
    }
}

fn provider(record: &SdkValue, credentials: Credentials) -> Result<Provider> {
    let text = |name: &str| record.get_text(name).unwrap_or("").to_owned();
    let part_size = DEFAULT_PART_SIZE;
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
    Ok(provider)
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

/// The fields of a bucket record from `request`: r2 is always region
/// `auto` and an S3 endpoint `us-east-1`; the piece size is the default.
fn bucket_record(request: &BucketConnect) -> Result<SdkValue> {
    let (name, url) = (request.name.as_str(), request.url.as_str());
    let key = SdkValue::text(&request.access_key);
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
            ("access_key", key),
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
        ("region", SdkValue::text("us-east-1")),
        ("access_key", key),
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
    pub fn new(
        file: &str,
        op: &str,
        bytes: u64,
        sha256: &[u8; 32],
        heads: Option<&[u8; 32]>,
    ) -> Self {
        Self::from_record(SdkValue::map([
            ("file", SdkValue::text(file)),
            (
                "heads",
                heads.map(|heads| hex_text(heads)).unwrap_or_default(),
            ),
            (
                "last",
                SdkValue::map([
                    ("op", SdkValue::text(op)),
                    ("at", SdkValue::Int(home::now_ms())),
                    ("bytes", SdkValue::Int(bytes as i64)),
                    ("sha256", hex_text(sha256)),
                ]),
            ),
        ]))
    }

    /// The bucket file it backs up to.
    pub fn file(&self) -> Option<&str> {
        self.fields.get_text("file")
    }

    /// The heads digest it records, as hex.
    pub fn heads(&self) -> Option<String> {
        match self.fields.get("heads") {
            Some(SdkValue::Text(text)) => Some(text.clone()),
            Some(SdkValue::Bytes(bytes)) => Some(hex(bytes)),
            _ => None,
        }
    }

    /// The sha256 of the copy its last transfer moved.
    pub fn sha256(&self) -> Option<[u8; 32]> {
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

    /// Whether a transfer through it ever worked (it records a `last`): a
    /// link written only for a failed first attempt has none.
    pub fn transferred(&self) -> bool {
        matches!(self.fields.get("last"), Some(SdkValue::Map(_)))
    }

    /// This link with `attempt` recording a transfer `call` (`create` or
    /// `restore`) that did not work, and why: `{call, at, error}`. A link
    /// that did not exist yet names `file` and nothing else.
    pub fn with_attempt(link: Option<Self>, file: &str, call: &str, error: &str) -> Self {
        let fields = link.map_or_else(
            || SdkValue::map([("file", SdkValue::text(file))]),
            |link| link.fields,
        );
        let attempt = SdkValue::map([
            ("call", SdkValue::text(call)),
            ("at", SdkValue::Int(home::now_ms())),
            ("error", SdkValue::text(error)),
        ]);
        Self::from_record(home::with_field(fields, crate::get::ATTEMPT, attempt))
    }
}
