//! #819 P-B writes: the CLI's PostgreSQL file writes through the host
//! worker, on held write sessions, on a real server. Needs
//! LAYERFS_TEST_POSTGRES_URL: an administrator's URL (CREATEROLE, CREATEDB)
//! of a server that authenticates by password (SCRAM); prints NOT_RUN
//! without it. Each test makes its own role and database and counts that
//! role's backends, so a held session shows as backends that stay, and a
//! new login as a new one.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "../support/host_worker.rs"]
mod fixture;
#[path = "../support/postgresql.rs"]
mod pg;

use fixture::Fixture;
use layerfs_home::names::HOST_MACHINE;
use layerfs_protocol::sdk::SdkValue;
use pg::*;
use std::path::Path;
use std::time::{Duration, Instant};

/// A home with Store `pg` and project `p` on `db`, and Branch `work`,
/// which the writes change (`main` changes only through publish).
fn forked(db: &Db) -> Fixture {
    let home = connected(db, db.at());
    ok(&home.paths.home, &["branch", "fork", "pg/p", "work"]);
    home
}

/// The Commits of Branch `work` of `pg/p`.
fn commits(home: &Path) -> usize {
    let listed = ok(home, &["commit", "list", "pg/p", "work", "--limit", "200"]);
    match listed.get("items") {
        Some(SdkValue::List(items)) => items.len(),
        _ => panic!("no Commit list: {listed:?}"),
    }
}

/// A write's CLI answer names its new Commit.
fn committed(answer: &SdkValue) {
    assert!(answer.get_text("commit").is_some(), "{answer:?}");
}

fn text(home: &Path, path: &str) -> String {
    let read = ok(home, &["file", "read", "pg/p", "work", path]);
    read.get_text("text").unwrap_or_default().to_owned()
}

/// The worker's `postgresql` ping field.
fn sessions(home: &Fixture) -> SdkValue {
    home.ping().get("postgresql").cloned().unwrap_or_default()
}

fn count(value: &SdkValue, name: &str) -> i64 {
    value.get_int(name).unwrap_or(-1)
}

/// The kept outcome records of the home's host worker.
fn retained(home: &Path) -> Vec<SdkValue> {
    let dir = home.join("machine").join(HOST_MACHINE).join("retained");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .map(|entry| {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            layerfs_home::json::parse(&text).unwrap()
        })
        .collect()
}

/// The answer of a finished write: its value, or its refusal's code.
fn outcome(child: std::process::Child) -> Result<SdkValue, String> {
    let output = finish(child);
    let value = decode(&output);
    match output.status.success() {
        true => Ok(value),
        false => Err(value.get_text("error").unwrap_or("?").to_owned()),
    }
}

/// Waits until the worker lends `writes` write slots.
fn lending(home: &Fixture, writes: i64) -> SdkValue {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let now = sessions(home);
        if count(&now, "writes") == writes || Instant::now() > end {
            return now;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn postgresql_writes_commit_once_each_on_held_sessions() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "wheld");
    let home = forked(&db);
    let path = &home.paths.home;
    assert!(db.settle(0).is_empty(), "connect and init leave no backend");
    let before = commits(path);

    // a streamed write starts the worker and opens one write session
    let written = answered(
        path,
        &["file", "write", "pg/p", "work", "/w.txt"],
        Some("one\n"),
    );
    committed(&written);
    assert!(home.paths.socket.exists(), "the write started the worker");
    let held = sessions(&home);
    assert_eq!(count(&held, "idle"), 1, "one write session held: {held:?}");
    assert_eq!(count(&held, "writes"), 0, "no write slot lent");
    std::thread::sleep(Duration::from_millis(300));
    let backends = db.backends();
    assert!(!backends.is_empty(), "the held session keeps its backends");

    // later writes of every kind reuse it: no new login
    let changes = r#"[{"op":"mkdir","path":"/d"},{"op":"write","path":"/d/f","content":"x\n"}]"#;
    let later: [(&[&str], Option<&str>); 4] = [
        (&["file", "mkdir", "pg/p", "work", "/e"], None),
        (&["file", "write", "pg/p", "work", "/w.txt"], Some("two\n")),
        (&["file", "apply", "pg/p", "work"], Some(changes)),
        (&["file", "rm", "pg/p", "work", "/a.txt"], None),
    ];
    for (words, stdin) in later {
        let answer = answered(path, words, stdin);
        committed(&answer);
    }
    assert_eq!(commits(path), before + 5, "one Commit per write");
    assert_eq!(db.backends(), backends, "no new login for later writes");
    assert_eq!(text(path, "/w.txt"), "two\n");
    assert_eq!(text(path, "/d/f"), "x\n");
    let listed = ok(path, &["file", "list", "pg/p", "work", "/"]);
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}");
    };
    let names: Vec<_> = items.iter().filter_map(|i| i.get_text("path")).collect();
    assert!(!names.contains(&"/a.txt"), "{names:?}");
    assert!(retained(path).is_empty(), "no outcome was uncertain");

    // the direct route (the SDK's) sees the same Commits
    let args = SdkValue::map([
        ("project", SdkValue::text("pg/p")),
        ("branch", SdkValue::text("work")),
        ("limit", SdkValue::Int(200)),
    ]);
    let sdk = layerfs_api::call(Some(path), "commit.list", args, &mut |_| {}).unwrap();
    assert!(matches!(sdk.get("items"), Some(SdkValue::List(i)) if i.len() == before + 5));
    layerfs_api::close();

    // no password outside the saved login: not in logs, records or traces
    drain(&home);
    no_secret(path, &db.password);
}

#[test]
fn a_worker_killed_after_the_end_leaves_its_write_unknown_and_never_resent() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "wkill");
    let home = forked(&db);
    let path = &home.paths.home;
    ok(path, &["file", "mkdir", "pg/p", "work", "/warm"]);
    let before = commits(path);

    // the write's input is all sent; its publication waits on the lane
    let lane = Lane::hold(&db);
    let child = spawn(
        path,
        &["file", "write", "pg/p", "work", "/k.txt"],
        Some("k\n"),
    );
    assert_eq!(waiting_on_lock(&db, 1), 1, "the write waits on the lane");
    let worker = home.ping().get_int("pid").unwrap();
    let killed = std::process::Command::new("kill")
        .args(["-9", &worker.to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    assert_eq!(outcome(child).unwrap_err(), "Unknown");
    drop(lane);

    // nothing sent it again: at most its own one Commit, now and later
    std::thread::sleep(Duration::from_millis(500));
    let after = commits(path);
    assert!(
        after == before || after == before + 1,
        "{before} -> {after}"
    );
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(commits(path), after, "no later Commit of the lost write");

    // a later write starts a new worker and commits once
    let answer = call(
        path,
        &["file", "write", "pg/p", "work", "/n.txt"],
        Some("n\n"),
    );
    committed(&answer.unwrap());
    assert_eq!(commits(path), after + 1);
    drain(&home);
}

#[test]
fn a_server_drop_is_unknown_only_for_writes_in_flight() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "wdrop");
    let home = forked(&db);
    let path = home.paths.home.clone();
    ok(&path, &["file", "mkdir", "pg/p", "work", "/warm"]);
    let before = commits(&path);

    // writes in flight wait on the lane; the rest wait for a write slot,
    // unsent
    let lane = Lane::hold(&db);
    let callers: Vec<_> = (0..6)
        .map(|n| {
            let target = format!("/s{n}.txt");
            let words = ["file", "write", "pg/p", "work", &target];
            spawn(&path, &words, Some("s\n"))
        })
        .collect();
    let held = lending(&home, 2);
    assert_eq!(count(&held, "writes"), 2, "the write share: {held:?}");
    let in_flight = waiting_on_lock(&db, 1);
    assert!((1..=2).contains(&in_flight), "{in_flight}");
    std::thread::sleep(Duration::from_millis(300));
    let mut admin = db.admin();
    let terminated = admin
        .query(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
             WHERE datname = $1 AND usename = $1 AND wait_event_type = 'Lock'",
            &[&db.name],
        )
        .unwrap()
        .len();
    std::thread::sleep(Duration::from_millis(200));
    drop(lane);
    let outcomes: Vec<_> = callers.into_iter().map(outcome).collect();
    eprintln!("outcomes: {outcomes:?}");
    let lost: Vec<_> = outcomes.iter().filter_map(|o| o.as_ref().err()).collect();
    let served = outcomes.iter().filter(|o| o.is_ok()).count();
    assert_eq!(
        lost.len(),
        terminated,
        "only the writes in flight were lost"
    );
    assert!(lost.iter().all(|code| *code == "Unknown"), "{lost:?}");
    // each uncertain outcome was recorded before its answer
    let kept = retained(&path);
    assert_eq!(kept.len(), lost.len(), "{kept:?}");
    assert!(kept.iter().all(|r| r.get_text("kind") == Some("uncertain")));
    // nothing was resent: the served ones once, a lost one at most once
    let after = commits(&path);
    assert!(after >= before + served && after <= before + served + lost.len());
    let answer = call(&path, &["file", "mkdir", "pg/p", "work", "/later"], None);
    committed(&answer.unwrap());
    drain(&home);
}

#[test]
fn reads_stay_prompt_while_writes_wait_on_the_store_writer() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "wfair");
    let home = forked(&db);
    let path = home.paths.home.clone();
    ok(&path, READ);
    ok(&path, &["file", "mkdir", "pg/p", "work", "/warm"]);
    let before = commits(&path);

    // six writes wait on a held Store writer: streamed and single-change
    let lane = Lane::hold(&db);
    let writers: Vec<_> = (0..6)
        .map(|n| {
            let target = format!("/f{n}");
            match n % 2 {
                0 => spawn(
                    &path,
                    &["file", "write", "pg/p", "work", &target],
                    Some("f\n"),
                ),
                _ => spawn(&path, &["file", "mkdir", "pg/p", "work", &target], None),
            }
        })
        .collect();
    let held = lending(&home, 2);
    assert_eq!(count(&held, "writes"), 2, "the write share: {held:?}");
    assert!(waiting_on_lock(&db, 1) >= 1);

    // reads of the same Store keep their slots and answer at once
    for words in [READ, STAT, LIST, READ, STAT, LIST] {
        let began = Instant::now();
        call(&path, words, None).unwrap_or_else(|code| panic!("{words:?}: {code}"));
        let took = began.elapsed();
        assert!(took < Duration::from_secs(5), "{words:?} took {took:?}");
    }
    let readers: Vec<_> = (0..4).map(|_| spawn(&path, STAT, None)).collect();
    let began = Instant::now();
    for reader in readers {
        assert!(outcome(reader).is_ok());
    }
    assert!(began.elapsed() < Duration::from_secs(10));
    let ping = home.ping();
    assert_eq!(count(&sessions(&home), "writes"), 2, "still waiting");
    assert!(ping.get_int("ordinary").unwrap() <= 1, "{ping:?}");

    drop(lane);
    for writer in writers {
        committed(&outcome(writer).unwrap_or_else(|code| panic!("{code}")));
    }
    assert_eq!(commits(&path), before + 6, "each write committed once");
    assert_eq!(count(&lending(&home, 0), "writes"), 0);
    drain(&home);
}

/// As after a server restart: the held write session is gone and the
/// server refuses the new one (here the role may no longer log in). The
/// waiting write is refused before anything is sent, `Failed`, in plain
/// words with its check and no login; nothing committed or resent, and a
/// later write commits once.
#[test]
fn a_refused_new_session_is_said_plainly_with_nothing_sent() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "wrefuse");
    let home = forked(&db);
    let path = home.paths.home.clone();
    ok(&path, &["file", "mkdir", "pg/p", "work", "/warm"]);
    let before = commits(&path);
    let mut admin = db.admin();
    let login = |allowed: &str| format!("ALTER ROLE {} {allowed}", db.name);
    admin.batch_execute(&login("NOLOGIN")).unwrap();
    admin
        .query(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename = $1",
            &[&db.name],
        )
        .unwrap();
    assert!(db.settle(0).is_empty(), "the held session is gone");

    let output = finish(spawn(
        &path,
        &["file", "mkdir", "pg/p", "work", "/refused"],
        None,
    ));
    let refusal = decode(&output);
    assert!(!output.status.success(), "{refusal:?}");
    assert_eq!(refusal.get_text("error"), Some("Failed"), "{refusal:?}");
    let message = refusal.get_text("message").unwrap_or_default();
    assert_eq!(
        message,
        "the PostgreSQL server refused a new session (it may be restarting, \
         or the database, user or password changed); nothing was sent and \
         nothing was committed"
    );
    assert_eq!(
        refusal.get_text("fix"),
        Some("layerfs store get pg --with live")
    );
    assert!(!format!("{refusal:?}").contains(&db.password));

    admin.batch_execute(&login("LOGIN")).unwrap();
    assert_eq!(commits(&path), before, "nothing was committed");
    let answer = call(&path, &["file", "mkdir", "pg/p", "work", "/later"], None);
    committed(&answer.unwrap());
    assert_eq!(commits(&path), before + 1, "nothing was resent");
    assert!(retained(&path).is_empty(), "the refusal was definite");
    drain(&home);
    no_secret(&path, &db.password);
}
