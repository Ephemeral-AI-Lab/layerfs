//! #768 stage 2: the ordinary PostgreSQL driver sends a pipeline of
//! requests in one flight and collects every reply.
//!
//! The proof that a flight is one network wait is a gate: the session
//! connects through a local TCP proxy that forwards the client's bytes at
//! once but can hold every byte the server sends back. With the gate shut
//! the client receives nothing, so a client that waited for any reply
//! before sending its next request could not get past its first request.
//! A separate observer connection then sees, from the server:
//!
//! * for the execute flight, the flight's rows committed: the server ran
//!   `BEGIN`, every statement and `COMMIT`, so the client sent them all
//!   without a reply;
//! * for the prepare flight, the backend idle with the last statement's
//!   text as its query (`pg_stat_activity`): the server parsed every
//!   statement, so every Parse went out without a reply.
//!
//! Only then is the gate opened and the replies delivered. The counters
//! must say one round trip per flight. An error in the middle leaves the
//! later replies collected (`25P02`) and the transaction rolled back; a
//! request that cannot be encoded stops the flight before it, and the
//! session rolls back what was sent.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
mod support;

use layerfs_persistence::internals::{
    postgresql_pipeline, BackendError, PipelineReply, PipelineRequest,
};
use layerfs_persistence::{PostgresConfig, PostgresHandles, PostgresHost, SqlWork};
use postgres::error::SqlState;
use postgres::types::ToSql;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use support::pg::{url, Schema};

const BINDING: &[u8] = b"postgresql-pipeline";
const CURSOR: [u8; 32] = [76; 32];
/// How long the observer waits for the server to show a held flight.
const OBSERVE: Duration = Duration::from_secs(10);

type Gate = Arc<(Mutex<bool>, Condvar)>;

/// A TCP proxy to the server whose server-to-client direction can be held.
struct Proxy {
    port: u16,
    gate: Gate,
}

/// The gate shut while this lives; dropping it (also on a panic) opens it.
struct Held<'a>(&'a Gate);

impl Drop for Held<'_> {
    fn drop(&mut self) {
        let (open, changed) = &**self.0;
        *open.lock().unwrap() = true;
        changed.notify_all();
    }
}

impl Proxy {
    fn start(upstream: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let gate: Gate = Arc::new((Mutex::new(true), Condvar::new()));
        let shared = gate.clone();
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(client) = client else { return };
                let Ok(server) = TcpStream::connect(upstream) else {
                    return;
                };
                let _ = client.set_nodelay(true);
                let _ = server.set_nodelay(true);
                let (back_from, back_to) =
                    (server.try_clone().unwrap(), client.try_clone().unwrap());
                std::thread::spawn(move || pump(client, server, None));
                let gate = shared.clone();
                std::thread::spawn(move || pump(back_from, back_to, Some(gate)));
            }
        });
        Self { port, gate }
    }

    fn hold(&self) -> Held<'_> {
        *self.gate.0.lock().unwrap() = false;
        Held(&self.gate)
    }
}

/// Copies `from` to `to`; with a gate, each chunk waits until it is open.
fn pump(mut from: TcpStream, mut to: TcpStream, gate: Option<Gate>) {
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        if let Some(gate) = &gate {
            let (open, changed) = &**gate;
            let mut open = open.lock().unwrap();
            while !*open {
                open = changed.wait(open).unwrap();
            }
        }
        if to.write_all(&buffer[..read]).is_err() {
            break;
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

/// The proxy's connection string for `url`'s server, under `application`.
fn proxied(url: &str, port: u16, application: &str) -> (String, SocketAddr) {
    let config = PostgresConfig::from_str(url).unwrap();
    let host = match &config.get_hosts()[0] {
        PostgresHost::Tcp(host) => host.clone(),
        _ => panic!("the test server must be reached over TCP"),
    };
    let upstream = (
        host.as_str(),
        config.get_ports().first().copied().unwrap_or(5432),
    )
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();
    let mut proxied = format!(
        "host=127.0.0.1 port={port} user={} dbname={} sslmode=disable application_name={application}",
        config.get_user().unwrap(),
        config.get_dbname().unwrap_or(config.get_user().unwrap()),
    );
    if let Some(password) = config.get_password() {
        proxied.push_str(&format!(
            " password={}",
            String::from_utf8(password.to_vec()).unwrap()
        ));
    }
    (proxied, upstream)
}

#[derive(Debug, PartialEq, Eq)]
struct Delta {
    statements: u64,
    round_trips: u64,
    prepares: u64,
}

fn delta(before: &SqlWork, after: &SqlWork) -> Delta {
    Delta {
        statements: after.statements - before.statements,
        round_trips: after.round_trips - before.round_trips,
        prepares: after.prepares - before.prepares,
    }
}

/// Waits (bounded) until `check` holds on the observer connection.
fn observe(observer: &mut postgres::Client, check: impl Fn(&mut postgres::Client) -> bool) -> bool {
    let until = Instant::now() + OBSERVE;
    while Instant::now() < until {
        if check(observer) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

fn committed(observer: &mut postgres::Client, table: &str, keys: &[i64]) -> i64 {
    observer
        .query_one(
            &format!("SELECT count(*) FROM {table} WHERE k = ANY($1)"),
            &[&keys],
        )
        .unwrap()
        .get(0)
}

/// One insert's parameters.
fn pair<'a>(k: &'a i64, value: &'a &'a [u8]) -> [&'a (dyn ToSql + Sync); 2] {
    [k, value]
}

fn row_count(reply: &PipelineReply) -> usize {
    match reply {
        PipelineReply::Rows(rows) => rows.len(),
        PipelineReply::Messages(_) => panic!("a statement answers rows"),
    }
}

#[test]
fn a_pipeline_is_one_flight_with_every_reply() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url.clone(), "pipeline");
    drop(schema.create(BINDING, CURSOR));
    // The test's table lives outside the Store's schema, which the open checks.
    let table_schema = format!("{}_t", schema.name);
    let table = format!("{table_schema}.pipe");
    let mut observer = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    observer
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {table_schema} CASCADE; CREATE SCHEMA {table_schema}; \
             CREATE TABLE {table} (k int8 PRIMARY KEY, v bytea NOT NULL)"
        ))
        .unwrap();
    struct Drops(String, String);
    impl Drop for Drops {
        fn drop(&mut self) {
            if let Ok(mut client) = postgres::Client::connect(&self.0, postgres::NoTls) {
                let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.1));
            }
        }
    }
    let _drops = Drops(url.clone(), table_schema.clone());

    let application = format!("lfs_pipeline_{}", std::process::id());
    let (_, upstream) = proxied(&url, 0, &application);
    let proxy = Proxy::start(upstream);
    let mut selection = schema.selection();
    selection.url = proxied(&url, proxy.port, &application).0;
    let handles = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();

    let insert = format!("INSERT INTO {table} (k, v) VALUES ($1, $2)");
    let count = format!("SELECT k FROM {table} WHERE k <= $1");
    let value: &[u8] = b"pipelined";
    let keys: Vec<i64> = (1..=9).collect();

    // 1. Cold: two statements not yet prepared. One prepare flight (both
    // Parse messages sent before either reply, shown by the server having
    // parsed the second while every reply is held), then one execute flight.
    let (k1, k2, two) = (pair(&keys[0], &value), pair(&keys[1], &value), 2i64);
    let cold = [
        PipelineRequest::Simple("BEGIN"),
        PipelineRequest::Statement {
            sql: &insert,
            params: &k1,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Statement {
            sql: &insert,
            params: &k2,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Statement {
            sql: &count,
            params: &[&two],
            bytes: 8,
        },
        PipelineRequest::Simple("COMMIT"),
    ];
    let before = handles.diagnostics().unwrap();
    let (replies, parsed) = std::thread::scope(|scope| {
        let held = proxy.hold();
        let worker = scope.spawn(|| postgresql_pipeline(&handles, &cold));
        let parsed = observe(&mut observer, |observer| {
            observer
                .query_opt(
                    "SELECT 1 FROM pg_stat_activity WHERE application_name = $1 \
                     AND state = 'idle' AND query = $2",
                    &[&application, &count],
                )
                .unwrap()
                .is_some()
        });
        drop(held);
        (worker.join().unwrap(), parsed)
    });
    let replies = replies.unwrap();
    assert!(
        parsed,
        "the server parsed the last statement while every reply was held"
    );
    assert_eq!(
        delta(&before, &handles.diagnostics().unwrap()),
        Delta {
            statements: 5,
            round_trips: 2,
            prepares: 2
        }
    );
    assert!(replies.iter().all(Result::is_ok), "every request succeeds");
    assert_eq!(
        row_count(replies[3].as_ref().unwrap()),
        2,
        "the transaction sees its rows"
    );
    assert_eq!(committed(&mut observer, &table, &keys[..2]), 2);

    // 2. Warm: BEGIN, three inserts and COMMIT in one flight. The server
    // commits all three rows while every reply is still held.
    let (k3, k4, k5) = (
        pair(&keys[2], &value),
        pair(&keys[3], &value),
        pair(&keys[4], &value),
    );
    let warm = [
        PipelineRequest::Simple("BEGIN"),
        PipelineRequest::Statement {
            sql: &insert,
            params: &k3,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Statement {
            sql: &insert,
            params: &k4,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Statement {
            sql: &insert,
            params: &k5,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Simple("COMMIT"),
    ];
    let before = handles.diagnostics().unwrap();
    let (replies, seen) = std::thread::scope(|scope| {
        let held = proxy.hold();
        let worker = scope.spawn(|| postgresql_pipeline(&handles, &warm));
        let seen = observe(&mut observer, |observer| {
            committed(observer, &table, &keys[2..5]) == 3
        });
        drop(held);
        (worker.join().unwrap(), seen)
    });
    let replies = replies.unwrap();
    assert!(
        seen,
        "the server committed the flight while every reply was held"
    );
    assert_eq!(
        delta(&before, &handles.diagnostics().unwrap()),
        Delta {
            statements: 5,
            round_trips: 1,
            prepares: 0
        }
    );
    assert_eq!(replies.len(), 5);
    assert!(replies.iter().all(Result::is_ok), "every request succeeds");

    // 3. An error in the middle: a duplicate key. The later insert is
    // answered 25P02, COMMIT rolls the transaction back, and nothing of it
    // is kept. Still one flight, and the session stays usable.
    let (k6, k7) = (pair(&keys[5], &value), pair(&keys[6], &value));
    let failing = [
        PipelineRequest::Simple("BEGIN"),
        PipelineRequest::Statement {
            sql: &insert,
            params: &k6,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Statement {
            sql: &insert,
            params: &k6,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Statement {
            sql: &insert,
            params: &k7,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Simple("COMMIT"),
    ];
    let before = handles.diagnostics().unwrap();
    let replies = postgresql_pipeline(&handles, &failing).unwrap();
    assert_eq!(
        delta(&before, &handles.diagnostics().unwrap()),
        Delta {
            statements: 5,
            round_trips: 1,
            prepares: 0
        }
    );
    assert!(replies[0].is_ok() && replies[1].is_ok());
    let duplicate = replies[2].as_ref().err().expect("the duplicate fails");
    assert_eq!(duplicate.code, Some(SqlState::UNIQUE_VIOLATION));
    assert_eq!(duplicate.error, BackendError::Integrity);
    let skipped = replies[3]
        .as_ref()
        .err()
        .expect("the later insert is skipped");
    assert_eq!(skipped.code, Some(SqlState::IN_FAILED_SQL_TRANSACTION));
    assert!(
        replies[4].is_ok(),
        "COMMIT of an aborted transaction answers ROLLBACK"
    );
    assert_eq!(
        committed(&mut observer, &table, &keys[5..7]),
        0,
        "rolled back"
    );

    // 4. A request that cannot be encoded (text for an int8): it and every
    // later request are never sent, and the session rolls back what was.
    let wrong: [&(dyn ToSql + Sync); 2] = [&"eight", &value];
    let k8 = pair(&keys[7], &value);
    let k9 = pair(&keys[8], &value);
    let unsendable = [
        PipelineRequest::Simple("BEGIN"),
        PipelineRequest::Statement {
            sql: &insert,
            params: &k8,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Statement {
            sql: &insert,
            params: &wrong,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Statement {
            sql: &insert,
            params: &k9,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Simple("COMMIT"),
    ];
    let before = handles.diagnostics().unwrap();
    let replies = postgresql_pipeline(&handles, &unsendable).unwrap();
    assert_eq!(
        delta(&before, &handles.diagnostics().unwrap()),
        Delta {
            statements: 2,
            round_trips: 2,
            prepares: 0
        },
        "one flight of two requests and one ROLLBACK"
    );
    assert!(replies[0].is_ok() && replies[1].is_ok());
    for reply in &replies[2..] {
        let refusal = reply.as_ref().err().expect("not sent");
        assert_eq!(
            (refusal.error, refusal.code.clone()),
            (BackendError::Integrity, None)
        );
    }
    assert_eq!(
        committed(&mut observer, &table, &keys[7..9]),
        0,
        "rolled back"
    );

    // The session is still usable: nothing above lost a reply.
    let nine = 9i64;
    let replies = postgresql_pipeline(
        &handles,
        &[PipelineRequest::Statement {
            sql: &count,
            params: &[&nine],
            bytes: 8,
        }],
    )
    .unwrap();
    assert_eq!(row_count(replies[0].as_ref().unwrap()), 5);
    drop(handles);
}

/// #768 independent verification, finding 3: a request the client never
/// sent because it could not be encoded as a protocol message (an
/// `io::Error`: a NUL byte in a simple text, or in a statement's text,
/// which fails its Parse) lost no reply. The flight's sent prefix is rolled
/// back, the refusal is definite (`Integrity`, unsent), and the session is
/// not quarantined: it answers the next flight, outside any transaction.
#[test]
fn an_unencodable_request_rolls_back_and_keeps_the_session() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url.clone(), "pipeline_nul");
    drop(schema.create(BINDING, CURSOR));
    let table_schema = format!("{}_t", schema.name);
    let table = format!("{table_schema}.pipe");
    let mut observer = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    observer
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {table_schema} CASCADE; CREATE SCHEMA {table_schema}; \
             CREATE TABLE {table} (k int8 PRIMARY KEY, v bytea NOT NULL)"
        ))
        .unwrap();
    struct Drops(String, String);
    impl Drop for Drops {
        fn drop(&mut self) {
            if let Ok(mut client) = postgres::Client::connect(&self.0, postgres::NoTls) {
                let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.1));
            }
        }
    }
    let _drops = Drops(url.clone(), table_schema.clone());
    let handles = PostgresHandles::open_writable(&schema.selection(), BINDING, CURSOR).unwrap();
    let insert = format!("INSERT INTO {table} (k, v) VALUES ($1, $2)");
    let value: &[u8] = b"nul";
    let (one, two, three) = (1i64, 2i64, 3i64);
    let (k1, k2, k3) = (pair(&one, &value), pair(&two, &value), pair(&three, &value));
    // Prepare the insert first, so the flight below sends no prepare.
    let warm = postgresql_pipeline(
        &handles,
        &[PipelineRequest::Statement {
            sql: &insert,
            params: &k3,
            bytes: 8 + value.len() as u64,
        }],
    )
    .unwrap();
    assert!(warm[0].is_ok());

    // 1. A simple text with a NUL byte after a sent BEGIN and insert.
    let unencodable = [
        PipelineRequest::Simple("BEGIN"),
        PipelineRequest::Statement {
            sql: &insert,
            params: &k1,
            bytes: 8 + value.len() as u64,
        },
        PipelineRequest::Simple("SELECT 1\0"),
        PipelineRequest::Simple("COMMIT"),
    ];
    let before = handles.diagnostics().unwrap();
    let replies = postgresql_pipeline(&handles, &unencodable).unwrap();
    assert_eq!(
        delta(&before, &handles.diagnostics().unwrap()),
        Delta {
            statements: 2,
            round_trips: 2,
            prepares: 0
        },
        "one flight of the two sent requests and one ROLLBACK"
    );
    assert!(replies[0].is_ok() && replies[1].is_ok());
    for reply in &replies[2..] {
        let refusal = reply.as_ref().err().expect("not sent");
        assert_eq!(
            (refusal.error, refusal.code.clone(), refusal.sent),
            (BackendError::Integrity, None, false),
            "an unsent request is the client's definite refusal"
        );
    }
    assert_eq!(committed(&mut observer, &table, &[1]), 0, "rolled back");
    // The session still serves, and no transaction is left open: an insert
    // on its own commits at once.
    let replies = postgresql_pipeline(
        &handles,
        &[PipelineRequest::Statement {
            sql: &insert,
            params: &k2,
            bytes: 8 + value.len() as u64,
        }],
    )
    .expect("the session is not quarantined");
    assert!(replies[0].is_ok(), "{:?}", replies[0].as_ref().err());
    assert_eq!(
        committed(&mut observer, &table, &[2]),
        1,
        "autocommitted: no transaction was left open"
    );

    // 2. A statement whose text holds a NUL byte: its Parse cannot be
    // encoded, so it is never sent. The flight is refused definitely and
    // the session stays usable.
    let nul = format!("SELECT k FROM {table} WHERE k = $1\0");
    let refused = postgresql_pipeline(
        &handles,
        &[PipelineRequest::Statement {
            sql: &nul,
            params: &[&one],
            bytes: 8,
        }],
    )
    .err()
    .expect("an unencodable statement does not prepare");
    assert_eq!(refused, BackendError::Integrity, "a definite refusal");
    let count = format!("SELECT k FROM {table} WHERE k <= $1");
    let replies = postgresql_pipeline(
        &handles,
        &[PipelineRequest::Statement {
            sql: &count,
            params: &[&three],
            bytes: 8,
        }],
    )
    .expect("the session is not quarantined");
    assert_eq!(row_count(replies[0].as_ref().unwrap()), 2);
    drop(handles);
}
