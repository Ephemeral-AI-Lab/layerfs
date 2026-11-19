//! The `backup` family (#533 step 11, H §7.3; call surface 2.0): buckets
//! connected by name in `backup/<bucket>/record.json` (an r2 or S3-endpoint
//! bucket and prefix with its key's id; the secret in the home's secret
//! folder), and one link per Store per bucket in the Store's record (O23).
//! A backup is a snapshot on the Store's machine whose bytes go through the
//! caller's process: machines never hold bucket keys. Objects are
//! `<prefix>/<file>/store.sqlite` (whole or a manifest) plus its pieces.
//! LayerFS never deletes a bucket object.
//! This file connects and disconnects buckets and keeps the links; `list`,
//! `create` and `restore` are in their own files.
pub use super::create::{create, BackupCreate};
use super::record::{BucketConnect, BucketRecord, StoreLink};
pub use super::restore::{restore, BackupRestore};
use super::secret;
use crate::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bucket::backup as remote;
use layerfs_bucket::bucket::RemoteError;
use layerfs_home::home::{self, paths};
use layerfs_home::names::BucketName;
use layerfs_home::outcome::{failed, refused, Outcome};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `backup.list`: the Store files in a bucket, or in every bucket.
pub fn list(home: &Path, bucket: Option<&str>) -> Outcome {
    super::list::list(home, bucket)
}

fn taken(home: &Path, name: &str) -> Result<()> {
    if paths::bucket_record(home, name).exists() {
        return Err(refused(
            "Exists",
            format!("a bucket named {name} is connected; to change its key, disconnect it and connect it again"),
        ));
    }
    Ok(())
}

/// `backup.connect(name, url, { access_key, secret_key })`: one listing
/// proves the bucket answers with this key before anything is recorded
/// (`Unavailable` when it cannot be reached or refuses the key); then the
/// secret goes to the home's secret folder and the record is written.
pub fn connect(home: &Path, request: &BucketConnect) -> Outcome {
    let name = request.name.as_str();
    if request.access_key.is_empty() || request.secret_key.is_empty() {
        return Err(refused(
            "InvalidInput",
            "a bucket's key needs its id and its secret",
        ));
    }
    let record = BucketRecord::from_url(request)?;
    taken(home, name)?;
    let driver = record.driver_with(request.credentials())?;
    remote::list_stores(driver.as_ref()).map_err(|error| match error {
        RemoteError::Invalid(why) => refused("InvalidInput", why),
        RemoteError::Refused { status, code } => refused(
            "Unavailable",
            format!(
                "{name} refused the key at {}: HTTP {status} {code}; nothing was recorded",
                record.url()
            ),
        ),
        other => refused(
            "Unavailable",
            format!(
                "{name} did not answer at {}: {other}; nothing was recorded",
                record.url()
            ),
        ),
    })?;
    let _lock = home::lock::take(home)?;
    taken(home, name)?;
    secret::write(home, name, &request.secret_key)?;
    if let Err(refusal) = record.write(home) {
        let _ = secret::remove(home, name);
        return Err(refusal);
    }
    Ok(record.answer("connected"))
}

/// `backup.disconnect(bucket)`: forgets a bucket, its key and every Store's
/// link to it; its files stay. `unlinked` names the Stores whose link was
/// removed (the first 20; `unlinked_count` only when there were more).
pub fn disconnect(home: &Path, name: &BucketName) -> Outcome {
    let name = name.as_str();
    BucketRecord::of(home, name)?;
    let _lock = home::lock::take(home)?;
    let mut unlinked = Vec::new();
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let _store = crate::record::lock(home, &store)?;
        let Some(record) = StoreRecord::read(home, &store)? else {
            continue;
        };
        if let Some(kept) = record.without_link(name) {
            kept.write(home)?;
            unlinked.push(store);
        }
    }
    std::fs::remove_dir_all(
        paths::bucket_record(home, name)
            .parent()
            .expect("in a folder"),
    )
    .map_err(failed)?;
    secret::remove(home, name)?;
    layerfs_home::home::generation::moved(home);
    let (unlinked, count) = layerfs_home::helpers::shown_names(&unlinked);
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("unlinked", unlinked),
        ("unlinked_count", count),
        ("outcome", SdkValue::text("disconnected")),
        ("bucket", SdkValue::text(name)),
    ]))
}

/// The Store's link to `bucket` after a transfer, under the Store's lock
/// (the caller does not hold it).
pub fn write_link(home: &Path, store: &str, bucket: &str, link: StoreLink) -> Result<()> {
    let _store = crate::record::lock(home, store)?;
    let record = StoreRecord::read(home, store)?
        .ok_or_else(|| refused("NotFound", format!("no Store named {store}")))?;
    record.with_link(bucket, &link).write(home)
}
