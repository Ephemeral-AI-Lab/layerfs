//! PostgreSQL Store calls that need no PostgreSQL server (owner,
//! 2026-11-24): `store.connect` refuses a URL with a user or a password, a
//! new Store without its user or password, and a connected Store with no
//! user or saved password to dial with, before anything is dialled or
//! written; `store.disconnect`
//! refuses `InUse` while another process holds the Store and forgets its
//! saved password; `store.attach` and `store.detach` refuse a sqlite
//! Store, a missing machine, `native` when it does not reach the server, a loopback URL a machine does not
//! share and a detach while a Sandbox runs there; `store.doctor --machine`
//! refuses a machine that is not attached. The removed calls are gone from
//! the table. The URLs name 127.0.0.1 port 1, where nothing listens, so a
//! request would fail with another code.
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-pg-refusal-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn refusal(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkRefusal {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).unwrap_err()
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

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// Records PostgreSQL Store `name` at `url` (port 1: nothing listens) as
/// `store.connect` writes it, user `app`, with `extra` fields; its
/// password is saved when `password`.
fn connected(home: &Path, name: &str, url: &str, extra: &[(&str, SdkValue)], password: bool) {
    let mut fields = vec![
        ("name", t(name)),
        ("kind", t("postgresql")),
        ("url", t(url)),
        ("user", t("app")),
        ("schema", t("layerfs")),
        ("synchronous_commit", t("off")),
        ("binding", SdkValue::Bytes(vec![7; 32])),
        ("cursor", SdkValue::Bytes(vec![9; 32])),
    ];
    fields.extend(extra.iter().cloned());
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    write_record(&paths::store_record(home, name), &SdkValue::Map(fields)).unwrap();
    if password {
        layerfs_store::postgresql::login::save(home, name, "hunter2-secret").unwrap();
    }
}

/// A machine record of `platform` (nothing runs).
fn machine(home: &Path, name: &str, platform: &str) {
    write_record(
        &paths::machine_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("platform", t(platform)),
            ("vm_name", t(&format!("layerfs-test-{name}"))),
        ]),
    )
    .unwrap();
}

/// Sandbox `sandbox` of project `project`, on machine `machine`, as its
/// record keeps it (nothing runs).
fn sandbox_on(home: &Path, sandbox: &str, project: &str, machine: &str) {
    write_record(
        &paths::sandbox_record(home, sandbox),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", t(sandbox)),
            ("project", t(project)),
            ("machine", t(machine)),
            ("created", SdkValue::Int(1)),
            ("id", t(sandbox)),
            ("platform", t("lima")),
            ("runtime", t("namespaces")),
            ("backend", t("namespaces")),
        ]),
    )
    .unwrap();
}

#[test]
fn connect_refuses_a_login_in_the_url_and_a_missing_login() {
    let home = tempdir("connect");
    home::open(&home).unwrap();
    let connect = |store: &str, url: &str, more: &[(&str, SdkValue)]| {
        let mut args = vec![("store", t(store)), ("url", t(url))];
        args.extend(more.iter().cloned());
        call(&home, "store.connect", &args)
    };
    let login = [("user", t("app")), ("password", t("secret"))];
    for url in [
        "postgresql://alice:secret@127.0.0.1:1/app",
        "postgresql://alice@127.0.0.1:1/app",
        "postgresql://:secret@127.0.0.1:1/app",
        "postgresql://127.0.0.1:1/app?password=secret",
        "postgres://127.0.0.1:1/app?sslmode=disable&PassWord=secret",
        "postgresql://127.0.0.1:1/app?pass%77ord=secret",
        "postgresql://127.0.0.1:1/app?sslpassword=secret",
        "postgresql://127.0.0.1:1/app?user=bob",
        // a password file is a login too (spec v23 V23-13)
        "postgresql://127.0.0.1:1/app?passfile=/home/a/.pgpass",
        "postgresql://127.0.0.1:1/app?sslmode=require&PassFile=%2Ftmp%2Fp",
    ] {
        let refused = connect("pg", url, &login).unwrap_err();
        assert_eq!(refused.0, "InvalidInput", "{url}: {refused:?}");
        assert!(
            refused.1.contains("user or holds a password"),
            "{url}: {refused:?}"
        );
        assert!(!paths::store_dir(&home, "pg").exists(), "{url}");
    }
    let url = "postgresql://127.0.0.1:1/app";
    // a new Store needs its user and password, neither empty
    for more in [
        vec![],
        vec![("user", t("app"))],
        vec![("password", t("secret"))],
        vec![("user", t("")), ("password", t("secret"))],
        vec![("user", t("app")), ("password", t(""))],
    ] {
        let refused = connect("pg", url, &more).unwrap_err();
        assert_eq!(refused.0, "InvalidInput", "{more:?}: {refused:?}");
        assert!(!paths::store_dir(&home, "pg").exists());
    }
    let refused = connect("pg", url, &[]).unwrap_err();
    assert!(
        refused.1.contains("--user <user> --password-stdin"),
        "{refused:?}"
    );
    let mut sometimes = login.to_vec();
    sometimes.push(("synchronous_commit", t("sometimes")));
    assert_eq!(
        connect("pg", url, &sometimes).unwrap_err().0,
        "InvalidInput"
    );
    // the removed options are unknown
    for removed in [("auth", t("pg")), ("machine", t("lab"))] {
        let mut args = login.to_vec();
        args.push(removed.clone());
        assert_eq!(
            connect("pg", url, &args).unwrap_err().0,
            "InvalidInput",
            "{removed:?}"
        );
    }
    // a connected Store whose password is not saved needs one given
    connected(&home, "pg", url, &[], false);
    let refused = connect("pg", url, &[]).unwrap_err();
    assert_eq!(refused.0, "InvalidInput", "{refused:?}");
    assert!(refused.1.contains("--password-stdin"), "{refused:?}");
    // a connected record naming no user needs one given; nothing is written
    connected(&home, "nameless", url, &[], true);
    let path = paths::store_record(&home, "nameless");
    let record = home::read_record(&path).unwrap().unwrap();
    let SdkValue::Map(fields) = record else {
        panic!("a map");
    };
    let fields = fields.into_iter().filter(|(k, _)| k != "user").collect();
    write_record(&path, &SdkValue::Map(fields)).unwrap();
    let before = std::fs::read(&path).unwrap();
    let refused = connect("nameless", url, &[]).unwrap_err();
    assert_eq!(refused.0, "InvalidInput", "{refused:?}");
    assert!(refused.1.contains("--user"), "{refused:?}");
    assert_eq!(before, std::fs::read(&path).unwrap());
    let listed = ok(&home, "store.list", &[("detail", SdkValue::Bool(true))]);
    let printed = layerfs_api::json::print(&listed);
    assert!(!printed.contains("\"auth\""), "{printed}");
    // a sqlite Store of the name exists
    write_record(
        &paths::store_record(&home, "lite"),
        &SdkValue::map([("name", t("lite")), ("kind", t("sqlite"))]),
    )
    .unwrap();
    assert_eq!(connect("lite", url, &login).unwrap_err().0, "Exists");
    let refused = call(&home, "store.disconnect", &[("store", t("lite"))]).unwrap_err();
    assert_eq!(refused.0, "InvalidInput");
    assert!(refused.1.contains("store remove lite"), "{refused:?}");
    // the saved-login calls and store.update are gone
    for removed in [
        "store.save_auth",
        "store.auths",
        "store.remove_auth",
        "store.update",
    ] {
        let refused = refusal(&home, removed, &[("store", t("pg"))]);
        assert_eq!(refused.code, "InvalidInput", "{removed}: {refused:?}");
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn disconnect_refuses_a_held_store_then_forgets_it_and_its_password() {
    let home = tempdir("held");
    home::open(&home).unwrap();
    connected(&home, "pg", "postgresql://127.0.0.1:1/app", &[], true);
    let secret = layerfs_api::home::secrets::dir(&home).join("login");
    let file = std::fs::read_dir(&secret)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(mode(&file), 0o600);
    assert_eq!(mode(&secret), 0o700);
    // process 1 is always alive: its lease holds the Store
    let lease = paths::lease_dir(&home, "pg").join("1");
    std::fs::create_dir_all(lease.parent().unwrap()).unwrap();
    std::fs::write(&lease, "1\n").unwrap();
    let args = [("store", t("pg")), ("detail", SdkValue::Bool(true))];
    let (code, message) = call(&home, "store.disconnect", &args).unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    assert!(paths::store_record(&home, "pg").is_file());
    assert!(file.is_file());
    std::fs::remove_file(&lease).unwrap();
    // `machine` is no option of it any more
    let mut with_machine = args.to_vec();
    with_machine.push(("machine", t("lab")));
    assert_eq!(
        call(&home, "store.disconnect", &with_machine)
            .unwrap_err()
            .0,
        "InvalidInput"
    );
    let done = ok(&home, "store.disconnect", &args);
    assert_eq!(
        layerfs_api::json::print(&done),
        r#"{"ok":true,"outcome":"disconnected","store":"pg"}"#
    );
    assert!(!paths::store_dir(&home, "pg").exists());
    assert!(!file.exists(), "its password is forgotten");
    let _ = std::fs::remove_dir_all(home);
}

/// A Store call on a PostgreSQL Store holds its lease while it runs: the
/// "server" accepts and answers nothing until the lease is seen.
#[test]
fn a_postgresql_store_call_holds_the_stores_lease() {
    use std::time::{Duration, Instant};
    let home = tempdir("lease");
    home::open(&home).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("postgresql://127.0.0.1:{port}/app");
    connected(&home, "pg", &url, &[], true);
    write_record(
        &paths::project_record(&home, "pg", "app"),
        &SdkValue::map([("name", t("app"))]),
    )
    .unwrap();
    let running = {
        let home = home.clone();
        std::thread::spawn(move || {
            let args = SdkValue::map([
                ("project", t("pg/app")),
                ("branch", t("main")),
                ("path", t("/a")),
            ]);
            layerfs_api::call(Some(&home), "file.read", args, &mut |_| {}).map_err(|r| r.code)
        })
    };
    let (stream, _) = listener.accept().unwrap();
    let lease = paths::lease_dir(&home, "pg").join(std::process::id().to_string());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !lease.is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(lease.is_file(), "the running call holds the lease");
    drop(stream);
    assert!(running.join().unwrap().is_err(), "nothing answered");
    assert!(!lease.exists(), "a Store that did not open keeps no lease");
    let _ = std::fs::remove_dir_all(home);
}

/// The refusal of a URL carrying a login has a fix that gives the address
/// alone and the login apart (spec v23 V23-13, LF11): always for a new
/// Store, which needs one; on a connected Store only when the URL may hold
/// one. The fix never carries the password.
#[test]
fn a_login_in_the_url_is_refused_with_the_address_alone_as_the_fix() {
    let home = tempdir("login-fix");
    home::open(&home).unwrap();
    let fix = |args: &[(&str, SdkValue)]| {
        let refused = refusal(&home, "store.connect", args);
        assert_eq!(refused.code, "InvalidInput", "{refused:?}");
        let fix = refused.detail.get_text("fix").map(str::to_owned);
        fix.unwrap_or_else(|| panic!("no fix: {refused:?}"))
    };
    let login = "layerfs store connect pg <url> --user <user> --password-stdin";
    let first = fix(&[
        ("store", t("pg")),
        (
            "url",
            t("postgresql://alice:secret@127.0.0.1:1/app?sslmode=disable&passfile=/p"),
        ),
    ]);
    // the client does not read `passfile`: no form of the URL is shown, and
    // no user (`<url>` and `<user>` to fill in)
    assert_eq!(first, login);
    let no_user = fix(&[
        ("store", t("pg")),
        ("url", t("postgresql://127.0.0.1:1/app?password=secret")),
    ]);
    assert_eq!(no_user, login);
    connected(&home, "pg", "postgresql://127.0.0.1:1/app", &[], true);
    let readdress = fix(&[
        ("store", t("pg")),
        ("url", t("postgresql://127.0.0.1:2/app?sslpassword=secret")),
    ]);
    assert_eq!(readdress, login);
    // a URL that is not a PostgreSQL one is refused with no fix (a fix would
    // repeat its login)
    let bad = refusal(
        &home,
        "store.connect",
        &[
            ("store", t("pg2")),
            ("url", t("postgresq://alice:secret@127.0.0.1:1/app")),
        ],
    );
    assert_eq!(bad.detail.get_text("fix"), None, "{bad:?}");
    assert!(!format!("{bad:?}").contains("secret"), "{bad:?}");
    let fragment = fix(&[
        ("store", t("pg")),
        ("url", t("postgresql://127.0.0.1:2/app#x?password=secret")),
    ]);
    assert_eq!(fragment, login);
    // a `#` alone on a connected Store needs no login
    let hash = fix(&[
        ("store", t("pg")),
        ("url", t("postgresql://127.0.0.1:2/app#x")),
    ]);
    assert_eq!(hash, "layerfs store connect pg <url>");
    for fix in [&first, &no_user, &readdress, &fragment] {
        assert!(!fix.contains("secret"), "{fix}");
    }
    let _ = std::fs::remove_dir_all(home);
}

/// A record whose URL holds a login never shows it (spec v23 V23-3;
/// conformance v23 F1): `store.get` and `store.list` hide it whole.
#[test]
fn an_old_url_login_is_never_shown() {
    let home = tempdir("old-url");
    home::open(&home).unwrap();
    for url in [
        // the client does not read `sslpassword`
        "postgresql://alice:hunter2@127.0.0.1:1/app?sslmode=disable&sslpassword=sekrit",
        // one the client reads is hidden whole too: no login is cut out
        "postgresql://alice:hunter2@127.0.0.1:1/app?sslmode=disable&password=sekrit",
    ] {
        connected(&home, "old", url, &[], false);
        let got = ok(
            &home,
            "store.get",
            &[("store", t("old")), ("detail", SdkValue::Bool(true))],
        );
        let shown = layerfs_api::json::print(&got);
        assert!(
            shown.contains("postgresql://<hidden: not an address alone>"),
            "{shown}"
        );
        let listed = layerfs_api::json::print(&ok(
            &home,
            "store.list",
            &[("detail", SdkValue::Bool(true))],
        ));
        for secret in ["hunter2", "sekrit", "alice", "127.0.0.1"] {
            assert!(!shown.contains(secret), "{secret} in {shown}");
            assert!(!listed.contains(secret), "{secret} in {listed}");
        }
    }
    let _ = std::fs::remove_dir_all(home);
}

/// `store.attach` and `store.detach` refuse what is not theirs before any
/// dial; a machine that does not share this host's loopback is refused a
/// loopback URL with the connect that fixes it; a detach is refused while
/// a Sandbox of the Store runs on the machine, and a machine not attached
/// is `NotFound`; `store.doctor --machine` names the attach for a machine
/// not attached.
#[cfg(feature = "machine")]
#[test]
fn attach_and_detach_refuse_what_is_not_theirs() {
    let home = tempdir("attach");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    let url = "postgresql://127.0.0.1:1/app";
    connected(&home, "pg", url, &[], true);
    write_record(
        &paths::store_record(&home, "lite"),
        &SdkValue::map([("name", t("lite")), ("kind", t("sqlite"))]),
    )
    .unwrap();
    machine(&home, "dev", "lima");
    let on = |store: &str, machine: &str| [("store", t(store)), ("machine", t(machine))];
    // `machine` is required
    assert_eq!(
        refusal(&home, "store.attach", &[("store", t("pg"))]).code,
        "InvalidInput"
    );
    let lite = refusal(&home, "store.attach", &on("lite", "dev"));
    assert_eq!(lite.code, "InvalidInput", "{lite:?}");
    assert!(
        lite.message.contains("layerfs store move lite dev"),
        "{lite:?}"
    );
    assert_eq!(
        refusal(&home, "store.attach", &on("nope", "dev")).code,
        "NotFound"
    );
    // `native` is checked like any machine: nothing listens on the
    // Store's address, so it is not attached
    let native = refusal(&home, "store.attach", &on("pg", "native"));
    assert_eq!(native.code, "Unavailable", "{native:?}");
    assert_eq!(
        refusal(&home, "store.detach", &on("pg", "native")).code,
        "NotFound"
    );
    assert_eq!(
        refusal(&home, "store.attach", &on("pg", "ghost")).code,
        "NotFound"
    );
    // lima does not share this host's loopback on macOS: the Store is
    // connected again with an address the machine reaches; nothing is
    // recorded
    let lima = layerfs_platform::sandbox_allowed(
        layerfs_platform::Platform::Lima,
        layerfs_platform::HostOs::current(),
        layerfs_platform::StoreKind::Postgresql,
    );
    let loopback = refusal(&home, "store.attach", &on("pg", "dev"));
    match lima {
        Ok(()) if cfg!(target_os = "macos") => {
            assert_eq!(loopback.code, "Unsupported", "{loopback:?}");
            assert!(
                loopback.message.contains("host.lima.internal")
                    && loopback
                        .message
                        .contains("`layerfs store connect pg <url>`"),
                "{loopback:?}"
            );
        }
        Ok(()) => {}
        Err(reason) => assert!(loopback.message.ends_with(reason), "{loopback:?}"),
    }
    let record = home::read_record(&paths::store_record(&home, "pg"))
        .unwrap()
        .unwrap();
    assert!(record.get("attached").is_none(), "{record:?}");
    // detach: not attached is NotFound
    assert_eq!(
        refusal(&home, "store.detach", &on("pg", "dev")).code,
        "NotFound"
    );
    // doctor --machine of a machine not attached names the attach
    let doctor = refusal(&home, "store.doctor", &on("pg", "dev"));
    assert_eq!(doctor.code, "InvalidInput", "{doctor:?}");
    assert_eq!(
        doctor.detail.get_text("fix"),
        Some("layerfs store attach pg --machine dev")
    );
    // attached (as `store.attach` records it), with a Sandbox of its
    // project on the machine: the detach is refused, naming it
    connected(
        &home,
        "pg",
        url,
        &[("attached", SdkValue::List(vec![t("dev")]))],
        true,
    );
    sandbox_on(&home, "s1", "pg/app", "dev");
    sandbox_on(&home, "s2", "lite/app", "dev");
    let held = refusal(&home, "store.detach", &on("pg", "dev"));
    assert_eq!(held.code, "InUse", "{held:?}");
    assert!(
        held.message
            .starts_with("1 Sandbox(es) of Store pg's projects run on machine dev: s1"),
        "{held:?}"
    );
    assert_eq!(
        held.detail.get_text("fix"),
        Some("layerfs sandbox remove s1 && layerfs store detach pg --machine dev")
    );
    std::fs::remove_dir_all(paths::sandbox_dir(&home, "s1")).unwrap();
    let detached = ok(
        &home,
        "store.detach",
        &[
            ("store", t("pg")),
            ("machine", t("dev")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(
        layerfs_api::json::print(&detached),
        r#"{"ok":true,"outcome":"detached","store":"pg","machine":"dev"}"#
    );
    let record = home::read_record(&paths::store_record(&home, "pg"))
        .unwrap()
        .unwrap();
    assert!(record.get("attached").is_none(), "{record:?}");
    let _ = std::fs::remove_dir_all(home);
}
