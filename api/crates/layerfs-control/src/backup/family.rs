//! The `backup` family (#533 step 11, H §7.3): buckets connected by name in
//! `backup/<bucket>/record.json` (an r2 or MinIO bucket and prefix with a
//! key reference, never a secret), and one link per Store per bucket in the
//! Store's record (O23). A backup is a snapshot on the Store's machine whose
//! bytes go through the caller's process: machines never hold bucket keys.
//! Objects are `<prefix>/<file>/store.sqlite` (whole or a manifest) plus its
//! pieces. LayerFS never deletes a bucket object.
use super::record::{remote_error, BucketRecord, StoreLink};
use crate::common::context::Context;
use crate::common::home::lease;
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused, Outcome, View};
use crate::common::place::{self, Executor, Start};
use crate::project::record::ProjectRecord;
use crate::store::record::{StoreKind, StoreRecord, StoreRef};
use crate::store::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup as remote;
use layerfs_store_backup::bucket::{pieces, Driver, Metadata};
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

fn hex(bytes: &[u8; 32]) -> SdkValue {
    SdkValue::text(pieces::hex(bytes))
}

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
fn list(home: &Path, only: Option<&str>) -> Outcome {
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

/// The Store's link to `bucket` after a transfer.
fn save_link(home: &Path, store: &str, bucket: &str, link: StoreLink) -> Result<()> {
    let _lock = home::lock::take(home)?;
    write_link(home, store, bucket, link)
}

/// [`save_link`] for a caller that holds the home lock.
fn write_link(home: &Path, store: &str, bucket: &str, link: StoreLink) -> Result<()> {
    let record = StoreRecord::read(home, store)?
        .ok_or_else(|| refused("NotFound", format!("no Store named {store}")))?;
    record.with_link(bucket, &link).write(home)
}

fn progress(context: &mut Context<'_>, phase: &str) {
    context.progress(SdkValue::map([("phase", SdkValue::text(phase))]));
}

/// Metadata key naming the home and machine a backup came from,
/// `<home id>/<machine>`.
const EXPORTED_FROM: &str = "layerfs-exported-from";

/// What the bucket must hold for a create: the copy this home last moved
/// through the Store's link to that file, or nothing. A link that records
/// no sha256 knows no copy, so the file must be absent.
fn expected(record: &StoreRecord, bucket: &str, file: &str) -> remote::Expected {
    match record.link(bucket).filter(|link| link.file() == Some(file)) {
        Some(link) => link
            .sha256()
            .map_or(remote::Expected::Absent, remote::Expected::Sha256),
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
    let bucket_record = BucketRecord::of(&home, bucket)?;
    let record = StoreRecord::read(&home, store)?
        .ok_or_else(|| refused("NotFound", format!("no Store named {store}")))?;
    // the record parsed once (R4)
    let store_ref = record.store_ref()?;
    if let StoreRef::Postgres { .. } = store_ref {
        return Err(refused(
            "InvalidInput",
            format!("Store {store} is a PostgreSQL database; back it up with PostgreSQL's tools"),
        ));
    }
    record.moving()?;
    let driver = bucket_record.driver()?;
    let expected = expected(&record, bucket, &file);
    let machine = store_ref.machine()?.to_owned();
    let executor = place::plan(&home, store_ref, Start::OnDemand)?;
    let identity = crate::store::identity::identity_in(&record)?;
    let home_id = home::ensure(&home)?.id;
    let object = remote::store_object(&file).map_err(|why| refused("InvalidInput", why))?;
    let local_steps = matches!(executor, Executor::Local);
    let executor = std::cell::RefCell::new(executor);
    let _lease = lease::hold(&home, store)?;
    check(driver.as_ref(), &object, expected, &home_id)?;
    progress(context, "snapshot");
    let step = |op: &str, more: Vec<(&str, SdkValue)>| {
        let mut fields = vec![
            ("store", SdkValue::text(store)),
            ("store_record", record.to_record().clone()),
        ];
        fields.extend(more);
        let args = SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
        match &mut *executor.borrow_mut() {
            Executor::Local => crate::store::transfer::steps::run(&home, op, &args),
            Executor::Worker(worker) => {
                let request = SdkValue::map([("op", SdkValue::text(op)), ("args", args)]);
                worker.call("store.file", request, &mut |_| {})
            }
        }
    };
    let staged = step("stage", vec![])?;
    // `native` stages beside the Store; a worker's copy comes here
    let local = match local_steps {
        true => {
            let mut name = paths::store_file(&home, store).into_os_string();
            name.push(".backup");
            std::path::PathBuf::from(name)
        }
        false => paths::store_dir(&home, store).join("backup.staging"),
    };
    let result = (|| {
        if !local_steps {
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
    if !local_steps {
        let _ = std::fs::remove_file(&local);
    }
    let _ = step("unstage", vec![]);
    let (moved, heads) = result?;
    let saved = StoreLink::new(&file, "create", moved.bytes, &moved.sha256, heads.as_ref());
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
    crate::common::names::StoreName::parse(store)?;
    let bucket = args.get_text("bucket").unwrap_or("");
    let driver = BucketRecord::of(&home, bucket)?.driver()?;
    let existing = StoreRecord::read(&home, store)?;
    let file = args
        .get_text("file")
        .map(str::to_owned)
        .or_else(|| {
            let link = existing.as_ref()?.link(bucket)?;
            link.file().map(str::to_owned)
        })
        .unwrap_or_else(|| store.to_owned());
    let mut _lease = None;
    let identity = match &existing {
        None => None,
        Some(record) => {
            if record.kind()? == StoreKind::Postgresql {
                return Err(refused(
                    "InvalidInput",
                    format!("Store {store} is a PostgreSQL database"),
                ));
            }
            let identity = crate::store::identity::identity_in(record)?;
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
        None => crate::sandbox::registry::random()?,
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
        _ => StoreRecord::restored(store, &loaded.binding, &cursor),
    };
    let found = projects_in(&home, store, &record, &staged)?;
    progress(context, "install");
    let _lock = home::lock::take(&home)?;
    let destination = paths::store_file(&home, store);
    let now = StoreRecord::read(&home, store)?;
    match (&existing, now) {
        (None, None) if !destination.exists() => {
            home::private_dir(&paths::store_dir(&home, store))?;
            // a link never replaces a Store that appeared meanwhile
            std::fs::hard_link(&staged, &destination).map_err(failed)?;
            record.write(&home)?;
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
            if now.field("binding") != record.field("binding") {
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
        StoreLink::new(
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
        let random = crate::sandbox::registry::random()?;
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
fn replaceable(home: &Path, store: &str, bucket: &str, record: &StoreRecord) -> Result<()> {
    let machine = record.machine()?;
    if machine != "native" {
        return Err(refused("Unsupported", format!("Store {store} is on {machine}; a restore replaces a Store on native: move it there first")));
    }
    record.moving()?;
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
    crate::inventory::can_retire_store(home, store)?;
    let linked = record.link(bucket).and_then(|link| link.heads());
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

/// `(name, stack)` of each project the Store file `file` holds, read on a
/// connection of this call's own that is closed before this returns.
fn projects_in(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    file: &Path,
) -> Result<Vec<(String, SdkValue)>> {
    let args = SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("store", SdkValue::text(store)),
        ("store_record", record.to_record().clone()),
        ("storePath", SdkValue::text(file.display().to_string())),
    ]);
    let SdkValue::List(items) = crate::store::identity::list_projects(&args)? else {
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
        if ProjectRecord::read(home, store, name)?.is_none() {
            ProjectRecord::found(store, name, stack.clone()).write(home)?;
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
