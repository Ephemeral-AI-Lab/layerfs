//! The `backup` family (#533 step 11, H §7.3): buckets connected by name in
//! `backup/<bucket>/record.json` (an r2 or MinIO bucket and prefix with a
//! key reference, never a secret), and one link per Store per bucket in the
//! Store's record (O23). A backup is a snapshot on the Store's machine whose
//! bytes go through the caller's process: machines never hold bucket keys.
//! Objects are `<prefix>/<file>/store.sqlite` (whole or a manifest) plus its
//! pieces. LayerFS never deletes a bucket object.
//! This file dispatches, connects and disconnects buckets and keeps the
//! links; `list`, `create` and `restore` are in their own files.
use super::create::create;
use super::list::list;
use super::record::{remote_error, BucketRecord, StoreLink};
use super::restore::restore;
use crate::common::context::Context;
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused, Outcome, View};
use crate::store::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup as remote;
use layerfs_store_backup::bucket::pieces;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Runs one `backup` call.
pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    match call {
        "connect" => connect(&home, args),
        "disconnect" => disconnect(&home, args.get_text("name").unwrap_or("")),
        "list" => list(&home, args.get_text("bucket")),
        "create" => create(context, args),
        "restore" => restore(context, args),
        _ => Err(refused("InvalidInput", format!("no call backup.{call}"))),
    }
}

/// `backup.connect(name, url, { key, region, part_size })`: one listing
/// proves the bucket answers with this key before it is recorded.
fn connect(home: &Path, args: &SdkValue) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    crate::common::names::BucketName::parse(name)?;
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

/// Forgets a bucket and every Store's link to it; its objects stay.
fn disconnect(home: &Path, name: &str) -> Outcome {
    BucketRecord::of(home, name)?;
    let _lock = home::lock::take(home)?;
    let mut unlinked = Vec::new();
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let _store = crate::store::record::lock(home, &store)?;
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

pub(super) fn hex(bytes: &[u8; 32]) -> SdkValue {
    SdkValue::text(pieces::hex(bytes))
}

/// The Store's link to `bucket` after a transfer.
pub(super) fn save_link(home: &Path, store: &str, bucket: &str, link: StoreLink) -> Result<()> {
    write_link(home, store, bucket, link)
}

/// [`save_link`], under the Store's lock (the caller does not hold it).
pub(super) fn write_link(home: &Path, store: &str, bucket: &str, link: StoreLink) -> Result<()> {
    let _store = crate::store::record::lock(home, store)?;
    let record = StoreRecord::read(home, store)?
        .ok_or_else(|| refused("NotFound", format!("no Store named {store}")))?;
    record.with_link(bucket, &link).write(home)
}
