//! The S3 API driver against a real MinIO: single PUT, ranged GET, HEAD,
//! DELETE, multipart above the part size, pieces, and a failed multipart
//! that is aborted and leaves no parts. Against an `https://` MinIO (built
//! with `--features tls`) the same tests run over TLS, except the ones that
//! inject faults through a plain-HTTP proxy, which say they were skipped.
//!
//! The service tests are ignored by default: start a MinIO with
//! `tests/minio_service.sh start NAME`, eval its exports, then run
//! `cargo test -p layerfs-remote-storage --test minio -- --ignored`.
use layerfs_remote_storage::minio::{self, MinioSettings};
use layerfs_remote_storage::pieces;
use layerfs_remote_storage::s3_api::{part_size, S3Client, S3Config, MAX_PARTS, MIN_PART};
use layerfs_remote_storage::{CredentialRef, Driver, Limits, Metadata, RemoteError, Span};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, thread};

const MIB: u64 = 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn unique(tag: &str) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{tag}-{}-{}-{nonce}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn scratch(tag: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(unique(tag));
    fs::create_dir_all(&path).unwrap();
    path
}

fn bytes(n: u64, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(131).wrapping_add(seed) ^ (i >> 13) as u8)
        .collect()
}

/// The service under test, in its own namespace.
fn settings(tag: &str) -> MinioSettings {
    let mut settings = MinioSettings::from_env()
        .expect("LAYERFS_REMOTE_* of a running MinIO (tests/minio_service.sh)");
    settings.prefix = format!("tests/{}", unique(tag));
    settings.part_size = MIN_PART;
    settings
}

fn client(settings: &MinioSettings) -> S3Client {
    minio::connect(settings).unwrap()
}

#[derive(Clone, Copy, PartialEq)]
enum Fault {
    /// Reply 500 to part 2 without forwarding it.
    RefusePart2,
    /// Forward part 2, then close without its reply.
    DropPart2Reply,
    /// Reply 500 to part 2 and to the abort.
    RefusePart2AndAbort,
}

/// A one-request-per-connection proxy in front of MinIO that records each
/// request line and injects `fault`. It reads plain HTTP, so an `https://`
/// MinIO has none: the caller says so and skips (these tests run against an
/// `http://` MinIO; `multipart_direct_round_trip` covers multipart over TLS).
fn proxy(upstream: &str, fault: Option<Fault>) -> Option<(String, Arc<Mutex<Vec<String>>>)> {
    let Some(upstream) = upstream.strip_prefix("http://") else {
        eprintln!("skipped: the fault proxy reads plain HTTP; {upstream} is not http://");
        return None;
    };
    let upstream = upstream.to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    thread::spawn(move || {
        for client in listener.incoming() {
            let Ok(client) = client else { return };
            let upstream = upstream.clone();
            let log = log.clone();
            thread::spawn(move || relay(client, &upstream, fault, &log));
        }
    });
    Some((endpoint, seen))
}

fn relay(client: TcpStream, upstream: &str, fault: Option<Fault>, log: &Mutex<Vec<String>>) {
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut reader = BufReader::new(client.try_clone().unwrap());
    let mut head = Vec::new();
    loop {
        let mut line = Vec::new();
        if reader.read_until(b'\n', &mut line).unwrap() == 0 {
            return;
        }
        let end = line == b"\r\n";
        head.extend(line);
        if end {
            break;
        }
    }
    let text = String::from_utf8(head.clone()).unwrap();
    let request = text.lines().next().unwrap().to_owned();
    let length: u64 = text
        .lines()
        .find_map(|l| l.strip_prefix("content-length: "))
        .map_or(0, |v| v.trim().parse().unwrap());
    let mut body = Vec::new();
    (&mut reader).take(length).read_to_end(&mut body).unwrap();
    log.lock().unwrap().push(request.clone());
    let part2 = request.starts_with("PUT ") && request.contains("partNumber=2&");
    let abort = request.starts_with("DELETE ") && request.contains("?uploadId=");
    let mut client = client;
    let refuse = match fault {
        Some(Fault::RefusePart2) => part2,
        Some(Fault::RefusePart2AndAbort) => part2 || abort,
        _ => false,
    };
    if refuse {
        let xml = "<Error><Code>InjectedFault</Code></Error>";
        let reply = format!(
            "HTTP/1.1 500 Internal Server Error\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{xml}",
            xml.len()
        );
        client.write_all(reply.as_bytes()).unwrap();
        return;
    }
    let mut server = TcpStream::connect(upstream).unwrap();
    server.write_all(&head).unwrap();
    server.write_all(&body).unwrap();
    let mut reply = Vec::new();
    server.read_to_end(&mut reply).unwrap();
    if part2 && fault == Some(Fault::DropPart2Reply) {
        let _ = client.shutdown(Shutdown::Both);
        return;
    }
    client.write_all(&reply).unwrap();
}

fn requests(seen: &Mutex<Vec<String>>) -> Vec<String> {
    seen.lock()
        .unwrap()
        .iter()
        .map(|line| {
            let (method, rest) = line.split_once(' ').unwrap();
            let query = rest
                .split(' ')
                .next()
                .unwrap()
                .split_once('?')
                .map(|(_, q)| q);
            let kind = match query {
                None => "object",
                Some("uploads=") => "create",
                Some(q) if q.starts_with("partNumber=") => {
                    return format!("{method} part {}", &q[11..q.find('&').unwrap()]);
                }
                Some(q) if q.starts_with("uploadId=") => "upload",
                Some(_) => "other",
            };
            format!("{method} {kind}")
        })
        .collect()
}

#[test]
fn part_sizes_and_config_refusals() {
    assert_eq!(part_size(12 * MIB, MIN_PART).unwrap(), MIN_PART);
    // 100 GiB at 5 MiB would need 20,480 parts: the size rises to whole MiB
    let size = part_size(100 * 1024 * MIB, MIN_PART).unwrap();
    assert_eq!(size, 11 * MIB);
    assert!((100 * 1024 * MIB).div_ceil(size) <= MAX_PARTS);
    assert!(part_size(MIB, MIB).is_err());
    let config = |endpoint: &str| S3Config {
        endpoint: endpoint.to_owned(),
        bucket: "layerfs-test".to_owned(),
        prefix: String::new(),
        region: "us-east-1".to_owned(),
        credentials: layerfs_remote_storage::Credentials {
            access_key: "a".to_owned(),
            secret_key: "b".to_owned(),
        },
        connect_timeout: Duration::from_secs(1),
        io_timeout: Duration::from_secs(1),
        part_size: MIN_PART,
        limits: minio::limits(),
    };
    // https:// needs the tls feature; without it the endpoint is refused before anything is sent
    if cfg!(feature = "tls") {
        assert!(S3Client::new(config("https://example.com")).is_ok());
        assert_eq!(
            S3Client::new(config("https://example.com"))
                .unwrap()
                .config()
                .endpoint,
            "https://example.com"
        );
    } else {
        assert!(matches!(
            S3Client::new(config("https://example.com")),
            Err(RemoteError::Unsupported(_))
        ));
    }
    assert!(matches!(
        S3Client::new(config("ftp://example.com")),
        Err(RemoteError::Invalid(_))
    ));
    // any reachable host: a container name, a host alias, IPv6, a trailing slash
    for endpoint in [
        "http://lfs_minio-1:9000",
        "http://host.docker.internal:9000/",
        "http://[::1]:9000",
        "http://[fd00::2]",
        "http://minio",
    ] {
        assert!(S3Client::new(config(endpoint)).is_ok(), "{endpoint}");
    }
    for endpoint in [
        "http://",
        "http://h:0",
        "http://h:9000/path",
        "http://[::1",
        "http://a b",
    ] {
        assert!(S3Client::new(config(endpoint)).is_err(), "{endpoint}");
    }
    let mut bad = config("http://127.0.0.1:9");
    bad.bucket = "No_Caps".to_owned();
    assert!(S3Client::new(bad).is_err());
    // nothing listens on port 9: refused before any byte is sent
    let client = S3Client::new(config("http://127.0.0.1:9")).unwrap();
    assert!(matches!(client.stat("x"), Err(RemoteError::Unavailable(_))));
    assert!(CredentialRef::parse("env:OTHER_PREFIX").is_err());
    assert!(CredentialRef::parse("file:relative").is_err());
    assert_eq!(
        CredentialRef::parse("env:LAYERFS_REMOTE").unwrap().text(),
        "env:LAYERFS_REMOTE"
    );
}

#[test]
#[ignore = "needs MinIO: tests/minio_service.sh"]
fn single_put_ranged_get_stat_delete() {
    let settings = settings("single");
    let remote = client(&settings);
    let root = scratch("single");
    let data = bytes(MIB + 17, 5);
    fs::write(root.join("file"), &data).unwrap();
    let metadata = Metadata::from([("purpose".to_owned(), "single-put".to_owned())]);
    remote
        .upload_file("store.sqlite", &root.join("file"), None, &metadata)
        .unwrap();
    let stat = remote.stat("store.sqlite").unwrap().unwrap();
    assert_eq!(stat.bytes, data.len() as u64);
    assert_eq!(stat.metadata, metadata);
    let mut whole = Vec::new();
    remote
        .download_file("store.sqlite", None, &mut whole)
        .unwrap();
    assert_eq!(whole, data);
    let mut part = Vec::new();
    let span = Span {
        offset: 1000,
        length: 70_000,
    };
    assert_eq!(
        remote
            .download_file("store.sqlite", Some(span), &mut part)
            .unwrap(),
        70_000
    );
    assert_eq!(part, &data[1000..71_000]);
    remote.delete("store.sqlite").unwrap();
    assert_eq!(remote.stat("store.sqlite").unwrap(), None);
    assert_eq!(
        remote.download_file("store.sqlite", None, &mut Vec::new()),
        Err(RemoteError::Missing)
    );
}

#[test]
#[ignore = "needs MinIO: tests/minio_service.sh"]
fn multipart_above_the_part_size() {
    let mut settings = settings("multipart");
    let Some((endpoint, seen)) = proxy(&settings.endpoint, None) else {
        return;
    };
    settings.endpoint = endpoint;
    let remote = client(&settings);
    let root = scratch("multipart");
    let data = bytes(2 * MIN_PART + 3 * MIB + 3, 11);
    fs::write(root.join("file"), &data).unwrap();
    let sent = pieces::upload(
        &remote,
        "store.sqlite",
        &root.join("file"),
        &Metadata::new(),
    )
    .unwrap();
    assert_eq!(sent.pieces, 0);
    let put = requests(&seen);
    assert_eq!(
        put[..5],
        [
            "POST create",
            "PUT part 1",
            "PUT part 2",
            "PUT part 3",
            "POST upload"
        ]
    );
    assert_eq!(
        remote.stat("store.sqlite").unwrap().unwrap().bytes,
        data.len() as u64
    );
    assert!(remote.pending_uploads("store.sqlite").unwrap().is_empty());
    let mut out = fs::File::create(root.join("down")).unwrap();
    let got = pieces::download(&remote, "store.sqlite", &mut out).unwrap();
    assert_eq!(got.sha256, sent.sha256);
    assert_eq!(fs::read(root.join("down")).unwrap(), data);
}

/// Multipart straight to the service, over whichever scheme it serves
/// (`https://` with the `tls` feature): three parts, nothing left pending,
/// and the same bytes back.
#[test]
#[ignore = "needs MinIO: tests/minio_service.sh"]
fn multipart_direct_round_trip() {
    let remote = client(&settings("direct"));
    let root = scratch("direct");
    let data = bytes(2 * MIN_PART + 3 * MIB + 3, 17);
    fs::write(root.join("file"), &data).unwrap();
    let sent = pieces::upload(
        &remote,
        "store.sqlite",
        &root.join("file"),
        &Metadata::new(),
    )
    .unwrap();
    assert_eq!(sent.pieces, 0);
    assert_eq!(
        remote.stat("store.sqlite").unwrap().unwrap().bytes,
        data.len() as u64
    );
    assert!(remote.pending_uploads("store.sqlite").unwrap().is_empty());
    let mut out = fs::File::create(root.join("down")).unwrap();
    let got = pieces::download(&remote, "store.sqlite", &mut out).unwrap();
    assert_eq!(got.sha256, sent.sha256);
    assert_eq!(fs::read(root.join("down")).unwrap(), data);
}

/// The same namespace, reached directly instead of through the proxy.
fn settings_direct(proxied: &MinioSettings) -> MinioSettings {
    MinioSettings {
        endpoint: MinioSettings::from_env().unwrap().endpoint,
        ..proxied.clone()
    }
}

fn failed_multipart(fault: Fault) {
    let mut settings = settings("abort");
    let Some((endpoint, seen)) = proxy(&settings.endpoint, Some(fault)) else {
        return;
    };
    settings.endpoint = endpoint;
    let remote = client(&settings);
    let root = scratch("abort");
    fs::write(root.join("file"), bytes(3 * MIN_PART, 2)).unwrap();
    let error = remote
        .upload_file("store.sqlite", &root.join("file"), None, &Metadata::new())
        .unwrap_err();
    match fault {
        Fault::RefusePart2 => assert_eq!(
            error,
            RemoteError::Refused {
                status: 500,
                code: "InjectedFault".to_owned()
            }
        ),
        Fault::DropPart2Reply => assert!(matches!(error, RemoteError::Uncertain(_)), "{error}"),
        Fault::RefusePart2AndAbort => {
            let RemoteError::PartsLeft { upload, abort, .. } = error else {
                panic!("{error}")
            };
            assert!(matches!(*abort, RemoteError::Refused { status: 500, .. }));
            // the parts stay listed until an operator aborts the upload
            assert_eq!(
                remote.pending_uploads("store.sqlite").unwrap(),
                [upload.clone()]
            );
            client(&settings_direct(&settings))
                .abort_upload("store.sqlite", &upload)
                .unwrap();
        }
    }
    // part 2 was sent once, never again; then the upload was aborted
    assert_eq!(
        requests(&seen)[..4],
        ["POST create", "PUT part 1", "PUT part 2", "DELETE upload"]
    );
    assert!(remote.pending_uploads("store.sqlite").unwrap().is_empty());
    assert_eq!(remote.stat("store.sqlite").unwrap(), None);
}

#[test]
#[ignore = "needs MinIO: tests/minio_service.sh"]
fn refused_part_aborts_and_leaves_no_parts() {
    failed_multipart(Fault::RefusePart2);
}

#[test]
#[ignore = "needs MinIO: tests/minio_service.sh"]
fn failed_abort_reports_the_parts_left() {
    failed_multipart(Fault::RefusePart2AndAbort);
}

#[test]
#[ignore = "needs MinIO: tests/minio_service.sh"]
fn lost_part_reply_is_uncertain_not_resent_and_aborted() {
    failed_multipart(Fault::DropPart2Reply);
}

#[test]
#[ignore = "needs MinIO: tests/minio_service.sh"]
fn file_over_max_goes_up_as_pieces() {
    let settings = settings("pieces");
    let mut config = client(&settings).config().clone();
    config.limits = Limits {
        max_file: 8 * MIB,
        max_request: 8 * MIB,
        piece_size: 6 * MIB,
    };
    config.part_size = 8 * MIB;
    let remote = S3Client::new(config).unwrap();
    let root = scratch("pieces");
    let data = bytes(13 * MIB, 4);
    fs::write(root.join("file"), &data).unwrap();
    let sent = pieces::upload(
        &remote,
        "store.sqlite",
        &root.join("file"),
        &Metadata::new(),
    )
    .unwrap();
    assert_eq!(sent.pieces, 3);
    let mut out = fs::File::create(root.join("down")).unwrap();
    let got = pieces::download(&remote, "store.sqlite", &mut out).unwrap();
    assert_eq!(
        (got.bytes, got.pieces, got.sha256),
        (13 * MIB, 3, sent.sha256)
    );
    assert_eq!(fs::read(root.join("down")).unwrap(), data);
    pieces::delete(&remote, "store.sqlite").unwrap();
    assert_eq!(
        remote.stat(&pieces::piece_name("store.sqlite", 0)).unwrap(),
        None
    );
}
