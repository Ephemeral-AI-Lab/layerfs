//! A PostgreSQL Store's login (owner, 2026-11-24): the host is the one
//! place a Store's URL and login are saved, and every attached machine
//! dials with them. The user is the Store record's `user`; the password is
//! kept in the home's secret folder as secret kind `login` under the
//! Store's name (`secret/login/<store>.json`, `{ password, changed }`),
//! never answered, logged or written into a record. [`dial`] puts both
//! into the address a call dials, only when it dials.
use super::address::with_login;
use crate::record::StoreRecord;
use layerfs_home::home::secrets::{read_secret, remove_secret, save_secret};
use layerfs_home::outcome::{refused, shell_word, with_fix};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The secret kind a Store's password is kept as.
pub const KIND: &str = "login";

/// A Store's saved password. No `Debug`: it holds the password.
pub struct Saved {
    password: String,
    /// When it was last saved, ms since 1970.
    pub changed_ms: Option<i64>,
}

impl Drop for Saved {
    fn drop(&mut self) {
        layerfs_home::wipe::text(std::mem::take(&mut self.password));
    }
}

impl Saved {
    /// The password, for the URL a call dials and nothing else.
    pub fn password(&self) -> &str {
        &self.password
    }
}

/// Store `store`'s saved password, or `None` when none is saved; a file
/// holding no password reads as none.
pub fn read(home: &Path, store: &str) -> Result<Option<Saved>> {
    let Some(saved) = read_secret(home, KIND, store)? else {
        return Ok(None);
    };
    let password = saved.get_text("password").filter(|p| !p.is_empty());
    Ok(password.map(|password| Saved {
        password: password.to_owned(),
        changed_ms: saved.get_int("changed"),
    }))
}

/// Saves `password` as Store `store`'s, replacing the one there, the
/// caller holding the home lock.
pub fn save(home: &Path, store: &str, password: &str) -> Result<()> {
    let value = SdkValue::map([
        ("password", SdkValue::text(password)),
        ("changed", SdkValue::Int(layerfs_home::home::now_ms())),
    ]);
    let saved = save_secret(home, KIND, store, &value);
    layerfs_home::wipe::value(value);
    saved
}

/// Store `store`'s saved login as kept, whole, to put back with
/// [`put_back`]; `None` when there is none. No `Debug` reaches it.
pub(crate) fn kept(home: &Path, store: &str) -> Result<Option<SdkValue>> {
    read_secret(home, KIND, store)
}

/// Puts back what [`kept`] read: that login, or none.
pub(crate) fn put_back(home: &Path, store: &str, kept: Option<&SdkValue>) -> Result<()> {
    match kept {
        Some(value) => save_secret(home, KIND, store, value),
        None => forget(home, store),
    }
}

/// Forgets Store `store`'s saved password, if any.
pub fn forget(home: &Path, store: &str) -> Result<()> {
    remove_secret(home, KIND, store).map(|_| ())
}

/// The fix of a PostgreSQL Store whose password this home does not hold:
/// connect it again with the password, its user kept. Like every fix it
/// names the address `<url>`, never text of a URL (`store get` shows it).
pub fn password_fix(record: &StoreRecord) -> String {
    format!(
        "layerfs store connect {} <url> --password-stdin",
        shell_word(record.name())
    )
}

/// `url`, an address of PostgreSQL Store `record`, with the Store's user
/// and saved password put in now, read from the home the record was read
/// from: refused when the record names no user, when the password is not
/// saved ([`password_fix`]), or when `url` is not an address alone
/// (`with_login`).
pub fn dial(record: &StoreRecord, url: &str) -> Result<String> {
    let user = crate::record::required(record.text("user").map(str::to_owned), "user")?;
    with_login(url, &user, saved(record)?.password())
}

/// Store `record`'s saved password, read from the home the record was read
/// from; refused as [`dial`] refuses when the home holds none.
pub(crate) fn saved(record: &StoreRecord) -> Result<Saved> {
    let name = record.name();
    let home = record.home().ok_or_else(|| {
        refused(
            "Failed",
            format!("PostgreSQL Store {name}'s record was not read from a home; its login cannot be read"),
        )
    })?;
    let Some(saved) = read(home, name)? else {
        let message = format!("PostgreSQL Store {name}'s password is not saved in this home");
        return Err(with_fix(
            refused("InvalidInput", message),
            password_fix(record),
        ));
    };
    Ok(saved)
}

/// The record's own address with its user and `password`, a login a caller
/// resolved and sent with one request (#819 P-B): what a host worker dials.
/// The worker reads no password itself.
pub(crate) fn dial_sent(record: &StoreRecord, password: &str) -> Result<String> {
    let url = crate::record::required(record.text("url").map(str::to_owned), "url")?;
    let user = crate::record::required(record.text("user").map(str::to_owned), "user")?;
    with_login(&url, &user, password)
}
