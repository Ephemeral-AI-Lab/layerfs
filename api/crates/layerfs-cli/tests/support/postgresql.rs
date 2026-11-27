//! Shared helpers of the CLI's PostgreSQL host worker tests (#819 P-B):
//! a private role and database per test on the server
//! `LAYERFS_TEST_POSTGRES_URL` names, its backends counted, CLI calls and
//! a table lock that holds the worker's statements.
#![allow(dead_code)]
use super::fixture::Fixture;
use layerfs_api::worker::{read_host_frame, write_host_frame, HOST_MAX_REPLY, MAX_HELLO};
use layerfs_protocol::sdk::SdkValue;
use postgres::{config::Host, Client as Postgres, Config, NoTls};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const CLI: &str = env!("CARGO_BIN_EXE_layerfs");
pub const DEADLINE: Duration = Duration::from_secs(40);

pub struct Server {
    pub admin: Config,
    pub host: String,
    pub port: u16,
}

pub fn server() -> Option<Server> {
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

pub fn random() -> String {
    let mut bytes = [0u8; 12];
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut bytes)
        .unwrap();
    layerfs_protocol::sdk::hex(&bytes)
}

/// One role and its database; dropped with the test.
pub struct Db {
    pub admin: Config,
    pub name: String,
    pub password: String,
    pub host: String,
    pub port: u16,
}

impl Db {
    pub fn new(server: &Server, tag: &str) -> Self {
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

    pub fn admin(&self) -> Postgres {
        self.admin.connect(NoTls).unwrap()
    }

    /// The server's own address.
    pub fn at(&self) -> (&str, u16) {
        (&self.host, self.port)
    }

    pub fn url(&self, (host, port): (&str, u16)) -> String {
        format!("postgresql://{host}:{port}/{}?sslmode=disable", self.name)
    }

    /// The role's backends now.
    pub fn backends(&self) -> BTreeSet<i32> {
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
    pub fn settle(&self, count: usize) -> BTreeSet<i32> {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let backends = self.backends();
            if backends.len() == count || Instant::now() > end {
                return backends;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn rotate(&mut self) {
        self.password = random();
        (self.admin())
            .batch_execute(&format!(
                "ALTER ROLE {} PASSWORD '{}'",
                self.name, self.password
            ))
            .unwrap();
    }

    pub fn generation(&self) -> i64 {
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

pub fn finish(mut child: std::process::Child) -> Output {
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

pub fn spawn(home: &Path, words: &[&str], stdin: Option<&str>) -> std::process::Child {
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

pub fn decode(output: &Output) -> SdkValue {
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let text = std::str::from_utf8(bytes).unwrap();
    layerfs_home::json::parse(text).unwrap_or_else(|error| panic!("{error:?}: {text}"))
}

/// The answer of `layerfs <words>`, or its refusal code.
pub fn call(home: &Path, words: &[&str], stdin: Option<&str>) -> Result<SdkValue, String> {
    let output = finish(spawn(home, words, stdin));
    let value = decode(&output);
    match output.status.success() {
        true => Ok(value),
        false => Err(value.get_text("error").unwrap_or("?").to_owned()),
    }
}

/// The answer of `layerfs <words>` given `stdin`; a refusal fails the test
/// with the whole refusal.
pub fn answered(home: &Path, words: &[&str], stdin: Option<&str>) -> SdkValue {
    let output = finish(spawn(home, words, stdin));
    let value = decode(&output);
    assert!(output.status.success(), "{words:?} refused: {value:?}");
    value
}

pub fn ok(home: &Path, words: &[&str]) -> SdkValue {
    call(home, words, None).unwrap_or_else(|code| panic!("{words:?} refused {code}"))
}

pub const READ: &[&str] = &["file", "read", "pg/p", "main", "/a.txt"];
pub const STAT: &[&str] = &["file", "stat", "pg/p", "main", "/a.txt"];
pub const LIST: &[&str] = &["file", "list", "pg/p", "main", "/"];

/// Starts `home`'s host worker and waits for it: since #833 a first call
/// that finds no ready worker runs direct, so a test of the worker's held
/// sessions starts it first.
pub fn warm(home: &Fixture) {
    let host = if cfg!(target_os = "macos") {
        "macos-host"
    } else {
        "linux-host"
    };
    ok(&home.paths.home, &["machine", "start", host]);
    home.ping();
}

/// A home with PostgreSQL Store `pg` connected at `port` and project `p`,
/// and its host worker started.
pub fn connected(db: &Db, at: (&str, u16)) -> Fixture {
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
    warm(&home);
    home
}

pub fn connect(home: &Path, db: &Db, at: (&str, u16)) {
    connect_as(home, "pg", db, at);
}

pub fn connect_as(home: &Path, name: &str, db: &Db, at: (&str, u16)) {
    let url = db.url(at);
    let words = ["store", "connect", name, &url, "--user", &db.name];
    let words = [&words[..], &["--password-stdin"]].concat();
    call(home, &words, Some(&db.password)).unwrap();
}

pub fn revision(home: &Path) -> String {
    revision_of(home, "pg")
}

pub fn revision_of(home: &Path, store: &str) -> String {
    let record = std::fs::read_to_string(home.join(format!("store/{store}/record.json"))).unwrap();
    let record = layerfs_home::json::parse(&record).unwrap();
    record.get_text("login_revision").unwrap().to_owned()
}

/// One raw host request, as a caller that read the record earlier sends it.
pub fn raw(home: &Fixture, revision: &str, password: &str) -> SdkValue {
    let socket = sent(home, revision, password, "pb-raw-1");
    let end = Instant::now() + Duration::from_secs(10);
    read_host_frame(&socket, HOST_MAX_REPLY, Some(end))
        .unwrap()
        .unwrap()
}

/// The socket of one raw `fs.stat` request `id`, sent and not yet answered.
pub fn sent(home: &Fixture, revision: &str, password: &str, id: &str) -> UnixStream {
    sent_to(home, ("pg", "p", "/a.txt"), revision, password, id)
}

/// The socket of one raw `fs.stat` request `id` of `path` in `project` of
/// `store`, sent and not yet answered.
pub fn sent_to(
    home: &Fixture,
    target: (&str, &str, &str),
    revision: &str,
    password: &str,
    id: &str,
) -> UnixStream {
    sent_built(home, |generation| {
        stat_request(target, (revision, password), id, generation, Vec::new())
    })
}

/// The socket of one raw host request, made by `build` from the worker's
/// generation, sent and not yet answered.
pub fn sent_built(home: &Fixture, build: impl FnOnce(SdkValue) -> SdkValue) -> UnixStream {
    let socket = UnixStream::connect(&home.paths.socket).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    let hello = read_host_frame(&socket, MAX_HELLO, Some(end))
        .unwrap()
        .unwrap();
    let request = build(hello.get("generation").unwrap().clone());
    write_host_frame(&socket, &request, HOST_MAX_REPLY, end).unwrap();
    socket
}

/// A raw `fs.stat` request `id` of `path` in `project` of `store` at the
/// login (`revision`, `password`), its `fields` holding `extra` after
/// `branch` and `path`.
pub fn stat_request(
    (store, project, path): (&str, &str, &str),
    (revision, password): (&str, &str),
    id: &str,
    generation: SdkValue,
    extra: Vec<(String, SdkValue)>,
) -> SdkValue {
    let mut fields = vec![
        ("branch".to_owned(), SdkValue::text("main")),
        ("path".to_owned(), SdkValue::text(path)),
    ];
    fields.extend(extra);
    let args = SdkValue::map([
        ("method", SdkValue::text("fs.stat")),
        ("store", SdkValue::text(store)),
        ("project", SdkValue::text(project)),
        ("fields", SdkValue::Map(fields)),
        ("login_revision", SdkValue::text(revision)),
        ("password", SdkValue::text(password)),
    ]);
    SdkValue::map([
        ("op", SdkValue::text("store.postgresql_read")),
        ("args", args),
        ("generation", generation),
        ("request_id", SdkValue::text(id)),
    ])
}

pub fn error_code(answer: &SdkValue) -> Option<&str> {
    answer.get("error").and_then(|error| error.get_text("code"))
}

pub fn drain(home: &Fixture) {
    if home.paths.control.exists() {
        home.drain();
        let end = Instant::now() + Duration::from_secs(10);
        while home.paths.socket.exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Every file of `root` but the saved login holds no `password` byte run.
pub fn no_secret(root: &Path, password: &str) {
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

pub struct Locked {
    pub _session: Postgres,
}

impl Locked {
    pub fn new(db: &Db, table: &str) -> Self {
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

/// The role's backends waiting on a lock.
pub fn waiting_on_lock(db: &Db, count: usize) -> usize {
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

/// The Store's publication lane (`store_lane`) held by another session:
/// every write waits on it at its publication, while reads go on.
pub struct Lane {
    _session: Postgres,
}

impl Lane {
    pub fn hold(db: &Db) -> Self {
        let mut config = db.admin.clone();
        config.dbname(&db.name);
        let mut client = config.connect(NoTls).unwrap();
        client
            .batch_execute("BEGIN; SELECT 1 FROM layerfs.store_lane WHERE id = 1 FOR UPDATE")
            .unwrap();
        Self { _session: client }
    }
}
