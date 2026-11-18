//! `backup.list`: every Store file in the connected buckets, with the Store
//! of this home linked to it.
use super::heads::store_heads;
use super::record::{BucketRecord, StoreLink};
use crate::record::StoreRecord;
use layerfs_bridge::control::SdkValue;
use layerfs_bucket::backup as remote;
use layerfs_bucket::bucket::{pieces, RemoteError};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, Outcome};
use std::collections::BTreeMap;
use std::path::Path;

/// One Store's link to one bucket: `(bucket, file, store, link)`.
type Linked = (String, String, String, StoreLink);

/// The Stores' heads, each read at most once per listing.
struct Heads<'a> {
    home: &'a Path,
    records: BTreeMap<String, StoreRecord>,
    read: BTreeMap<String, Result<String, String>>,
}

impl Heads<'_> {
    fn of(&mut self, store: &str) -> Result<String, String> {
        if let Some(read) = self.read.get(store) {
            return read.clone();
        }
        let read = match self.records.get(store) {
            Some(record) => store_heads(self.home, record),
            None => Err(format!("no Store named {store}")),
        };
        self.read.insert(store.to_owned(), read.clone());
        read
    }
}

/// Whether linked Store `store` changed since its link's last transfer.
fn behind(heads: &mut Heads<'_>, store: &str, link: &StoreLink) -> Result<bool, String> {
    let then = link.heads().ok_or("the link records no heads")?;
    Ok(heads.of(store)? != then)
}

/// Whether the bucket's file is not the one the link last moved.
fn changed(link: &StoreLink, copy: &remote::Copy) -> Result<bool, String> {
    let then = link
        .sha256()
        .ok_or("the link records no sha256 of its last copy")?;
    let now = copy.sha256.ok_or(format!(
        "the listing read no sha256 of {} (at most {} per listing)",
        copy.file,
        remote::copies::MAX_LIST_STATS
    ))?;
    Ok(now != then)
}

fn hex(bytes: Option<&[u8; 32]>) -> SdkValue {
    SdkValue::opt_text(bytes.map(|b| pieces::hex(b)))
}

/// One row, in its long form.
fn row(
    bucket: &str,
    file: &str,
    copy: Option<&remote::Copy>,
    store: Option<&str>,
    facts: [Option<Result<bool, String>>; 2],
) -> SdkValue {
    let mut messages = Vec::new();
    let mut fact = |read: &Option<Result<bool, String>>| match read {
        Some(Ok(yes)) => SdkValue::Bool(*yes),
        Some(Err(why)) => {
            messages.push(why.clone());
            SdkValue::Null
        }
        None => SdkValue::Null,
    };
    let [is_behind, is_changed] = facts;
    let (behind, changed) = (fact(&is_behind), fact(&is_changed));
    let modified = copy.and_then(|c| c.modified);
    SdkValue::map([
        ("bucket", SdkValue::text(bucket)),
        ("file", SdkValue::text(file)),
        (
            "size",
            copy.map_or(SdkValue::Null, |c| SdkValue::Int(c.bytes as i64)),
        ),
        ("store", SdkValue::opt_text(store)),
        ("behind", behind),
        ("changed", changed),
        ("missing", SdkValue::Bool(copy.is_none())),
        (
            "message",
            SdkValue::opt_text((!messages.is_empty()).then(|| messages.join("; "))),
        ),
        (
            "modified_at",
            SdkValue::opt_text(modified.map(|ms| layerfs_home::time::iso(ms as i64))),
        ),
        ("sha256", hex(copy.and_then(|c| c.sha256.as_ref()))),
        ("heads_sha256", hex(copy.and_then(|c| c.heads.as_ref()))),
    ])
}

/// A bucket that could not be listed: `{bucket, blocked_by, reason,
/// message}`, `reason` `refused` when it answered and refused, `timeout`
/// when it ran out of time, else `lost`.
fn not_listed(record: &BucketRecord, bucket: &str, error: Result<RemoteError, String>) -> SdkValue {
    let url = record.url();
    let (reason, message) = match error {
        Ok(RemoteError::Refused { status, code }) => (
            "refused",
            format!("{bucket} refused the listing at {url}: HTTP {status} {code}"),
        ),
        Ok(other) => {
            let text = other.to_string();
            let lower = text.to_lowercase();
            let reason = match lower.contains("timed out") || lower.contains("deadline") {
                true => "timeout",
                false => "lost",
            };
            (reason, format!("{bucket} did not answer at {url}: {text}"))
        }
        Err(why) => ("lost", format!("{bucket} was not asked: {why}")),
    };
    SdkValue::map([
        ("bucket", SdkValue::text(bucket)),
        ("blocked_by", SdkValue::Null),
        ("reason", SdkValue::text(reason)),
        ("message", SdkValue::text(message)),
    ])
}

/// `backup.list(bucket?)`: one row per Store file in each connected bucket
/// (or in `only`), with the Store of this home linked to it, and one per
/// link whose file the bucket does not hold (`missing`). `behind`: the
/// Store changed since the link's last transfer (its heads read where it
/// lives, once per Store); `changed`: the bucket's file is not the one the
/// link last moved; each null with a `message` when it could not be read.
/// A bucket that cannot be listed is named in `not_listed`.
pub fn list(home: &Path, only: Option<&str>) -> Outcome {
    let (mut items, mut skipped) = (Vec::new(), Vec::new());
    let mut buckets = Vec::new();
    match only {
        Some(name) => buckets.push(BucketRecord::of(home, name)?),
        // a bucket whose record cannot be read is not listed; the others are
        None => {
            for name in paths::names(&home.join("backup")).map_err(failed)? {
                match BucketRecord::of(home, &name) {
                    Ok(record) => buckets.push(record),
                    Err(refusal) => skipped.push(SdkValue::map([
                        ("bucket", SdkValue::text(&name)),
                        ("blocked_by", SdkValue::Null),
                        ("reason", SdkValue::text("lost")),
                        (
                            "message",
                            SdkValue::text(format!(
                                "the record of bucket {name} could not be read: {}",
                                refusal.message
                            )),
                        ),
                    ])),
                }
            }
        }
    };
    let mut links: Vec<Linked> = Vec::new();
    let mut heads = Heads {
        home,
        records: BTreeMap::new(),
        read: BTreeMap::new(),
    };
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let Some(record) = StoreRecord::read(home, &store)? else {
            continue;
        };
        for (bucket, link) in record.links() {
            let file = link.file().unwrap_or(&store).to_owned();
            links.push((bucket, file, store.clone(), link));
        }
        heads.records.insert(store, record);
    }
    for record in &buckets {
        let bucket = record.name().to_owned();
        let listed = match record.driver(home) {
            Err(refusal) => Err(Err(refusal.message)),
            Ok(driver) => remote::list_stores(driver.as_ref()).map_err(Ok),
        };
        let copies = match listed {
            Ok(copies) => copies,
            Err(error) => {
                skipped.push(not_listed(record, &bucket, error));
                continue;
            }
        };
        let here = |(b, ..): &&Linked| *b == bucket;
        for copy in &copies {
            let mut linked = links
                .iter()
                .filter(here)
                .filter(|(_, file, ..)| *file == copy.file)
                .peekable();
            if linked.peek().is_none() {
                items.push(row(&bucket, &copy.file, Some(copy), None, [None, None]));
            }
            for (_, _, store, link) in linked {
                let facts = [
                    Some(behind(&mut heads, store, link)),
                    Some(changed(link, copy)),
                ];
                items.push(row(&bucket, &copy.file, Some(copy), Some(store), facts));
            }
        }
        // a link whose file the bucket does not hold; a link that never
        // moved a file (only a failed first attempt) names nothing there
        for (_, file, store, link) in links.iter().filter(here) {
            if link.transferred() && !copies.iter().any(|c| c.file == *file) {
                items.push(row(&bucket, file, None, Some(store), [None, None]));
            }
        }
    }
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("not_listed", SdkValue::List(skipped)),
    ]))
}
