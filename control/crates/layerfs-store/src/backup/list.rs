//! `backup.list`: every Store file on the connected buckets with its state.
use super::family::hex;
use super::record::{remote_error, BucketRecord, StoreLink};
use crate::record::{StoreKind, StoreRecord};
use layerfs_bridge::control::SdkValue;
use layerfs_bucket::backup as remote;
use layerfs_bucket::bucket::pieces;
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, Outcome};
use std::path::Path;

/// Whether linked Store `store` changed since its link's last transfer: its
/// heads now against the link's. `Err` says why that cannot be read here.
fn store_changed(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    link: &StoreLink,
) -> std::result::Result<bool, String> {
    let then = link.heads().ok_or("the link records no heads")?;
    if let Ok(StoreKind::Postgresql) = record.kind() {
        return Err(format!("Store {store} is a PostgreSQL database"));
    }
    let machine = record.machine().map_err(|refusal| refusal.message)?;
    if machine != "native" {
        return Err(format!(
            "Store {store} is on {machine}; its heads are read on native only"
        ));
    }
    let now = remote::heads(&paths::store_file(home, store))?;
    Ok(pieces::hex(&now) != then)
}

/// Whether the bucket's copy changed since the link's last transfer: its
/// sha256 against the one that transfer moved.
fn bucket_changed(link: &StoreLink, copy: &remote::Copy) -> std::result::Result<bool, String> {
    let then = link
        .sha256()
        .ok_or("the link records no sha256 of its last copy")?;
    let now = copy.sha256.ok_or(format!(
        "the listing read no sha256 of {} (at most {} stats per listing)",
        copy.file,
        remote::copies::MAX_LIST_STATS
    ))?;
    Ok(now != then)
}

/// A linked copy's state and, for `unknown`, why.
fn state(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    link: &StoreLink,
    copy: &remote::Copy,
) -> (&'static str, Option<String>) {
    match (
        store_changed(home, store, record, link),
        bucket_changed(link, copy),
    ) {
        (Ok(false), Ok(false)) => ("current", None),
        (Ok(true), Ok(false)) => ("store_changed", None),
        (Ok(false), Ok(true)) => ("bucket_changed", None),
        (Ok(true), Ok(true)) => ("both_changed", None),
        (store, bucket) => {
            let why: Vec<String> = [store.err(), bucket.err()].into_iter().flatten().collect();
            ("unknown", Some(why.join("; ")))
        }
    }
}

/// `backup.list(bucket?)`: every Store file on the connected buckets, one
/// item per Store of this home linked to it (one with no `store` when none
/// is), each with its state: `current` (neither the Store nor the copy
/// changed since the link's last transfer), `store_changed` (the Store
/// has), `bucket_changed` (the copy has: written by another Store or
/// home), `both_changed`, `unknown` (with its `reason`: a Store on another
/// machine, a link or listing without a heads or sha256), `unlinked` (no
/// Store of this home links it) or `missing` (linked, gone from the bucket).
pub fn list(home: &Path, only: Option<&str>) -> Outcome {
    let buckets = match only {
        Some(name) => vec![name.to_owned()],
        None => paths::names(&home.join("backup")).map_err(failed)?,
    };
    // (bucket, file, store, link, store record)
    let mut links = Vec::new();
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let Some(record) = StoreRecord::read(home, &store)? else {
            continue;
        };
        for (bucket, link) in record.links() {
            let file = link.file().unwrap_or(&store).to_owned();
            links.push((bucket, file, store.clone(), link, record.clone()));
        }
    }
    let mut items = Vec::new();
    for bucket in buckets {
        let record = BucketRecord::of(home, &bucket)?;
        let copies = remote::list_stores(record.driver()?.as_ref()).map_err(remote_error)?;
        for copy in &copies {
            let item = |store: SdkValue, state: &str, reason: Option<String>| {
                let mut fields = vec![
                    ("bucket", SdkValue::text(&bucket)),
                    ("file", SdkValue::text(&copy.file)),
                    ("bytes", SdkValue::Int(copy.bytes as i64)),
                    (
                        "modified",
                        copy.modified
                            .map(|m| SdkValue::Int(m as i64))
                            .unwrap_or_default(),
                    ),
                    ("heads", copy.heads.as_ref().map(hex).unwrap_or_default()),
                    ("store", store),
                    ("state", SdkValue::text(state)),
                ];
                if let Some(reason) = reason {
                    fields.push(("reason", SdkValue::text(reason)));
                }
                SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
            };
            let mut linked = links
                .iter()
                .filter(|(b, f, ..)| *b == bucket && *f == copy.file)
                .peekable();
            if linked.peek().is_none() {
                items.push(item(SdkValue::Null, "unlinked", None));
            }
            for (_, _, store, link, record) in linked {
                let (state, reason) = state(home, store, record, link, copy);
                items.push(item(SdkValue::text(store), state, reason));
            }
        }
        for (b, file, store, ..) in &links {
            if *b == bucket && !copies.iter().any(|c| c.file == *file) {
                items.push(SdkValue::map([
                    ("bucket", SdkValue::text(&bucket)),
                    ("file", SdkValue::text(file)),
                    ("store", SdkValue::text(store)),
                    ("state", SdkValue::text("missing")),
                ]));
            }
        }
    }
    Ok(SdkValue::map([("items", SdkValue::List(items))]))
}
