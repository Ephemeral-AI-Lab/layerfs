//! Upload outcomes and limits (layerfs-project #4, #6) against a local fake
//! server: a multipart completion whose reply is lost, with its abort lost
//! too, stays `Uncertain` (the object may have been replaced); a lost part
//! with a lost abort is `PartsLeft` (it was not); a peer that takes the body
//! in a trickle fails the request instead of holding it; MinIO's largest
//! object is what one multipart upload carries, and a part may not exceed
//! the provider's request limit.
use layerfs_store_backup::bucket::s3_api::{part_size, S3Client, S3Config, MAX_PART, MAX_PARTS};
use layerfs_store_backup::bucket::{minio, r2, Credentials, Driver, Metadata, RemoteError};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};

const MIB: u64 = 1024 * 1024;

/// What the server does with the request it matches.
#[derive(Clone, Copy, PartialEq)]
enum Act {
    Reply,
    /// Reads the request, then closes without answering.
    HangUp,
    /// Reads the body 1 KiB every 20 ms and never answers.
    Trickle,
}

/// The server's act for each kind of request.
#[derive(Clone, Copy)]
struct Script {
    part2: Act,
    complete: Act,
    abort: Act,
    put: Act,
}

fn server(script: Script) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            thread::spawn(move || serve(stream, script));
        }
    });
    endpoint
}

fn serve(stream: TcpStream, script: Script) {
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut out = stream;
    loop {
        let mut head = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let end = line == "\r\n";
            head.push_str(&line);
            if end {
                break;
            }
        }
        let line = head.lines().next().unwrap().to_owned();
        let length: u64 = head
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().to_owned())
            })
            .map_or(0, |v| v.parse().unwrap());
        let (act, status, extra, body) = if line.starts_with("POST ") && !line.contains("uploadId")
        {
            let xml = "<InitiateMultipartUploadResult><UploadId>u1</UploadId></InitiateMultipartUploadResult>";
            (Act::Reply, 200, String::new(), xml.to_owned())
        } else if line.starts_with("PUT ") && line.contains("partNumber=") {
            let act = if line.contains("partNumber=2") {
                script.part2
            } else {
                Act::Reply
            };
            (act, 200, "etag: \"e\"\r\n".to_owned(), String::new())
        } else if line.starts_with("POST ") {
            let xml = "<CompleteMultipartUploadResult></CompleteMultipartUploadResult>";
            (script.complete, 200, String::new(), xml.to_owned())
        } else if line.starts_with("DELETE ") {
            (script.abort, 204, String::new(), String::new())
        } else {
            (script.put, 200, "etag: \"e\"\r\n".to_owned(), String::new())
        };
        if act == Act::Trickle {
            let mut piece = [0u8; 1024];
            while reader.read(&mut piece).unwrap_or(0) > 0 {
                thread::sleep(Duration::from_millis(20));
            }
            return;
        }
        std::io::copy(&mut (&mut reader).take(length), &mut std::io::sink()).unwrap();
        if act == Act::HangUp {
            let _ = out.shutdown(Shutdown::Both);
            return;
        }
        let reply = format!(
            "HTTP/1.1 {status} test\r\n{extra}content-length: {}\r\n\r\n{body}",
            body.len()
        );
        out.write_all(reply.as_bytes()).unwrap();
    }
}

fn client(endpoint: &str, io_timeout: Duration, part_size: u64) -> S3Client {
    S3Client::new(S3Config {
        endpoint: endpoint.to_owned(),
        bucket: "layerfs-test".to_owned(),
        prefix: String::new(),
        region: "us-east-1".to_owned(),
        credentials: Credentials {
            access_key: "a".to_owned(),
            secret_key: "b".to_owned(),
        },
        connect_timeout: Duration::from_secs(5),
        io_timeout,
        part_size,
        limits: minio::limits(),
    })
    .unwrap()
}

/// A file of `bytes` bytes, unique to `tag`.
fn file(tag: &str, bytes: u64) -> PathBuf {
    let path = std::env::temp_dir().join(format!("layerfs-outcomes-{tag}-{}", std::process::id()));
    std::fs::write(&path, vec![7u8; bytes as usize]).unwrap();
    path
}

/// Uploads `path` on a thread of its own; a hang fails the test after 30 s.
fn upload(client: S3Client, path: PathBuf) -> Result<(), RemoteError> {
    let (done, result) = channel();
    thread::spawn(move || {
        let _ = done.send(client.upload_file("obj", &path, None, &Metadata::new()));
        let _ = std::fs::remove_file(&path);
    });
    result
        .recv_timeout(Duration::from_secs(30))
        .expect("the upload did not end within 30 s")
}

const OK: Script = Script {
    part2: Act::Reply,
    complete: Act::Reply,
    abort: Act::Reply,
    put: Act::Reply,
};

#[test]
fn a_lost_completion_with_a_lost_abort_stays_uncertain() {
    let endpoint = server(Script {
        complete: Act::HangUp,
        abort: Act::HangUp,
        ..OK
    });
    let error = upload(
        client(&endpoint, Duration::from_secs(5), 5 * MIB),
        file("complete", 6 * MIB),
    )
    .unwrap_err();
    let RemoteError::Uncertain(why) = &error else {
        panic!("{error:?}");
    };
    assert!(why.contains("abort of multipart upload u1 failed"), "{why}");
}

#[test]
fn a_lost_part_with_a_lost_abort_leaves_parts_but_no_object() {
    let endpoint = server(Script {
        part2: Act::HangUp,
        abort: Act::HangUp,
        ..OK
    });
    let error = upload(
        client(&endpoint, Duration::from_secs(5), 5 * MIB),
        file("part", 11 * MIB),
    )
    .unwrap_err();
    assert!(
        matches!(&error, RemoteError::PartsLeft { upload, .. } if upload == "u1"),
        "{error:?}"
    );
}

#[test]
fn a_peer_taking_the_body_in_a_trickle_fails_the_request() {
    let endpoint = server(Script {
        put: Act::Trickle,
        ..OK
    });
    let started = Instant::now();
    let error = upload(
        client(&endpoint, Duration::from_secs(1), 64 * MIB),
        file("trickle", 32 * MIB),
    )
    .unwrap_err();
    let RemoteError::Uncertain(why) = &error else {
        panic!("{error:?}");
    };
    assert!(why.contains("send stalled"), "{why}");
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn minio_objects_stop_where_one_multipart_upload_does() {
    let limits = minio::limits();
    assert_eq!(limits.max_file, MAX_PARTS * MAX_PART);
    assert!(part_size(limits.max_file, minio::DEFAULT_PART_SIZE).is_ok());
    assert!(part_size(limits.max_file + 1, minio::DEFAULT_PART_SIZE).is_err());
    // anything larger goes up as pieces, each within one upload
    assert_eq!(limits.pieces(limits.max_file), 0);
    assert!(limits.pieces(limits.max_file + 1) > 0);
    assert!(part_size(limits.piece_size, minio::DEFAULT_PART_SIZE).is_ok());
}

#[test]
fn a_part_may_not_exceed_the_providers_request_limit() {
    let config = |part_size: u64| S3Config {
        endpoint: "http://127.0.0.1:9".to_owned(),
        bucket: "layerfs-test".to_owned(),
        prefix: String::new(),
        region: "auto".to_owned(),
        credentials: Credentials {
            access_key: "a".to_owned(),
            secret_key: "b".to_owned(),
        },
        connect_timeout: Duration::from_secs(5),
        io_timeout: Duration::from_secs(5),
        part_size,
        limits: r2::limits(),
    };
    let max = r2::limits().max_request;
    let refused = config(MAX_PART).validate().unwrap_err();
    assert!(
        matches!(&refused, RemoteError::Invalid(why) if why.contains("request limit")),
        "{refused:?}"
    );
    config(max).validate().unwrap();
}
