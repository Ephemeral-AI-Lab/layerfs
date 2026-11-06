//! The `backup` family (#533 step 11, H §7.3): buckets connected by name in
//! `backup/<bucket>/record.json` (an r2 or MinIO bucket and prefix with a
//! key reference, never a secret), and one link per Store per bucket in the
//! Store's record (O23). A backup is a snapshot on the Store's machine whose
//! bytes go through the caller's process: machines never hold bucket keys.
//! Objects are `<prefix>/<file>/store.sqlite` (whole or a manifest) plus its
//! pieces. LayerFS never deletes a bucket object.
//! This file lists backups, connects and disconnects buckets and keeps the
//! links; `list`, `create` and `restore` are in their own files.
pub use super::create::create;
use super::record::{remote_error, BucketRecord, StoreLink};
pub use super::restore::restore;
use crate::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bucket::backup as remote;
use layerfs_bucket::bucket::pieces;
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome, View};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `backup.list`: the backups in a bucket, or in every bucket.
pub fn list(home: &Path, args: &SdkValue) -> Outcome {
    super::list::list(home, args.get_text("bucket"))
}

/// `backup.connect(name, url, { key, region, part_size })`: one listing
/// proves the bucket answers with this key before it is recorded.
pub fn connect(home: &Path, args: &SdkValue) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    layerfs_home::names::BucketName::parse(name)?;
    let record = BucketRecord::from_url(name, args.get_text("url").unwrap_or(""), args)?;
    // one listing proves the bucket answers with this key
    let driver = record.driver()?;
    remote::list_stores(driver.as_ref()).map_err(remote_error)?;
    let _lock = home::lock::take(home)?;
    if paths::bucket_record(home, name).exists() {
        return Err(refused(
            "Exists",
            format!("a bucket named {name} is connected"),
        ));
    }
    record.write(home)?;
    Ok(record.to_sdk(View::Full))
}

/// `backup.disconnect(name)`: forgets a bucket and every Store's link to
/// it; its objects stay.
pub fn disconnect(home: &Path, args: &SdkValue) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
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
            unlinked.push(SdkValue::text(&store));
        }
    }
    std::fs::remove_dir_all(
        paths::bucket_record(home, name)
            .parent()
            .expect("in a folder"),
    )
    .map_err(failed)?;
    Ok(SdkValue::map([
        ("disconnected", SdkValue::text(name)),
        ("unlinked", SdkValue::List(unlinked)),
    ]))
}

pub fn hex(bytes: &[u8; 32]) -> SdkValue {
    SdkValue::text(pieces::hex(bytes))
}

/// The Store's link to `bucket` after a transfer.
pub fn save_link(home: &Path, store: &str, bucket: &str, link: StoreLink) -> Result<()> {
    write_link(home, store, bucket, link)
}

/// [`save_link`], under the Store's lock (the caller does not hold it).
pub fn write_link(home: &Path, store: &str, bucket: &str, link: StoreLink) -> Result<()> {
    let _store = crate::record::lock(home, store)?;
    let record = StoreRecord::read(home, store)?
        .ok_or_else(|| refused("NotFound", format!("no Store named {store}")))?;
    record.with_link(bucket, &link).write(home)
}
