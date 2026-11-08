//! Absolute diagnostic deadlines cover trickling HTTP headers/body and TLS handshake I/O.
#[path = "support/doctor_s3.rs"]
mod peer;
use layerfs_bucket::bucket::s3_api::S3Client;
use peer::{Mode, Server};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};
fn endpoint(
    body: impl FnOnce(TcpStream) + Send + 'static,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    body(stream);
                    return;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => panic!("accept: {error}"),
            }
        }
    });
    (endpoint, thread)
}
#[test]
fn trickling_headers_and_body_do_not_restart_the_overall_budget() {
    for body in [false, true] {
        let (template, server) = endpoint(move |mut stream| {
            let mut byte = [0];
            let _ = stream.read(&mut byte);
            if body {
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\n");
            }
            let bytes = if body {
                b"<ListBucketResult>".as_slice()
            } else {
                b"HTTP/1.1 200 OK\r\n".as_slice()
            };
            for byte in bytes {
                if stream.write_all(&[*byte]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let config_source = Server::new(Mode::default());
        let mut config = config_source.config();
        config.endpoint = template;
        let client = S3Client::new(config).unwrap();
        let start = Instant::now();
        assert!(client
            .diagnostic_list(start + Duration::from_millis(80))
            .is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }
}
#[cfg(feature = "tls")]
#[test]
fn tls_handshake_wait_is_bounded_without_disabling_certificate_checks() {
    let (endpoint, server) = endpoint(|mut stream| {
        let mut bytes = [0; 128];
        let _ = stream.read(&mut bytes);
        std::thread::sleep(Duration::from_millis(180));
    });
    let config_source = Server::new(Mode::default());
    let mut config = config_source.config();
    config.endpoint = endpoint.replacen("http://", "https://", 1);
    let client = S3Client::new(config).unwrap();
    let start = Instant::now();
    assert!(client
        .diagnostic_list(start + Duration::from_millis(80))
        .is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    server.join().unwrap();
}
#[test]
fn expired_budget_sends_no_request() {
    let server = Server::new(Mode::default());
    assert!(server.client().diagnostic_list(Instant::now()).is_err());
    assert!(server.state.lock().unwrap().requests.is_empty());
}

#[test]
fn hostname_diagnostics_require_the_network_owners_bounded_resolver() {
    fn resolve(
        host: &str,
        port: u16,
        until: Instant,
    ) -> std::io::Result<Vec<std::net::SocketAddr>> {
        assert_eq!(host, "diagnostic.fixture.invalid");
        assert!(until > Instant::now());
        Ok(vec![std::net::SocketAddr::from(([127, 0, 0, 1], port))])
    }
    let server = Server::new(Mode::default());
    let mut config = server.config();
    config.endpoint = config
        .endpoint
        .replace("127.0.0.1", "diagnostic.fixture.invalid");
    let client = S3Client::new(config).unwrap();
    assert!(client
        .diagnostic_list(Instant::now() + Duration::from_secs(1))
        .is_err());
    assert!(
        server.state.lock().unwrap().requests.is_empty(),
        "no ordinary/unbounded DNS fallback"
    );
    let client = client.with_diagnostic_resolver(resolve);
    client
        .diagnostic_list(Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert_eq!(server.state.lock().unwrap().requests.len(), 1);
}

#[test]
fn tcp_address_fallback_sends_one_put_and_never_replays_a_lost_reply() {
    fn resolve(_: &str, port: u16, _: Instant) -> std::io::Result<Vec<std::net::SocketAddr>> {
        Ok(vec![
            std::net::SocketAddr::new(std::net::Ipv6Addr::LOCALHOST.into(), port),
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        ])
    }
    for lost_put in [false, true] {
        let server = Server::new(Mode {
            lost_put,
            ..Mode::default()
        });
        let mut config = server.config();
        config.endpoint = config
            .endpoint
            .replace("127.0.0.1", "diagnostic.fixture.invalid");
        let client = S3Client::new(config)
            .unwrap()
            .with_diagnostic_resolver(resolve);
        // This fixture is explicitly versioned; make PUT the first wire request
        // so TCP fallback is exercised on the mutation's connection admission.
        let result = client.diagnostic_create(peer::TOKEN, Instant::now() + Duration::from_secs(1));
        if lost_put {
            assert!(matches!(
                result,
                Err(layerfs_bucket::bucket::RemoteError::Uncertain(_))
            ));
        } else {
            assert!(result.is_ok());
        }
        let state = server.state.lock().unwrap();
        assert_eq!(state.requests.len(), 1, "no HTTP operation replay");
        assert_eq!(state.requests[0].method, "PUT");
        assert_eq!(state.versions.len(), 1, "one committed immutable version");
    }
}
