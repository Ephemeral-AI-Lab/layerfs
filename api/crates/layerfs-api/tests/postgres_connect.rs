//! PostgreSQL Stores (#533 steps 10a and 10b): connect makes the `layerfs`
//! schema in an empty database and records it as a Store, with the password
//! a reference resolved per call; Init writes a project into it; Store
//! calls then run on it; disconnect forgets it and keeps its data, and a
//! second connect finds the project. The same database under a second name
//! is `Exists`. Needs LAYERFS_TEST_PG_URL (a server
//! URL without a password, to a database with no `layerfs` schema; the
//! test leaves one behind) and LAYERFS_TEST_PG_PASSWORD; without them the
//! test says so and passes.
#![cfg(feature = "postgresql")]
use layerfs_api::calls;
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
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
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

    // a URL holding a password is refused before any effect
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, host) = rest.split_once('@').unwrap();
    let with_secret = format!("{scheme}://{user}:x@{host}");
    let refused = call(
        &home,
        "store.connect",
        &[("name", t("pg")), ("url", t(&with_secret))],
    );
    assert_eq!(refused.unwrap_err().0, "InvalidInput");
    let password = || ("password", t("env:LAYERFS_TEST_PG_PASSWORD"));
    // an empty database: connect makes the schema, with no project yet
    let connected = ok(
        &home,
        "store.connect",
        &[("name", t("pg")), ("url", t(&url)), password()],
    );
    assert_eq!(connected.get("projects"), Some(&SdkValue::List(vec![])));
    // Init into it, with progress on the call's stream
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello pg\n").unwrap();
    let made = ok(
        &home,
        "project.init",
        &[
            ("project", t("pg/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    assert_eq!(made.get_text("project"), Some("pg/api"));
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
    assert_eq!(
        call(
            &home,
            "store.connect",
            &[("name", t("pg")), ("url", t(&url)), password()]
        )
        .unwrap_err()
        .0,
        "Exists"
    );
    // the same database under a second name (#533 review P1-15): by its
    // URL, and through another spelling of the host by the schema's binding
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
            &[("name", t("pg-again")), ("url", t(&second)), password()],
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
            ("from_branch", t("main")),
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
    let status = ok(&home, "store.status", &[("name", t("pg"))]);
    assert_eq!(
        status.get("server").and_then(|s| s.get("reachable")),
        Some(&SdkValue::Bool(true)),
        "{status:?}"
    );

    assert_eq!(
        call(&home, "store.remove", &[("name", t("pg"))])
            .unwrap_err()
            .0,
        "InvalidInput"
    );
    // this process's own open copy does not block a disconnect
    ok(&home, "store.disconnect", &[("name", t("pg"))]);
    assert!(!paths::store_dir(&home, "pg").exists());
    let again = ok(
        &home,
        "store.connect",
        &[("name", t("pg2")), ("url", t(&url)), password()],
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
/// not a server that took no connection (#535 terminal pass, lane K3).
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
    let (server, _) = url.rsplit_once('/').unwrap();
    let missing = format!("{server}/lfs_no_such_database");
    let (code, message) = call(
        &home,
        "store.connect",
        &[
            ("name", t("gone")),
            ("url", t(&missing)),
            ("password", t("env:LAYERFS_TEST_PG_PASSWORD")),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(
        message.contains("the server refused the session"),
        "{message}"
    );
    assert!(!paths::store_dir(&home, "gone").exists());
    std::fs::remove_dir_all(&home).unwrap();
}
