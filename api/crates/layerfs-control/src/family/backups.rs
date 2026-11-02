//! The `backup` family (#533 step 11, H §7.3): buckets connected by name in
//! `backup/<bucket>/record.json` (an r2 or MinIO bucket and prefix with a
//! key reference, never a secret), and one link per Store per bucket in the
//! Store's record (O23). A backup is a snapshot on the Store's machine whose
//! bytes go through the caller's process: machines never hold bucket keys.
//! Objects are `<prefix>/<file>/store.sqlite` (whole or a manifest) plus its
//! pieces. LayerFS never deletes a bucket object.
use crate::common::context::Context;
use crate::common::home::lease;
use crate::common::home::{self, paths, read_record, with_field, write_record};
use crate::common::outcome::{failed, refused, Outcome};
use crate::store::files::CHUNK;
use crate::store::route;
use crate::store::store_ref::{kind_of, StoreKind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup::{self as remote, Provider};
use layerfs_store_backup::bucket::minio::{MinioSettings, DEFAULT_PART_SIZE};
use layerfs_store_backup::bucket::r2::R2Settings;
use layerfs_store_backup::bucket::{pieces, CredentialRef, Driver, Metadata, RemoteError};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

fn bucket_of(home: &Path, name: &str) -> Result<SdkValue> {
    crate::common::names::name("bucket", name).map_err(|why| refused("InvalidInput", why))?;
    read_record(&paths::bucket_record(home, name))?
        .ok_or_else(|| refused("NotFound", format!("no bucket named {name}")))
}

fn remote_error(error: RemoteError) -> SdkRefusal {
    match error {
        RemoteError::Uncertain(why) => refused("Unknown", why),
        RemoteError::Invalid(why) => refused("InvalidInput", why),
        other => refused("Failed", other.to_string()),
    }
}

/// The driver of a bucket record. Resolves its key now; sends nothing.
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

/// A bucket record from `url`: `http(s)://host:port/<bucket>/<prefix>` for
/// MinIO, `r2://<account>/<bucket>/<prefix>[?jurisdiction=eu|fedramp]` for r2.
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

/// Runs one `backup` call.
pub fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    match call {
        "connect" => {
            let name = args.get_text("name").unwrap_or("");
            crate::common::names::name("bucket", name)
                .map_err(|why| refused("InvalidInput", why))?;
            let record = bucket_record(name, args.get_text("url").unwrap_or(""), args)?;
            // one listing proves the bucket answers with this key
            let driver = driver(&record)?;
            remote::list_stores(driver.as_ref()).map_err(remote_error)?;
            let _lock = home::lock::take(&home)?;
            if paths::bucket_record(&home, name).exists() {
                return Err(refused(
                    "Exists",
                    format!("a bucket named {name} is connected"),
                ));
            }
            write_record(&paths::bucket_record(&home, name), &record)?;
            Ok(record)
        }
        "disconnect" => disconnect(&home, args.get_text("name").unwrap_or("")),
        "list" => list(&home, args.get_text("bucket")),
        "create" => create(context, args),
        "restore" => restore(context, args),
        _ => Err(refused("InvalidInput", format!("no call backup.{call}"))),
    }
}

/// Forgets a bucket and every Store's link to it; its objects stay.
fn disconnect(home: &Path, name: &str) -> Outcome {
    bucket_of(home, name)?;
    let _lock = home::lock::take(home)?;
    let mut unlinked = Vec::new();
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let path = paths::store_record(home, &store);
        let Some(record) = read_record(&path)? else {
            continue;
        };
        if let Some(SdkValue::Map(links)) = record.get("backups") {
            if links.iter().any(|(bucket, _)| bucket == name) {
                let kept: Vec<_> = links.iter().filter(|(b, _)| b != name).cloned().collect();
                write_record(
                    &path,
                    &with_field(record.clone(), "backups", SdkValue::Map(kept)),
                )?;
                unlinked.push(SdkValue::text(&store));
            }
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

fn hex(bytes: &[u8; 32]) -> SdkValue {
    SdkValue::text(pieces::hex(bytes))
}

/// The heads digest a link records, as hex.
fn link_heads(link: &SdkValue) -> Option<String> {
    match link.get("heads") {
        Some(SdkValue::Text(text)) => Some(text.clone()),
        Some(SdkValue::Bytes(bytes)) => Some(pieces::hex(bytes)),
        _ => None,
    }
}

/// Whether linked Store `store` changed since its link's last transfer: its
/// heads now against the link's. `Err` says why that cannot be read here.
fn store_changed(
    home: &Path,
    store: &str,
    record: &SdkValue,
    link: &SdkValue,
) -> std::result::Result<bool, String> {
    let then = link_heads(link).ok_or("the link records no heads")?;
    if let Ok(StoreKind::Postgresql) = kind_of(record) {
        return Err(format!("Store {store} is a PostgreSQL database"));
    }
    let machine = route::machine(record, store).map_err(|refusal| refusal.message)?;
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
fn bucket_changed(link: &SdkValue, copy: &remote::Copy) -> std::result::Result<bool, String> {
    let then = link_sha256(link).ok_or("the link records no sha256 of its last copy")?;
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
    record: &SdkValue,
    link: &SdkValue,
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
fn list(home: &Path, only: Option<&str>) -> Outcome {
    let buckets = match only {
        Some(name) => vec![name.to_owned()],
        None => paths::names(&home.join("backup")).map_err(failed)?,
    };
    // (bucket, file, store, link, store record)
    let mut links = Vec::new();
    for store in paths::names(&home.join("store")).map_err(failed)? {
        let Some(record) = read_record(&paths::store_record(home, &store))? else {
            continue;
        };
        if let Some(SdkValue::Map(fields)) = record.get("backups") {
            for (bucket, link) in fields {
                let file = link.get_text("file").unwrap_or(&store).to_owned();
                links.push((
                    bucket.clone(),
                    file,
                    store.clone(),
                    link.clone(),
                    record.clone(),
                ));
            }
        }
    }
    let mut items = Vec::new();
    for bucket in buckets {
        let record = bucket_of(home, &bucket)?;
        let copies = remote::list_stores(driver(&record)?.as_ref()).map_err(remote_error)?;
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

/// The Store's link to `bucket` after a transfer.
fn save_link(home: &Path, store: &str, bucket: &str, link: SdkValue) -> Result<()> {
    let _lock = home::lock::take(home)?;
    write_link(home, store, bucket, link)
}

/// [`save_link`] for a caller that holds the home lock.
fn write_link(home: &Path, store: &str, bucket: &str, link: SdkValue) -> Result<()> {
    let path = paths::store_record(home, store);
    let record = read_record(&path)?
        .ok_or_else(|| refused("NotFound", format!("no Store named {store}")))?;
    let links = match record.get("backups") {
        Some(SdkValue::Map(fields)) => SdkValue::Map(fields.clone()),
        _ => SdkValue::Map(Vec::new()),
    };
    write_record(
        &path,
        &with_field(record, "backups", with_field(links, bucket, link)),
    )
}

fn link(file: &str, op: &str, bytes: u64, sha256: &[u8; 32], heads: Option<&[u8; 32]>) -> SdkValue {
    SdkValue::map([
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
    ])
}

fn progress(context: &mut Context<'_>, phase: &str) {
    context.progress(SdkValue::map([("phase", SdkValue::text(phase))]));
}

/// Metadata key naming the home and machine a backup came from,
/// `<home id>/<machine>`.
const EXPORTED_FROM: &str = "layerfs-exported-from";

/// The sha256 of the copy a link's last transfer moved.
fn link_sha256(link: &SdkValue) -> Option<[u8; 32]> {
    link.get("last").and_then(|l| match l.get("sha256") {
        Some(SdkValue::Text(t)) => pieces::parse_hex32(t),
        Some(SdkValue::Bytes(b)) => b.as_slice().try_into().ok(),
        _ => None,
    })
}

/// What the bucket must hold for a create: the copy this home last moved
/// through the Store's link to that file, or nothing. A link that records
/// no sha256 knows no copy, so the file must be absent.
fn expected(record: &SdkValue, bucket: &str, file: &str) -> remote::Expected {
    let link = record.get("backups").and_then(|b| b.get(bucket));
    match link.filter(|link| link.get_text("file") == Some(file)) {
        Some(link) => link_sha256(link).map_or(remote::Expected::Absent, remote::Expected::Sha256),
        None => remote::Expected::Absent,
    }
}

/// `backup.create(store, bucket, file?)`: the Store's machine stages a
/// consistent snapshot beside the Store (`store.file` `stage`, in this
/// process for `native`); this process uploads it with its sha256, heads,
/// binding and `layerfs-exported-from`. A worker's snapshot comes here in
/// pieces first: the worker never sees the key.
fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let store = args.get_text("store").unwrap_or("");
    let bucket = args.get_text("bucket").unwrap_or("");
    let file = args.get_text("file").unwrap_or(store).to_owned();
    let bucket_record = bucket_of(&home, bucket)?;
    let record = read_record(&paths::store_record(&home, store))?
        .ok_or_else(|| refused("NotFound", format!("no Store named {store}")))?;
    if kind_of(&record)? == StoreKind::Postgresql {
        return Err(refused(
            "InvalidInput",
            format!("Store {store} is a PostgreSQL database; back it up with PostgreSQL's tools"),
        ));
    }
    route::moving(&record, store)?;
    let driver = driver(&bucket_record)?;
    let expected = expected(&record, bucket, &file);
    let identity = crate::host::stores::identity_of(&record)?;
    let machine = route::machine(&record, store)?;
    let home_id = home::ensure(&home)?.id;
    let object = remote::store_object(&file).map_err(|why| refused("InvalidInput", why))?;
    let endpoint = route::endpoint(&home, &machine)?;
    let _lease = lease::hold(&home, store)?;
    check(driver.as_ref(), &object, expected, &home_id)?;
    progress(context, "snapshot");
    let step = |op: &str, more: Vec<(&str, SdkValue)>| {
        let mut fields = vec![
            ("store", SdkValue::text(store)),
            ("store_record", record.clone()),
        ];
        fields.extend(more);
        let args = SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
        match &endpoint {
            None => crate::store::files::run(&home, op, &args),
            Some(endpoint) => {
                let request = SdkValue::map([("op", SdkValue::text(op)), ("args", args)]);
                crate::worker::call(endpoint, "store.file", request, &mut |_| {})
            }
        }
    };
    let staged = step("stage", vec![])?;
    // `native` stages beside the Store; a worker's copy comes here
    let local = match &endpoint {
        None => {
            let mut name = paths::store_file(&home, store).into_os_string();
            name.push(".backup");
            std::path::PathBuf::from(name)
        }
        Some(_) => paths::store_dir(&home, store).join("backup.staging"),
    };
    let result = (|| {
        if endpoint.is_some() {
            progress(context, "send");
            fetch_staged(&step, &local)?;
            let (_, sha256) = pieces::sha256_file(&local, None).map_err(remote_error)?;
            if Some(pieces::hex(&sha256).as_str()) != staged.get_text("sha256") {
                return Err(refused(
                    "Failed",
                    "the staged copy is not the snapshot the worker made",
                ));
            }
        }
        let heads = staged.get_text("heads").and_then(pieces::parse_hex32);
        let mut metadata = Metadata::from([
            (
                remote::create::SQLITE_VERSION.to_owned(),
                staged.get_text("sqlite").unwrap_or("").to_owned(),
            ),
            (
                remote::create::BINDING.to_owned(),
                pieces::hex(&identity.binding),
            ),
            (EXPORTED_FROM.to_owned(), format!("{home_id}/{machine}")),
        ]);
        if let Some(heads) = &heads {
            metadata.insert(remote::HEADS.to_owned(), pieces::hex(heads));
        }
        progress(context, "upload");
        check(driver.as_ref(), &object, expected, &home_id)?;
        let moved =
            pieces::upload(driver.as_ref(), &object, &local, &metadata).map_err(remote_error)?;
        Ok((moved, heads))
    })();
    if endpoint.is_some() {
        let _ = std::fs::remove_file(&local);
    }
    let _ = step("unstage", vec![]);
    let (moved, heads) = result?;
    let saved = link(&file, "create", moved.bytes, &moved.sha256, heads.as_ref());
    save_link(&home, store, bucket, saved)?;
    Ok(SdkValue::map([
        ("store", SdkValue::text(store)),
        ("bucket", SdkValue::text(bucket)),
        ("file", SdkValue::text(&file)),
        ("object", SdkValue::text(&object)),
        ("bytes", SdkValue::Int(moved.bytes as i64)),
        ("pieces", SdkValue::Int(moved.pieces as i64)),
        ("sha256", hex(&moved.sha256)),
        ("heads", heads.as_ref().map(hex).unwrap_or_default()),
    ]))
}

/// One `store.file` step on the Store's machine: its name and arguments.
type Step<'a> = dyn Fn(&str, Vec<(&str, SdkValue)>) -> Outcome + 'a;

/// Copies the snapshot a worker staged to `local`, in pieces of [`CHUNK`].
fn fetch_staged(step: &Step<'_>, local: &Path) -> Result<()> {
    let mut out = std::fs::File::create(local).map_err(failed)?;
    let mut offset = 0u64;
    loop {
        let piece = step(
            "read",
            vec![
                ("offset", SdkValue::Int(offset as i64)),
                ("len", SdkValue::Int(CHUNK as i64)),
                ("staged", SdkValue::Bool(true)),
            ],
        )?;
        let Some(SdkValue::Bytes(bytes)) = piece.get("bytes") else {
            return Ok(());
        };
        std::io::Write::write_all(&mut out, bytes).map_err(failed)?;
        offset += bytes.len() as u64;
        if (bytes.len() as u64) < CHUNK {
            return Ok(());
        }
    }
}

/// The bucket holds what this home expects at `object`, and no other home
/// wrote it (`Exists` when its `layerfs-exported-from` names another home).
fn check(
    driver: &dyn Driver,
    object: &str,
    expected: remote::Expected,
    home_id: &str,
) -> Result<()> {
    let found = driver.stat(object).map_err(remote_error)?;
    if let Some(from) = found.as_ref().and_then(|m| m.metadata.get(EXPORTED_FROM)) {
        if from.split_once('/').map_or(from.as_str(), |(id, _)| id) != home_id {
            return Err(refused(
                "Exists",
                format!(
                    "the bucket's {object} was written by another home ({from}); back up to another file"
                ),
            ));
        }
    }
    let ok = match (expected, &found) {
        (remote::Expected::Any, _) => true,
        (remote::Expected::Absent, None) => true,
        (remote::Expected::Sha256(want), Some(meta)) => {
            meta.metadata
                .get(pieces::SHA256)
                .and_then(|s| pieces::parse_hex32(s))
                == Some(want)
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(refused(
            "Conflict",
            format!(
                "the bucket's {object} is not the copy this home last moved; nothing was uploaded"
            ),
        ))
    }
}

/// `backup.restore(store, bucket, file?)`: the backup as a sqlite Store on
/// `native`: a new Store when `store` does not exist, else it replaces an
/// idle Store on `native` unchanged since its last backup or restore
/// (`Conflict` otherwise; `InUse` while a process holds it or a Sandbox of
/// its projects runs). The download, its checks and the read of its
/// projects happen before the home lock; the lock covers the re-checks, the
/// install and the record writes only.
fn restore(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let store = args.get_text("store").unwrap_or("");
    crate::common::names::name("store", store).map_err(|why| refused("InvalidInput", why))?;
    let bucket = args.get_text("bucket").unwrap_or("");
    let bucket_record = bucket_of(&home, bucket)?;
    let driver = driver(&bucket_record)?;
    let existing = read_record(&paths::store_record(&home, store))?;
    let file = args
        .get_text("file")
        .map(str::to_owned)
        .or_else(|| {
            existing.as_ref().and_then(|r| {
                r.get("backups")?
                    .get(bucket)?
                    .get_text("file")
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| store.to_owned());
    let mut _lease = None;
    let identity = match &existing {
        None => None,
        Some(record) => {
            if kind_of(record)? == StoreKind::Postgresql {
                return Err(refused(
                    "InvalidInput",
                    format!("Store {store} is a PostgreSQL database"),
                ));
            }
            let identity = crate::host::stores::identity_of(record)?;
            // refused before the download too; checked again before the install
            {
                let _lock = home::lock::take(&home)?;
                replaceable(&home, store, bucket, record)?;
            }
            _lease = Some(lease::hold(&home, store)?);
            Some(identity)
        }
    };
    let cursor = match &identity {
        Some(identity) => identity.cursor_key,
        None => crate::host::registry::random()?,
    };
    progress(context, "download");
    let staging = Staging::make(&home)?;
    let staged = staging.0.join(remote::STORE_FILE);
    let loaded = remote::load(driver.as_ref(), &file, &staged, cursor)
        .map_err(|failure| refused("Failed", failure.to_string()))?;
    let record = match (&existing, &identity) {
        (Some(record), Some(identity)) => {
            if loaded.binding != identity.binding {
                return Err(refused(
                    "Failed",
                    format!(
                        "the bucket's {file} has another binding ({}), not Store {store}'s",
                        pieces::hex(&loaded.binding)
                    ),
                ));
            }
            record.clone()
        }
        _ => SdkValue::map([
            ("name", SdkValue::text(store)),
            ("kind", SdkValue::text("sqlite")),
            ("binding", SdkValue::Bytes(loaded.binding.clone())),
            ("cursor", SdkValue::Bytes(cursor.to_vec())),
            ("machine", SdkValue::text("native")),
            ("profile", SdkValue::text("disposable")),
            ("created", SdkValue::Int(home::now_ms())),
        ]),
    };
    let found = projects_in(&home, store, &record, &staged)?;
    progress(context, "install");
    let _lock = home::lock::take(&home)?;
    let destination = paths::store_file(&home, store);
    let now = read_record(&paths::store_record(&home, store))?;
    match (&existing, now) {
        (None, None) if !destination.exists() => {
            home::private_dir(&paths::store_dir(&home, store))?;
            // a link never replaces a Store that appeared meanwhile
            std::fs::hard_link(&staged, &destination).map_err(failed)?;
            write_record(&paths::store_record(&home, store), &record)?;
        }
        (None, _) => {
            return Err(refused(
                "Exists",
                format!("a Store named {store} was made while this restore downloaded"),
            ));
        }
        (Some(_), None) => {
            return Err(refused(
                "NotFound",
                format!("Store {store} was removed while this restore downloaded"),
            ));
        }
        (Some(_), Some(now)) => {
            if now.get("binding") != record.get("binding") {
                return Err(refused(
                    "Conflict",
                    format!("Store {store} was replaced while this restore downloaded"),
                ));
            }
            replaceable(&home, store, bucket, &now)?;
            std::fs::rename(&staged, &destination).map_err(failed)?;
        }
    }
    write_link(
        &home,
        store,
        bucket,
        link(
            &file,
            "restore",
            loaded.bytes,
            &loaded.sha256,
            loaded.heads.as_ref(),
        ),
    )?;
    let projects = write_projects(&home, store, &found)?;
    Ok(SdkValue::map([
        ("store", SdkValue::text(store)),
        ("bucket", SdkValue::text(bucket)),
        ("file", SdkValue::text(&file)),
        ("bytes", SdkValue::Int(loaded.bytes as i64)),
        ("sha256", hex(&loaded.sha256)),
        ("heads", loaded.heads.as_ref().map(hex).unwrap_or_default()),
        ("projects", projects),
    ]))
}

/// A restore's own download folder, `store/.restore-<random>/` (hidden from
/// the Store names, and on the home's filesystem so the install is one
/// rename); removed with whatever it still holds when the restore ends.
struct Staging(std::path::PathBuf);

impl Staging {
    fn make(home: &Path) -> Result<Self> {
        let random = crate::host::registry::random()?;
        let dir = home
            .join("store")
            .join(format!(".restore-{}", pieces::hex(&random[..12])));
        home::private_dir(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Refuses replacing Store `store` unless it is an idle sqlite Store on
/// `native`, unchanged since its last backup or restore through `bucket`.
/// Under the home lock.
fn replaceable(home: &Path, store: &str, bucket: &str, record: &SdkValue) -> Result<()> {
    let machine = route::machine(record, store)?;
    if machine != "native" {
        return Err(refused("Unsupported", format!("Store {store} is on {machine}; a restore replaces a Store on native: move it there first")));
    }
    route::moving(record, store)?;
    let destination = paths::store_file(home, store);
    // this process's own open copy closes first
    crate::store::open::close(&destination)?;
    let holders = lease::holders(home, store)?;
    if !holders.is_empty() {
        return Err(refused(
            "InUse",
            format!("Store {store} is open in process {holders:?}"),
        ));
    }
    let sandboxes = sandboxes_of(home, store)?;
    if !sandboxes.is_empty() {
        return Err(refused(
            "InUse",
            format!(
                "Store {store} has running Sandboxes: {}",
                sandboxes.join(", ")
            ),
        ));
    }
    let linked = record
        .get("backups")
        .and_then(|b| b.get(bucket))
        .and_then(link_heads);
    let now = remote::heads(&destination).ok().map(|h| pieces::hex(&h));
    if linked.is_none() || now != linked {
        return Err(refused(
            "Conflict",
            format!("Store {store} changed since its last backup or restore through {bucket}; back it up first or restore under a new name"),
        ));
    }
    // every opener (a daemon, a mounted Workspace, an SDK process) holds
    // SQLite's sidecars while the Store is open
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = destination.as_os_str().to_owned();
        sidecar.push(suffix);
        if std::path::Path::new(&sidecar).exists() {
            return Err(refused(
                "InUse",
                format!("Store {store} is open (its {suffix} exists)"),
            ));
        }
    }
    Ok(())
}

/// The Sandboxes of any machine whose project is in Store `store`.
fn sandboxes_of(home: &Path, store: &str) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for machine in paths::names(&home.join("machine")).map_err(failed)? {
        let dir = paths::machine_dir(home, &machine).join("sandbox");
        for sandbox in paths::names(&dir).map_err(failed)? {
            let record = read_record(&dir.join(&sandbox).join("record.json"))?;
            let project = record
                .as_ref()
                .and_then(|r| r.get_text("project"))
                .unwrap_or("");
            if project.split_once('/').map(|(s, _)| s) == Some(store) {
                found.push(sandbox);
            }
        }
    }
    Ok(found)
}

/// `(name, stack)` of each project the Store file `file` holds, read on a
/// connection of this call's own that is closed before this returns.
fn projects_in(
    home: &Path,
    store: &str,
    record: &SdkValue,
    file: &Path,
) -> Result<Vec<(String, SdkValue)>> {
    let args = SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("store", SdkValue::text(store)),
        ("store_record", record.clone()),
        ("storePath", SdkValue::text(file.display().to_string())),
    ]);
    let SdkValue::List(items) = crate::host::stores::list_projects(&args)? else {
        return Err(refused("Failed", "the Store's project list is not a list"));
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            let name = item.get_text("name")?.to_owned();
            Some((name, item.get("stack").cloned().unwrap_or_default()))
        })
        .collect())
}

/// The project records of a restored Store, rebuilt from what it holds:
/// one per project; a record whose project it lacks is removed. Under the
/// home lock, after [`replaceable`] found no Sandbox of the Store.
fn write_projects(home: &Path, store: &str, found: &[(String, SdkValue)]) -> Result<SdkValue> {
    let mut names = Vec::new();
    for (name, stack) in found {
        let path = paths::project_record(home, store, name);
        if read_record(&path)?.is_none() {
            write_record(
                &path,
                &SdkValue::map([("name", SdkValue::text(name)), ("stack", stack.clone())]),
            )?;
        }
        names.push(SdkValue::text(name));
    }
    for old in paths::names(&paths::store_dir(home, store).join("project")).map_err(failed)? {
        if !names.contains(&SdkValue::text(&old)) {
            std::fs::remove_dir_all(paths::project_dir(home, store, &old)).map_err(failed)?;
        }
    }
    Ok(SdkValue::List(names))
}
