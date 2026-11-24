//! `store.update` of a PostgreSQL Store: another saved login (`auth`), or
//! its `synchronous_commit`; one of the two is required. A new login is
//! checked once from this process, on the Store's address, before it is
//! saved: the server is reached with it and holds this Store. An address
//! change is a `store connect`, a machine's own address is removed with
//! `store disconnect --machine`.
use crate::postgresql::check::{postgresql_record, same_store};
use crate::postgresql::connect::{selection, synchronous_commit};
use crate::postgresql::login;
use crate::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{refused, with_fix, Outcome};
use std::path::Path;

/// What `store.update` changes of a Store.
#[derive(Clone, Debug)]
pub struct StoreUpdate {
    /// The PostgreSQL Store.
    pub store: StoreName,
    /// The name of its new saved login.
    pub auth: Option<String>,
    /// Its new `synchronous_commit`.
    pub synchronous_commit: Option<String>,
}

/// `record` with the request's fields in place of its own.
fn changed(record: StoreRecord, request: &StoreUpdate) -> StoreRecord {
    let fields = [
        ("auth", &request.auth),
        ("synchronous_commit", &request.synchronous_commit),
    ];
    fields
        .into_iter()
        .fold(record, |record, (field, value)| match value {
            Some(value) => record.with(field, SdkValue::text(value)),
            None => record,
        })
}

/// `store.update`: changes the Store's saved login and `synchronous_commit`.
/// Every refusal of its input comes before any connection; a new login is
/// dialled once from here on the Store's address and must read this
/// Store's id (`Unavailable` otherwise). A record keeping a URL that is not
/// an address alone (`refuse_login`: one that may hold a login, or a `#`)
/// is refused with its fix, nothing written: no update rewrites a URL. The
/// record is written under the home lock and the Store's, `Conflict` when
/// it was disconnected or connected to another database meanwhile; this
/// process's open sessions on it are closed (`InUse` while one of its calls
/// runs). The login is checked again under the lock, and the write leaves
/// no old field (spec v23 V23-6): an update to the Store's own login is its
/// `outdated` fix. A running Sandbox keeps the connection it started with.
/// Answers `{ ok, outcome: updated, store, auth, synchronous_commit }`.
pub fn update(home_dir: &Path, request: &StoreUpdate) -> Outcome {
    let name = &request.store;
    let record = postgresql_record(home_dir, name)?;
    record.unblocked()?;
    if request.auth.is_none() && request.synchronous_commit.is_none() {
        return Err(refused(
            "InvalidInput",
            "nothing to update: give --auth or --synchronous-commit (an address is `store connect`)",
        ));
    }
    if let Some(sync) = &request.synchronous_commit {
        synchronous_commit(sync)?;
    }
    if let Some(auth) = &request.auth {
        login::saved(home_dir, auth)?;
    }
    // a URL that is not an address alone is never rewritten (it may hold a
    // login): the fix, built from the record as it is, replaces it with
    // steps that dial only with the login given here
    if record.refused_urls() {
        let message = format!(
            "Store {name}'s record keeps a URL that is not an address alone (it may hold a login); nothing was written"
        );
        let (auth, sync) = (
            request.auth.as_deref(),
            request.synchronous_commit.as_deref(),
        );
        return Err(with_fix(
            refused("InvalidInput", message),
            login::update_fix(&record, auth, sync),
        ));
    }
    if request.auth.is_some() {
        let trial = changed(record.clone(), request).without_old_fields();
        same_store(&trial, &selection(&trial)?).map_err(|refusal| named(refusal, name))?;
    }
    let home_lock = home::lock::take(home_dir)?;
    let store_lock = crate::record::lock(home_dir, name)?;
    let current = StoreRecord::read(home_dir, name)?.filter(|r| r.binding() == record.binding());
    let Some(current) = current else {
        return Err(refused(
            "Conflict",
            format!("Store {name} changed while its login was checked; update it again"),
        ));
    };
    current.unblocked()?;
    // the login again under the home lock, which `store remove-auth` takes:
    // one removed while it was checked is not named (spec v23 V23-7)
    if let Some(auth) = &request.auth {
        login::saved(home_dir, auth)?;
    }
    // a URL replaced meanwhile by one that is not an address alone is
    // never rewritten either
    if current.refused_urls() {
        return Err(refused(
            "Conflict",
            format!("Store {name} changed while its login was checked; update it again"),
        ));
    }
    crate::open::close(&current.postgresql_key(home_dir, name))?;
    let current = changed(current, request).without_old_fields();
    current.write(home_dir)?;
    drop((store_lock, home_lock));
    let text = |key: &str| current.text(key).map_or(SdkValue::Null, SdkValue::text);
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("updated")),
        ("store", SdkValue::text(name.as_str())),
        ("auth", text("auth")),
        ("synchronous_commit", text("synchronous_commit")),
    ]))
}

/// `refusal` of the check, naming the Store.
fn named(refusal: SdkRefusal, name: &str) -> SdkRefusal {
    refused(
        &refusal.code,
        format!("PostgreSQL Store {name}: {}", refusal.message),
    )
}
