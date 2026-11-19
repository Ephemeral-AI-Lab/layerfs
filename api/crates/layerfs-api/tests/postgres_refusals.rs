//! PostgreSQL Store calls that need no PostgreSQL server (call surface 2.0,
//! `pg.html`): saved logins live in the home's secret folder (0600 files in
//! a 0700 folder), are never answered or logged, and are removed only when
//! no Store names them; `store.connect` refuses a URL with a user or a
//! password, a first connect without `--auth` and a connected Store's
//! `--auth`; `store.disconnect` refuses `InUse` while another process holds
//! the Store. The URLs name 127.0.0.1 port 1, where nothing listens, so a
//! request would fail with another code.
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
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

fn code(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> String {
    call(home, name, args).unwrap_err().0
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
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

/// Records PostgreSQL Store `name` at `url` (port 1: nothing listens),
/// naming login `auth` when given.
fn connected(home: &Path, name: &str, url: &str, auth: Option<&str>) {
    let mut fields = vec![
        ("name", t(name)),
        ("kind", t("postgresql")),
        ("url", t(url)),
        ("schema", t("layerfs")),
        ("synchronous_commit", t("off")),
        ("binding", SdkValue::Bytes(vec![7; 32])),
        ("cursor", SdkValue::Bytes(vec![9; 32])),
    ];
    if let Some(auth) = auth {
        fields.push(("auth", t(auth)));
    }
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    write_record(&paths::store_record(home, name), &SdkValue::Map(fields)).unwrap();
}

#[test]
fn a_login_is_saved_privately_listed_without_its_password_and_removed_when_unused() {
    let home = tempdir("logins");
    home::open(&home).unwrap();
    let saved = ok(
        &home,
        "store.save_auth",
        &[
            ("auth", t("pg-app")),
            ("user", t("app")),
            ("password", t("hunter2-secret")),
        ],
    );
    assert_eq!(layerfs_api::json::print(&saved), r#"{"ok":true}"#);
    let file = home.join("secret").join("auth").join("pg-app.json");
    assert_eq!(mode(&file), 0o600);
    assert_eq!(mode(file.parent().unwrap()), 0o700);
    assert_eq!(mode(&home.join("secret")), 0o700);
    // a Store names it; the list says so, never with the password
    connected(
        &home,
        "shop",
        "postgresql://127.0.0.1:1/shop",
        Some("pg-app"),
    );
    let listed = ok(&home, "store.auths", &[]);
    assert_eq!(
        layerfs_api::json::print(&listed),
        r#"{"items":[{"auth":"pg-app","user":"app","stores":["shop"]}]}"#
    );
    let long = ok(&home, "store.auths", &[("detail", SdkValue::Bool(true))]);
    let item = &match long.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("{other:?}"),
    }[0];
    assert!(
        item.get_text("created_at")
            .is_some_and(|at| at.ends_with('Z')),
        "{item:?}"
    );
    // saving again rotates it: the answer says so, the creation time stays
    let rotated = ok(
        &home,
        "store.save_auth",
        &[
            ("auth", t("pg-app")),
            ("user", t("app2")),
            ("password", t("second-secret")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(rotated.get_text("outcome"), Some("rotated"));
    assert_eq!(rotated.get_text("user"), Some("app2"));
    assert_eq!(
        rotated.get("stores"),
        Some(&SdkValue::List(vec![t("shop")]))
    );
    // no password outside the secret file: not in a record, not in the log
    let mut files = Vec::new();
    texts(&home, &mut files);
    for (path, text) in &files {
        if *path != file {
            assert!(
                !text.contains("hunter2-secret") && !text.contains("second-secret"),
                "{path:?}"
            );
        }
    }
    // required and checked
    assert_eq!(
        code(
            &home,
            "store.save_auth",
            &[("auth", t("x")), ("password", t("p"))]
        ),
        "InvalidInput"
    );
    assert_eq!(
        code(
            &home,
            "store.save_auth",
            &[("auth", t("x")), ("user", t("u"))]
        ),
        "InvalidInput"
    );
    assert_eq!(
        code(
            &home,
            "store.save_auth",
            &[
                ("auth", t("Bad/Name")),
                ("user", t("u")),
                ("password", t("p"))
            ]
        ),
        "InvalidInput"
    );
    // removal: refused while a Store names it, listing the Stores
    let refused = layerfs_api::call(
        Some(&home),
        "store.remove_auth",
        SdkValue::map([("auth", t("pg-app"))]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refused.code, "InUse", "{}", refused.message);
    assert!(refused.message.contains("shop"), "{}", refused.message);
    std::fs::remove_dir_all(paths::store_dir(&home, "shop")).unwrap();
    let removed = ok(
        &home,
        "store.remove_auth",
        &[("auth", t("pg-app")), ("detail", SdkValue::Bool(true))],
    );
    assert_eq!(removed.get_text("outcome"), Some("removed"));
    assert!(!file.exists());
    assert_eq!(
        code(&home, "store.remove_auth", &[("auth", t("pg-app"))]),
        "NotFound"
    );
    assert_eq!(
        layerfs_api::json::print(&ok(&home, "store.auths", &[])),
        r#"{"items":[]}"#
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn connect_refuses_a_login_in_the_url_a_missing_auth_and_a_settings_change() {
    let home = tempdir("connect");
    home::open(&home).unwrap();
    ok(
        &home,
        "store.save_auth",
        &[
            ("auth", t("pg-app")),
            ("user", t("app")),
            ("password", t("secret")),
        ],
    );
    let connect = |store: &str, url: &str, more: &[(&str, SdkValue)]| {
        let mut args = vec![("store", t(store)), ("url", t(url))];
        args.extend(more.iter().cloned());
        call(&home, "store.connect", &args)
    };
    let auth = [("auth", t("pg-app"))];
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
        let refused = connect("pg", url, &auth).unwrap_err();
        assert_eq!(refused.0, "InvalidInput", "{url}: {refused:?}");
        assert!(
            refused.1.contains("user or holds a password"),
            "{url}: {refused:?}"
        );
        assert!(!paths::store_dir(&home, "pg").exists(), "{url}");
    }
    let url = "postgresql://127.0.0.1:1/app";
    // a first connect needs a saved login
    let refused = connect("pg", url, &[]).unwrap_err();
    assert_eq!(refused.0, "InvalidInput", "{refused:?}");
    assert!(refused.1.contains("--auth"), "{refused:?}");
    assert_eq!(
        connect("pg", url, &[("auth", t("nobody"))]).unwrap_err().0,
        "NotFound"
    );
    // a Store not connected yet takes no machine's address
    let refused = connect("pg", url, &[("auth", t("pg-app")), ("machine", t("lab"))]).unwrap_err();
    assert_eq!(refused.0, "InvalidInput", "{refused:?}");
    assert!(!paths::store_dir(&home, "pg").exists());
    // a connected Store: an address only, never a login or the setting
    connected(&home, "pg", url, Some("pg-app"));
    let before = std::fs::read(paths::store_record(&home, "pg")).unwrap();
    for (more, said) in [
        (&auth[..], "--auth"),
        (
            &[("synchronous_commit", t("on"))][..],
            "--synchronous-commit",
        ),
    ] {
        let refused = connect("pg", url, more).unwrap_err();
        assert_eq!(refused.0, "InvalidInput", "{refused:?}");
        assert!(refused.1.contains(said), "{refused:?}");
    }
    let refused = connect("pg", url, &[("auth", t("pg-app")), ("machine", t("lab"))]).unwrap_err();
    assert_eq!(
        refused,
        (
            "InvalidInput".to_owned(),
            "a machine's own connection takes no --auth; pg uses pg-app".to_owned()
        )
    );
    assert_eq!(
        before,
        std::fs::read(paths::store_record(&home, "pg")).unwrap()
    );
    // a sqlite Store of the name exists
    write_record(
        &paths::store_record(&home, "lite"),
        &SdkValue::map([("name", t("lite")), ("kind", t("sqlite"))]),
    )
    .unwrap();
    assert_eq!(connect("lite", url, &auth).unwrap_err().0, "Exists");
    // update: one of the two is required, a login must be saved; disconnect
    // of a machine with no own address is NotFound; a sqlite Store refuses
    assert_eq!(
        code(&home, "store.update", &[("store", t("pg"))]),
        "InvalidInput"
    );
    assert_eq!(
        code(
            &home,
            "store.update",
            &[("store", t("pg")), ("auth", t("nobody"))]
        ),
        "NotFound"
    );
    assert_eq!(
        code(
            &home,
            "store.update",
            &[("store", t("pg")), ("synchronous_commit", t("sometimes"))]
        ),
        "InvalidInput"
    );
    assert_eq!(
        code(
            &home,
            "store.update",
            &[("store", t("lite")), ("auth", t("pg-app"))]
        ),
        "InvalidInput"
    );
    let updated = ok(
        &home,
        "store.update",
        &[
            ("store", t("pg")),
            ("synchronous_commit", t("on")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(
        layerfs_api::json::print(&updated),
        r#"{"ok":true,"outcome":"updated","store":"pg","auth":"pg-app","synchronous_commit":"on"}"#
    );
    assert_eq!(
        code(
            &home,
            "store.disconnect",
            &[("store", t("pg")), ("machine", t("lab"))]
        ),
        "NotFound"
    );
    let refused = call(&home, "store.disconnect", &[("store", t("lite"))]).unwrap_err();
    assert_eq!(refused.0, "InvalidInput");
    assert!(refused.1.contains("store remove lite"), "{refused:?}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn disconnect_refuses_a_postgresql_store_another_process_holds() {
    let home = tempdir("held");
    home::open(&home).unwrap();
    connected(&home, "pg", "postgresql://alice@127.0.0.1:1/app", None);
    // process 1 is always alive: its lease holds the Store
    let lease = paths::lease_dir(&home, "pg").join("1");
    std::fs::create_dir_all(lease.parent().unwrap()).unwrap();
    std::fs::write(&lease, "1\n").unwrap();
    let args = [("store", t("pg")), ("detail", SdkValue::Bool(true))];
    let (code, message) = call(&home, "store.disconnect", &args).unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    assert!(paths::store_record(&home, "pg").is_file());
    std::fs::remove_file(&lease).unwrap();
    let done = ok(&home, "store.disconnect", &args);
    assert_eq!(
        layerfs_api::json::print(&done),
        r#"{"ok":true,"outcome":"disconnected","store":"pg","machine":null}"#
    );
    assert!(!paths::store_dir(&home, "pg").exists());
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
    ok(
        &home,
        "store.save_auth",
        &[
            ("auth", t("pg-app")),
            ("user", t("alice")),
            ("password", t("secret")),
        ],
    );
    connected(
        &home,
        "pg",
        &format!("postgresql://127.0.0.1:{port}/app"),
        Some("pg-app"),
    );
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
/// alone and names the login with `save-auth`/`--auth` (spec v23 V23-13,
/// LF11); a connected Store, which takes no `--auth` on connect, names it
/// with `store update --auth` first. The fix never carries the password.
#[test]
fn a_login_in_the_url_is_refused_with_the_address_alone_as_the_fix() {
    let home = tempdir("login-fix");
    home::open(&home).unwrap();
    let refusal = |args: &[(&str, SdkValue)]| {
        let args = SdkValue::Map(
            args.iter()
                .map(|(n, v)| ((*n).to_owned(), v.clone()))
                .collect(),
        );
        let refused =
            layerfs_api::call(Some(&home), "store.connect", args, &mut |_| {}).unwrap_err();
        assert_eq!(refused.code, "InvalidInput", "{refused:?}");
        let fix = refused.detail.get_text("fix").map(str::to_owned);
        fix.unwrap_or_else(|| panic!("no fix: {refused:?}"))
    };
    let first = refusal(&[
        ("store", t("pg")),
        (
            "url",
            t("postgresql://alice:secret@127.0.0.1:1/app?sslmode=disable&passfile=/p"),
        ),
    ]);
    assert_eq!(
        first,
        "layerfs store save-auth <auth> --user alice --password-stdin && layerfs store connect pg 'postgresql://127.0.0.1:1/app?sslmode=disable' --auth <auth>"
    );
    let no_user = refusal(&[
        ("store", t("pg")),
        ("url", t("postgresql://127.0.0.1:1/app?password=secret")),
    ]);
    assert_eq!(
        no_user,
        "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg postgresql://127.0.0.1:1/app --auth <auth>"
    );
    connected(&home, "pg", "postgresql://127.0.0.1:1/app", Some("pg-app"));
    let readdress = refusal(&[
        ("store", t("pg")),
        ("url", t("postgresql://127.0.0.1:2/app?sslpassword=secret")),
    ]);
    assert_eq!(
        readdress,
        "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update pg --auth <auth> && layerfs store connect pg postgresql://127.0.0.1:2/app"
    );
    // a URL that is not a PostgreSQL one is refused with no fix (a fix would
    // repeat its login); a login hidden after a fragment is dropped with it
    let args = SdkValue::map([
        ("store", t("pg2")),
        ("url", t("postgresq://alice:secret@127.0.0.1:1/app")),
    ]);
    let bad = layerfs_api::call(Some(&home), "store.connect", args, &mut |_| {}).unwrap_err();
    assert_eq!(bad.detail.get_text("fix"), None, "{bad:?}");
    assert!(!format!("{bad:?}").contains("secret"), "{bad:?}");
    let fragment = refusal(&[
        ("store", t("pg")),
        ("url", t("postgresql://127.0.0.1:2/app#x?password=secret")),
    ]);
    assert!(
        fragment.ends_with("store connect pg postgresql://127.0.0.1:2/app"),
        "{fragment}"
    );
    // every shape names the login path (V23-13) and never the password
    for fix in [&first, &no_user, &readdress, &fragment] {
        assert!(fix.contains("save-auth") && fix.contains("--auth"), "{fix}");
        assert!(!fix.contains("secret"), "{fix}");
    }
    let _ = std::fs::remove_dir_all(home);
}

/// An outdated record whose URL still holds a login never shows it in its
/// `connections` rows (spec v23 V23-3; conformance v23 F1), and a login file
/// that cannot be read is saved again by the fix its broken row gives
/// (V23-8; conformance v23 F2).
#[test]
fn an_old_url_login_is_never_shown_and_a_broken_login_file_is_saved_again() {
    let home = tempdir("old-url");
    home::open(&home).unwrap();
    connected(
        &home,
        "old",
        "postgresql://alice:hunter2@127.0.0.1:1/app?sslmode=disable&sslpassword=sekrit",
        Some("pg-app"),
    );
    let got = ok(
        &home,
        "store.get",
        &[("store", t("old")), ("detail", SdkValue::Bool(true))],
    );
    let shown = layerfs_api::json::print(&got);
    assert!(shown.contains("127.0.0.1"), "{shown}");
    // the user may be named (the no-login fix names it); a password never
    for secret in ["hunter2", "sekrit"] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
    let listed = layerfs_api::json::print(&ok(
        &home,
        "store.list",
        &[("detail", SdkValue::Bool(true))],
    ));
    for secret in ["hunter2", "sekrit"] {
        assert!(!listed.contains(secret), "{secret} in {listed}");
    }
    let bad = layerfs_api::home::secrets::dir(&home).join("auth");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(bad.join("bad.json"), "{not json").unwrap();
    std::fs::set_permissions(bad.join("bad.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let saved = ok(
        &home,
        "store.save_auth",
        &[
            ("auth", t("bad")),
            ("user", t("u")),
            ("password", t("pw")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert!(
        layerfs_api::json::print(&saved).contains("bad"),
        "{saved:?}"
    );
    let auths = layerfs_api::json::print(&ok(&home, "store.auths", &[]));
    assert!(auths.contains("\"bad\""), "{auths}");
    assert!(!auths.contains("broken"), "{auths}");
    let _ = std::fs::remove_dir_all(home);
}
