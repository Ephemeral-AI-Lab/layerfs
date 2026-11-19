//! Saved PostgreSQL logins (call surface 2.0, `pg.html`): a user and a
//! password with no address, saved once in the home's secret folder as
//! secret kind `auth` and named by any number of Stores (a Store record's
//! `auth`). `store.save_auth`, `store.auths` and `store.remove_auth` are
//! here, with [`dial`], which puts a Store's login into the address it
//! dials, and [`auth_of`], the login a Store names. A password is never
//! answered, logged or written into a record.
use crate::record::{StoreKind, StoreRecord};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers::{map_of, shown_names};
use layerfs_home::home;
use layerfs_home::home::secrets::{read_secret, remove_secret, save_secret, secret_names};
use layerfs_home::outcome::{refused, shell_word, with_fix, with_next, Outcome};
use layerfs_home::time::iso;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The secret kind a saved login is kept as: `secret/auth/<auth>.json`.
pub const KIND: &str = "auth";

/// One saved login. No `Debug`: it holds a password.
pub struct Login {
    /// The PostgreSQL role.
    pub user: String,
    password: String,
    created_ms: Option<i64>,
    updated_ms: Option<i64>,
}

impl Login {
    /// The password, for the URL a call dials and nothing else.
    pub fn password(&self) -> &str {
        &self.password
    }
}

/// The saved login `auth`, `NotFound` naming the call that saves it.
pub fn read(home: &Path, auth: &str) -> Result<Login> {
    let saved = read_secret(home, KIND, auth)?.ok_or_else(|| {
        refused(
            "NotFound",
            format!(
                "no saved login named {auth}; `layerfs store save-auth {auth} --user <user> --password-stdin` saves one"
            ),
        )
    })?;
    let text = |key: &str| saved.get_text(key).map(str::to_owned);
    let (Some(user), Some(password)) = (text("user"), text("password")) else {
        return Err(refused(
            "Failed",
            format!("the saved login {auth} has no user or no password; save it again"),
        ));
    };
    Ok(Login {
        user,
        password,
        created_ms: saved.get_int("created"),
        updated_ms: saved.get_int("updated"),
    })
}

/// The saved login PostgreSQL Store `record` names (its `auth`), if any:
/// what `store.get` and each `store.list` item answer as `auth`.
pub fn auth_of(record: &StoreRecord) -> Option<&str> {
    match record.kind() {
        Ok(StoreKind::Postgresql) => record.text("auth"),
        _ => None,
    }
}

/// `url`, an address of PostgreSQL Store `record`, with the Store's saved
/// login put in now, read from the home the record was read from. A record
/// naming no login (one connected before saved logins) is refused with the
/// fix that saves one and names it.
pub fn dial(record: &StoreRecord, url: &str) -> Result<String> {
    let Some(auth) = record.text("auth") else {
        let message = format!("PostgreSQL Store {} names no saved login", record.name());
        return Err(with_fix(
            refused("InvalidInput", message),
            no_auth_fix(record),
        ));
    };
    let home = record.home().ok_or_else(|| {
        refused(
            "Failed",
            format!(
                "PostgreSQL Store {}'s record was not read from a home; its login {auth} cannot be read",
                record.name()
            ),
        )
    })?;
    let login = read(home, auth)?;
    layerfs_home::secrets::with_login(url, &login.user, login.password())
}

/// The fix of PostgreSQL Store `record` naming no saved login: save one,
/// then name it. `--user` is the user its URL names when it names one (a
/// record from before saved logins), shell-quoted, else `<user>`.
pub fn no_auth_fix(record: &StoreRecord) -> String {
    let user = record.text("url").and_then(super::connect::url_user);
    let user = user.map_or_else(|| "<user>".to_owned(), |user| shell_word(&user));
    format!(
        "layerfs store save-auth <auth> --user {user} --password-stdin && layerfs store update {} --auth <auth>",
        shell_word(record.name())
    )
}

/// The PostgreSQL Stores whose record names login `auth`, by name.
pub fn stores_using(home: &Path, auth: &str) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for record in crate::record::stores(home)? {
        if auth_of(&record) == Some(auth) {
            found.push(record.name().to_owned());
        }
    }
    Ok(found)
}

/// `stores` as an answer shows them: the first 20 names, and the count
/// only when there were more.
fn shown(stores: &[String]) -> [(&'static str, SdkValue); 2] {
    let (names, count) = shown_names(stores);
    [("stores", names), ("stores_count", count)]
}

/// `store.save_auth(auth, { user, password })`: saves the login in the
/// home's secret folder; saving again under the same name rotates it, and
/// every Store using it dials with the new one from its next connection.
/// Answers `{ ok, outcome: saved | rotated, auth, user, stores,
/// stores_count }`, never the password. Under the home lock, so a removal
/// never races it.
pub fn save(home_dir: &Path, auth: &str, user: &str, password: &str) -> Outcome {
    layerfs_home::names::name(KIND, auth).map_err(|why| refused("InvalidInput", why))?;
    if user.is_empty() {
        return Err(refused("InvalidInput", "--user is required"));
    }
    if password.is_empty() {
        return Err(refused(
            "InvalidInput",
            "the password is empty; give it on stdin with --password-stdin",
        ));
    }
    let _lock = home::lock::take(home_dir)?;
    let now = home::now_ms();
    let before = read_secret(home_dir, KIND, auth)?;
    let created = before
        .as_ref()
        .and_then(|saved| saved.get_int("created"))
        .unwrap_or(now);
    let value = SdkValue::map([
        ("user", SdkValue::text(user)),
        ("password", SdkValue::text(password)),
        ("created", SdkValue::Int(created)),
        ("updated", SdkValue::Int(now)),
    ]);
    save_secret(home_dir, KIND, auth, &value)?;
    let stores = stores_using(home_dir, auth)?;
    let outcome = if before.is_some() { "rotated" } else { "saved" };
    let mut fields = vec![
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text(outcome)),
        ("auth", SdkValue::text(auth)),
        ("user", SdkValue::text(user)),
    ];
    fields.extend(shown(&stores));
    Ok(map_of(fields))
}

/// `store.auths()`: every saved login, `{ items: [{ auth, user, stores,
/// stores_count, created_at, updated_at }] }`, never a password.
pub fn list(home_dir: &Path) -> Outcome {
    let mut items = Vec::new();
    for auth in secret_names(home_dir, KIND)? {
        let login = read(home_dir, &auth)?;
        let stores = stores_using(home_dir, &auth)?;
        let at = |ms: Option<i64>| ms.map_or(SdkValue::Null, |ms| SdkValue::Text(iso(ms)));
        let mut fields = vec![
            ("auth", SdkValue::text(&auth)),
            ("user", SdkValue::text(&login.user)),
        ];
        fields.extend(shown(&stores));
        fields.push(("created_at", at(login.created_ms)));
        fields.push(("updated_at", at(login.updated_ms)));
        items.push(map_of(fields));
    }
    Ok(SdkValue::map([("items", SdkValue::List(items))]))
}

/// `store.remove_auth(auth)`: removes a saved login no Store names.
/// Refused `InUse` while a Store names it, listing those Stores (`stores`),
/// and `NotFound` when there is none. Under the home lock, which a first
/// connect also takes to write the record that names it.
pub fn remove(home_dir: &Path, auth: &str) -> Outcome {
    layerfs_home::names::name(KIND, auth).map_err(|why| refused("InvalidInput", why))?;
    let _lock = home::lock::take(home_dir)?;
    let stores = stores_using(home_dir, auth)?;
    if !stores.is_empty() {
        let refusal = refused(
            "InUse",
            format!(
                "the login {auth} is used by {}: `layerfs store update <store> --auth <other>` or `layerfs store disconnect <store>` first",
                stores.join(", ")
            ),
        );
        let names = stores.iter().map(SdkValue::text).collect();
        return Err(with_next(refusal, "stores", SdkValue::List(names)));
    }
    if !remove_secret(home_dir, KIND, auth)? {
        return Err(refused("NotFound", format!("no saved login named {auth}")));
    }
    Ok(map_of(vec![
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("removed")),
        ("auth", SdkValue::text(auth)),
    ]))
}

/// Refused `NotFound` unless login `auth` is saved in `home`.
pub fn saved(home_dir: &Path, auth: &str) -> Result<()> {
    layerfs_home::names::name(KIND, auth).map_err(|why| refused("InvalidInput", why))?;
    read(home_dir, auth).map(|_| ())
}
