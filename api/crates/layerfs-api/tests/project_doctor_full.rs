//! `project doctor --full` (`project.data`): everything main's head and each
//! Branch's head reach is read and checked, with `data` progress events and
//! nothing written to the Store. A damaged object is a `project.data` error
//! naming it, in each project that reaches it: an object two projects share
//! is read once per project. Without `--full` nothing of the data is read.
//! On Linux the test reports the `--full` call's time and peak memory (one
//! sample, `VmHWM` reset just before the call).
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfull-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

type Events = Vec<SdkValue>;

fn call_events(home: &Path, method: &str, fields: &[(&str, SdkValue)]) -> (SdkValue, Events) {
    let args = SdkValue::Map(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    );
    let mut events = Vec::new();
    let answer = layerfs_api::call(Some(home), method, args, &mut |event| events.push(event))
        .unwrap_or_else(|r| panic!("{method}: {r:?}"));
    (answer, events)
}

fn call(home: &Path, method: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    call_events(home, method, fields).0
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        other => panic!("{key}: {other:?} in {value:?}"),
    }
}

fn doctor(home: &Path, project: &str, full: bool) -> (SdkValue, Events) {
    call_events(
        home,
        "project.doctor",
        &[
            ("project", t(project)),
            ("full", SdkValue::Bool(full)),
            ("detail", SdkValue::Bool(true)),
        ],
    )
}

/// The checks the project's node shows, in order, with their outcomes.
fn rows(report: &SdkValue) -> Vec<(String, String)> {
    let tree = report.get("tree").unwrap();
    list(tree, "checks")
        .iter()
        .map(|row| {
            (
                row.get_text("check").unwrap().to_owned(),
                row.get_text("outcome").unwrap().to_owned(),
            )
        })
        .collect()
}

/// Peak resident memory of this process, in KiB, on Linux.
fn peak_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// A source tree: small files, a larger file of varied bytes, a folder and
/// a link.
fn source(tag: &str) -> PathBuf {
    let dir = folder(tag);
    std::fs::write(dir.join("a.txt"), "hello").unwrap();
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    let mut state = 0x2545_f491_u32;
    let big: Vec<u8> = (0..400_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state & 0xff) as u8
        })
        .collect();
    std::fs::write(dir.join("src/big.bin"), big).unwrap();
    symlink("a.txt", dir.join("link")).unwrap();
    dir
}

#[test]
fn sqlite_full_reads_main_and_each_branch_and_names_a_damaged_object_in_each_project() {
    let home = folder("home");
    let created = layerfs_api::call(
        Some(&home),
        "store.create",
        SdkValue::map([("name", t("shop"))]),
        &mut |_| {},
    );
    if let Err(refusal) = &created {
        if refusal.code == "Unsupported" {
            eprintln!("NOT_RUN: this host opens no sqlite Store: {refusal:?}");
            let _ = std::fs::remove_dir_all(&home);
            return;
        }
    }
    created.unwrap();
    check(&home, "sqlite", &|home| {
        // the Store loses the largest object, which every head of both
        // projects reaches
        let file = home.join("store/shop/store.sqlite");
        let file = if file.is_file() {
            file
        } else {
            find_store_file(home).expect("the Store file")
        };
        let db = rusqlite::Connection::open(&file).unwrap();
        let id: Vec<u8> = db
            .query_row(
                "SELECT object_id FROM object_location ORDER BY canonical_length DESC, object_id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        db.execute("DELETE FROM object_location WHERE object_id=?1", [&id])
            .unwrap();
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
        hex(&id)
    });
    let _ = std::fs::remove_dir_all(&home);
}

/// The same on a PostgreSQL Store, in a database of its own (a 2.0 Store
/// lives in that database's fixed `layerfs` schema), dropped at the end.
/// `LAYERFS_TEST_POSTGRES_URL` is `postgresql://user@host:port/db?...`.
#[test]
fn postgresql_full_reads_main_and_each_branch_and_names_a_damaged_object_in_each_project() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL unset");
        return;
    };
    let database = format!("pdfull_{}", std::process::id());
    let admin = |sql: &str| {
        let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        client.batch_execute(sql).unwrap();
    };
    admin(&format!("DROP DATABASE IF EXISTS {database}"));
    admin(&format!("CREATE DATABASE {database}"));
    let (base, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let (server, _) = base.rsplit_once('/').unwrap();
    let store_url = match query {
        "" => format!("{server}/{database}"),
        query => format!("{server}/{database}?{query}"),
    };
    let home = folder("pg-home");
    // the URL's user becomes a saved login (a trust-auth server takes any
    // password); the address is connected without it
    let (scheme, rest) = store_url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    call(
        &home,
        "store.save_auth",
        &[
            ("auth", t("pg")),
            ("user", t(user)),
            ("password", t("unused")),
        ],
    );
    call(
        &home,
        "store.connect",
        &[
            ("store", t("shop")),
            ("url", t(&format!("{scheme}://{address}"))),
            ("auth", t("pg")),
            ("synchronous_commit", t("off")),
        ],
    );
    // the Store doctor reaches the same Store through the shared admission
    let (store, _) = call_events(
        &home,
        "store.doctor",
        &[("store", t("shop")), ("detail", SdkValue::Bool(true))],
    );
    let store_rows = rows(&store);
    eprintln!("store doctor: {store_rows:?}");
    assert!(
        !store_rows.is_empty() && store_rows.iter().all(|(_, outcome)| outcome == "pass"),
        "{store_rows:?}"
    );
    check(&home, "pg", &|_| {
        let mut client = postgres::Client::connect(&store_url, postgres::NoTls).unwrap();
        let row = client
            .query_one(
                "SELECT object_id FROM layerfs.object_location ORDER BY canonical_length DESC, object_id LIMIT 1",
                &[],
            )
            .unwrap();
        let id: Vec<u8> = row.get(0);
        // the Store's own triggers and keys would refuse the damage
        client
            .batch_execute("SET session_replication_role = replica")
            .unwrap();
        client
            .execute(
                "DELETE FROM layerfs.object_location WHERE object_id=$1",
                &[&id],
            )
            .unwrap();
        hex(&id)
    });
    let _ = std::fs::remove_dir_all(&home);
    admin(&format!("DROP DATABASE IF EXISTS {database} WITH (FORCE)"));
}

/// Two projects of Store `shop` in `home`, two Branches of one; the doctor
/// without and with `--full`; then `damage` makes the Store lose one object
/// both projects reach (answering its id) and each project names it once.
fn check(home: &Path, engine: &str, damage: &dyn Fn(&Path) -> String) {
    // each case its own source: the cases run in parallel
    let tree = source(&format!("source-{engine}"));
    let folder_text = t(tree.to_str().unwrap());
    for project in ["shop/app", "shop/twin"] {
        call(
            home,
            "project.init",
            &[("project", t(project)), ("folder", folder_text.clone())],
        );
    }
    // two Branches of app, each one Commit ahead of main
    for (branch, path, content) in [("dev", "/new", "abc"), ("feature", "/src/more.rs", "x")] {
        call(
            home,
            "branch.fork",
            &[
                ("project", t("shop/app")),
                ("name", t(branch)),
                ("from", t("main")),
            ],
        );
        call(
            home,
            "file.write",
            &[
                ("project", t("shop/app")),
                ("branch", t(branch)),
                ("path", t(path)),
                ("content", t(content)),
            ],
        );
    }

    // without --full nothing of the data is read
    let (shallow, events) = doctor(home, "shop/app", false);
    assert_eq!(
        rows(&shallow),
        [
            ("project.record".to_owned(), "pass".to_owned()),
            ("project.listed".to_owned(), "pass".to_owned())
        ],
        "{shallow:?}"
    );
    assert_eq!(
        shallow.get("tree").unwrap().get_text("coverage"),
        Some("shallow")
    );
    assert_eq!(events.len(), 1, "one finished thing: {events:?}");

    // --full on the clean Store: three trees (main, dev, feature), no finding
    // `5` resets this process's peak resident size to its current one
    let before = std::fs::write("/proc/self/clear_refs", "5")
        .ok()
        .and_then(|()| peak_kib());
    let started = Instant::now();
    let (clean, events) = doctor(home, "shop/app", true);
    let elapsed = started.elapsed();
    let peak = peak_kib();
    eprintln!(
        "MEASURE project doctor --full: {} ms, peak RSS {:?} KiB (RSS at reset {:?} KiB)",
        elapsed.as_millis(),
        peak.filter(|_| before.is_some()),
        before
    );
    assert_eq!(clean.get("ok"), Some(&SdkValue::Bool(true)), "{clean:?}");
    assert_eq!(
        rows(&clean),
        [
            ("project.record".to_owned(), "pass".to_owned()),
            ("project.listed".to_owned(), "pass".to_owned()),
            ("project.data".to_owned(), "pass".to_owned())
        ],
        "{clean:?}"
    );
    assert_eq!(
        clean.get("tree").unwrap().get_text("coverage"),
        Some("full")
    );
    let events: Vec<&SdkValue> = events
        .iter()
        .map(|event| event.get("progress").expect("a progress event"))
        .collect();
    let data: Vec<&&SdkValue> = events
        .iter()
        .filter(|event| event.get_text("phase") == Some("data"))
        .collect();
    assert_eq!(
        data.iter()
            .map(|event| (event.get_int("done"), event.get_int("total")))
            .collect::<Vec<_>>(),
        [
            (Some(0), Some(3)),
            (Some(1), Some(3)),
            (Some(2), Some(3)),
            (Some(3), Some(3))
        ],
        "{events:?}"
    );
    let last = events.last().unwrap();
    assert_eq!(last.get_text("phase"), Some("self"), "{events:?}");
    assert!(last.get("node").is_some(), "{last:?}");

    layerfs_api::close_idle(std::time::Duration::ZERO);
    let lost = damage(home);
    for project in ["shop/app", "shop/twin"] {
        let (damaged, _) = doctor(home, project, true);
        assert_eq!(
            damaged.get("ok"),
            Some(&SdkValue::Bool(false)),
            "{damaged:?}"
        );
        let found: Vec<&SdkValue> = list(&damaged, "findings")
            .iter()
            .filter(|finding| finding.get_text("check") == Some("project.data"))
            .collect();
        assert!(!found.is_empty(), "{project}: {damaged:?}");
        assert!(found
            .iter()
            .all(|f| f.get_text("severity") == Some("error")));
        assert!(found.iter().all(|f| f.get_text("project") == Some(project)));
        let named: Vec<&&SdkValue> = found
            .iter()
            .filter(|f| f.get_text("message").is_some_and(|m| m.contains(&lost)))
            .collect();
        assert_eq!(
            named.len(),
            1,
            "read once per project: {project}: {found:?}"
        );
        assert!(
            named[0].get_text("message").unwrap().contains("is missing"),
            "{named:?}"
        );
        let row = rows(&damaged)
            .into_iter()
            .find(|(check, _)| check == "project.data");
        assert_eq!(row.map(|(_, outcome)| outcome).as_deref(), Some("error"));
    }
    layerfs_api::close_idle(std::time::Duration::ZERO);
    let _ = std::fs::remove_dir_all(&tree);
}

/// The one `store.sqlite` under `home`.
fn find_store_file(home: &Path) -> Option<PathBuf> {
    let mut stack = vec![home.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| name == "store.sqlite") {
                return Some(path);
            }
        }
    }
    None
}

fn hex(bytes: &[u8]) -> String {
    layerfs_bridge::sdk_values::hex(bytes)
}
