//! `store.update` of a PostgreSQL Store's own connection: its URL (no
//! password), its password's reference and `synchronous_commit`. A new URL
//! or password is checked from this process first (the server is reached
//! and holds this Store), as its Store calls will dial it. A machine's own
//! connection is `layerfs-control`'s part of the call, checked from that
//! machine.
use crate::postgresql::check::{postgresql_record, same_store};
use crate::postgresql::connect::{
    refuse_password, selection, synchronous_commit, target, unclaimed_by_others,
};
use crate::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_home::secrets::resolve;
use std::path::Path;

/// What `store.update` changes of a Store's own connection. No `Debug`: it
/// names a password reference.
#[derive(Clone)]
pub struct StoreUpdate {
    /// The PostgreSQL Store.
    pub name: StoreName,
    /// Its new URL, without a password.
    pub url: Option<String>,
    /// Its password's new reference.
    pub password: Option<String>,
    /// Its new `synchronous_commit`.
    pub synchronous_commit: Option<String>,
}

/// `record` with the request's fields in place of its own.
fn changed(record: StoreRecord, request: &StoreUpdate) -> StoreRecord {
    let fields = [
        ("url", &request.url),
        ("password", &request.password),
        ("synchronous_commit", &request.synchronous_commit),
    ];
    fields
        .into_iter()
        .fold(record, |record, (field, value)| match value {
            Some(value) => record.with(field, SdkValue::text(value)),
            None => record,
        })
}

/// `store.update` without a machine: changes the Store's URL, password
/// reference and `synchronous_commit`. Every refusal of its input comes
/// before any connection; a new URL or password is dialled once from here
/// and must reach this Store (`Unavailable` otherwise, `Exists` when the
/// URL names a database another Store of the home connects). The record is
/// written under the home lock and the Store's, `Conflict` when it was
/// disconnected or connected to another database meanwhile; this
/// process's open sessions on it are closed (`InUse` while one of its
/// calls runs). A running Sandbox keeps the connection it started with.
pub fn update(home_dir: &Path, request: &StoreUpdate) -> Outcome {
    let name = &request.name;
    let record = postgresql_record(home_dir, name)?;
    record.unblocked()?;
    if request.url.is_none() && request.password.is_none() && request.synchronous_commit.is_none() {
        return Err(refused(
            "InvalidInput",
            "nothing to update: give --url, --password or --synchronous-commit",
        ));
    }
    if let Some(url) = &request.url {
        refuse_password(url)?;
    }
    if let Some(sync) = &request.synchronous_commit {
        synchronous_commit(sync)?;
    }
    if let Some(password) = &request.password {
        resolve(password)?;
    }
    let dialled = request.url.is_some() || request.password.is_some();
    if dialled {
        let trial = changed(record.clone(), request);
        let probe = selection(&trial)?;
        if let Some(url) = &request.url {
            unclaimed_by_others(home_dir, name, &target(url)?, record.binding())?;
        }
        same_store(&trial, &probe).map_err(|refusal| named(refusal, name))?;
    }
    let home_lock = home::lock::take(home_dir)?;
    let store_lock = crate::record::lock(home_dir, name)?;
    let current = StoreRecord::read(home_dir, name)?.filter(|r| r.binding() == record.binding());
    let Some(current) = current else {
        return Err(refused(
            "Conflict",
            format!("Store {name} changed while its connection was checked; update it again"),
        ));
    };
    current.unblocked()?;
    crate::open::close(&current.postgresql_key(home_dir, name))?;
    changed(current, request).write(home_dir)?;
    drop((store_lock, home_lock));
    crate::sqlite::family::get(home_dir, name)
}

/// `refusal` of the check, naming the Store.
fn named(refusal: SdkRefusal, name: &str) -> SdkRefusal {
    refused(
        &refusal.code,
        format!("PostgreSQL Store {name}: {}", refusal.message),
    )
}
