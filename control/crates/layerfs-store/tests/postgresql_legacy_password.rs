//! A PostgreSQL Store recorded before saved logins (a user-only URL and a
//! `password` reference, no `auth`) still dials: the reference is resolved
//! on each call and put in beside the URL's own user, the record's or the
//! dialling machine's own, and the record is never converted. A URL built
//! here is compared, never printed: a failure shows its masked form.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::private_dir;
use layerfs_store::record::{url, url_on, OwnConnection, StoreRecord};
use std::path::{Path, PathBuf};

const ADDRESS: &str = "postgresql://owner@db.example:5432/layerfs_db?sslmode=disable";

/// A private folder of this test's own under the system's temporary folder.
fn folder(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "layerfs-pg-legacy-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    private_dir(&dir).expect("a private folder");
    dir
}

/// `file` holding `password` and a trailing newline, as a preset writes it.
fn password_file(dir: &Path, name: &str, password: &str) -> String {
    let file = dir.join(name);
    std::fs::write(&file, format!("{password}\n")).expect("a password file");
    file.display().to_string()
}

/// A record as a home written before saved logins holds it.
fn legacy(home: &Path, url: &str, reference: &str, machines: Option<SdkValue>) -> StoreRecord {
    let mut fields = vec![
        ("name", SdkValue::text("pg")),
        ("kind", SdkValue::text("postgresql")),
        ("url", SdkValue::text(url)),
        ("schema", SdkValue::text("layerfs")),
        ("synchronous_commit", SdkValue::text("off")),
        ("password", SdkValue::text(reference)),
    ];
    if let Some(machines) = machines {
        fields.push(("machines", machines));
    }
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v));
    StoreRecord::from_record("pg", SdkValue::Map(fields.collect())).in_home(home)
}

/// `url` with the password between `user:` and `@` masked.
fn masked(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(scheme), Some(at)) => {
            let userinfo = &url[scheme + 3..at];
            let user = userinfo.split(':').next().unwrap_or("");
            format!("{}{user}:***{}", &url[..scheme + 3], &url[at..])
        }
        _ => "<no userinfo>".to_owned(),
    }
}

fn assert_dials(built: &str, expected: &str) {
    assert!(
        built == expected,
        "dialled {} (masked), expected {} (masked), or the password differs",
        masked(built),
        masked(expected)
    );
}

/// The refusal `dialled` must be; a URL built instead is never printed.
fn refused_with(
    dialled: Result<String, layerfs_bridge::control::SdkRefusal>,
    what: &str,
) -> layerfs_bridge::control::SdkRefusal {
    match dialled {
        Ok(built) => panic!("{what} dialled {} (masked)", masked(&built)),
        Err(refusal) => refusal,
    }
}

#[test]
fn a_file_reference_dials_with_its_password_beside_the_urls_user() {
    let home = folder("file");
    let reference = format!("file:{}", password_file(&home, "pw", "s3cr@t/x y"));
    let record = legacy(&home, ADDRESS, &reference, None);
    let expected = "postgresql://owner:s3cr%40t%2Fx%20y@db.example:5432/layerfs_db?sslmode=disable";
    assert_dials(&url(&record).expect("the Store's URL"), expected);
    // a machine with no connection of its own dials the same
    assert_dials(
        &url_on(&record, "box").expect("the machine's URL"),
        expected,
    );
    // read only: the record keeps its reference and names no login
    assert_eq!(record.text("password"), Some(reference.as_str()));
    assert_eq!(record.text("auth"), None);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_env_reference_dials_with_the_variables_value() {
    let home = folder("env");
    let variable = format!("LAYERFS_TEST_LEGACY_PW_{}", std::process::id());
    std::env::set_var(&variable, "from-env");
    let record = legacy(&home, ADDRESS, &format!("env:{variable}"), None);
    let built = url(&record).expect("the Store's URL");
    assert_dials(
        &built,
        "postgresql://owner:from-env@db.example:5432/layerfs_db?sslmode=disable",
    );
    std::env::remove_var(&variable);
    let unset = refused_with(url(&record), "an unset variable");
    assert_eq!(unset.code, "Unavailable", "{}", unset.message);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_unreadable_file_is_unavailable_and_a_bad_reference_invalid() {
    let home = folder("refusals");
    let missing = format!("file:{}", home.join("absent").display());
    let refusal = url(&legacy(&home, ADDRESS, &missing, None)).err();
    assert_eq!(refusal.map(|r| r.code).as_deref(), Some("Unavailable"));

    for reference in ["plain-text", "file:relative/pw"] {
        let refusal = url(&legacy(&home, ADDRESS, reference, None)).err();
        assert_eq!(
            refusal.map(|r| r.code).as_deref(),
            Some("InvalidInput"),
            "{reference}"
        );
    }

    // an address with no user (one a 2.0 connect gives) names the fix
    let reference = format!("file:{}", password_file(&home, "pw", "pw"));
    let no_user = legacy(
        &home,
        "postgresql://db.example/layerfs_db",
        &reference,
        None,
    );
    let refusal = refused_with(url(&no_user), "a URL with no user");
    assert_eq!(refusal.code, "InvalidInput");
    let fix = refusal.detail.get_text("fix").unwrap_or("");
    assert_eq!(fix, "layerfs store update pg --auth <auth>");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_machines_own_reference_and_address_dial_for_that_machine() {
    let home = folder("own");
    let store_pw = format!("file:{}", password_file(&home, "store", "store-pw"));
    let own_pw = format!("file:{}", password_file(&home, "own", "own-pw"));
    let machines = SdkValue::map([
        (
            "only-password",
            SdkValue::map([("password", SdkValue::text(&own_pw))]),
        ),
        (
            "both",
            SdkValue::map([
                (
                    "url",
                    SdkValue::text("postgresql://guest@10.0.0.2/layerfs_db"),
                ),
                ("password", SdkValue::text(&own_pw)),
            ]),
        ),
        (
            "address",
            SdkValue::map([(
                "url",
                SdkValue::text("postgresql://guest@10.0.0.3/layerfs_db"),
            )]),
        ),
    ]);
    let record = legacy(&home, ADDRESS, &store_pw, Some(machines));
    let on = |machine: &str| url_on(&record, machine).expect("the machine's URL");
    assert_dials(
        &on("only-password"),
        "postgresql://owner:own-pw@db.example:5432/layerfs_db?sslmode=disable",
    );
    assert_dials(&on("both"), "postgresql://guest:own-pw@10.0.0.2/layerfs_db");
    assert_dials(
        &on("address"),
        "postgresql://guest:store-pw@10.0.0.3/layerfs_db",
    );

    // another machine's write keeps every older entry as it was
    let own = OwnConnection {
        url: Some("postgresql://db.example/other".to_owned()),
        password: None,
    };
    let written = record.clone().with_own_connection("new", &own);
    for machine in ["only-password", "both", "address"] {
        assert_eq!(
            written.own_connection(machine),
            record.own_connection(machine),
            "{machine}"
        );
    }
    // a 2.0 connect of a machine replaces its whole entry
    let replaced = record.with_own_connection("both", &own);
    assert_eq!(replaced.own_connection("both"), Some(own));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_saved_login_wins_over_an_older_reference() {
    let home = folder("auth");
    layerfs_store::postgresql::login::save(&home, "main", "role", "login-pw").expect("a login");
    let reference = format!("file:{}", password_file(&home, "pw", "old-pw"));
    let record = legacy(&home, ADDRESS, &reference, None).with("auth", SdkValue::text("main"));
    assert_dials(
        &url(&record).expect("the Store's URL"),
        "postgresql://role:login-pw@db.example:5432/layerfs_db?sslmode=disable",
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// With LAYERFS_TEST_PG_URL (a server URL with its user and no password),
/// a legacy record of it reaches the server through `store.status`'s read.
/// A trust-auth test server accepts any password, so this proves only that
/// the call goes through with the reference resolved, not the password.
#[test]
fn a_legacy_record_reaches_a_real_server() {
    let Ok(server) = std::env::var("LAYERFS_TEST_PG_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_URL unset");
        return;
    };
    let home = folder("real");
    let reference = format!("file:{}", password_file(&home, "pw", "any password"));
    let record = legacy(&home, &server, &reference, None);
    let status = layerfs_store::postgresql::connect::status(&record);
    assert_eq!(
        status.get("reachable"),
        Some(&SdkValue::Bool(true)),
        "the server was not reached: {:?}",
        status.get("error").or(status.get("unavailable"))
    );
    let _ = std::fs::remove_dir_all(&home);
}
