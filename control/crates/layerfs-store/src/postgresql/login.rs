//! Saved PostgreSQL logins (call surface 2.0, `pg.html`): a user and a
//! password with no address, saved once in the home's secret folder as
//! secret kind `auth` and named by any number of Stores (a Store record's
//! `auth`). `store.save_auth`, `store.auths` and `store.remove_auth` are
//! here, with [`dial`], which puts a Store's login into the address it
//! dials, and [`auth_of`], the login a Store names. A password is never
//! answered, logged or written into a record.
use super::address::{refuse_login, with_login};
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

/// The first step of a fix that saves a login: its name and user to fill
/// in (no user is ever read from a URL).
pub const SAVE: &str = "layerfs store save-auth <auth> --user <user> --password-stdin";

/// The fix of PostgreSQL Store `record` whose record alone makes it
/// `broken` ([`crate::state::postgresql`]): [`no_auth_fix`] when it names
/// no saved login, [`missing_fix`] when the home does not hold the one it
/// names, else [`outdated_fix`].
pub fn fix_of(record: &StoreRecord) -> String {
    fix_with(record, None)
}

/// [`fix_of`] with the `synchronous_commit` a refused `store update` was
/// given, if any.
fn fix_with(record: &StoreRecord, sync: Option<&str>) -> String {
    let steps = match record.text("auth") {
        None => steps(record, true, "<auth>", sync),
        Some(auth) if record.home().is_some_and(|home| is_missing(home, auth)) => {
            steps(record, true, &shell_word(auth), sync)
        }
        Some(auth) => steps(record, false, &outdated_auth(record, auth), sync),
    };
    steps.join(" && ")
}

/// The steps that leave PostgreSQL Store `record` naming login `auth` (a
/// name, or `<auth>`) with every URL an address alone, the login saved
/// first when `save` (`<user>` to fill in). Every step dials only with
/// `auth`, never with the login the record names, so the steps work when
/// that one no longer authenticates; a URL that is not an address alone
/// (`refuse_login`) is never rewritten, only replaced by a connect of the
/// address alone (`<url>`). When the Store's own address is one, the Store
/// is disconnected (refused `InUse` while a Sandbox of it exists; its
/// data stays) and connected again with `--auth`, keeping its
/// `synchronous_commit` (`sync` when given, else the recorded one), then
/// every machine's own address is connected again. Otherwise each such
/// machine's own address is disconnected first (`store disconnect
/// --machine` dials nothing), the login is named with `store update
/// --auth` (with `sync` when given), and the machine's address alone is
/// connected last: `store connect --machine` checks it from that machine,
/// which must be running and run PostgreSQL Sandboxes (review 57d0aea5ad
/// N2).
fn steps(record: &StoreRecord, save: bool, auth: &str, sync: Option<&str>) -> Vec<String> {
    let store = shell_word(record.name());
    let refused = |url: &str| refuse_login(url).is_err();
    let flag = |sync: Option<&str>| {
        sync.map_or(String::new(), |sync| {
            format!(" --synchronous-commit {}", shell_word(sync))
        })
    };
    let connect_on = |machine: &str| {
        let on = shell_word(machine);
        format!("layerfs store connect {store} <url> --machine {on}")
    };
    let mut steps = Vec::new();
    if save {
        steps.push(format!(
            "layerfs store save-auth {auth} --user <user> --password-stdin"
        ));
    }
    let own = record.own_connections();
    if record.text("url").is_some_and(refused) {
        let sync = flag(sync.or(record.text("synchronous_commit")));
        steps.push(format!("layerfs store disconnect {store}"));
        steps.push(format!(
            "layerfs store connect {store} <url> --auth {auth}{sync}"
        ));
        steps.extend(own.iter().map(|(machine, _)| connect_on(machine)));
        return steps;
    }
    let held: Vec<&String> = (own.iter())
        .filter(|(_, own)| own.url.as_deref().is_some_and(refused))
        .map(|(machine, _)| machine)
        .collect();
    steps.extend(held.iter().map(|machine| {
        let on = shell_word(machine);
        format!("layerfs store disconnect {store} --machine {on}")
    }));
    steps.push(format!(
        "layerfs store update {store} --auth {auth}{}",
        flag(sync)
    ));
    steps.extend(held.iter().map(|machine| connect_on(machine)));
    steps
}

/// The login an `outdated` fix names: the record's own, `auth`, unless an
/// address is replaced, whose connect then takes `<auth>` (the record's
/// own login or another that authenticates; `store get` names the
/// record's).
fn outdated_auth(record: &StoreRecord, auth: &str) -> String {
    match record.refused_urls() {
        true => "<auth>".to_owned(),
        false => shell_word(auth),
    }
}

/// The fix of `store update <store> --auth <auth>` (or `--synchronous-commit
/// <sync>` alone) refused because PostgreSQL Store `record` (as it is,
/// not as the update would leave it) keeps a URL that is not an address
/// alone: the [`steps`] that name `auth` (the record's own fix when none
/// is given), each dialling only with `auth`, the given `sync` carried
/// (review 57d0aea5ad S1, e0aa26e149 S3 and N1).
pub fn update_fix(record: &StoreRecord, auth: Option<&str>, sync: Option<&str>) -> String {
    match auth {
        Some(auth) => steps(record, false, &shell_word(auth), sync).join(" && "),
        None => fix_with(record, sync),
    }
}

/// The fix of a refused `store connect` (`connect`, its address `<url>`)
/// of URL holding a login on connected PostgreSQL Store `record`: the
/// login saved, the [`steps`] that name it, then `connect` unless a step
/// already connects that address.
pub fn login_fix(record: &StoreRecord, connect: &str) -> String {
    let mut steps = steps(record, true, "<auth>", None);
    let with_auth = format!("{connect} --auth ");
    if !(steps.iter()).any(|step| step == connect || step.starts_with(&with_auth)) {
        steps.push(connect.to_owned());
    }
    steps.join(" && ")
}

/// The fix of PostgreSQL Store `record` naming no saved login: save one,
/// then name it ([`steps`]). `--user` is `<user>`.
pub fn no_auth_fix(record: &StoreRecord) -> String {
    steps(record, true, "<auth>", None).join(" && ")
}

/// The fix of PostgreSQL Store `record` naming login `auth` whose record
/// holds what 2.0 no longer writes (spec v23 V23-6): name it again, which
/// drops the old fields, after replacing any URL that is not an address
/// alone, whose connect names `<auth>` ([`steps`]).
pub fn outdated_fix(record: &StoreRecord, auth: &str) -> String {
    steps(record, false, &outdated_auth(record, auth), None).join(" && ")
}

/// Whether `home` holds no saved login `auth` (its file is absent; one
/// that cannot be read is not missing, and `store auths` shows it broken).
pub fn is_missing(home: &Path, auth: &str) -> bool {
    matches!(read_secret(home, KIND, auth), Ok(None))
}

/// The fix of PostgreSQL Store `record` naming login `auth` that is not
/// saved (spec v23 V23-7): save it under that name, then name it
/// ([`steps`]). `--user` is `<user>`.
pub fn missing_fix(record: &StoreRecord, auth: &str) -> String {
    steps(record, true, &shell_word(auth), None).join(" && ")
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
