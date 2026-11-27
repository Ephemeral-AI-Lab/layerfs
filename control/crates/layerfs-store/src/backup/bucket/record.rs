//! A bucket's record, `backup/<bucket>/record.json` (#536 §9.2): an r2 or
//! S3-endpoint bucket and prefix with its key's id, made from the URL
//! `backup.connect` is given, and the driver it names. The key's secret is
//! in the home's secret folder ([`super::secret`]), never in the record.
//! The URL is read by its provider (`providers/*/url.rs`, chosen in
//! [`super::registry`]); each Store's link to a bucket is
//! [`crate::backup::copy::link`].
use super::secret;
use crate::backup::bucket::{Credentials, Driver, RemoteError};
use crate::backup::copy::Provider;
use layerfs_home::home::{self, paths, read_record, write_entity};
use layerfs_home::names::BucketName;
use layerfs_home::outcome::{refused, View};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
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

    /// Whether `other` names the same place: the same provider and the
    /// same URL after parsing (account or endpoint, bucket, prefix and r2's
    /// jurisdiction), with the host (an endpoint's, or r2's account) in
    /// lower case and an endpoint's default port (`:443` for https, `:80`
    /// for http) left out; slashes around the bucket and prefix are already
    /// gone when the record is made (RV3 NOTE 5). The key and the name are
    /// not compared.
    pub fn same_place(&self, other: &Self) -> bool {
        self.fields.get_text("provider") == other.fields.get_text("provider")
            && self.place() == other.place()
    }

    /// The URL [`Self::same_place`] compares: [`Self::url`] with its host
    /// in lower case and without a default port.
    fn place(&self) -> String {
        let text = |name: &str| self.fields.get_text(name).unwrap_or("");
        let host = match self.fields.get_text("provider") {
            Some("r2") => text("account").to_ascii_lowercase(),
            _ => {
                let endpoint = text("endpoint");
                let (scheme, host) = endpoint.split_once("://").unwrap_or(("", endpoint));
                let host = host.to_ascii_lowercase();
                let host = match scheme {
                    "https" => host.strip_suffix(":443").unwrap_or(&host),
                    "http" => host.strip_suffix(":80").unwrap_or(&host),
                    _ => &host,
                };
                format!("{scheme}://{host}")
            }
        };
        let path = match text("prefix") {
            "" => text("bucket").to_owned(),
            prefix => format!("{}/{prefix}", text("bucket")),
        };
        format!("{host}/{path}?{}", text("jurisdiction"))
    }

    /// This record with key id `access_key`; every other field is kept as
    /// read.
    pub fn with_access_key(&self, access_key: &str) -> Self {
        let fields = home::with_field(
            self.fields.clone(),
            "access_key",
            SdkValue::text(access_key),
        );
        Self::from_record(&self.name, fields)
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
    /// is refused with the fix that connects it again at its URL, which
    /// keeps its key and its Stores' links (spec v21).
    pub fn credentials(&self, home: &Path) -> Result<Credentials> {
        let Some(access_key) = self.fields.get_text("access_key") else {
            let name = &self.name;
            let refusal = refused(
                "Failed",
                format!("bucket {name}'s record names no access key; connect it again at its URL with its key"),
            );
            return Err(layerfs_home::outcome::with_fix(
                refusal,
                self.connect_again(),
            ));
        };
        // a missing or empty secret has the same fix (V21-4; RC-A F4)
        let secret_key = secret::read(home, &self.name, || self.connect_again())?;
        Ok(Credentials {
            access_key: access_key.to_owned(),
            secret_key,
        })
    }

    /// The fix of a bucket whose key is missing or refused: connecting it
    /// again at its URL with its key, which keeps its Stores' links (spec
    /// v21, V21-4); name and URL shell-quoted.
    pub fn connect_again(&self) -> String {
        format!(
            "layerfs backup connect {} {} --access-key <key> --key-stdin",
            layerfs_home::outcome::shell_word(&self.name),
            layerfs_home::outcome::shell_word(&self.url())
        )
    }

    /// The driver of this bucket. Reads its key now; sends nothing.
    pub fn driver(&self, home: &Path) -> Result<Box<dyn Driver>> {
        self.driver_with(self.credentials(home)?)
    }

    /// The driver of this bucket with key `credentials`; sends nothing.
    pub fn driver_with(&self, credentials: Credentials) -> Result<Box<dyn Driver>> {
        Provider::from_record(&self.fields, credentials)?
            .driver()
            .map_err(remote_error)
    }

    /// Diagnostic S3 client from the exact saved provider settings and key
    /// `credentials`; sends nothing.
    pub fn diagnostic_client(
        &self,
        credentials: Credentials,
    ) -> Result<crate::backup::s3_api::S3Client> {
        match Provider::from_record(&self.fields, credentials)? {
            Provider::Minio(settings) => {
                crate::backup::providers::s3::connect(&settings).map_err(remote_error)
            }
            Provider::R2(settings) => {
                crate::backup::providers::r2::connect(&settings).map_err(remote_error)
            }
        }
    }

    /// A bucket record from `request`'s URL:
    /// `http(s)://host:port/<bucket>/<prefix>` for an S3 endpoint,
    /// `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]` for r2.
    pub fn from_url(request: &BucketConnect) -> Result<Self> {
        Ok(Self::from_record(
            &request.name,
            crate::backup::bucket::registry::record_of_url(request)?,
        ))
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
