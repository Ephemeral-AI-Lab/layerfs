//! A Store URL whose host is `localhost` reaches a server on either loopback,
//! even where `localhost` does not resolve (a Sandbox root without
//! `/etc/hosts`); IP literals, a given `hostaddr`, Unix sockets and other
//! host names are dialled as given. Needs `LAYERFS_TEST_PG_LOOPBACK_URL`
//! (`postgres://user:pw@localhost:<port>/<db>`, no query) and
//! `LAYERFS_TEST_PG_LOOPBACK_LISTEN` (`127.0.0.1` or `::1`, the only address
//! the server listens on); NOT_RUN without them. With
//! `LAYERFS_TEST_PG_LOOPBACK_TLS=1` the server's certificate names only
//! `localhost` and is trusted through `SSL_CERT_FILE`.
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::{port::PersistenceError, StoragePolicy};

struct Server {
    url: String,
    listen: &'static str,
    other: &'static str,
}

fn server() -> Option<Server> {
    let (Ok(url), Ok(listen)) = (
        std::env::var("LAYERFS_TEST_PG_LOOPBACK_URL"),
        std::env::var("LAYERFS_TEST_PG_LOOPBACK_LISTEN"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_LOOPBACK_URL or _LISTEN is unset");
        return None;
    };
    assert!(url.contains("@localhost:") && !url.contains('?'), "{url}");
    let (listen, other) = match listen.as_str() {
        "127.0.0.1" => ("127.0.0.1", "[::1]"),
        "::1" => ("[::1]", "127.0.0.1"),
        other => panic!("LAYERFS_TEST_PG_LOOPBACK_LISTEN={other}"),
    };
    Some(Server { url, listen, other })
}

impl Server {
    fn with_host(&self, host: &str, query: &str) -> String {
        let url = self.url.replace("@localhost:", &format!("@{host}:"));
        format!("{url}?{query}")
    }
    fn port(&self) -> &str {
        let tail = self.url.rsplit_once(':').unwrap().1;
        tail.split('/').next().unwrap()
    }
}

fn create(url: &str, tag: &str) -> Result<(), PersistenceError> {
    let schema = format!("lfs_lh_{tag}_{}", std::process::id());
    let selection = PostgresSelection::new(url, &schema);
    let made = PostgresHandles::create(
        &selection,
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"postgresql-localhost".to_vec(),
            cursor_key: [7; 32],
            incarnation: 1,
        },
    );
    let made = made.map(drop);
    if made.is_ok() {
        PostgresHandles::open_read_only(&selection, b"postgresql-localhost", [7; 32])?;
    }
    made
}

fn drop_schema(server: &Server, tag: &str) {
    let admin = server.with_host(server.listen, "sslmode=disable");
    if let Ok(mut client) = postgres::Client::connect(&admin, postgres::NoTls) {
        let schema = format!("lfs_lh_{tag}_{}", std::process::id());
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
}

fn reaches(server: &Server, url: &str, tag: &str) -> Result<(), PersistenceError> {
    let made = create(url, tag);
    drop_schema(server, tag);
    made
}

#[test]
fn localhost_reaches_the_server_on_its_loopback() {
    let Some(s) = server() else { return };
    for (i, host) in ["localhost", "LocalHost"].into_iter().enumerate() {
        let url = s.with_host(host, "sslmode=disable");
        reaches(&s, &url, &format!("name{i}")).unwrap();
    }
}

#[test]
fn ip_literals_are_dialled_as_given() {
    let Some(s) = server() else { return };
    reaches(&s, &s.with_host(s.listen, "sslmode=disable"), "lit_listen").unwrap();
    let other = s.with_host(s.other, "sslmode=disable&connect_timeout=5");
    assert!(reaches(&s, &other, "lit_other").is_err(), "{}", s.other);
}

#[test]
fn a_given_hostaddr_is_kept() {
    let Some(s) = server() else { return };
    let bare = |h: &str| h.trim_matches(['[', ']']).to_owned();
    let listen = format!("sslmode=disable&hostaddr={}", bare(s.listen));
    reaches(&s, &s.with_host("localhost", &listen), "addr_listen").unwrap();
    let other = format!(
        "sslmode=disable&connect_timeout=5&hostaddr={}",
        bare(s.other)
    );
    let outcome = reaches(&s, &s.with_host("localhost", &other), "addr_other");
    assert!(outcome.is_err(), "hostaddr={} was not kept", s.other);
}

#[test]
fn another_host_name_or_a_socket_gets_no_loopback() {
    let Some(s) = server() else { return };
    let url = s.with_host(
        "lfs-no-such-host.invalid",
        "sslmode=disable&connect_timeout=5",
    );
    assert!(reaches(&s, &url, "name_other").is_err());
    let user = s
        .url
        .split("//")
        .nth(1)
        .unwrap()
        .split([':', '@'])
        .next()
        .unwrap();
    let socket = format!(
        "host=/nonexistent-lfs-socket port={} user={user} dbname=postgres sslmode=disable connect_timeout=5",
        s.port()
    );
    assert!(reaches(&s, &socket, "socket").is_err());
}

#[test]
fn verify_full_is_refused_as_before() {
    let Some(s) = server() else { return };
    let url = s.with_host("localhost", "sslmode=verify-full");
    assert_eq!(
        reaches(&s, &url, "verify_full").unwrap_err(),
        PersistenceError::Malformed
    );
}

#[test]
fn tls_checks_the_name_localhost() {
    let Some(s) = server() else { return };
    if std::env::var("LAYERFS_TEST_PG_LOOPBACK_TLS").as_deref() != Ok("1") {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_LOOPBACK_TLS is not 1");
        return;
    }
    reaches(&s, &s.with_host("localhost", "sslmode=require"), "tls_name").unwrap();
    let literal = s.with_host(s.listen, "sslmode=require&connect_timeout=5");
    assert!(
        reaches(&s, &literal, "tls_literal").is_err(),
        "a certificate naming only localhost must not pass for {}",
        s.listen
    );
}
