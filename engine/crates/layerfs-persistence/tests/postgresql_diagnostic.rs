//! Real PostgreSQL audits and actual stalled-handshake cancellation.
#![cfg(feature = "postgresql")]
use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    diagnostic::{self, Options, Outcome},
    PostgresHandles, PostgresSelection,
};
use layerfs_storage::{Storage, StoragePolicy};
use std::{
    io::Read,
    net::TcpListener,
    time::{Duration, Instant},
};

#[test]
fn tcp_peer_that_stalls_authentication_is_bounded_and_closed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0u8; 1024];
        loop {
            match socket.read(&mut bytes) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    });
    let selection = PostgresSelection::new(
        format!("postgresql://unused@{address}/unused?sslmode=disable"),
        "layerfs",
    );
    let started = Instant::now();
    let mut observations = Vec::new();
    diagnostic::postgresql_connection(
        &selection,
        b"doctor-pg-fixture",
        Options {
            deadline: Some(started + Duration::from_millis(120)),
            ..Options::default()
        },
        &mut fixture_resolver,
        &mut |o| observations.push(o),
    )
    .unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(observations.iter().any(|o| o.outcome == Outcome::Unknown));
    assert!(
        peer.join().unwrap(),
        "stalled socket must close when connect future is cancelled"
    );
}

#[test]
fn deadline_driver_reads_one_view_and_ordinary_sync_writes_still_work() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let schema = format!("doctor_audit_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    let authority = HistoryCatalogConfig {
        binding_key: b"doctor-pg-fixture".to_vec(),
        cursor_key: [71; 32],
        incarnation: 7,
    };
    let writer =
        PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &authority).unwrap();
    let put = |text: &[u8]| {
        let storage = Storage::new(writer.storage.clone()).unwrap();
        let save = storage.begin_save().unwrap();
        save.accept(
            FinalizedObject::new(
                ObjectRole::WholeFile,
                encode_whole_file_payload(text).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        save.finish().unwrap();
    };
    put(b"original PostgreSQL saved payload");
    let deadline = Instant::now() + Duration::from_secs(10);
    let reader = PostgresHandles::open_diagnostic(
        &selection,
        &authority.binding_key,
        authority.cursor_key,
        Some(deadline),
        &mut fixture_resolver,
    )
    .unwrap();
    let before = reader.diagnostics().unwrap();
    let mut observations = Vec::new();
    reader
        .diagnose(
            Options {
                deadline: Some(deadline),
                ..Options::default()
            },
            &mut |o| {
                if o.id == "store.snapshot" {
                    put(b"published after diagnostic snapshot");
                }
                observations.push(o);
            },
        )
        .unwrap();
    assert!(
        !observations.iter().any(|o| o.outcome == Outcome::Fail),
        "{observations:?}"
    );
    assert!(
        observations
            .iter()
            .any(|o| o.id == "store.objects.summary" && o.message.starts_with("1 winning")),
        "{observations:?}"
    );
    if std::env::var_os("LAYERFS_TEST_EXPECT_AMCHECK").is_some() {
        assert!(
            observations
                .iter()
                .any(|o| o.id == "store.database.relations.summary" && o.outcome == Outcome::Pass),
            "{observations:?}"
        );
    }
    let after = reader.diagnostics().unwrap();
    assert_eq!(after.write_transactions, before.write_transactions);
    assert_eq!(after.write_commits, before.write_commits);
    drop(reader);
    put(b"ordinary synchronous writer after diagnostics");
    let mut connection = Vec::new();
    diagnostic::postgresql_connection(
        &selection,
        b"doctor-pg-fixture",
        Options {
            deadline: Some(Instant::now() + Duration::from_secs(3)),
            ..Options::default()
        },
        &mut fixture_resolver,
        &mut |o| connection.push(o),
    )
    .unwrap();
    assert!(
        connection
            .iter()
            .any(|o| o.id == "store.connection.authentication" && o.outcome == Outcome::Pass),
        "{connection:?}"
    );
    drop(writer);
    let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    client
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .unwrap();
}

#[test]
fn lock_wait_cancellation_releases_backend_work_and_snapshot() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let schema = format!("doctor_cancel_{}", std::process::id());
    let application = format!("doctor_cancel_{}", std::process::id());
    // URL syntax is not reserialized or logged; an explicit startup option
    // marks the diagnostic session for independent backend observation.
    let diagnostic_url = format!(
        "{url}{}application_name={application}",
        if url.contains('?') { "&" } else { "?" }
    );
    let selection = PostgresSelection::new(&diagnostic_url, &schema);
    let authority = HistoryCatalogConfig {
        binding_key: b"doctor-pg-cancel".to_vec(),
        cursor_key: [71; 32],
        incarnation: 7,
    };
    let writer =
        PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &authority).unwrap();
    let deadline = Instant::now() + Duration::from_millis(250);
    let reader = PostgresHandles::open_diagnostic(
        &selection,
        &authority.binding_key,
        authority.cursor_key,
        Some(deadline),
        &mut fixture_resolver,
    )
    .unwrap();
    let mut lock = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    lock.batch_execute(&format!(
        "BEGIN; LOCK TABLE {schema}.object_location IN ACCESS EXCLUSIVE MODE"
    ))
    .unwrap();
    let mut observations = Vec::new();
    let started = Instant::now();
    assert!(reader
        .diagnose(
            Options {
                deadline: Some(deadline),
                ..Options::default()
            },
            &mut |o| observations.push(o)
        )
        .is_err());
    assert!(started.elapsed() < Duration::from_millis(1500));
    let mut monitor = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let count:i64=monitor.query_one("SELECT count(*) FROM pg_stat_activity WHERE application_name=$1 AND (state LIKE 'idle in transaction%' OR (state='active' AND wait_event_type='Lock'))",&[&application]).unwrap().get(0);
    assert_eq!(
        count, 0,
        "backend work/snapshot must be released; {observations:?}"
    );
    drop(reader);
    lock.batch_execute("ROLLBACK").unwrap();
    drop(writer);
    monitor
        .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
        .unwrap();
}

fn fixture_resolver(
    host: &str,
    port: u16,
    _deadline: Instant,
) -> std::io::Result<Vec<std::net::SocketAddr>> {
    host.parse::<std::net::IpAddr>()
        .map(|ip| vec![std::net::SocketAddr::new(ip, port)])
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "fixture accepts numeric hosts only",
            )
        })
}
