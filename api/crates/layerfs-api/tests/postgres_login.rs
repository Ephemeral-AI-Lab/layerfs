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
use layerfs_protocol::provision::install::SdkValue;
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

    /// The admin URL's start, `postgres://user@host:port` (its user and any
    /// password), in place of [`Self::base`].
    fn admin_base(&self) -> String {
        let config: postgres::Config = self.admin.parse().unwrap();
        let host = match config.get_hosts().first() {
            Some(postgres::config::Host::Tcp(host)) => host.clone(),
            other => panic!("a TCP test server: {other:?}"),
        };
        let port = config.get_ports().first().copied().unwrap_or(5432);
        format!("postgresql://{}@{host}:{port}", self.user)
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
        layerfs_home::json::print(&connected),
        format!(
            r#"{{"ok":true,"projects":[],"projects_count":null,"outcome":"connected","store":"pg","engine":"postgresql","url":"{url}","user":"{user}","synchronous_commit":"local","machines":[],"here":null}}"#
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
    // the connect's own check is this computer's reach, as the doctor's is
    let here = |got: &SdkValue| got.get("reach").cloned().unwrap_or_default();
    let reach = here(&got);
    assert_eq!(reach.get("ok"), Some(&SdkValue::Bool(true)), "{got:?}");
    assert!(reach.get_int("latency_ms").is_some(), "{got:?}");
    let first_at = reach.get_text("at").unwrap().to_owned();
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
    // an update records its check too
    let reach = here(&got);
    assert_eq!(reach.get("ok"), Some(&SdkValue::Bool(true)), "{got:?}");
    assert!(reach
        .get_text("at")
        .is_some_and(|at| at >= first_at.as_str()));
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
    let seen_file = paths::store_record(&home, "pg").with_file_name("observed.json");
    let seen = || std::fs::read(&seen_file).unwrap();
    let before = (record(), std::fs::read(&file).unwrap());
    let seen_before = seen();
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
        assert_eq!(seen(), seen_before, "a refused connect records no reach");
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
/// (here stopped lima machines, which wait, and `native`, attached like any
/// other machine and refused until it is), or from `--machine` alone;
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
    // `native` is an ordinary machine: not attached, it is refused
    let on_native = [
        ("store", t("pg")),
        ("machine", t(layerfs_home::names::HOST_MACHINE)),
    ];
    let (code, message) = call(&home, "store.doctor", &on_native).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("not attached"), "{message}");
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
    // attached like any machine, checked from it (it shares this host's
    // loopback), then checked by the doctor with the others
    let attached = long(&home, "store.attach", &on_native);
    assert_eq!(
        attached.get_text("outcome"),
        Some("attached"),
        "{attached:?}"
    );
    assert!(attached.get_int("latency_ms").is_some(), "{attached:?}");
    let again = long(&home, "store.attach", &on_native);
    assert_eq!(again.get_text("outcome"), Some("checked"), "{again:?}");
    let report = long(&home, "store.doctor", &on_native);
    assert_eq!(
        froms(&report),
        [layerfs_home::names::HOST_MACHINE],
        "{report:?}"
    );
    let report = long(&home, "store.doctor", &[("store", t("pg"))]);
    assert_eq!(
        froms(&report),
        ["dev", "lab", layerfs_home::names::HOST_MACHINE],
        "{report:?}"
    );
    let got = long(&home, "store.get", &[("store", t("pg"))]);
    let Some(SdkValue::List(rows)) = got.get("machines") else {
        panic!("{got:?}");
    };
    let native = rows
        .iter()
        .find(|row| row.get_text("machine") == Some(layerfs_home::names::HOST_MACHINE));
    assert_eq!(
        native.and_then(|row| row.get_text("reached")),
        Some("ok"),
        "{rows:?}"
    );
    let detached = long(&home, "store.detach", &on_native);
    assert_eq!(detached.get_text("outcome"), Some("detached"));
    // this computer not let in (its password forgotten): no machine is
    // checked, since each would only repeat it
    std::fs::remove_file(password_file(&home, "pg").unwrap()).unwrap();
    let report = long(&home, "store.doctor", &[("store", t("pg"))]);
    assert_eq!(froms(&report), Vec::<String>::new(), "{report:?}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// A refusal's flat field `key` (its `next`), as the call boundary
/// answers it beside `error` and `message`.
fn flat(refusal: &layerfs_protocol::provision::install::SdkRefusal, key: &str) -> SdkValue {
    let next = refusal.detail.get("next").cloned().unwrap_or_default();
    next.get(key).cloned().unwrap_or_default()
}

/// The full refusal of a call.
fn refusal(
    home: &Path,
    name: &str,
    args: &[(&str, SdkValue)],
) -> layerfs_protocol::provision::install::SdkRefusal {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    match layerfs_api::call(Some(home), name, args, &mut |_| {}) {
        Ok(answer) => panic!("{name} answered {answer:?}"),
        Err(refusal) => refusal,
    }
}

/// A connect refusal names the connection step it stopped at, as the
/// doctor's `store.connection.<step>`: nothing answering is `transport`,
/// a role the server does not know `authentication`, a database it does
/// not have `database`; a dry run answers the same steps in `here` and
/// saves nothing, and a first connect's dry run of an empty database
/// says so (`empty`) without making the schema.
#[test]
fn connect_refusals_and_dry_runs_name_the_step() {
    let Some(mut server) = Server::new() else {
        return;
    };
    let url = server.database("step");
    let empty_db = server.database("stepempty");
    let user = server.user.clone();
    let home = tempdir("step");
    home::open(&home).unwrap();
    let new = |address: &str, role: &str| {
        vec![
            ("store", t("pg")),
            ("url", t(address)),
            ("user", t(role)),
            ("password", t("pw")),
        ]
    };
    let nowhere = "postgresql://127.0.0.1:1/db?sslmode=disable&connect_timeout=5";
    let missing_db = url.replace("pglogin_step_", "pglogin_nosuchdb_");
    for (address, role, step) in [
        (nowhere, user.as_str(), "transport"),
        (url.as_str(), "layerfs_no_such_role", "authentication"),
        (missing_db.as_str(), user.as_str(), "database"),
    ] {
        let refused = refusal(&home, "store.connect", &new(address, role));
        assert_eq!(flat(&refused, "step"), t(step), "{}", refused.message);
        let mut dry = new(address, role);
        dry.push(("dry_run", SdkValue::Bool(true)));
        let tested = long(&home, "store.connect", &dry);
        assert_eq!(tested.get_text("outcome"), Some("tested"), "{tested:?}");
        assert_eq!(tested.get("ok"), Some(&SdkValue::Bool(false)), "{tested:?}");
        let here = tested.get("here").unwrap();
        assert_eq!(here.get("ok"), Some(&SdkValue::Bool(false)), "{tested:?}");
        assert_eq!(here.get_text("step"), Some(step), "{tested:?}");
        assert_eq!(
            here.get_text("code"),
            Some(refused.code.as_str()),
            "{tested:?}"
        );
        assert!(!paths::store_dir(&home, "pg").exists());
    }
    // an empty database: tested, empty, and still without the schema
    let mut dry = new(&empty_db, &user);
    dry.push(("dry_run", SdkValue::Bool(true)));
    let tested = long(&home, "store.connect", &dry);
    assert_eq!(tested.get("ok"), Some(&SdkValue::Bool(true)), "{tested:?}");
    let here = tested.get("here").unwrap();
    assert_eq!(here.get("empty"), Some(&SdkValue::Bool(true)), "{tested:?}");
    assert!(here.get_int("latency_ms").is_some(), "{tested:?}");
    assert_eq!(tested.get("machines"), Some(&SdkValue::List(vec![])));
    assert!(!paths::store_dir(&home, "pg").exists());
    assert!(password_file(&home, "pg").is_none());
    let admin = empty_db.replace(&server.base, &server.admin_base());
    let mut client = postgres::Client::connect(&admin, postgres::NoTls).unwrap();
    let schemas = client
        .query_one(
            "SELECT count(*) FROM pg_namespace WHERE nspname = 'layerfs'",
            &[],
        )
        .unwrap();
    assert_eq!(schemas.get::<_, i64>(0), 0, "a dry run made the schema");
    // connected, a new address where nothing answers: refused at
    // `transport` with nothing saved; its dry run says the same
    ok(&home, "store.connect", &new(&url, &user));
    let record = || std::fs::read(paths::store_record(&home, "pg")).unwrap();
    let before = record();
    let again = [("store", t("pg")), ("url", t(nowhere))];
    let refused = refusal(&home, "store.connect", &again);
    assert_eq!(refused.code, "Unavailable", "{}", refused.message);
    assert_eq!(
        flat(&refused, "step"),
        t("transport"),
        "{}",
        refused.message
    );
    let mut dry = again.to_vec();
    dry.push(("dry_run", SdkValue::Bool(true)));
    let tested = long(&home, "store.connect", &dry);
    let here = tested.get("here").unwrap();
    assert_eq!(here.get_text("step"), Some("transport"), "{tested:?}");
    assert_eq!(record(), before);
    std::fs::remove_dir_all(&home).unwrap();
}

/// `store.attach --dry-run` makes attach's check from the machine and
/// records nothing: not the attachment, not the reach. A connect that
/// changes the address is checked from every attached machine first and
/// refused `Conflict`, nothing saved, when one fails (here a stopped lima
/// machine), naming it in `machines` with the detach as its fix; its dry
/// run answers each machine's check; a change of the setting alone checks
/// no machine.
#[cfg(feature = "machine")]
#[test]
fn dry_runs_record_nothing_and_an_address_change_checks_attached_machines() {
    let Some(mut server) = Server::new() else {
        return;
    };
    let url = server.database("machines");
    let user = server.user.clone();
    let home = tempdir("machines");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    let connect = [
        ("store", t("pg")),
        ("url", t(&url)),
        ("user", t(&user)),
        ("password", t("pw")),
    ];
    ok(&home, "store.connect", &connect);
    let seen_file = paths::store_record(&home, "pg").with_file_name("observed.json");
    let seen = || std::fs::read(&seen_file).unwrap_or_default();
    let record = || std::fs::read(paths::store_record(&home, "pg")).unwrap();
    // a dry run from native: tested, nothing recorded
    let (record_before, seen_before) = (record(), seen());
    let on_native = [
        ("store", t("pg")),
        ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ("dry_run", SdkValue::Bool(true)),
    ];
    let tested = long(&home, "store.attach", &on_native);
    assert_eq!(tested.get_text("outcome"), Some("tested"), "{tested:?}");
    assert_eq!(
        tested.get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE),
        "{tested:?}"
    );
    assert!(tested.get_int("latency_ms").is_some(), "{tested:?}");
    assert_eq!(record(), record_before, "a dry run attached");
    assert_eq!(seen(), seen_before, "a dry run recorded a reach");
    ok(&home, "store.attach", &on_native[..2]);
    // a stopped lima machine, attached as `store.attach` records it
    home::write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([
            ("name", t("dev")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-dev")),
        ]),
    )
    .unwrap();
    let path = paths::store_record(&home, "pg");
    let held = home::read_record(&path).unwrap().unwrap();
    let attached = SdkValue::List(vec![t(layerfs_home::names::HOST_MACHINE), t("dev")]);
    home::write_record(&path, &home::with_field(held, "attached", attached)).unwrap();
    // its dry run is refused as its attach would be, naming the machine
    let on_dev = [
        ("store", t("pg")),
        ("machine", t("dev")),
        ("dry_run", SdkValue::Bool(true)),
    ];
    let refused = refusal(&home, "store.attach", &on_dev);
    assert_eq!(flat(&refused, "machine"), t("dev"), "{refused:?}");
    // the same Store at another address: refused, dev named, nothing saved
    let moved = url.replacen("127.0.0.1", "localhost", 1);
    assert_ne!(moved, url, "the test server is at 127.0.0.1");
    let (record_before, seen_before) = (record(), seen());
    let change = [("store", t("pg")), ("url", t(&moved))];
    let refused = refusal(&home, "store.connect", &change);
    assert_eq!(refused.code, "Conflict", "{}", refused.message);
    assert!(refused.message.contains("dev"), "{}", refused.message);
    assert!(
        !refused.message.contains(layerfs_home::names::HOST_MACHINE),
        "{}",
        refused.message
    );
    let machines = flat(&refused, "machines");
    let SdkValue::List(rows) = &machines else {
        panic!("{refused:?}");
    };
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].get_text("machine"), Some("dev"));
    assert_eq!(rows[0].get("ok"), Some(&SdkValue::Bool(false)));
    assert_eq!(
        refused.detail.get_text("fix"),
        Some("layerfs store detach pg --machine dev")
    );
    assert_eq!(record(), record_before, "a refused connect saved");
    // its dry run: this computer and each machine, nothing saved or seen
    let mut dry = change.to_vec();
    dry.push(("dry_run", SdkValue::Bool(true)));
    let tested = long(&home, "store.connect", &dry);
    assert_eq!(tested.get_text("outcome"), Some("tested"), "{tested:?}");
    assert_eq!(tested.get("ok"), Some(&SdkValue::Bool(false)), "{tested:?}");
    let here = tested.get("here").unwrap();
    assert_eq!(here.get("ok"), Some(&SdkValue::Bool(true)), "{tested:?}");
    let Some(SdkValue::List(rows)) = tested.get("machines") else {
        panic!("{tested:?}");
    };
    let row = |name: &str| {
        let found = rows
            .iter()
            .find(|row| row.get_text("machine") == Some(name));
        found.unwrap_or_else(|| panic!("{name}: {rows:?}")).clone()
    };
    assert_eq!(
        row(layerfs_home::names::HOST_MACHINE).get("ok"),
        Some(&SdkValue::Bool(true))
    );
    assert!(
        row(layerfs_home::names::HOST_MACHINE)
            .get_int("latency_ms")
            .is_some(),
        "{rows:?}"
    );
    assert_eq!(row("dev").get("ok"), Some(&SdkValue::Bool(false)));
    assert!(row("dev").get_text("message").is_some(), "{rows:?}");
    assert_eq!(record(), record_before, "a dry run saved");
    assert_eq!(seen(), seen_before, "a dry run recorded a reach");
    // the setting alone: no machine is dialled, so dev does not refuse it
    let setting = [
        ("store", t("pg")),
        ("url", t(&url)),
        ("synchronous_commit", t("on")),
    ];
    let updated = long(&home, "store.connect", &setting);
    assert_eq!(updated.get_text("outcome"), Some("updated"), "{updated:?}");
    std::fs::remove_dir_all(&home).unwrap();
}
