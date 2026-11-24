//! Saved PostgreSQL logins (call surface 2.0, `pg.html`): a user and a
//! password with no address, saved once in the home's secret folder as
//! secret kind `auth` and named by any number of Stores (a Store record's
//! `auth`). `store.save_auth`, `store.auths` and `store.remove_auth` are
//! here, with [`dial`], which puts a Store's login into the address it
//! dials, and [`auth_of`], the login a Store names. A password is never
//! answered, logged or written into a record.
use super::address::{self, refuse_login, with_login};
use crate::record::{StoreKind, StoreRecord};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers::{map_of, shown_names};
use layerfs_home::home;
use layerfs_home::home::secrets::{read_secret, remove_secret, save_secret, secret_names};
use layerfs_home::outcome::{refused, shell_word, with_fix, with_next, Outcome};
use layerfs_home::state::{Reason, State};
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
    // a URL that is not an address alone (an older record's) is never
    // dialled: its fix writes or connects one
    refuse_login(url).map_err(|refusal| {
        let message = format!(
            "PostgreSQL Store {}'s URL: {}",
            record.name(),
            refusal.message
        );
        with_fix(refused(&refusal.code, message), fix_of(record))
    })?;
    let login = read(home, auth)?;
    with_login(url, &login.user, login.password())
}

/// The fix of PostgreSQL Store `record` whose record alone makes it
/// `broken` ([`crate::state::postgresql`]): [`no_auth_fix`] when it names
/// no saved login, [`missing_fix`] when the home does not hold the one it
/// names, else [`outdated_fix`].
pub fn fix_of(record: &StoreRecord) -> String {
    match record.text("auth") {
        None => no_auth_fix(record),
        Some(auth) if record.home().is_some_and(|home| is_missing(home, auth)) => {
            missing_fix(record, auth)
        }
        Some(auth) => outdated_fix(record, auth),
    }
}

/// The steps that leave PostgreSQL Store `record` naming login `auth` (a
/// name, or `<auth>`) with every URL an address alone: the login saved
/// first when `save`; each machine's own address with no clean form
/// ([`address::clean`]) replaced by the address alone (`store connect
/// --machine`, `<url>` with an `@` or `#` in it written `%40` or `%23`);
/// then the login named with `store update --auth`, which writes every URL
/// clean. A Store address with no clean form is never rewritten: the Store
/// is connected again with the address alone, after a `store disconnect`
/// when the record names no login, as a connected Store's connect takes no
/// `--auth`.
fn fix(record: &StoreRecord, save: bool, auth: &str) -> String {
    let store = shell_word(record.name());
    let unclean = |url: &str| address::clean(url).is_none();
    let named = record.text("auth").is_some();
    let address = record.text("url").is_some_and(unclean);
    let mut steps = Vec::new();
    if save {
        let user = fix_user(record);
        steps.push(format!(
            "layerfs store save-auth {auth} --user {user} --password-stdin"
        ));
    }
    // A machine's own address is connected again once a login is named.
    // With none, `store connect --machine` cannot dial: the address goes
    // first (`store disconnect --machine`, or with the whole Store), the
    // login is named, and the address alone is connected last, so the
    // machine does not stay on the Store's address.
    let mut last = Vec::new();
    for (machine, own) in record.own_connections() {
        let Some(url) = own.url.as_deref() else {
            continue;
        };
        let on = shell_word(&machine);
        let connect = format!("layerfs store connect {store} <url> --machine {on}");
        match (named, address, unclean(url)) {
            (true, _, true) => steps.push(connect),
            (false, false, true) => {
                steps.push(format!("layerfs store disconnect {store} --machine {on}"));
                last.push(connect);
            }
            (false, true, _) => last.push(connect),
            (_, _, false) => {}
        }
    }
    steps.push(match (address, named) {
        (false, _) => format!("layerfs store update {store} --auth {auth}"),
        (true, true) => format!("layerfs store connect {store} <url>"),
        (true, false) => format!(
            "layerfs store disconnect {store} && layerfs store connect {store} <url> --auth {auth}"
        ),
    });
    steps.extend(last);
    steps.join(" && ")
}

/// The fix of PostgreSQL Store `record` naming no saved login: save one,
/// then name it ([`fix`]). `--user` is the user its URL names when it
/// names one (a record from before saved logins), shell-quoted, else
/// `<user>`.
pub fn no_auth_fix(record: &StoreRecord) -> String {
    fix(record, true, "<auth>")
}

/// The fix of PostgreSQL Store `record` naming login `auth` whose record
/// holds what 2.0 no longer writes (spec v23 V23-6): name it again, which
/// writes the record clean ([`fix`]).
pub fn outdated_fix(record: &StoreRecord, auth: &str) -> String {
    fix(record, false, &shell_word(auth))
}

/// Whether `home` holds no saved login `auth` (its file is absent; one
/// that cannot be read is not missing, and `store auths` shows it broken).
pub fn is_missing(home: &Path, auth: &str) -> bool {
    matches!(read_secret(home, KIND, auth), Ok(None))
}

/// The fix of PostgreSQL Store `record` naming login `auth` that is not
/// saved (spec v23 V23-7): save it under that name, then update the Store
/// to it, which also rewrites any old field. `--user` as [`no_auth_fix`]
/// chooses it.
pub fn missing_fix(record: &StoreRecord, auth: &str) -> String {
    fix(record, true, &shell_word(auth))
}

/// The `--user` a fix names: the user the client reads in the record's
/// URL (one written before saved logins) when it has a clean form
/// ([`address::url_user`]), shell-quoted, else `<user>`.
fn fix_user(record: &StoreRecord) -> String {
    let user = record.text("url").and_then(address::url_user);
    user.map_or_else(|| "<user>".to_owned(), |user| shell_word(&user))
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
    // a login file that cannot be read is written again: that is the fix its
    // broken `auths` row gives (V23-8), so its old fields are not kept
    let before = read_secret(home_dir, KIND, auth).unwrap_or(None);
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

/// `store.auths()`: every saved login, `{ items: [{ auth, user, state,
/// reason, fix, stores, stores_count, created_at, updated_at }] }`, never a
/// password. A login file that cannot be read is `broken` `failed` with the
/// save that rewrites it (spec v23 V23-8), not a failed list.
pub fn list(home_dir: &Path) -> Outcome {
    let mut items = Vec::new();
    for auth in secret_names(home_dir, KIND)? {
        let stores = stores_using(home_dir, &auth)?;
        let at = |ms: Option<i64>| ms.map_or(SdkValue::Null, |ms| SdkValue::Text(iso(ms)));
        // a login file that cannot be read is its own broken row, with the
        // save that writes it again; the others are listed as usual
        let (login, state) = match read(home_dir, &auth) {
            Ok(login) => (Some(login), State::Ready),
            Err(_) => {
                let fix = format!(
                    "layerfs store save-auth {} --user <user> --password-stdin",
                    shell_word(&auth)
                );
                (None, State::broken(Reason::Failed, Some(&fix)))
            }
        };
        let mut fields = vec![
            ("auth", SdkValue::text(&auth)),
            (
                "user",
                SdkValue::opt_text(login.as_ref().map(|l| l.user.clone())),
            ),
        ];
        fields.extend(state.fields());
        fields.extend(shown(&stores));
        fields.push(("created_at", at(login.as_ref().and_then(|l| l.created_ms))));
        fields.push(("updated_at", at(login.as_ref().and_then(|l| l.updated_ms))));
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
