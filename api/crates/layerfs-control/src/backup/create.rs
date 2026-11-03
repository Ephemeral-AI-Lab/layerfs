//! `backup.create`: a snapshot on the Store's machine, uploaded by this process.
use super::family::{hex, progress, save_link};
use super::record::{remote_error, BucketRecord, StoreLink};
use crate::common::context::Context;
use crate::common::home::lease;
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused, Outcome};
use crate::common::place::{self, Executor};
use crate::store::record::{StoreRecord, StoreRef};
use crate::store::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup as remote;
use layerfs_store_backup::bucket::{pieces, Driver, Metadata};
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

/// `backup.create(store, bucket, file?)`: the Store's machine stages a
/// consistent snapshot beside the Store (`store.file` `stage`, in this
/// process for `native`); this process uploads it with its sha256, heads,
/// binding and `layerfs-exported-from`. A worker's snapshot comes here in
/// pieces first: the worker never sees the key.
pub(super) fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
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
    record.unblocked()?;
    let driver = bucket_record.driver()?;
    let expected = expected(&record, bucket, &file);
    let machine = store_ref.machine()?.to_owned();
    let executor = place::plan(&home, store_ref)?;
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
