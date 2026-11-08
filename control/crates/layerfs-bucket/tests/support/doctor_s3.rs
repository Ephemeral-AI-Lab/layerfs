//! Fault peer for diagnostic S3 requests; no real service or bucket is changed.
#![allow(dead_code)]
use layerfs_bucket::bucket::minio;
use layerfs_bucket::bucket::s3_api::{S3Client, S3Config, MIN_PART};
use layerfs_bucket::bucket::Credentials;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

pub const TOKEN: &str = "0000000000000000000000000000000000000000000000000000000000000001";
#[derive(Clone, Copy, Debug, Default)]
pub struct Mode {
    pub list_denied: bool,
    pub signature_failure: bool,
    pub region_failure: bool,
    pub stall_get: bool,
    pub versioning_denied: bool,
    pub unversioned: bool,
    pub put_denied: bool,
    pub unsupported_put: bool,
    pub collision: bool,
    pub lost_put: bool,
    pub error_after_put: bool,
    pub lost_delete: bool,
    pub error_after_delete: bool,
    pub denied_delete: bool,
    pub bad_download: bool,
    pub replace_before_delete: bool,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: BTreeMap<String, String>,
}
#[derive(Default)]
pub struct State {
    pub requests: Vec<Request>,
    pub versions: BTreeMap<String, Vec<u8>>,
    pub token: String,
}
pub struct Server {
    pub endpoint: String,
    pub state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub fn new(mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        if mode.collision {
            state
                .lock()
                .unwrap()
                .versions
                .insert("existing".into(), b"existing backup bytes".to_vec());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, data) = (stop.clone(), state.clone());
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        serve(&mut stream, mode, &data);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(error) => panic!("mock accept: {error}"),
                }
            }
        });
        Self {
            endpoint,
            state,
            stop,
            thread: Some(thread),
        }
    }
    pub fn client(&self) -> S3Client {
        S3Client::new(self.config()).unwrap()
    }
    pub fn config(&self) -> S3Config {
        S3Config {
            endpoint: self.endpoint.clone(),
            bucket: "bucket".into(),
            prefix: "saved/prefix".into(),
            region: "us-east-1".into(),
            credentials: Credentials {
                access_key: "mock-access-key".into(),
                secret_key: "mock-secret-key".into(),
            },
            connect_timeout: Duration::from_secs(2),
            io_timeout: Duration::from_secs(2),
            part_size: MIN_PART,
            limits: minio::limits(),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
fn serve(stream: &mut TcpStream, mode: Mode, state: &Mutex<State>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut first = String::new();
    if reader.read_line(&mut first).unwrap_or(0) == 0 {
        return;
    }
    let mut words = first.split_whitespace();
    let method = words.next().unwrap().to_owned();
    let target = words.next().unwrap().to_owned();
    let mut headers = BTreeMap::new();
    let mut head_size = first.len();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        head_size += line.len();
        assert!(head_size < 16384);
        if line == "\r\n" {
            break;
        }
        let (name, value) = line.split_once(':').unwrap();
        headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
    }
    let length = headers
        .get("content-length")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(length < 4096);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    assert!(headers
        .get("authorization")
        .unwrap()
        .starts_with("AWS4-HMAC-SHA256 "));
    let mut state = state.lock().unwrap();
    state.requests.push(Request {
        method: method.clone(),
        target: target.clone(),
        headers: headers.clone(),
    });
    if target.contains("list-type=2") {
        assert!(target.contains("prefix=saved%2Fprefix%2F"));
        assert!(target.contains("max-keys=1"));
        if mode.signature_failure {
            reply(stream,403,&[],b"<Error><Code>SignatureDoesNotMatch</Code><Message>mock-secret-key</Message></Error>");
            return;
        }
        if mode.region_failure {
            reply(stream,400,&[],b"<Error><Code>AuthorizationHeaderMalformed</Code><Message>mock-access-key</Message></Error>");
            return;
        }
        if mode.list_denied {
            reply(
                stream,
                403,
                &[],
                b"<Error><Code>AccessDenied</Code></Error>",
            );
        } else {
            reply(stream,200,&[],b"<ListBucketResult><KeyCount>0</KeyCount><IsTruncated>false</IsTruncated></ListBucketResult>");
        }
        return;
    }
    if target.contains("versioning=") {
        if mode.versioning_denied {
            reply(
                stream,
                403,
                &[],
                b"<Error><Code>AccessDenied</Code></Error>",
            );
        } else if mode.unversioned {
            reply(stream, 200, &[], b"<VersioningConfiguration/>");
        } else {
            reply(
                stream,
                200,
                &[],
                b"<VersioningConfiguration><Status>Enabled</Status></VersioningConfiguration>",
            );
        }
        return;
    }
    assert!(target.starts_with("/bucket/saved/prefix/_layerfs_doctor/"));
    if method == "PUT" {
        assert_eq!(headers.get("if-none-match").map(String::as_str), Some("*"));
        assert!(headers
            .get("authorization")
            .unwrap()
            .contains("if-none-match"));
        if mode.collision {
            reply(
                stream,
                412,
                &[],
                b"<Error><Code>PreconditionFailed</Code></Error>",
            );
            return;
        }
        if mode.put_denied {
            reply(
                stream,
                403,
                &[],
                b"<Error><Code>AccessDenied</Code></Error>",
            );
            return;
        }
        if mode.unsupported_put {
            reply(
                stream,
                501,
                &[],
                b"<Error><Code>NotImplemented</Code></Error>",
            );
            return;
        }
        state.token = headers.get("x-amz-meta-layerfs-doctor").unwrap().clone();
        state.versions.insert("owned-v1".into(), body);
        if mode.error_after_put {
            reply(
                stream,
                500,
                &[],
                b"<Error><Code>InternalError</Code></Error>",
            );
            return;
        }
        if !mode.lost_put {
            reply(stream, 200, &[("x-amz-version-id", "owned-v1")], b"");
        }
        return;
    }
    if method == "GET" {
        assert!(target.ends_with("versionId=owned-v1") || !target.contains('?'));
        let body = if mode.bad_download {
            b"corrupt".to_vec()
        } else {
            state.versions.get("owned-v1").unwrap().clone()
        };
        if mode.stall_get {
            let head=format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\nx-amz-version-id: owned-v1\r\nx-amz-meta-layerfs-doctor: {}\r\n\r\n",body.len(),state.token);
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body[..1]).unwrap();
            std::thread::sleep(Duration::from_millis(600));
            return;
        }
        reply(
            stream,
            200,
            &[
                ("x-amz-version-id", "owned-v1"),
                ("x-amz-meta-layerfs-doctor", &state.token),
            ],
            &body,
        );
        return;
    }
    assert_eq!(method, "DELETE");
    assert!(target.ends_with("versionId=owned-v1"));
    if mode.replace_before_delete {
        state.versions.insert(
            "replacement-v2".into(),
            b"replacement backup bytes".to_vec(),
        );
    }
    if mode.denied_delete {
        reply(
            stream,
            403,
            &[],
            b"<Error><Code>AccessDenied</Code></Error>",
        );
        return;
    }
    state.versions.remove("owned-v1");
    if mode.error_after_delete {
        reply(
            stream,
            500,
            &[],
            b"<Error><Code>InternalError</Code></Error>",
        );
        return;
    }
    if !mode.lost_delete {
        reply(stream, 204, &[("x-amz-version-id", "owned-v1")], b"");
    }
}
fn reply(stream: &mut TcpStream, status: u16, headers: &[(&str, &str)], body: &[u8]) {
    let mut reply = format!(
        "HTTP/1.1 {status} result\r\ncontent-length: {}\r\nconnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        reply.push_str(&format!("{name}: {value}\r\n"));
    }
    reply.push_str("\r\n");
    stream.write_all(reply.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
}
