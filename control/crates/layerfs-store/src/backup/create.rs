//! `backup.create`: a snapshot on the Store's machine, uploaded by this process.
use super::family::{hex, save_link};
use super::progress::Shown;
use super::record::{remote_error, BucketRecord, StoreLink};
use crate::lease;
use crate::place::{self, Executor};
use crate::record::{StoreRecord, StoreRef};
use crate::sqlite::transfer::step::{StepFrame, StoreStep};
use crate::sqlite::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bucket::backup as remote;
use layerfs_bucket::bucket::{pieces, Driver, Metadata};
use layerfs_home::events::Events;
use layerfs_home::home::flock::{try_lock, unlock};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
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
/// process for `native`); this process uploads it with its sha256, heads,
/// binding and `layerfs-exported-from`. A worker's snapshot comes here in
/// pieces first: the worker never sees the key.
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
    let driver = bucket_record.driver()?;
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
    let mut shown = Shown::new();
    shown.phase(events, "check");
    check(driver.as_ref(), &object, expected, &home_id)?;
    shown.phase(events, "snapshot");
    let step = |step: StoreStep| {
        let frame = StepFrame {
            store: store.to_owned(),
            store_record: Some(record.to_record().clone()),
            step,
        };
        crate::sqlite::family::step(&home, &mut executor.borrow_mut(), &frame)
    };
    let staged = step(StoreStep::Stage)?;
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
            let total = staged.get_int("bytes").map(|n| n as u64);
            shown.bytes(events, "send", 0, total);
            fetch_staged(&step, &local, &mut |done| {
                shown.bytes(events, "send", done, total)
            })?;
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
        shown.phase(events, "check");
        check(driver.as_ref(), &object, expected, &home_id)?;
        // `hash`, then `upload` with its bytes and pieces
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
    let _ = step(StoreStep::Unstage);
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

/// One create of a Store at a time in a home, since each stages its snapshot
/// at the same path: an `flock` on `store/<s>/backup.lock`, taken without
/// waiting and held until the create ends (a killed create's goes with it).
struct Creating(std::fs::File);

impl Creating {
    fn take(home: &Path, store: &str) -> Result<Self> {
        #[cfg(unix)]
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(paths::store_dir(home, store).join("backup.lock"))
                .map_err(failed)?
        };
        // Windows: mode and O_NOFOLLOW are unix open guards; the lock file is
        // a plain zero-byte file under the home's DACL.
        #[cfg(not(unix))]
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
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
