//! The S3 client's kept-alive connections (#510) against a local fake
//! server: requests share a connection and never ask the server to close;
//! a server close, a closed idle connection or a reply read to its close
//! is not reused; a failure on a kept connection, and a reply body cut
//! short, are `Uncertain` and the request is never sent again.
use layerfs_remote_storage::minio;
use layerfs_remote_storage::s3_api::{S3Client, S3Config, MIN_PART};
use layerfs_remote_storage::{Condition, Credentials, Driver, RemoteError, Span};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SIZE: u64 = 1000;
const WAIT: Duration = Duration::from_secs(5);

fn byte(at: u64) -> u8 {
    (at % 251) as u8
}

/// What the server does with one request.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    /// Answers and keeps the connection.
    Reply,
    /// Answers with `connection: close`, then closes.
    ReplyClose,
    /// Answers, then closes without saying so.
    ReplyThenHangUp,
    /// Answers a body framed by the close (no length), then closes.
    ReplyUntilClose,
    /// Closes without answering.
    HangUp,
    /// Sends the head and part of the body, then closes.
    Cut,
}

/// One request the server read: its connection, its request line and
/// whether it asked for the connection to close.
#[derive(Clone, Debug)]
struct Seen {
    connection: usize,
    line: String,
    close: bool,
}

struct Server {
    endpoint: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    /// One message each time the server closed a connection.
    closed: Receiver<()>,
}

/// A server whose `n`th request (from 0, over all connections) gets `acts[n]`
/// (`Reply` beyond the list).
fn server(acts: Vec<Act>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (tell, closed) = channel();
    let log = seen.clone();
    thread::spawn(move || {
        for (index, stream) in listener.incoming().enumerate() {
            let Ok(stream) = stream else { return };
            let (log, acts, tell) = (log.clone(), acts.clone(), tell.clone());
            thread::spawn(move || serve(stream, index, &log, &acts, &tell));
        }
    });
    Server {
        endpoint,
        seen,
        closed,
    }
}

fn serve(
    stream: TcpStream,
    connection: usize,
    log: &Mutex<Vec<Seen>>,
    acts: &[Act],
    tell: &Sender<()>,
) {
    stream.set_read_timeout(Some(WAIT)).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut out = stream;
    loop {
        let mut head = Vec::new();
        loop {
            let mut line = Vec::new();
            if reader.read_until(b'\n', &mut line).unwrap_or(0) == 0 {
                let _ = tell.send(());
                return;
            }
            let end = line == b"\r\n";
            head.extend(line);
            if end {
                break;
            }
        }
        let text = String::from_utf8(head).unwrap();
        let line = text.lines().next().unwrap().to_owned();
        let field = |name: &str| {
            text.lines()
                .find_map(|l| {
                    l.split_once(':')
                        .filter(|(n, _)| n.eq_ignore_ascii_case(name))
                })
                .map(|(_, v)| v.trim().to_owned())
        };
        let length: u64 = field("content-length").map_or(0, |v| v.parse().unwrap());
        std::io::copy(&mut (&mut reader).take(length), &mut std::io::sink()).unwrap();
        let close = field("connection").is_some_and(|v| v.eq_ignore_ascii_case("close"));
        let act = {
            let mut log = log.lock().unwrap();
            log.push(Seen {
                connection,
                line: line.clone(),
                close,
            });
            acts.get(log.len() - 1).copied().unwrap_or(Act::Reply)
        };
        if act == Act::HangUp {
            let _ = out.shutdown(Shutdown::Both);
            let _ = tell.send(());
            return;
        }
        let (status, mut headers, body) = answer(&line, field("range"));
        let mut sent = body.as_slice();
        match act {
            Act::ReplyClose => headers.push("connection: close".to_owned()),
            Act::ReplyUntilClose => headers.retain(|h| !h.starts_with("content-length")),
            Act::Cut => sent = &body[..body.len() * 3 / 5],
            _ => {}
        }
        let mut reply = format!("HTTP/1.1 {status} test\r\n");
        for header in headers {
            reply.push_str(&header);
            reply.push_str("\r\n");
        }
        reply.push_str("\r\n");
        out.write_all(reply.as_bytes()).unwrap();
        out.write_all(sent).unwrap();
        if act != Act::Reply {
            let _ = out.shutdown(Shutdown::Both);
            let _ = tell.send(());
            return;
        }
    }
}

/// A HEAD or ranged GET of the one 1000-byte object, tag `"v1"`.
fn answer(line: &str, range: Option<String>) -> (u16, Vec<String>, Vec<u8>) {
    let etag = "etag: \"v1\"".to_owned();
    if line.starts_with("HEAD ") {
        return (
            200,
            vec![format!("content-length: {SIZE}"), etag],
            Vec::new(),
        );
    }
    let (start, end) = range
        .and_then(|r| {
            let (a, b) = r.strip_prefix("bytes=")?.split_once('-')?;
            Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?))
        })
        .unwrap_or((0, SIZE - 1));
    let body: Vec<u8> = (start..=end).map(byte).collect();
    let headers = vec![
        format!("content-length: {}", body.len()),
        format!("content-range: bytes {start}-{end}/{SIZE}"),
        etag,
    ];
    (206, headers, body)
}

fn client(endpoint: &str) -> S3Client {
    S3Client::new(S3Config {
        endpoint: endpoint.to_owned(),
        bucket: "layerfs-test".to_owned(),
        prefix: String::new(),
        region: "us-east-1".to_owned(),
        credentials: Credentials {
            access_key: "a".to_owned(),
            secret_key: "b".to_owned(),
        },
        connect_timeout: WAIT,
        io_timeout: WAIT,
        part_size: MIN_PART,
        limits: minio::limits(),
    })
    .unwrap()
}

/// One ranged read of `length` bytes at `offset`, checked against the object.
fn read(client: &S3Client, offset: u64, length: u64) -> Result<(), RemoteError> {
    let mut out = Vec::new();
    let span = Span { offset, length };
    let ranged = client.download_range(
        "obj",
        span,
        &Condition::Match("\"v1\"".to_owned()),
        &mut out,
    )?;
    assert_eq!(ranged.bytes, length);
    assert_eq!(out, (offset..offset + length).map(byte).collect::<Vec<_>>());
    Ok(())
}

fn seen(server: &Server) -> Vec<Seen> {
    server.seen.lock().unwrap().clone()
}

fn connections(server: &Server) -> Vec<usize> {
    seen(server).iter().map(|s| s.connection).collect()
}

#[test]
fn requests_share_one_connection_and_never_ask_to_close() {
    let server = server(vec![]);
    let client = client(&server.endpoint);
    assert_eq!(client.stat("obj").unwrap().unwrap().bytes, SIZE);
    read(&client, 0, 100).unwrap();
    read(&client, 500, 500).unwrap();
    read(&client, 7, 1).unwrap();
    assert_eq!(connections(&server), vec![0, 0, 0, 0]);
    assert!(seen(&server).iter().all(|s| !s.close));
}

#[test]
fn a_connection_the_server_closes_is_not_reused() {
    // `connection: close`; a close without saying so, seen by the quiet check
    // before the next request is written; a body framed by the close
    for act in [Act::ReplyClose, Act::ReplyThenHangUp, Act::ReplyUntilClose] {
        let server = server(vec![act]);
        let client = client(&server.endpoint);
        read(&client, 0, 10).unwrap();
        // the close has happened before the next request is written
        server.closed.recv_timeout(WAIT).unwrap();
        thread::sleep(Duration::from_millis(50));
        read(&client, 10, 10).unwrap();
        read(&client, 20, 10).unwrap();
        assert_eq!(connections(&server), vec![0, 1, 1], "{act:?}");
    }
}

#[test]
fn a_failure_on_a_kept_connection_is_uncertain_and_not_resent() {
    let server = server(vec![Act::Reply, Act::HangUp]);
    let client = client(&server.endpoint);
    read(&client, 0, 10).unwrap();
    let error = read(&client, 10, 10).unwrap_err();
    let RemoteError::Uncertain(why) = &error else {
        panic!("{error:?}");
    };
    assert!(why.contains("on a connection kept"), "{why}");
    server.closed.recv_timeout(WAIT).unwrap();
    // sent once, on the kept connection; nothing was sent again
    assert_eq!(connections(&server), vec![0, 0]);
    // the client's next, different request connects afresh
    read(&client, 20, 10).unwrap();
    assert_eq!(connections(&server), vec![0, 0, 1]);
    assert!(seen(&server).iter().all(|s| s.line.starts_with("GET ")));
}

#[test]
fn a_body_cut_short_is_uncertain_and_its_connection_dropped() {
    let server = server(vec![Act::Cut]);
    let client = client(&server.endpoint);
    let error = read(&client, 0, 100).unwrap_err();
    let RemoteError::Uncertain(why) = &error else {
        panic!("{error:?}");
    };
    assert!(why.contains("reply body ended early"), "{why}");
    assert!(why.contains("40 bytes short"), "{why}");
    read(&client, 100, 100).unwrap();
    assert_eq!(connections(&server), vec![0, 1]);
}
