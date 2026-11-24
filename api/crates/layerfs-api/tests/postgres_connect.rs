//! PostgreSQL Stores (#533 steps 10a and 10b; owner, 2026-11-24): connect
//! makes the `layerfs` schema in an empty database and records it as a
//! Store with its user, its password saved in the home's secret folder and
//! put into the URL per call; Init writes a project into it; Store calls
//! then run on it; a connect of the connected Store keeps its login;
//! disconnect forgets it and its password and keeps its data, and a second
//! connect finds the project. The same database under a second name is
//! `Exists`, by the id in its schema. Needs LAYERFS_TEST_PG_URL (a server
//! URL with its user and no password, to a database with no `layerfs`
//! schema; the test leaves one behind) and LAYERFS_TEST_PG_PASSWORD (the
//! user's password); without them the test says so and passes.
use layerfs_api::home::{self, paths};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-pg-{tag}-{}", std::process::id()));
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

/// `url`'s address without its user, and the login: its user with
/// LAYERFS_TEST_PG_PASSWORD.
fn login(url: &str) -> (String, [(&'static str, SdkValue); 2]) {
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    let password = std::env::var("LAYERFS_TEST_PG_PASSWORD").unwrap();
    (
        format!("{scheme}://{address}"),
        [("user", t(user)), ("password", t(&password))],
    )
}

#[test]
fn connect_read_and_disconnect() {
    let (Ok(url), Ok(_)) = (
        std::env::var("LAYERFS_TEST_PG_URL"),
        std::env::var("LAYERFS_TEST_PG_PASSWORD"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_URL / LAYERFS_TEST_PG_PASSWORD unset");
        return;
    };
    let home = tempdir("home");
    home::open(&home).unwrap();

    let (address, login) = login(&url);
    let with_login = |args: &[(&'static str, SdkValue)]| {
        let mut args = args.to_vec();
        args.extend(login.iter().cloned());
        args
    };
    // a URL naming its user is refused before any effect
    let refused = call(
        &home,
        "store.connect",
        &with_login(&[("store", t("pg")), ("url", t(&url))]),
    );
    assert_eq!(refused.unwrap_err().0, "InvalidInput");
    let url = address;
    // an empty database: connect makes the schema, with no project yet
    let connected = ok(
        &home,
        "store.connect",
        &with_login(&[
            ("store", t("pg")),
            ("url", t(&url)),
            ("detail", SdkValue::Bool(true)),
        ]),
    );
    assert_eq!(connected.get("projects"), Some(&SdkValue::List(vec![])));
    assert_eq!(connected.get_text("outcome"), Some("connected"));
    assert_eq!(connected.get("user"), Some(&login[0].1));
    assert_eq!(connected.get_text("url"), Some(url.as_str()));
    assert!(connected.get("password").is_none(), "{connected:?}");
    assert_eq!(connected.get_text("synchronous_commit"), Some("local"));
    // Init into it, with progress on the call's stream
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello pg\n").unwrap();
    let made = ok(
        &home,
        "project.init",
        &[
            ("project", t("pg/api")),
            ("folder", t(&folder.display().to_string())),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(made.get_text("project"), Some("pg/api"));
    // a PostgreSQL Store is on no machine
    assert_eq!(made.get("machine"), Some(&SdkValue::Null));
    assert!(paths::project_record(&home, "pg", "api").is_file());
    assert_eq!(
        call(
            &home,
            "project.init",
            &[
                ("project", t("pg/api")),
                ("folder", t(&folder.display().to_string())),
            ],
        )
        .unwrap_err()
        .0,
        "Exists"
    );
    let record = std::fs::read_to_string(paths::store_record(&home, "pg")).unwrap();
    assert!(
        !record.contains(&std::env::var("LAYERFS_TEST_PG_PASSWORD").unwrap()),
        "no password on disk"
    );
    // a connect of the connected Store with no login keeps the saved one
    let updated = ok(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(updated.get_text("outcome"), Some("updated"), "{updated:?}");
    // the same database under a second name (#533 review P1-15): by the
    // id in its schema, whatever the address says
    let alias = if url.contains("127.0.0.1") {
        Some(url.replace("127.0.0.1", "localhost"))
    } else if url.contains("localhost") {
        Some(url.replace("localhost", "127.0.0.1"))
    } else {
        eprintln!("NOT_RUN: the alias case needs a URL host of 127.0.0.1 or localhost");
        None
    };
    for second in std::iter::once(url.clone()).chain(alias) {
        let refused = call(
            &home,
            "store.connect",
            &with_login(&[("store", t("pg-again")), ("url", t(&second))]),
        )
        .unwrap_err();
        assert_eq!(refused.0, "Exists", "{second}: {refused:?}");
        assert!(refused.1.contains("Store pg "), "{refused:?}");
        assert!(!paths::store_dir(&home, "pg-again").exists());
    }

    let read = ok(
        &home,
        "file.read",
        &[
            ("project", t("pg/api")),
            ("branch", t("main")),
            ("path", t("/README.md")),
        ],
    );
    assert_eq!(read.get_text("text"), Some("hello pg\n"));
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("pg/api")),
            ("name", t("fix")),
            ("from", t("main")),
        ],
    );
    ok(
        &home,
        "file.write",
        &[
            ("project", t("pg/api")),
            ("branch", t("fix")),
            ("path", t("/NEW.md")),
            ("content", t("written\n")),
        ],
    );
    let status = ok(
        &home,
        "store.get",
        &[
            ("store", t("pg")),
            ("with", t("live")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(status.get_text("state"), Some("ready"), "{status:?}");
    assert!(
        status
            .get("live")
            .and_then(|live| live.get_text("server_version"))
            .is_some(),
        "{status:?}"
    );

    let export_dir = tempdir("export").join("out");
    let exported = ok(
        &home,
        "project.export",
        &[
            ("project", t("pg/api")),
            ("destination", t(&export_dir.display().to_string())),
            ("branch", t("fix")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(exported.get_int("file_count"), Some(2), "{exported:?}");
    assert!(exported.get_text("commit").is_some(), "{exported:?}");
    assert_eq!(
        std::fs::read_to_string(export_dir.join("README.md")).unwrap(),
        "hello pg\n"
    );
    assert_eq!(
        std::fs::read_to_string(export_dir.join("NEW.md")).unwrap(),
        "written\n"
    );
    let _ = std::fs::remove_dir_all(export_dir.parent().unwrap());

    assert_eq!(
        call(&home, "store.remove", &[("store", t("pg"))])
            .unwrap_err()
            .0,
        "InvalidInput"
    );
    // this process's own open copy does not block a disconnect
    ok(&home, "store.disconnect", &[("store", t("pg"))]);
    assert!(!paths::store_dir(&home, "pg").exists());
    // its password goes with it
    let saved = layerfs_api::home::secrets::dir(&home).join("login");
    assert!(
        !std::fs::read_dir(&saved).is_ok_and(|mut files| files.next().is_some()),
        "{}",
        saved.display()
    );
    let again = ok(
        &home,
        "store.connect",
        &with_login(&[("store", t("pg2")), ("url", t(&url))]),
    );
    assert_eq!(
        again.get("projects"),
        Some(&SdkValue::List(vec![t("api")])),
        "the data stayed"
    );
    let kept = ok(
        &home,
        "file.read",
        &[
            ("project", t("pg2/api")),
            ("branch", t("fix")),
            ("path", t("/NEW.md")),
        ],
    );
    assert_eq!(kept.get_text("text"), Some("written\n"));
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

/// A database the server does not have is a session the server refused,
/// not a server that took no connection (#535 terminal pass, lane K3): it
/// is refused at the connection step `database`, in the server's words.
#[test]
fn a_missing_database_is_a_refused_session() {
    let (Ok(url), Ok(_)) = (
        std::env::var("LAYERFS_TEST_PG_URL"),
        std::env::var("LAYERFS_TEST_PG_PASSWORD"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_URL / LAYERFS_TEST_PG_PASSWORD unset");
        return;
    };
    let home = tempdir("missing");
    home::open(&home).unwrap();
    let (url, login) = login(&url);
    let (server, _) = url.rsplit_once('/').unwrap();
    let missing = format!("{server}/lfs_no_such_database");
    let mut args = vec![("store", t("gone")), ("url", t(&missing))];
    args.extend(login);
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    let refusal = layerfs_api::call(Some(&home), "store.connect", args, &mut |_| {}).unwrap_err();
    let message = &refusal.message;
    assert_eq!(refusal.code, "Unavailable", "{message}");
    assert!(message.contains("refused the connection"), "{message}");
    assert!(message.contains("lfs_no_such_database"), "{message}");
    let next = refusal.detail.get("next").cloned().unwrap_or_default();
    assert_eq!(next.get_text("step"), Some("database"), "{refusal:?}");
    assert!(!paths::store_dir(&home, "gone").exists());
    std::fs::remove_dir_all(&home).unwrap();
}
