//! `backup.connect` at a connected bucket's URL replaces its key and keeps
//! every Store's link to it (spec v21), against a local fake S3 endpoint
//! that accepts one key id at a time; it checks the key id only, so a key
//! it refuses is a different id. A failed proof changes nothing, another URL
//! under a taken name is `Exists` with the disconnect fix, a record that
//! cannot be written puts the old secret back, and the call log keeps the
//! secret's byte count only. Records only: runs on macOS.
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_home::names::BucketName;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::backup::bucket::record::{BucketConnect, BucketRecord};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-backup-rotate-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A fake S3 endpoint: a listing signed with key id `allowed` gets an
/// empty page, any other id `403 InvalidAccessKeyId`. It counts requests.
struct Fake {
    endpoint: String,
    allowed: Arc<Mutex<String>>,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Fake {
    fn new(allowed: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let allowed = Arc::new(Mutex::new(allowed.to_owned()));
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (key, count, flag) = (allowed.clone(), requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &key, &count),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(e) => panic!("fake accept: {e}"),
                }
            }
        });
        Self {
            endpoint,
            allowed,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn allow(&self, key: &str) {
        *self.allowed.lock().unwrap() = key.to_owned();
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One request on one connection: every reply closes it.
fn serve(stream: TcpStream, allowed: &Mutex<String>, count: &AtomicUsize) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut first = String::new();
    if reader.read_line(&mut first).unwrap_or(0) == 0 {
        return;
    }
    let mut key = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("authorization") {
                let id = value.split("Credential=").nth(1).unwrap_or("");
                key = id.split('/').next().unwrap_or("").to_owned();
            }
        }
    }
    count.fetch_add(1, Ordering::SeqCst);
    let listing = first.starts_with("GET ") && first.contains("list-type=2");
    let (status, body) = match listing && key == *allowed.lock().unwrap() {
        true => (
            200,
            "<ListBucketResult><KeyCount>0</KeyCount><IsTruncated>false</IsTruncated></ListBucketResult>",
        ),
        false => (403, "<Error><Code>InvalidAccessKeyId</Code></Error>"),
    };
    let reply = format!(
        "HTTP/1.1 {status} result\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = stream;
    let _ = stream.write_all(reply.as_bytes());
}

fn connect(
    home: &Path,
    url: &str,
    access: &str,
    secret: &str,
    detail: bool,
) -> Result<SdkValue, layerfs_protocol::sdk::SdkRefusal> {
    let mut args = vec![
        ("name".to_owned(), t("b1")),
        ("url".to_owned(), t(url)),
        ("access_key".to_owned(), t(access)),
        ("secret_key".to_owned(), t(secret)),
    ];
    if detail {
        args.push(("detail".to_owned(), SdkValue::Bool(true)));
    }
    layerfs_api::call(
        Some(crate::root::root_of(home)),
        "backup.connect",
        SdkValue::Map(args),
        &mut |_| {},
    )
}

fn secret_file(home: &Path) -> PathBuf {
    home.join("secret/bucket/b1.json")
}

fn bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A Store record that links `b1`, written by hand: rotation must not read
/// or write it.
fn linked_store(home: &Path) -> PathBuf {
    let path = paths::store_record(home, "shop");
    let link = SdkValue::map([
        ("file", t("shop")),
        ("heads", t("ab".repeat(32).as_str())),
        (
            "last",
            SdkValue::map([
                ("op", t("create")),
                ("at", SdkValue::Int(1_700_000_000_000)),
                ("bytes", SdkValue::Int(4096)),
                ("sha256", t("cd".repeat(32).as_str())),
            ]),
        ),
    ]);
    write_record(
        &path,
        &SdkValue::map([
            ("name", t("shop")),
            ("backups", SdkValue::map([("b1", link)])),
        ]),
    )
    .unwrap();
    path
}

#[test]
fn rotation_at_the_same_url_keeps_the_links_and_the_new_key() {
    let home = tempdir("same");
    let fake = Fake::new("AKID-1");
    let url = format!("{}/bucket/pre", fake.endpoint);
    let first = connect(&home, &url, "AKID-1", "secret-one", true).unwrap();
    assert_eq!(first.get_text("outcome"), Some("connected"), "{first:?}");
    let store = linked_store(&home);
    let before_store = bytes(&store);
    // anything else the bucket record holds stays as it was
    let record_path = paths::bucket_record(&home, "b1");
    let record = read_record(&record_path).unwrap().unwrap();
    let extra = SdkValue::map([("shop", t("kept"))]);
    write_record(
        &record_path,
        &home::with_field(record, "stores", extra.clone()),
    )
    .unwrap();

    // the same URL after parsing (a trailing slash), a new key: the short
    // form is a first connect's
    fake.allow("AKID-2");
    let rotated = connect(&home, &format!("{url}/"), "AKID-2", "secret-two", false).unwrap();
    assert_eq!(rotated, SdkValue::map([("ok", SdkValue::Bool(true))]));
    let record = read_record(&record_path).unwrap().unwrap();
    assert_eq!(record.get_text("access_key"), Some("AKID-2"));
    assert_eq!(record.get("stores"), Some(&extra), "{record:?}");
    assert_eq!(record.get_text("prefix"), Some("pre"));
    assert_eq!(bytes(&store), before_store, "the Store's link changed");
    // the secret file holds the new key and nothing of the old
    let kept = String::from_utf8(bytes(&secret_file(&home))).unwrap();
    assert!(
        kept.contains("secret-two") && !kept.contains("secret-one"),
        "secret file changed wrongly"
    );
    let mode = std::fs::metadata(secret_file(&home))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    // the home's key is the new one: the endpoint takes only AKID-2 now
    let bucket = BucketRecord::of(&home, "b1").unwrap();
    let credentials = bucket.credentials(&home).unwrap();
    assert_eq!(credentials.access_key, "AKID-2");
    assert_eq!(credentials.secret_key, "secret-two");
    let driver = bucket.driver(&home).unwrap();
    layerfs_store::backup::copy::list_stores(driver.as_ref()).unwrap();

    // the long form says it rotated
    fake.allow("AKID-3");
    let long = connect(&home, &url, "AKID-3", "secret-three", true).unwrap();
    assert_eq!(long.get_text("outcome"), Some("rotated"), "{long:?}");
    assert_eq!(long.get_text("access_key"), Some("AKID-3"));
    assert_eq!(long.get_text("url"), Some(url.as_str()));
    assert_eq!(bytes(&store), before_store, "the Store's link changed");
    drop(fake);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_failed_proof_keeps_the_old_key_and_the_record() {
    let home = tempdir("failed");
    let fake = Fake::new("AKID-1");
    let url = format!("{}/bucket/pre", fake.endpoint);
    connect(&home, &url, "AKID-1", "secret-one", false).unwrap();
    let record_path = paths::bucket_record(&home, "b1");
    let (record, secret) = (bytes(&record_path), bytes(&secret_file(&home)));
    // a key the bucket refuses
    let refused = connect(&home, &url, "AKID-2", "secret-two", false).unwrap_err();
    assert_eq!(refused.code, "Unavailable", "{refused:?}");
    assert_eq!(bytes(&record_path), record);
    assert_eq!(bytes(&secret_file(&home)), secret);
    // a bucket that does not answer
    drop(fake);
    let gone = connect(&home, &url, "AKID-2", "secret-two", false).unwrap_err();
    assert_eq!(gone.code, "Unavailable", "{gone:?}");
    assert_eq!(bytes(&record_path), record);
    assert_eq!(bytes(&secret_file(&home)), secret);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn another_url_under_a_taken_name_is_exists_with_the_disconnect_fix() {
    let home = tempdir("other");
    let fake = Fake::new("AKID-1");
    let url = format!("{}/bucket/pre", fake.endpoint);
    connect(&home, &url, "AKID-1", "secret-one", false).unwrap();
    let record_path = paths::bucket_record(&home, "b1");
    let (record, secret) = (bytes(&record_path), bytes(&secret_file(&home)));
    let sent = fake.requests();
    for other in [
        format!("{}/bucket/other", fake.endpoint),
        format!("{}/another/pre", fake.endpoint),
        "http://127.0.0.1:9/bucket/pre".to_owned(),
    ] {
        let refused = connect(&home, &other, "AKID-1", "secret-one", false).unwrap_err();
        assert_eq!(refused.code, "Exists", "{refused:?}");
        assert!(refused.message.contains("new name"), "{refused:?}");
        assert_eq!(
            refused.detail.get_text("fix"),
            Some("layerfs backup disconnect b1"),
            "{refused:?}"
        );
    }
    // refused before any request, and nothing changed
    assert_eq!(fake.requests(), sent);
    assert_eq!(bytes(&record_path), record);
    assert_eq!(bytes(&secret_file(&home)), secret);
    drop(fake);
    std::fs::remove_dir_all(home).unwrap();
}

/// The place an r2 URL names includes its jurisdiction; a trailing slash
/// does not change it.
#[test]
fn the_same_place_compares_the_parsed_url() {
    let record = |url: &str| {
        BucketRecord::from_url(&BucketConnect {
            name: BucketName::parse("b1").unwrap(),
            url: url.to_owned(),
            access_key: "id".to_owned(),
            secret_key: "secret".to_owned(),
        })
        .unwrap()
    };
    let eu = record("r2://acct/bucket/pre?jurisdiction=eu");
    assert!(eu.same_place(&record("r2://acct/bucket/pre/?jurisdiction=eu")));
    assert!(!eu.same_place(&record("r2://acct/bucket/pre")));
    assert!(!eu.same_place(&record("r2://acct/bucket/pre?jurisdiction=fedramp")));
    assert!(!eu.same_place(&record("r2://other/bucket/pre?jurisdiction=eu")));
    let s3 = record("http://127.0.0.1:9000/bucket/pre");
    assert!(s3.same_place(&record("http://127.0.0.1:9000/bucket/pre/")));
    assert!(!s3.same_place(&record("https://127.0.0.1:9000/bucket/pre")));
    assert!(!s3.same_place(&record("http://127.0.0.1:9000/bucket")));
    // the host's case and a default port do not change the place (RV3 NOTE 5)
    let host = record("https://S3.Example.com/bucket/pre");
    assert!(host.same_place(&record("https://s3.example.com/bucket/pre")));
    assert!(host.same_place(&record("https://s3.example.com:443/bucket/pre/")));
    assert!(!host.same_place(&record("https://s3.example.com:8443/bucket/pre")));
    assert!(!host.same_place(&record("http://s3.example.com/bucket/pre")));
    let plain = record("http://s3.example.com/bucket/pre");
    assert!(plain.same_place(&record("http://S3.EXAMPLE.COM:80/bucket/pre")));
    assert!(!plain.same_place(&record("http://s3.example.com/Bucket/pre")));
    assert!(eu.same_place(&record("r2://ACCT/bucket/pre?jurisdiction=eu")));
}

/// The call log of a rotation: the URL and the secret as byte counts, the
/// key id as it is, never the secret.
#[test]
fn the_call_log_keeps_the_secrets_byte_count_only() {
    let home = tempdir("log");
    let fake = Fake::new("AKID-1");
    let url = format!("{}/bucket/pre", fake.endpoint);
    connect(&home, &url, "AKID-1", "first-s3cr3t", false).unwrap();
    fake.allow("AKID-2");
    connect(&home, &url, "AKID-2", "rotated-s3cr3t-key", false).unwrap();
    let text = std::fs::read_to_string(paths::calls_file(&home)).unwrap();
    assert!(!text.contains("s3cr3t"), "a secret was logged");
    let last = layerfs_home::json::parse(text.lines().last().unwrap()).unwrap();
    let Some(SdkValue::List(words)) = last.get("argv") else {
        panic!("no argv: {last:?}")
    };
    let words: Vec<_> = words
        .iter()
        .map(|w| match w {
            SdkValue::Text(text) => text.clone(),
            other => panic!("argv word {other:?}"),
        })
        .collect();
    assert_eq!(&words[..3], ["backup", "connect", "b1"]);
    assert_eq!(words[3], format!("<{} bytes>", url.len()));
    assert!(words.contains(&"AKID-2".to_owned()), "{words:?}");
    let at = words.iter().position(|w| w == "--key-stdin").unwrap();
    assert_eq!(words[at + 1], "<18 bytes>", "{words:?}");
    drop(fake);
    std::fs::remove_dir_all(home).unwrap();
}

/// A record written before 2.0 names no key id: its fix connects it again
/// at its URL, and that connect is a rotation that keeps the Store's link.
#[test]
fn a_record_with_no_key_id_is_fixed_by_connecting_again() {
    let home = tempdir("old");
    let fake = Fake::new("AKID-1");
    let url = format!("{}/bucket/pre", fake.endpoint);
    write_record(
        &paths::bucket_record(&home, "b1"),
        &SdkValue::map([
            ("name", t("b1")),
            ("provider", t("minio")),
            ("endpoint", t(&fake.endpoint)),
            ("bucket", t("bucket")),
            ("prefix", t("pre")),
            ("region", t("us-east-1")),
        ]),
    )
    .unwrap();
    let store = linked_store(&home);
    let before_store = bytes(&store);
    let refused = BucketRecord::of(&home, "b1")
        .unwrap()
        .credentials(&home)
        .expect_err("no key id");
    assert_eq!(refused.code, "Failed");
    assert_eq!(
        refused.detail.get_text("fix"),
        Some(format!("layerfs backup connect b1 {url} --access-key <key> --key-stdin").as_str())
    );
    let long = connect(&home, &url, "AKID-1", "secret-one", true).unwrap();
    assert_eq!(long.get_text("outcome"), Some("rotated"), "{long:?}");
    let credentials = BucketRecord::of(&home, "b1")
        .unwrap()
        .credentials(&home)
        .unwrap();
    assert_eq!(credentials.access_key, "AKID-1");
    assert_eq!(bytes(&store), before_store, "the Store's link changed");
    drop(fake);
    std::fs::remove_dir_all(home).unwrap();
}

/// A rotation whose record cannot be written puts the secret it replaced
/// back (RV3 NOTE 4): the record and the secret are as they were.
#[test]
fn a_record_that_cannot_be_written_puts_the_old_secret_back() {
    let home = tempdir("putback");
    let fake = Fake::new("AKID-1");
    let url = format!("{}/bucket/pre", fake.endpoint);
    connect(&home, &url, "AKID-1", "secret-one", false).unwrap();
    let record_path = paths::bucket_record(&home, "b1");
    let (record, secret) = (bytes(&record_path), bytes(&secret_file(&home)));
    // the record's folder takes no new file: its write fails after the
    // secret's
    let folder = record_path.parent().unwrap().to_owned();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
    fake.allow("AKID-2");
    let refused = connect(&home, &url, "AKID-2", "secret-two", false);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    let refused = refused.unwrap_err();
    assert_ne!(refused.code, "Exists", "{refused:?}");
    assert!(
        !refused.message.contains("could not be put back"),
        "{refused:?}"
    );
    assert_eq!(bytes(&record_path), record);
    assert_eq!(bytes(&secret_file(&home)), secret);
    drop(fake);
    std::fs::remove_dir_all(home).unwrap();
}
