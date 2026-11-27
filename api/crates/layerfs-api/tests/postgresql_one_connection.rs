//! #819 P-A: a fresh CLI Store read authenticates once and still Full-checks
//! the live schema. PostgreSQL 14+ is required for the session counter.
//! Needs LAYERFS_TEST_POSTGRES_URL, with a PostgreSQL URL and CREATEDB.
use layerfs_api::Client;
use layerfs_bridge::control::SdkValue;
use layerfs_store::record::StoreRecord;
use postgres::{Client as Postgres, Config, NoTls};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct Fixture {
    admin: Config,
    database: String,
    home: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(mut admin) = self.admin.connect(NoTls) {
            let _ = admin.batch_execute(&format!("DROP DATABASE {} WITH (FORCE)", self.database));
        }
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn cli(home: &Path, method: &str, args: SdkValue) -> layerfs_home::outcome::Outcome {
    layerfs_api::call_scoped(Some(home), None, method, args, &mut |_| {}, Client::Cli)
}

fn file(home: &Path, method: &str, path: &str) -> layerfs_home::outcome::Outcome {
    cli(
        home,
        method,
        SdkValue::map([
            ("project", SdkValue::text("shop/api")),
            ("branch", SdkValue::text("main")),
            ("path", SdkValue::text(path)),
        ]),
    )
}

fn database_work(admin: &mut Postgres, database: &str) -> (i64, i32) {
    let row = admin
        .query_one(
            "SELECT pg_stat_get_db_sessions(oid), pg_stat_get_db_numbackends(oid)
             FROM pg_database WHERE datname = $1",
            &[&database],
        )
        .unwrap();
    (row.get(0), row.get(1))
}

fn await_sessions(admin: &mut Postgres, database: &str, minimum: i64) -> i64 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let (sessions, backends) = database_work(admin, database);
        if sessions >= minimum && backends == 0 {
            return sessions;
        }
        assert!(
            Instant::now() < deadline,
            "{sessions} sessions, {backends} active"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn one_login_per_postgresql_read_and_a_fresh_full_check() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let config: Config = url.parse().expect("a valid PostgreSQL test URL");
    let (scheme, rest) = url.split_once("://").expect("a PostgreSQL test URL");
    let (server, database_with_query) = rest.rsplit_once('/').expect("a database URL path");
    let address = server.rsplit_once('@').map_or(server, |(_, host)| host);
    let query = database_with_query
        .split_once('?')
        .map_or(String::new(), |(_, query)| format!("?{query}"));
    let database = format!("lfs819_pa_{}", std::process::id());
    let mut admin = config.connect(NoTls).unwrap();
    admin
        .batch_execute(&format!("CREATE DATABASE {database}"))
        .unwrap();
    let fixture = Fixture {
        admin: config,
        database,
        home: std::env::temp_dir().join(format!("lfs819-pa-{}", std::process::id())),
    };
    let source = fixture.home.join("source");
    let home = fixture.home.join("home");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), "hello\n").unwrap();
    cli(
        &home,
        "store.connect",
        SdkValue::map([
            ("store", SdkValue::text("shop")),
            (
                "url",
                SdkValue::text(format!("{scheme}://{address}/{}{query}", fixture.database)),
            ),
            (
                "user",
                SdkValue::text(fixture.admin.get_user().unwrap_or("postgres")),
            ),
            (
                "password",
                SdkValue::text(
                    std::str::from_utf8(fixture.admin.get_password().unwrap_or(b"unused")).unwrap(),
                ),
            ),
        ]),
    )
    .unwrap();
    cli(
        &home,
        "project.init",
        SdkValue::map([
            ("project", SdkValue::text("shop/api")),
            ("folder", SdkValue::text(source.to_str().unwrap())),
        ]),
    )
    .unwrap();
    layerfs_api::close();
    let before = await_sessions(&mut admin, &fixture.database, 1);
    std::thread::scope(|threads| {
        for i in 0..10 {
            let home = &home;
            threads.spawn(move || match i % 3 {
                0 => assert_eq!(
                    file(home, "file.read", "/a.txt").unwrap().get_text("text"),
                    Some("hello\n"),
                ),
                1 => assert_eq!(
                    file(home, "file.stat", "/a.txt").unwrap().get_text("type"),
                    Some("file"),
                ),
                _ => assert!(matches!(
                    file(home, "file.list", "/").unwrap().get("items"),
                    Some(SdkValue::List(items)) if items.len() == 1
                )),
            });
        }
    });
    let after = await_sessions(&mut admin, &fixture.database, before + 10);
    assert_eq!(after - before, 10, "exactly one backend per read");

    let record = StoreRecord::read(&home, "shop").unwrap().unwrap();
    record
        .clone()
        .with("binding", SdkValue::Bytes(vec![9; 32]))
        .write(&home)
        .unwrap();
    let refused = file(&home, "file.read", "/a.txt").unwrap_err();
    assert_eq!(refused.code, "Failed");
    assert!(refused.message.ends_with("Malformed"), "{refused:?}");
    record.write(&home).unwrap();
    let after_binding = await_sessions(&mut admin, &fixture.database, after + 1);
    assert_eq!(
        after_binding - after,
        1,
        "no retry after a binding mismatch"
    );

    // Adding a column that no file-read query names changes only the Full
    // schema contract. The next fresh read must refuse it at open.
    let mut damage = fixture.admin.clone();
    damage.dbname(&fixture.database);
    {
        let mut session = damage.connect(NoTls).unwrap();
        session
            .batch_execute(
                "SET statement_timeout TO '5s';
                 ALTER TABLE layerfs.history_meta ADD COLUMN unexpected text",
            )
            .unwrap();
    }
    let before_refusal = await_sessions(&mut admin, &fixture.database, after_binding + 1);
    assert_eq!(before_refusal - after_binding, 1);
    let refused = file(&home, "file.read", "/a.txt").unwrap_err();
    assert_eq!(refused.code, "Failed");
    assert!(refused.message.ends_with("Malformed"), "{refused:?}");
    let after_refusal = await_sessions(&mut admin, &fixture.database, before_refusal + 1);
    assert_eq!(
        after_refusal - before_refusal,
        1,
        "no retry after validation"
    );
    eprintln!("10 reads used 10 sessions; binding and Full schema refusals used one each");
}
