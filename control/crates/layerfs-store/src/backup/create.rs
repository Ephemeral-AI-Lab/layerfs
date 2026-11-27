//! `backup.create`: a snapshot on the Store's machine, uploaded by this process.
use super::progress::Shown;
use super::record::{remote_error, BucketRecord, StoreLink};
use crate::backup::bucket::{pieces, Driver, Metadata};
use crate::backup::copy as remote;
use crate::get::hex_text;
use crate::intent;
use crate::lease;
use crate::place::{self, Executor};
use crate::record::{StoreRecord, StoreRef};
use crate::sqlite::transfer::step::{StepFrame, StoreStep};
use crate::sqlite::transfer::steps::CHUNK;
use layerfs_home::events::Events;
use layerfs_home::home::flock::{try_lock, unlock};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_protocol::sdk::hex;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

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

/// `backup.create(store, bucket, file?)`, and `backup.restore`'s too. The
/// names are checked where each call checked them.
#[derive(Clone, Debug, Default)]
pub struct BackupCreate {
    /// The Store.
    pub store: String,
    /// The connected bucket.
    pub bucket: String,
    /// The bucket file, the Store's name (or its link's file) by default.
    pub file: Option<String>,
}

/// `backup.create(store, bucket, file?)`: the Store's machine stages a
/// consistent snapshot beside the Store (`store.file` `stage`, in this
/// process for a machine whose calls run here); this process uploads it with its sha256, heads,
/// binding and `layerfs-exported-from`. A worker's snapshot comes here in
/// pieces first: the worker never sees the key. The backup is the Store's
/// `op` (`backup`) from its check to its end ([`intent`]); phases `check`,
/// `snapshot`, `send` (from a machine, bytes), `hash` and `upload` (bytes).
pub fn create(home: &Path, events: &mut Events<'_>, request: &BackupCreate) -> Outcome {
    let home = home.to_path_buf();
    let (store, bucket) = (request.store.as_str(), request.bucket.as_str());
    let file = request.file.as_deref().unwrap_or(store).to_owned();
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
    record.unblocked()?;
    let driver = bucket_record.driver(&home)?;
    let expected = expected(&record, bucket, &file);
    let machine = store_ref.machine()?.to_owned();
    let executor = place::plan(&home, store_ref)?;
    let identity = crate::identity::identity_in(&record)?;
    let home_id = home::ensure(&home)?.id;
    let object = remote::store_object(&file).map_err(|why| refused("InvalidInput", why))?;
    let local_steps = matches!(executor, Executor::Local);
    let executor = std::cell::RefCell::new(executor);
    let _lease = lease::hold(&home, store)?;
    let _creating = Creating::take(&home, store)?;
    let args = SdkValue::map([
        ("bucket", SdkValue::text(bucket)),
        ("file", SdkValue::text(&file)),
    ]);
    let mut op = intent::begin(&home, store, intent::BACKUP, "check", args)?;
    let mut shown = Shown::new();
    shown.phase(events, "check");
    let step = |step: StoreStep| {
        let frame = StepFrame {
            store: store.to_owned(),
            store_record: Some(record.to_record().clone()),
            step,
        };
        crate::sqlite::family::step(&home, &mut executor.borrow_mut(), &frame)
    };
    // a machine whose calls run here stages beside the Store; a worker's
    // copy comes here
    let local = match local_steps {
        true => {
            let mut name = paths::store_file(&home, store).into_os_string();
            name.push(".backup");
            std::path::PathBuf::from(name)
        }
        false => paths::store_dir(&home, store).join("backup.staging"),
    };
    stale_staging(&home, (store, &machine), &local, &step);
    let mut staged_any = false;
    let result = (|| {
        check(driver.as_ref(), &object, expected, &home_id)?;
        intent::advance(&home, store, &mut op, "snapshot")?;
        shown.phase(events, "snapshot");
        staged_any = true;
        let staged = step(StoreStep::Stage)?;
        if !local_steps {
            intent::advance(&home, store, &mut op, "send")?;
            let total = staged.get_int("bytes").map(|n| n as u64);
            shown.bytes(events, "send", 0, total);
            fetch_staged(&step, &local, &mut |done| {
                shown.bytes(events, "send", done, total)
            })?;
            let (_, sha256) = pieces::sha256_file(&local, None).map_err(remote_error)?;
            if Some(hex(&sha256).as_str()) != staged.get_text("sha256") {
                return Err(refused(
                    "Failed",
                    "the staged copy is not the snapshot the worker made",
                ));
            }
        }
        let heads = staged.get_text("heads").and_then(pieces::parse_hex32);
        let mut metadata = Metadata::from([
            (
                remote::object::SQLITE_VERSION.to_owned(),
                staged.get_text("sqlite").unwrap_or("").to_owned(),
            ),
            (remote::object::BINDING.to_owned(), hex(&identity.binding)),
            (EXPORTED_FROM.to_owned(), format!("{home_id}/{machine}")),
        ]);
        if let Some(heads) = &heads {
            metadata.insert(remote::HEADS.to_owned(), hex(heads));
        }
        // the bucket may have changed while the snapshot was made
        check(driver.as_ref(), &object, expected, &home_id)?;
        intent::advance(&home, store, &mut op, "upload")?;
        // `hash`, then `upload` with its bytes
        let moved = shown
            .transfer(events, || {
                pieces::upload(driver.as_ref(), &object, &local, &metadata)
            })
            .map_err(remote_error)?;
        Ok((moved, heads))
    })();
    if !local_steps {
        let _ = std::fs::remove_file(&local);
    }
    if staged_any {
        let _ = step(StoreStep::Unstage);
    }
    let (moved, heads) = match result {
        Ok(done) => done,
        Err(refusal) => {
            super::ended::failed(&home, store, &op, (bucket, &file, "create"), &refusal);
            return Err(refusal);
        }
    };
    let saved = StoreLink::new(&file, "create", moved.bytes, &moved.sha256, heads.as_ref());
    super::ended::finished(&home, store, &op, bucket, &saved)?;
    Ok(SdkValue::map([
        ("file", SdkValue::text(&file)),
        ("size", SdkValue::Int(moved.bytes as i64)),
        ("outcome", SdkValue::text("created")),
        ("store", SdkValue::text(store)),
        ("bucket", SdkValue::text(bucket)),
        ("object", SdkValue::text(&object)),
        ("sha256", hex_text(&moved.sha256)),
        (
            "heads_sha256",
            heads
                .as_ref()
                .map(|heads| hex_text(heads))
                .unwrap_or_default(),
        ),
    ]))
}

/// Removes what a killed create of this Store left (spec v20): the copy
/// fetched from a worker (`store/<s>/backup.staging` in the home), the
/// snapshot this create stages at `local`, and the snapshot staged beside
/// the Store on its machine (the `unstage` step). The caller holds the
/// Store's lease and its [`Creating`] lock, so no create uses any of them
/// now. One attempt each: what was removed, and a removal that failed, is
/// a line of the home's `log/sweep.jsonl`; nothing here fails the create.
fn stale_staging(home: &Path, (store, machine): (&str, &str), local: &Path, step: &Step<'_>) {
    let log = |path: &Path, outcome| home::log_swept(home, "backup_staging", path, &outcome);
    let fetched = paths::store_dir(home, store).join("backup.staging");
    for path in [fetched.as_path(), local] {
        match std::fs::remove_file(path) {
            Ok(()) => log(path, Ok(None)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => log(path, Err(error.to_string())),
        }
    }
    let staged = format!("store/{store}/store.sqlite.backup on machine {machine}");
    match step(StoreStep::Unstage) {
        Ok(out) if out.get_bool("removed") => log(Path::new(&staged), Ok(None)),
        Ok(_) => {}
        Err(refusal) => log(Path::new(&staged), Err(refusal.message)),
    }
}

/// What a killed create left of Store `store` in this process's home (on
/// the machine whose calls run here), removed by a
/// restore that replaces it (spec v20): its two staged snapshots in the
/// home, under the create lock taken without waiting. The caller holds the
/// Store's lease. A create that runs keeps them; nothing here fails the
/// restore, and each removal is a line of `log/sweep.jsonl`.
pub(crate) fn stale_here(home: &Path, store: &str) {
    let Ok(_creating) = Creating::take(home, store) else {
        return;
    };
    let mut file = paths::store_file(home, store).into_os_string();
    file.push(".backup");
    let dir = paths::store_dir(home, store);
    for path in [dir.join("backup.staging"), std::path::PathBuf::from(file)] {
        match std::fs::remove_file(&path) {
            Ok(()) => home::log_swept(home, "backup_staging", &path, &Ok(None)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => home::log_swept(home, "backup_staging", &path, &Err(error.to_string())),
        }
    }
}

/// One create of a Store at a time in a home, since each stages its snapshot
/// at the same path: an `flock` on `store/<s>/backup.lock`, taken without
/// waiting and held until the create ends (a killed create's goes with it).
/// A create that takes it removes what a killed one left
/// ([`stale_staging`]); so does a restore that replaces the Store.
pub struct Creating(std::fs::File);

impl Creating {
    /// Takes the lock without waiting; `Busy` while a create holds it.
    pub fn take(home: &Path, store: &str) -> Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(paths::store_dir(home, store).join("backup.lock"))
            .map_err(failed)?;
        if !try_lock(&file, true).map_err(failed)? {
            return Err(refused(
                "Busy",
                format!("another backup.create of Store {store} is running; nothing was changed"),
            ));
        }
        Ok(Self(file))
    }
}

impl Drop for Creating {
    fn drop(&mut self) {
        unlock(&self.0);
    }
}

/// One `store.file` step on the Store's machine.
type Step<'a> = dyn Fn(StoreStep) -> Outcome + 'a;

/// Copies the snapshot a worker staged to `local`, in pieces of [`CHUNK`],
/// telling `sent` the bytes copied after each.
fn fetch_staged(step: &Step<'_>, local: &Path, sent: &mut dyn FnMut(u64)) -> Result<()> {
    let mut out = std::fs::File::create(local).map_err(failed)?;
    let mut offset = 0u64;
    loop {
        let piece = step(StoreStep::Read {
            offset,
            len: CHUNK,
            staged: true,
        })?;
        let Some(SdkValue::Bytes(bytes)) = piece.get("bytes") else {
            return Ok(());
        };
        std::io::Write::write_all(&mut out, bytes).map_err(failed)?;
        offset += bytes.len() as u64;
        sent(offset);
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
