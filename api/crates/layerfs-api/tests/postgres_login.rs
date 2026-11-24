//! A PostgreSQL Store's URL and login, saved on this computer only (owner,
//! 2026-11-24), against a real server: a first connect checks the login
//! before anything is saved and keeps the password in the home's secret
//! folder, never in a record or an answer; a connect of the connected Store
//! changes its address, user, password or `synchronous_commit`, keeping
//! what it is not given, and is refused (nothing written) when the new
//! values reach another Store; `store.get` and `store.list` answer the
//! address, user, password times and attached machines; `store.doctor`
//! checks from this computer, then from each attached machine, and not
//! from them when this computer was not let in. Needs
//! LAYERFS_TEST_POSTGRES_URL (an admin URL of a trust-auth server, in
//! postgres:// form); each test makes and drops its own databases.
use layerfs_api::home::{self, paths};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-pglogin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn long(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    let mut args = args.to_vec();
    args.push(("detail", SdkValue::Bool(true)));
    ok(home, name, &args)
}

/// The test server's databases made here, dropped when the test ends.
struct Server {
    admin: String,
    user: String,
    /// `postgresql://host:port`, no user.
    base: String,
    made: Vec<String>,
}

impl Server {
    fn new() -> Option<Self> {
        let Ok(admin) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
            eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL unset");
            return None;
        };
        let config: postgres::Config = admin.parse().unwrap();
        let user = config.get_user().unwrap_or("postgres").to_owned();
        let host = match config.get_hosts().first() {
            Some(postgres::config::Host::Tcp(host)) => host.clone(),
            other => panic!("a TCP test server: {other:?}"),
        };
        let port = config.get_ports().first().copied().unwrap_or(5432);
        Some(Self {
            admin,
            user,
            base: format!("postgresql://{host}:{port}"),
            made: Vec::new(),
        })
    }

    /// A new empty database's address, no user and no password.
    fn database(&mut self, tag: &str) -> String {
        let name = format!("pglogin_{tag}_{}", std::process::id());
        let mut client = postgres::Client::connect(&self.admin, postgres::NoTls).unwrap();
        for sql in [
            format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"),
            format!("CREATE DATABASE {name}"),
        ] {
            client.batch_execute(&sql).unwrap();
        }
        self.made.push(name.clone());
        format!("{}/{name}?sslmode=disable", self.base)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Ok(mut client) = postgres::Client::connect(&self.admin, postgres::NoTls) {
            for name in &self.made {
                let _ =
                    client.batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"));
            }
        }
    }
}

/// Every file under `dir`, with its text.
fn texts(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            texts(&path, out);
        } else {
            out.push((
                path.clone(),
                String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into(),
            ));
        }
    }
}

/// The saved password's file of Store `store`.
fn password_file(home: &Path, store: &str) -> Option<PathBuf> {
    let dir = layerfs_api::home::secrets::dir(home).join("login");
    let found = std::fs::read_dir(dir).ok()?;
    found
        .map(|entry| entry.unwrap().path())
        .find(|path| path.file_stem().is_some_and(|stem| stem == store))
}

#[test]
fn connect_checks_and_saves_the_login_and_a_reconnect_keeps_what_it_is_not_given() {
    let Some(mut server) = Server::new() else {
        return;
    };
    let url = server.database("a");
    let other = server.database("b");
    let empty_db = server.database("c");
    let user = server.user.clone();
    let home = tempdir("connect");
    home::open(&home).unwrap();
    // a login the server refuses is never saved: here a user it does not
    // know (a trust-auth server takes any password)
    let (code, _) = call(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("user", t("layerfs_no_such_role")),
            ("password", t("first-secret")),
        ],
    )
    .unwrap_err();
    assert_ne!(code, "InvalidInput");
    assert!(!paths::store_dir(&home, "pg").exists());
    assert!(password_file(&home, "pg").is_none());
    let connected = long(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("user", t(&user)),
            ("password", t("first-secret")),
        ],
    );
    assert_eq!(
        layerfs_api::json::print(&connected),
        format!(
            r#"{{"ok":true,"projects":[],"projects_count":null,"outcome":"connected","store":"pg","engine":"postgresql","url":"{url}","user":"{user}","synchronous_commit":"local","machines":[]}}"#
        )
    );
    let file = password_file(&home, "pg").expect("the password is saved");
    // its own Store's binding in a second database: the same data cannot be
    // connected twice, and another Store's schema is never this one's
    ok(
        &home,
        "store.connect",
        &[
            ("store", t("other")),
            ("url", t(&other)),
            ("user", t(&user)),
            ("password", t("x")),
        ],
    );
    let got = long(&home, "store.get", &[("store", t("pg"))]);
    let changed_at = got.get_text("password_changed_at").unwrap().to_owned();
    assert_eq!(got.get_text("url"), Some(url.as_str()));
    assert_eq!(got.get_text("user"), Some(user.as_str()));
    assert_eq!(got.get("password_saved"), Some(&SdkValue::Bool(true)));
    assert_eq!(got.get_text("synchronous_commit"), Some("local"));
    assert_eq!(got.get("machines"), Some(&SdkValue::List(vec![])));
    assert!(got.get("auth").is_none() && got.get("connections").is_none());
    // a reconnect with the setting alone keeps the user and password
    let updated = long(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("synchronous_commit", t("on")),
        ],
    );
    assert_eq!(updated.get_text("outcome"), Some("updated"));
    assert_eq!(updated.get_text("synchronous_commit"), Some("on"));
    assert_eq!(updated.get_text("user"), Some(user.as_str()));
    let got = long(&home, "store.get", &[("store", t("pg"))]);
    assert_eq!(
        got.get_text("password_changed_at"),
        Some(changed_at.as_str())
    );
    let first = std::fs::read(&file).unwrap();
    // a new password is saved
    long(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("password", t("second-secret")),
        ],
    );
    assert_ne!(std::fs::read(&file).unwrap(), first);
    // (`password_changed_at` is whole seconds: the file says it changed)
    let got = long(&home, "store.get", &[("store", t("pg"))]);
    assert!(got.get_text("password_changed_at").is_some(), "{got:?}");
    // new values that reach another Store, or no Store at all, are refused
    // and nothing is written
    let record = || std::fs::read(paths::store_record(&home, "pg")).unwrap();
    let before = (record(), std::fs::read(&file).unwrap());
    for (address, why) in [(&other, "holds another Store than pg"), (&empty_db, "")] {
        let (code, message) = call(
            &home,
            "store.connect",
            &[
                ("store", t("pg")),
                ("url", t(address)),
                ("password", t("third-secret")),
            ],
        )
        .unwrap_err();
        assert_ne!(code, "InvalidInput", "{message}");
        assert!(message.contains(why), "{message}");
        assert_eq!((record(), std::fs::read(&file).unwrap()), before);
    }
    // a URL holding a login is refused on a connected Store too
    let (code, _) = call(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url.replacen("://", &format!("://{user}@"), 1))),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "InvalidInput");
    assert_eq!(record(), before.0);
    // no password outside its secret file
    let mut files = Vec::new();
    texts(&home, &mut files);
    for (path, text) in &files {
        if path.starts_with(layerfs_api::home::secrets::dir(&home)) {
            continue;
        }
        for secret in ["first-secret", "second-secret", "third-secret"] {
            assert!(!text.contains(secret), "{secret} in {}", path.display());
        }
    }
    // `store.list`: the user and the machines' names, no saved-login name
    let listed = ok(&home, "store.list", &[]);
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}");
    };
    let pg = (items.iter())
        .find(|item| item.get_text("store") == Some("pg"))
        .unwrap();
    assert_eq!(pg.get_text("user"), Some(user.as_str()), "{pg:?}");
    assert!(pg.get("auth").is_none(), "{pg:?}");
    // disconnect forgets the password; the data stays for the next connect
    ok(&home, "store.disconnect", &[("store", t("pg"))]);
    assert!(!file.exists());
    let again = long(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("user", t(&user)),
            ("password", t("x")),
        ],
    );
    assert_eq!(again.get_text("outcome"), Some("connected"));
    std::fs::remove_dir_all(&home).unwrap();
}

/// The doctor checks from this computer, then from each attached machine
/// (here a stopped lima machine, which waits), or from `--machine` alone;
/// when this computer is not let in, no machine is checked. `store.get`
/// answers each attached machine as a row with its reach.
#[cfg(feature = "machine")]
#[test]
fn the_doctor_checks_attached_machines_only_after_this_computer() {
    let Some(mut server) = Server::new() else {
        return;
    };
    let url = server.database("doctor");
    let user = server.user.clone();
    let home = tempdir("doctor");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    ok(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("user", t(&user)),
            ("password", t("pw")),
        ],
    );
    for name in ["dev", "lab"] {
        home::write_record(
            &paths::machine_record(&home, name),
            &SdkValue::map([
                ("name", t(name)),
                ("platform", t("lima")),
                ("vm_name", t(&format!("layerfs-test-{name}"))),
            ]),
        )
        .unwrap();
    }
    // attached as `store.attach` records them
    let path = paths::store_record(&home, "pg");
    let record = home::read_record(&path).unwrap().unwrap();
    let attached = SdkValue::List(vec![t("dev"), t("lab")]);
    home::write_record(&path, &home::with_field(record, "attached", attached)).unwrap();
    // the machines `store.connection` was checked from (the tree's rows)
    let froms = |report: &SdkValue| {
        let checks = report.get("tree").and_then(|tree| tree.get("checks"));
        let Some(SdkValue::List(rows)) = checks else {
            panic!("{report:?}");
        };
        let from = rows
            .iter()
            .filter(|row| row.get_text("check") == Some("store.connection"));
        let mut found: Vec<String> = from
            .filter_map(|row| row.get_text("from").map(str::to_owned))
            .collect();
        found.sort();
        found
    };
    let report = long(&home, "store.doctor", &[("store", t("pg"))]);
    assert_eq!(froms(&report), ["dev", "lab"], "{report:?}");
    let report = long(
        &home,
        "store.doctor",
        &[("store", t("pg")), ("machine", t("lab"))],
    );
    assert_eq!(froms(&report), ["lab"], "{report:?}");
    // `native` is this computer, checked anyway
    let report = long(
        &home,
        "store.doctor",
        &[("store", t("pg")), ("machine", t("native"))],
    );
    assert_eq!(froms(&report), Vec::<String>::new(), "{report:?}");
    let got = long(&home, "store.get", &[("store", t("pg"))]);
    let reach = got.get("reach").unwrap();
    assert_eq!(reach.get("ok"), Some(&SdkValue::Bool(true)), "{got:?}");
    assert!(reach.get_int("latency_ms").is_some(), "{got:?}");
    let Some(SdkValue::List(rows)) = got.get("machines") else {
        panic!("{got:?}");
    };
    let names: Vec<_> = rows.iter().map(|row| row.get_text("machine")).collect();
    assert_eq!(names, [Some("dev"), Some("lab")]);
    assert!(
        rows.iter().all(|row| row.get("reach").is_some()),
        "{rows:?}"
    );
    let listed = ok(&home, "store.list", &[("detail", SdkValue::Bool(true))]);
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}");
    };
    assert_eq!(
        items[0].get("machines"),
        Some(&SdkValue::List(vec![t("dev"), t("lab")]))
    );
    // this computer not let in (its password forgotten): no machine is
    // checked, since each would only repeat it
    std::fs::remove_file(password_file(&home, "pg").unwrap()).unwrap();
    let report = long(&home, "store.doctor", &[("store", t("pg"))]);
    assert_eq!(froms(&report), Vec::<String>::new(), "{report:?}");
    std::fs::remove_dir_all(&home).unwrap();
}
