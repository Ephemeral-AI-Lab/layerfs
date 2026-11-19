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
use layerfs_home::outcome::{failed, refused, shell_word, with_fix, Outcome};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `backup.list`: the Store files in a bucket, or in every bucket.
pub fn list(home: &Path, bucket: Option<&str>) -> Outcome {
    super::list::list(home, bucket)
}

/// The bucket connected under `record`'s name: `None` when there is none,
/// its record when it names the same place ([`BucketRecord::same_place`]),
/// and `Exists` when it names another, with the fix that disconnects it (a
/// new name works too).
fn connected(home: &Path, record: &BucketRecord) -> Result<Option<BucketRecord>> {
    let name = record.name();
    let Some(value) = home::read_record(&paths::bucket_record(home, name))? else {
        return Ok(None);
    };
    let kept = BucketRecord::from_record(name, value);
    if kept.same_place(record) {
        return Ok(Some(kept));
    }
    let refusal = refused(
        "Exists",
        format!("a bucket named {name} is connected to another URL; connect this one under a new name, or disconnect {name} first"),
    );
    Err(with_fix(
        refusal,
        format!("layerfs backup disconnect {}", shell_word(name)),
    ))
}

/// `backup.connect(name, url, { access_key, secret_key })`: one listing
/// proves the bucket answers with this key before anything is recorded
/// (`Unavailable` when it cannot be reached or refuses the key, and nothing
/// changes); then the secret goes to the home's secret folder and the
/// record is written. A bucket already connected under the name at the same
/// URL has its key replaced (`rotated`, spec v21); at another URL the name
/// is `Exists`.
pub fn connect(home: &Path, request: &BucketConnect) -> Outcome {
    let name = request.name.as_str();
    if request.access_key.is_empty() || request.secret_key.is_empty() {
        return Err(refused(
            "InvalidInput",
            "a bucket's key needs its id and its secret",
        ));
    }
    let record = BucketRecord::from_url(request)?;
    connected(home, &record)?;
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
    if let Some(kept) = connected(home, &record)? {
        return rotate(home, &kept, request);
    }
    secret::write(home, name, &request.secret_key)?;
    if let Err(refusal) = record.write(home) {
        let _ = secret::remove(home, name);
        return Err(refusal);
    }
    Ok(record.answer("connected"))
}

/// Replaces connected bucket `kept`'s key with `request`'s, under the
/// home's lock: the secret by one rename, then the record with the new key
/// id by one rename; when the record cannot be written the secret kept
/// before is put back, and a put-back that fails is said in the refusal.
/// The Stores' links are in their own records and are not touched.
fn rotate(home: &Path, kept: &BucketRecord, request: &BucketConnect) -> Outcome {
    let name = kept.name();
    let before = secret::kept(home, name)?;
    secret::write(home, name, &request.secret_key)?;
    let rotated = kept.with_access_key(&request.access_key);
    if let Err(mut refusal) = rotated.write(home) {
        // the refusal says when the old secret could not be put back: the
        // folder then holds the new secret beside the old key id, which
        // the next listing refuses with the fix that connects again
        if let Err(back) = secret::put_back(home, name, before.as_ref()) {
            refusal.message = format!(
                "{}; the secret it replaced could not be put back either ({}): bucket {name} \
                 now holds the new secret with its old key id, and connecting it again \
                 settles it",
                refusal.message, back.message
            );
        }
        return Err(refusal);
    }
    Ok(rotated.answer("rotated"))
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
