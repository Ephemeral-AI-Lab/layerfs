//! #819 P-B §8.7: the CLI's PostgreSQL file reads through the host worker,
//! on a real server. Needs LAYERFS_TEST_POSTGRES_URL: an administrator's
//! URL (CREATEROLE, CREATEDB) of a server that authenticates by password
//! (SCRAM); prints NOT_RUN without it. Each test makes its own role and
//! database and counts that role's backends, so a held session shows as one
//! backend that stays, and each new login as a new one.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;

use fixture::Fixture;
use layerfs_api::names::HOST_MACHINE;
use layerfs_api::worker::{read_host_frame, write_host_frame, HOST_MAX_REPLY, MAX_HELLO};
use layerfs_bridge::control::SdkValue;
use postgres::{config::Host, Client as Postgres, Config, NoTls};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CLI: &str = env!("CARGO_BIN_EXE_layerfs");
const DEADLINE: Duration = Duration::from_secs(40);

struct Server {
    admin: Config,
    host: String,
    port: u16,
}

fn server() -> Option<Server> {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return None;
    };
    let admin: Config = url.parse().expect("a PostgreSQL URL");
    let Host::Tcp(host) = admin.get_hosts()[0].clone() else {
        panic!("a TCP server address");
    };
    let port = admin.get_ports().first().copied().unwrap_or(5432);
    Some(Server { admin, host, port })
}

fn random() -> String {
    let mut bytes = [0u8; 12];
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut bytes)
        .unwrap();
    layerfs_bridge::sdk_values::hex(&bytes)
}

/// One role and its database; dropped with the test.
struct Db {
    admin: Config,
    name: String,
    password: String,
    host: String,
    port: u16,
}

impl Db {
    fn new(server: &Server, tag: &str) -> Self {
        let name = format!("lpb_{tag}_{}", std::process::id());
        let password = random();
        let db = Db {
            admin: server.admin.clone(),
            name,
            password,
            host: server.host.clone(),
            port: server.port,
        };
        let mut admin = db.admin();
        for statement in [
            format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", db.name),
            format!("DROP ROLE IF EXISTS {}", db.name),
            format!("CREATE ROLE {} LOGIN PASSWORD '{}'", db.name, db.password),
            format!("CREATE DATABASE {0} OWNER {0}", db.name),
            // a PostgreSQL compaction reads the server's activity
            format!("GRANT pg_read_all_stats TO {}", db.name),
        ] {
            admin.batch_execute(&statement).unwrap();
        }
        db
    }

    fn admin(&self) -> Postgres {
        self.admin.connect(NoTls).unwrap()
    }

    /// The server's own address.
    fn at(&self) -> (&str, u16) {
        (&self.host, self.port)
    }

    fn url(&self, (host, port): (&str, u16)) -> String {
        format!("postgresql://{host}:{port}/{}?sslmode=disable", self.name)
    }

    /// The role's backends now.
    fn backends(&self) -> BTreeSet<i32> {
        let rows = (self.admin())
            .query(
                "SELECT pid FROM pg_stat_activity WHERE datname = $1 AND usename = $1",
                &[&self.name],
            )
            .unwrap();
        rows.iter().map(|row| row.get(0)).collect()
    }

    /// The role's backends once they settle to `count` (a closed session's
    /// backend leaves within the deadline).
    fn settle(&self, count: usize) -> BTreeSet<i32> {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let backends = self.backends();
            if backends.len() == count || Instant::now() > end {
                return backends;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn rotate(&mut self) {
        self.password = random();
        (self.admin())
            .batch_execute(&format!(
                "ALTER ROLE {} PASSWORD '{}'",
                self.name, self.password
            ))
            .unwrap();
    }

    fn generation(&self) -> i64 {
        let mut config = self.admin.clone();
        config.dbname(&self.name);
        let mut client = config.connect(NoTls).unwrap();
        let row = client
            .query_one(
                "SELECT catalog_incarnation FROM layerfs.history_meta WHERE id = 1",
                &[],
            )
            .unwrap();
        row.get(0)
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        if let Ok(mut admin) = self.admin.connect(NoTls) {
            let _ = admin.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
            let _ = admin.batch_execute(&format!("DROP ROLE IF EXISTS {}", self.name));
        }
    }
}

fn finish(mut child: std::process::Child) -> Output {
    let end = Instant::now() + DEADLINE;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > end {
            let _ = child.kill();
            panic!("layerfs did not end within {DEADLINE:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    child.wait_with_output().unwrap()
}

fn spawn(home: &Path, words: &[&str], stdin: Option<&str>) -> std::process::Child {
    let mut child = Command::new(CLI)
        .args(words)
        .arg("--json")
        .env("LAYERFS_HOME", home)
        .env_remove("LAYERFS_TRACE")
        .env_remove("LAYERFS_TRACE_STEP")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    child
}

fn decode(output: &Output) -> SdkValue {
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let text = std::str::from_utf8(bytes).unwrap();
    layerfs_api::json::parse(text).unwrap_or_else(|error| panic!("{error:?}: {text}"))
}

/// The answer of `layerfs <words>`, or its refusal code.
fn call(home: &Path, words: &[&str], stdin: Option<&str>) -> Result<SdkValue, String> {
    let output = finish(spawn(home, words, stdin));
    let value = decode(&output);
    match output.status.success() {
        true => Ok(value),
        false => Err(value.get_text("error").unwrap_or("?").to_owned()),
    }
}

fn ok(home: &Path, words: &[&str]) -> SdkValue {
    call(home, words, None).unwrap_or_else(|code| panic!("{words:?} refused {code}"))
}

const READ: &[&str] = &["file", "read", "pg/p", "main", "/a.txt"];
const STAT: &[&str] = &["file", "stat", "pg/p", "main", "/a.txt"];
const LIST: &[&str] = &["file", "list", "pg/p", "main", "/"];

/// A home with PostgreSQL Store `pg` connected at `port` and project `p`.
fn connected(db: &Db, at: (&str, u16)) -> Fixture {
    let home = Fixture::new();
    connect(&home.paths.home, db, at);
    let source = home.paths.home.join("source");
    std::fs::create_dir_all(source.join("dir")).unwrap();
    std::fs::write(source.join("a.txt"), "hello\n").unwrap();
    std::fs::write(source.join("dir/b.txt"), "two\n").unwrap();
    ok(
        &home.paths.home,
        &["project", "init", "pg/p", source.to_str().unwrap()],
    );
    home
}

fn connect(home: &Path, db: &Db, at: (&str, u16)) {
    connect_as(home, "pg", db, at);
}

fn connect_as(home: &Path, name: &str, db: &Db, at: (&str, u16)) {
    let url = db.url(at);
    let words = ["store", "connect", name, &url, "--user", &db.name];
    let words = [&words[..], &["--password-stdin"]].concat();
    call(home, &words, Some(&db.password)).unwrap();
}

fn revision(home: &Path) -> String {
    let record = std::fs::read_to_string(home.join("store/pg/record.json")).unwrap();
    let record = layerfs_api::json::parse(&record).unwrap();
    record.get_text("login_revision").unwrap().to_owned()
}

/// One raw host request, as a caller that read the record earlier sends it.
fn raw(home: &Fixture, revision: &str, password: &str) -> SdkValue {
    let socket = sent(home, revision, password, "pb-raw-1");
    let end = Instant::now() + Duration::from_secs(10);
    read_host_frame(&socket, HOST_MAX_REPLY, Some(end))
        .unwrap()
        .unwrap()
}

/// The socket of one raw `fs.stat` request `id`, sent and not yet answered.
fn sent(home: &Fixture, revision: &str, password: &str, id: &str) -> UnixStream {
    let socket = UnixStream::connect(&home.paths.socket).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    let hello = read_host_frame(&socket, MAX_HELLO, Some(end))
        .unwrap()
        .unwrap();
    let args = SdkValue::map([
        ("method", SdkValue::text("fs.stat")),
        ("store", SdkValue::text("pg")),
        ("project", SdkValue::text("p")),
        (
            "fields",
            SdkValue::map([
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text("/a.txt")),
            ]),
        ),
        ("login_revision", SdkValue::text(revision)),
        ("password", SdkValue::text(password)),
    ]);
    let request = SdkValue::map([
        ("op", SdkValue::text("store.postgresql_read")),
        ("args", args),
        ("generation", hello.get("generation").unwrap().clone()),
        ("request_id", SdkValue::text(id)),
    ]);
    write_host_frame(&socket, &request, HOST_MAX_REPLY, end).unwrap();
    socket
}

fn error_code(answer: &SdkValue) -> Option<&str> {
    answer.get("error").and_then(|error| error.get_text("code"))
}

fn drain(home: &Fixture) {
    if home.paths.control.exists() {
        home.drain();
        let end = Instant::now() + Duration::from_secs(10);
        while home.paths.socket.exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Every file of `root` but the saved login holds no `password` byte run.
fn no_secret(root: &Path, password: &str) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let kind = std::fs::symlink_metadata(&path).unwrap().file_type();
        if kind.is_dir() {
            no_secret(&path, password);
        } else if kind.is_file() && !path.ends_with("secret/login/pg.json") {
            let bytes = std::fs::read(&path).unwrap();
            let found = bytes
                .windows(password.len())
                .any(|window| window == password.as_bytes());
            assert!(!found, "{} holds the password", path.display());
        }
    }
}

#[test]
fn held_session_serves_later_reads_without_a_login() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "held");
    let home = connected(&db, db.at());
    let path = &home.paths.home;
    assert!(db.settle(0).is_empty(), "connect and init leave no backend");
    let first = ok(path, READ);
    assert_eq!(first.get_text("text"), Some("hello\n"));
    assert!(
        home.paths.socket.exists(),
        "the read started the host worker"
    );
    let held = db.settle(1);
    assert_eq!(held.len(), 1, "one held session after the first read");
    let stat = ok(path, STAT);
    let list = ok(path, LIST);
    assert_eq!(stat.get_text("type"), Some("file"));
    assert!(matches!(list.get("items"), Some(SdkValue::List(items)) if items.len() == 2));
    for _ in 0..5 {
        assert_eq!(ok(path, READ).get_text("text"), Some("hello\n"));
    }
    assert_eq!(db.backends(), held, "no new login for later reads");

    // The direct route (the SDK's) answers the same.
    for (words, routed) in [READ, STAT, LIST].iter().zip([&first, &stat, &list]) {
        let method = format!("file.{}", words[1]);
        let args = SdkValue::map([
            ("project", SdkValue::text("pg/p")),
            ("branch", SdkValue::text("main")),
            ("path", SdkValue::text(words[4])),
        ]);
        let sdk = layerfs_api::call(Some(path), &method, args, &mut |_| {}).unwrap();
        assert_eq!(&sdk, routed, "{method} through the worker matches the SDK");
    }
    layerfs_api::close();

    // No password outside the saved login: not in logs, records or traces.
    drain(&home);
    no_secret(path, &db.password);
    #[cfg(target_os = "linux")]
    {
        // a worker that served a PostgreSQL call is not dumpable
        ok(path, READ);
        let worker = home.ping().get_int("pid").unwrap();
        let environ = std::fs::read(format!("/proc/{worker}/environ"));
        assert!(environ.is_err(), "another process read the worker's memory");
        drain(&home);
    }
}

#[test]
fn rotation_retires_old_sessions_and_refuses_the_old_revision() {
    let Some(server) = server() else { return };
    let mut db = Db::new(&server, "rot");
    let home = connected(&db, db.at());
    let path = &home.paths.home;
    ok(path, READ);
    let old_backends = db.settle(1);
    let old_revision = revision(path);
    let old_password = db.password.clone();

    // A password changed on the server alone keeps the live session until
    // an explicit reconnect (§8.7); the connect is what rotates.
    db.rotate();
    ok(path, STAT);
    assert_eq!(db.backends(), old_backends);
    let url = db.url(db.at());
    let reconnect = ["store", "connect", "pg", &url, "--password-stdin"];
    call(path, &reconnect, Some(&db.password)).unwrap();
    let new_revision = revision(path);
    assert_ne!(new_revision, old_revision);
    assert!(
        db.settle(0).is_empty(),
        "the connect retired the held session"
    );

    let answer = ok(path, READ);
    assert_eq!(answer.get_text("text"), Some("hello\n"));
    let new_backends = db.settle(1);
    assert!(
        new_backends.is_disjoint(&old_backends),
        "a new login served it"
    );

    // A call admitted with the old login (read before the connect) is
    // refused with nothing run, and logs nothing in.
    let stale = raw(&home, &old_revision, &old_password);
    assert_eq!(error_code(&stale), Some("Conflict"), "{stale:?}");
    assert_eq!(db.backends(), new_backends);

    // The current revision with another password never reaches a held
    // session: it dials its own login, which the server refuses.
    let wrong = raw(&home, &new_revision, "not-the-password");
    assert!(wrong.get("error").is_some(), "{wrong:?}");
    // a refused login never names the password it sent
    let said = layerfs_api::json::print(&wrong);
    assert!(!said.contains("not-the-password"), "{said}");
    assert_eq!(db.settle(0).len(), 0, "the held session was retired");
    ok(path, READ);
    assert_eq!(db.settle(1).len(), 1);

    // Disconnect retires the worker's sessions too.
    ok(path, &["store", "disconnect", "pg"]);
    assert!(
        db.settle(0).is_empty(),
        "disconnect closed the held session"
    );
    let gone = call(path, READ, None).unwrap_err();
    assert!(
        matches!(gone.as_str(), "NotFound" | "NoSuchStore"),
        "{gone}"
    );
    drain(&home);
}

/// Holds `table` locked against reads on a session of its own until dropped.
struct Locked {
    _session: Postgres,
}

impl Locked {
    fn new(db: &Db, table: &str) -> Self {
        let mut config = db.admin.clone();
        config.dbname(&db.name);
        let mut client = config.connect(NoTls).unwrap();
        client
            .batch_execute(&format!(
                "BEGIN; LOCK TABLE layerfs.{table} IN ACCESS EXCLUSIVE MODE"
            ))
            .unwrap();
        Self { _session: client }
    }
}

#[test]
fn a_server_drop_is_unknown_only_for_calls_in_flight() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "drop");
    let home = connected(&db, db.at());
    let path = home.paths.home.clone();
    ok(&path, READ);
    db.settle(1);

    // The worker's sessions wait on a lock inside their statement; the
    // calls beyond the per-Store bound wait for a session, unsent.
    let lock = Locked::new(&db, "layer");
    let callers: Vec<_> = (0..6).map(|_| spawn(&path, STAT, None)).collect();
    let mut admin = db.admin();
    let waiting = |admin: &mut Postgres| -> Vec<i32> {
        let rows = admin
            .query(
                "SELECT pid FROM pg_stat_activity WHERE datname = $1 AND usename = $1
                 AND wait_event_type = 'Lock'",
                &[&db.name],
            )
            .unwrap();
        rows.iter().map(|row| row.get(0)).collect()
    };
    let end = Instant::now() + Duration::from_secs(20);
    let in_flight = loop {
        let pids = waiting(&mut admin);
        if pids.len() == 4 || Instant::now() > end {
            break pids;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        in_flight.len(),
        4,
        "the per-Store bound: four sessions in flight"
    );
    for pid in &in_flight {
        admin
            .execute("SELECT pg_terminate_backend($1)", &[pid])
            .unwrap();
    }
    std::thread::sleep(Duration::from_millis(200));
    drop(lock);
    let outcomes: Vec<_> = callers
        .into_iter()
        .map(|child| {
            let output = finish(child);
            let value = decode(&output);
            match output.status.success() {
                true => Ok(value),
                false => Err(value.get_text("error").unwrap_or("?").to_owned()),
            }
        })
        .collect();
    let lost: Vec<_> = outcomes.iter().filter_map(|o| o.as_ref().err()).collect();
    let served = outcomes.iter().filter(|o| o.is_ok()).count();
    eprintln!("outcomes: {outcomes:?}");
    assert_eq!(lost.len(), 4, "only the four calls in flight were lost");
    assert!(lost.iter().all(|code| *code == "Unknown"), "{lost:?}");
    assert_eq!(served, 2, "the waiting calls ran once on new sessions");
    assert_eq!(ok(&path, READ).get_text("text"), Some("hello\n"));
    drain(&home);
}

/// A TCP relay to the server whose connections can be cut silently: a cut
/// connection stays open on both ends and forwards nothing, as a NAT that
/// dropped its mapping does.
struct Relay {
    port: u16,
    cut: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    /// Cut connections on which the client sent more than a Terminate.
    probed: Arc<AtomicUsize>,
}

impl Relay {
    fn new(host: &str, port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let local = listener.local_addr().unwrap().port();
        let cut = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let upstream = format!("{host}:{port}");
        let probed = Arc::new(AtomicUsize::new(0));
        let (cuts, stops, probes) = (cut.clone(), stop.clone(), probed.clone());
        std::thread::spawn(move || {
            for client in listener.incoming() {
                if stops.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(client) = client else { continue };
                let Ok(server) = TcpStream::connect(&upstream) else {
                    continue;
                };
                let era = cuts.load(Ordering::SeqCst);
                for (outbound, mut from, mut to) in [
                    (
                        true,
                        client.try_clone().unwrap(),
                        server.try_clone().unwrap(),
                    ),
                    (false, server, client),
                ] {
                    let (cuts, probes) = (cuts.clone(), probes.clone());
                    std::thread::spawn(move || {
                        let mut buffer = [0u8; 16384];
                        let mut counted = false;
                        while let Ok(read) = from.read(&mut buffer) {
                            if read == 0 {
                                let _ = to.shutdown(std::net::Shutdown::Write);
                                return;
                            }
                            if cuts.load(Ordering::SeqCst) != era {
                                // a plaintext Terminate is `X` and length 4
                                let terminate = buffer[..read] == [b'X', 0, 0, 0, 4];
                                if outbound && !terminate && !counted {
                                    counted = true;
                                    probes.fetch_add(1, Ordering::SeqCst);
                                }
                                continue;
                            }
                            if to.write_all(&buffer[..read]).is_err() {
                                return;
                            }
                        }
                    });
                }
            }
        });
        Self {
            port: local,
            cut,
            stop,
            probed,
        }
    }

    fn cut(&self) {
        self.cut.fetch_add(1, Ordering::SeqCst);
    }

    fn probed(&self) -> usize {
        self.probed.load(Ordering::SeqCst)
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

#[test]
fn a_silent_cut_is_found_before_the_call_is_sent() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "cut");
    let relay = Relay::new(&server.host, server.port);
    // the relay listens in this process, beside the CLI and its worker
    let home = connected(&db, ("127.0.0.1", relay.port));
    let path = &home.paths.home;
    ok(path, READ);
    let held = db.settle(1);
    relay.cut();
    let began = Instant::now();
    let answer = ok(path, STAT);
    let took = began.elapsed();
    assert_eq!(answer.get_text("type"), Some("file"));
    // the readiness check waited its 5 s bound, then a new login served it
    assert!(took >= Duration::from_secs(5), "{took:?}");
    assert!(took < Duration::from_secs(20), "{took:?}");
    let after = db.backends();
    assert_eq!(after.difference(&held).count(), 1, "exactly one new login");
    assert_eq!(ok(path, READ).get_text("text"), Some("hello\n"));
    assert_eq!(db.backends(), after, "the new session is held");
    drain(&home);
}

#[test]
fn direct_writes_and_worker_reads_overlap_on_one_store() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "mix");
    let home = connected(&db, db.at());
    let path = home.paths.home.clone();
    ok(&path, &["branch", "fork", "pg/p", "work"]);
    let on_work = |words: &[&'static str]| -> Vec<&'static str> {
        let mut words = words.to_vec();
        words[3] = "work";
        words
    };
    ok(&path, &on_work(READ));
    let before = ok(&path, &["commit", "list", "pg/p", "work"]);
    let count = |list: &SdkValue| match list.get("items") {
        Some(SdkValue::List(items)) => items.len(),
        _ => panic!("{list:?}"),
    };
    let mut children = Vec::new();
    for n in 0..4 {
        let file = path.join(format!("w{n}"));
        std::fs::write(&file, format!("written {n}\n")).unwrap();
        let target = format!("/w{n}.txt");
        let words = ["file", "write", "pg/p", "work", &target, "--from"];
        let words = [&words[..], &[file.to_str().unwrap()]].concat();
        children.push((None, spawn(&path, &words, None)));
        for words in [READ, STAT, LIST] {
            children.push((Some(words), spawn(&path, &on_work(words), None)));
        }
    }
    for (read, child) in children {
        let output = finish(child);
        assert!(output.status.success(), "{read:?}: {output:?}");
    }
    let after = ok(&path, &["commit", "list", "pg/p", "work"]);
    assert_eq!(count(&after), count(&before) + 4, "four direct Commits");
    for n in 0..4 {
        let target = format!("/w{n}.txt");
        let read = ok(&path, &["file", "read", "pg/p", "work", &target]);
        assert_eq!(
            read.get_text("text"),
            Some(format!("written {n}\n").as_str())
        );
    }
    let listed = ok(&path, &on_work(LIST));
    assert!(matches!(listed.get("items"), Some(SdkValue::List(items)) if items.len() == 6));

    // A compaction beside warm sessions: a later read takes a session at
    // the Store's current generation, never a stale one.
    // A removed Branch's unique file leaves dead bytes for it to swap out.
    ok(&path, &["branch", "fork", "pg/p", "gone"]);
    let junk = path.join("junk");
    std::fs::write(
        &junk,
        (0..200_000u32)
            .map(|n| (n * 7919 % 251) as u8)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let write = ["file", "write", "pg/p", "gone", "/junk.bin", "--from"];
    ok(&path, &[&write[..], &[junk.to_str().unwrap()]].concat());
    ok(&path, &["branch", "remove", "pg/p", "gone"]);
    let generation = db.generation();
    let held = db.backends();
    ok(&path, &["store", "compact", "pg"]);
    assert_ne!(db.generation(), generation, "the compaction swapped");
    assert_eq!(ok(&path, READ).get_text("text"), Some("hello\n"));
    let now = db.settle(1);
    assert_eq!(now.len(), 1, "every stale session was dropped: {now:?}");
    assert!(now.is_disjoint(&held), "a new login at the new generation");
    drain(&home);
}

#[test]
fn sessions_stay_bounded_across_homes() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "homes");
    let homes: Vec<_> = (0..3)
        .map(|n| match n {
            0 => connected(&db, db.at()),
            _ => {
                let home = Fixture::new();
                connect(&home.paths.home, &db, db.at());
                home
            }
        })
        .collect();
    let peak = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (peak, done) = (peak.clone(), done.clone());
        let mut config = db.admin.clone();
        config.dbname("postgres");
        let name = db.name.clone();
        std::thread::spawn(move || {
            let mut admin = config.connect(NoTls).unwrap();
            while !done.load(Ordering::SeqCst) {
                let row = admin
                    .query_one(
                        "SELECT count(*) FROM pg_stat_activity WHERE datname = $1 AND usename = $1",
                        &[&name],
                    )
                    .unwrap();
                peak.fetch_max(row.get::<_, i64>(0) as usize, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    };
    let children: Vec<_> = (homes.iter())
        .flat_map(|home| {
            let path = home.paths.home.clone();
            (0..10).map(move |n| spawn(&path, [READ, STAT, LIST][n % 3], None))
        })
        .collect();
    for child in children {
        let output = finish(child);
        assert!(output.status.success(), "{output:?}");
    }
    done.store(true, Ordering::SeqCst);
    sampler.join().unwrap();
    let peak = peak.load(Ordering::SeqCst);
    eprintln!("peak backends across 3 homes: {peak}");
    assert!(
        peak <= 3 * 4,
        "{peak} backends: each home's worker keeps at most four"
    );
    assert!(db.settle(3 * 4).len() <= 3 * 4);
    for home in &homes {
        drain(home);
    }
    assert!(
        db.settle(0).is_empty(),
        "each drain closed its home's sessions"
    );
}

#[test]
fn stopped_host_and_records_without_a_revision_stay_direct() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "direct");
    let home = connected(&db, db.at());
    let path = &home.paths.home;
    ok(path, &["machine", "stop", HOST_MACHINE]);
    assert_eq!(ok(path, READ).get_text("text"), Some("hello\n"));
    assert!(
        !home.paths.socket.exists(),
        "a stopped host keeps the read direct"
    );
    assert!(
        db.settle(0).is_empty(),
        "the direct read closed its session"
    );
    ok(path, &["machine", "start", HOST_MACHINE]);
    drain(&home);

    // A record connected before login revisions: the direct route.
    let file = path.join("store/pg/record.json");
    let text = std::fs::read_to_string(&file).unwrap();
    let field = format!(",\"login_revision\":\"{}\"", revision(path));
    assert!(text.contains(&field), "{text}");
    std::fs::write(&file, text.replace(&field, "")).unwrap();
    assert_eq!(ok(path, STAT).get_text("type"), Some("file"));
    assert!(!home.paths.socket.exists(), "no revision: the direct route");
    assert!(db.settle(0).is_empty());
}

/// One management request on the worker's control socket.
fn manage(home: &Fixture, op: &str, args: SdkValue) -> SdkValue {
    let socket = UnixStream::connect(&home.paths.control).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    let hello = read_host_frame(&socket, MAX_HELLO, Some(end))
        .unwrap()
        .unwrap();
    let request = SdkValue::map([
        ("op", SdkValue::text(op)),
        ("args", args),
        ("generation", hello.get("generation").unwrap().clone()),
        ("request_id", SdkValue::text("pb-manage-1")),
    ]);
    write_host_frame(&socket, &request, HOST_MAX_REPLY, end).unwrap();
    read_host_frame(&socket, HOST_MAX_REPLY, Some(end))
        .unwrap()
        .unwrap()
}

/// The role's backends waiting on a lock.
fn waiting_on_lock(db: &Db, count: usize) -> usize {
    let mut admin = db.admin();
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let row = admin
            .query_one(
                "SELECT count(*) FROM pg_stat_activity WHERE datname = $1 AND usename = $1
                 AND wait_event_type = 'Lock'",
                &[&db.name],
            )
            .unwrap();
        let waiting = row.get::<_, i64>(0) as usize;
        if waiting >= count || Instant::now() > end {
            return waiting;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_retire_names_its_revision_and_never_fences_the_current_login() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "fence");
    let home = connected(&db, db.at());
    let path = &home.paths.home;
    ok(path, READ);
    let held = db.settle(1);
    let current = revision(path);

    // a retire that names the record's current revision changes nothing
    let answer = manage(
        &home,
        "store.postgresql_retire",
        SdkValue::map([
            ("store", SdkValue::text("pg")),
            ("revision", SdkValue::text(&current)),
        ]),
    );
    assert_eq!(
        answer.get("ok").and_then(|ok| ok.get_int("closed")),
        Some(0)
    );
    // one of another revision closes nothing of this login either
    let answer = manage(
        &home,
        "store.postgresql_retire",
        SdkValue::map([
            ("store", SdkValue::text("pg")),
            ("revision", SdkValue::text("00")),
        ]),
    );
    assert_eq!(
        answer.get("ok").and_then(|ok| ok.get_int("closed")),
        Some(0)
    );
    // a retire that names no revision is refused
    let answer = manage(
        &home,
        "store.postgresql_retire",
        SdkValue::map([("store", SdkValue::text("pg"))]),
    );
    assert_eq!(error_code(&answer), Some("InvalidInput"), "{answer:?}");
    for _ in 0..3 {
        assert_eq!(ok(path, STAT).get_text("type"), Some("file"));
    }
    assert_eq!(db.backends(), held, "the current login kept its session");

    // this route serves its own reads only
    let socket = UnixStream::connect(&home.paths.socket).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    let hello = read_host_frame(&socket, MAX_HELLO, Some(end))
        .unwrap()
        .unwrap();
    let args = SdkValue::map([
        ("method", SdkValue::text("fs.write")),
        ("store", SdkValue::text("pg")),
        ("project", SdkValue::text("p")),
        ("fields", SdkValue::map([])),
        ("login_revision", SdkValue::text(&current)),
        ("password", SdkValue::text(&db.password)),
    ]);
    let request = SdkValue::map([
        ("op", SdkValue::text("store.postgresql_read")),
        ("args", args),
        ("generation", hello.get("generation").unwrap().clone()),
        ("request_id", SdkValue::text("pb-write-1")),
    ]);
    write_host_frame(&socket, &request, HOST_MAX_REPLY, end).unwrap();
    let answer = read_host_frame(&socket, HOST_MAX_REPLY, Some(end))
        .unwrap()
        .unwrap();
    assert_eq!(error_code(&answer), Some("Unsupported"), "{answer:?}");

    // the ping reports the held sessions (§11)
    let ping = home.ping();
    let postgresql = ping.get("postgresql").expect("the ping's postgresql field");
    assert_eq!(postgresql.get_int("stores"), Some(1), "{ping:?}");
    assert_eq!(postgresql.get_int("idle"), Some(1), "{ping:?}");
    assert_eq!(postgresql.get_int("lent"), Some(0), "{ping:?}");
    // and so does the doctor's ping evidence
    let report = ok(path, &["machine", "doctor", HOST_MACHINE, "--detail"]);
    let Some(SdkValue::List(checks)) = report.get("tree").and_then(|tree| tree.get("checks"))
    else {
        panic!("doctor checks: {report:?}")
    };
    let pinged = checks
        .iter()
        .find(|row| row.get_text("check") == Some("machine.worker.ping"))
        .and_then(|row| row.get("evidence"))
        .and_then(|evidence| evidence.get("postgresql"))
        .expect("the doctor's postgresql evidence");
    assert_eq!(pinged.get_int("idle"), Some(1), "{pinged:?}");
    drain(&home);
}

#[test]
fn postgresql_reads_leave_the_ordinary_threads_to_other_stores() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "fair");
    let home = connected(&db, db.at());
    let path = home.paths.home.clone();
    let source = path.join("sqlite-source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("s.txt"), "sqlite\n").unwrap();
    ok(&path, &["store", "create", "sq"]);
    ok(
        &path,
        &["project", "init", "sq/q", source.to_str().unwrap()],
    );
    ok(&path, READ);
    db.settle(1);

    // more PostgreSQL reads than the worker has ordinary threads, all
    // stuck behind a lock or waiting for a session
    let lock = Locked::new(&db, "layer");
    let callers: Vec<_> = (0..12).map(|_| spawn(&path, STAT, None)).collect();
    assert_eq!(waiting_on_lock(&db, 4), 4, "four sessions in flight");
    std::thread::sleep(Duration::from_millis(300));
    let began = Instant::now();
    let sqlite = spawn(&path, &["file", "read", "sq/q", "main", "/s.txt"], None);
    let end = Instant::now() + Duration::from_secs(10);
    let mut sqlite = sqlite;
    let served = loop {
        if let Some(status) = sqlite.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() > end {
            let _ = sqlite.kill();
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let took = began.elapsed();
    drop(lock);
    let output = sqlite.wait_with_output().unwrap();
    assert!(
        served.is_some_and(|status| status.success()),
        "a SQLite read on another Store waited {took:?} behind PostgreSQL reads: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(decode(&output).get_text("text"), Some("sqlite\n"));
    for child in callers {
        let output = finish(child);
        assert!(output.status.success(), "{:?}", decode(&output));
    }
    drain(&home);
}

#[test]
fn a_failed_probe_closes_the_idle_sessions_and_opens_one() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "probe");
    let relay = Relay::new(&server.host, server.port);
    // the relay listens in this process, beside the CLI and its worker
    let home = connected(&db, ("127.0.0.1", relay.port));
    let path = home.paths.home.clone();
    ok(&path, READ);
    // four held sessions: four reads in flight at once
    let lock = Locked::new(&db, "layer");
    let callers: Vec<_> = (0..4).map(|_| spawn(&path, STAT, None)).collect();
    assert_eq!(waiting_on_lock(&db, 4), 4);
    drop(lock);
    for child in callers {
        assert!(finish(child).status.success());
    }
    let held = db.settle(4);
    assert_eq!(held.len(), 4);

    // every held connection goes silent: one probe waits out its bound,
    // then the call closes the other idle sessions unprobed and opens one
    relay.cut();
    let began = Instant::now();
    assert_eq!(ok(&path, STAT).get_text("type"), Some("file"));
    let took = began.elapsed();
    assert!(took >= Duration::from_secs(5), "{took:?}");
    // one probe, not four: counted on the wire, the time a loose bound
    assert_eq!(relay.probed(), 1, "probes sent on cut connections");
    assert!(took < Duration::from_secs(17), "{took:?}");
    let after = db.backends();
    assert_eq!(after.difference(&held).count(), 1, "exactly one new login");
    assert_eq!(
        home.ping()
            .get("postgresql")
            .and_then(|p| p.get_int("idle")),
        Some(1)
    );
    drain(&home);
}

/// A home with Stores `pg` (project `p`, on `db`) and `pg2` (project `q`,
/// on `other`).
fn two_stores(db: &Db, other: &Db) -> Fixture {
    let home = connected(db, db.at());
    let path = &home.paths.home;
    connect_as(path, "pg2", other, other.at());
    let source = path.join("source2");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("c.txt"), "other\n").unwrap();
    ok(
        path,
        &["project", "init", "pg2/q", source.to_str().unwrap()],
    );
    home
}

/// `child`'s answer if it ends within `limit`; killed otherwise.
fn within(mut child: std::process::Child, limit: Duration) -> Option<Output> {
    let end = Instant::now() + limit;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Some(child.wait_with_output().unwrap())
}

#[test]
fn a_blocked_store_leaves_the_postgresql_threads_to_another() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "hol");
    let other = Db::new(&server, "hol2");
    let home = two_stores(&db, &other);
    let path = home.paths.home.clone();
    ok(&path, READ);
    db.settle(1);

    // Store pg's four sessions wait on a lock, and eight more of its reads
    // queue for them: more than the PostgreSQL lane has threads
    let lock = Locked::new(&db, "layer");
    let callers: Vec<_> = (0..12).map(|_| spawn(&path, STAT, None)).collect();
    assert_eq!(waiting_on_lock(&db, 4), 4, "four sessions in flight");
    std::thread::sleep(Duration::from_millis(300));
    let other_read = ["file", "read", "pg2/q", "main", "/c.txt"];
    for _ in 0..3 {
        let began = Instant::now();
        let output = within(spawn(&path, &other_read, None), Duration::from_secs(10));
        let took = began.elapsed();
        let Some(output) = output else {
            drop(lock);
            panic!("a read of Store pg2 waited {took:?} behind Store pg's reads");
        };
        assert!(output.status.success(), "{:?}", decode(&output));
        assert_eq!(decode(&output).get_text("text"), Some("other\n"));
    }
    drop(lock);
    for child in callers {
        let output = finish(child);
        assert!(output.status.success(), "{:?}", decode(&output));
    }
    drain(&home);
}

/// The ping's `postgresql` field once `ready` holds, within 5 s.
fn postgresql_when(home: &Fixture, ready: impl Fn(&SdkValue) -> bool) -> SdkValue {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let ping = home.ping();
        let postgresql = ping.get("postgresql").cloned().expect("postgresql");
        if ready(&postgresql) || Instant::now() > end {
            return postgresql;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_full_postgresql_lane_refuses_within_its_bound_and_drops_gone_callers() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "cap");
    let home = connected(&db, db.at());
    let path = home.paths.home.clone();
    let source = path.join("sqlite-source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("s.txt"), "sqlite\n").unwrap();
    ok(&path, &["store", "create", "sq"]);
    ok(
        &path,
        &["project", "init", "sq/q", source.to_str().unwrap()],
    );
    ok(&path, READ);
    db.settle(1);
    let current = revision(&path);

    // a hung server: 4 reads in flight, 60 queued, the lane's 64
    let lock = Locked::new(&db, "layer");
    let mut open: Vec<Option<UnixStream>> = (0..64)
        .map(|at| Some(sent(&home, &current, &db.password, &format!("pb-cap-{at}"))))
        .collect();
    assert_eq!(waiting_on_lock(&db, 4), 4);
    let full = postgresql_when(&home, |p| p.get_int("reads") == Some(64));
    assert_eq!(full.get_int("reads"), Some(64), "{full:?}");
    assert_eq!(full.get_int("queued"), Some(60), "{full:?}");

    // the next read is refused unrun within the bound, naming the load,
    // and a SQLite read meanwhile completes
    let began = Instant::now();
    let extra = sent(&home, &current, &db.password, "pb-cap-extra");
    let sqlite = spawn(&path, &["file", "read", "sq/q", "main", "/s.txt"], None);
    let output = within(sqlite, Duration::from_secs(10)).expect("the SQLite read");
    assert_eq!(decode(&output).get_text("text"), Some("sqlite\n"));
    let end = Instant::now() + Duration::from_secs(15);
    let refused = read_host_frame(&extra, HOST_MAX_REPLY, Some(end))
        .unwrap()
        .unwrap();
    let took = began.elapsed();
    assert_eq!(error_code(&refused), Some("Unavailable"), "{refused:?}");
    let detail = refused.get("error").and_then(|e| e.get("detail")).unwrap();
    assert_eq!(detail.get_text("state"), Some("postgresql-reads-full"));
    assert_eq!(detail.get_int("reads"), Some(64));
    assert!(took >= Duration::from_secs(4), "{took:?}");
    assert!(took < Duration::from_secs(10), "{took:?}");

    // twenty queued callers go away: their reads are dropped unrun, and
    // the room is the next read's
    for slot in &mut open[40..60] {
        drop(slot.take());
    }
    let late = sent(&home, &current, &db.password, "pb-cap-late");
    let after = postgresql_when(&home, |p| p.get_int("reads") == Some(45));
    assert_eq!(after.get_int("reads"), Some(45), "{after:?}");
    assert_eq!(after.get_int("queued"), Some(41), "{after:?}");

    drop(lock);
    let end = Instant::now() + Duration::from_secs(30);
    for socket in open.iter().flatten().chain([&late]) {
        let answer = read_host_frame(socket, HOST_MAX_REPLY, Some(end))
            .unwrap()
            .unwrap();
        assert!(answer.get("ok").is_some(), "{answer:?}");
    }
    let idle = postgresql_when(&home, |p| p.get_int("reads") == Some(0));
    assert_eq!(idle.get_int("queued"), Some(0), "{idle:?}");
    drain(&home);
}

#[test]
fn a_read_still_probing_holds_no_lease() {
    let Some(server) = server() else { return };
    let db = Db::new(&server, "lease");
    let relay = Relay::new(&server.host, server.port);
    // the relay listens in this process, beside the CLI and its worker
    let home = connected(&db, ("127.0.0.1", relay.port));
    let path = home.paths.home.clone();
    ok(&path, READ);
    db.settle(1);

    // the held session goes silent: the next read waits out its probe
    // with no lease, so a disconnect meanwhile is not refused InUse
    relay.cut();
    let probing = spawn(&path, STAT, None);
    let end = Instant::now() + Duration::from_secs(4);
    while relay.probed() == 0 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(relay.probed(), 1, "the read is probing");
    let began = Instant::now();
    ok(&path, &["store", "disconnect", "pg"]);
    assert!(
        began.elapsed() < Duration::from_secs(3),
        "{:?}",
        began.elapsed()
    );
    let output = finish(probing);
    assert!(!output.status.success(), "{:?}", decode(&output));
    let code = decode(&output).get_text("error").unwrap_or("?").to_owned();
    assert!(
        matches!(code.as_str(), "NotFound" | "NoSuchStore"),
        "{code}"
    );
    // the cut connection's backend stays until the relay goes; the worker
    // keeps no session of the Store
    let held = postgresql_when(&home, |p| p.get_int("stores") == Some(0));
    assert_eq!(held.get_int("stores"), Some(0), "{held:?}");
    drain(&home);
}
