//! `backup.list` names a bucket it did not ask in `not_listed` with the
//! reason `refused` (the asking refused before any request), never `lost`,
//! which is only for a request that got no answer (RF re-check N-E): a
//! bucket whose key has no saved secret, and one whose record cannot be
//! read. Nothing is sent: neither bucket has an endpoint that answers.
use layerfs_api::home::{self, paths, write_record};
use layerfs_protocol::provision::install::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-backup-not-listed-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

#[test]
fn a_bucket_not_asked_is_refused_not_lost() {
    let home = tempdir("home");
    home::open(&home).unwrap();
    // a bucket whose key's secret the home does not keep
    write_record(
        &paths::bucket_record(&home, "nokey"),
        &SdkValue::map([
            ("name", SdkValue::text("nokey")),
            ("provider", SdkValue::text("minio")),
            ("endpoint", SdkValue::text("http://127.0.0.1:9")),
            ("bucket", SdkValue::text("nothing")),
            ("prefix", SdkValue::text("")),
            ("region", SdkValue::text("us-east-1")),
            ("access_key", SdkValue::text("user")),
        ]),
    )
    .unwrap();
    // a bucket whose record cannot be read
    let bad = paths::bucket_record(&home, "bad");
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "not json").unwrap();

    let listed = layerfs_api::call(
        Some(&home),
        "backup.list",
        SdkValue::Map(Vec::new()),
        &mut |_| {},
    )
    .unwrap_or_else(|refusal| panic!("{refusal:?}"));
    assert_eq!(listed.get("items"), Some(&SdkValue::List(Vec::new())));
    let Some(SdkValue::List(skipped)) = listed.get("not_listed") else {
        panic!("{listed:?}");
    };
    let mut seen: Vec<(Option<&str>, Option<&str>)> = (skipped.iter())
        .map(|entry| (entry.get_text("bucket"), entry.get_text("reason")))
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        [
            (Some("bad"), Some("refused")),
            (Some("nokey"), Some("refused"))
        ],
        "{listed:?}"
    );
    for entry in skipped {
        assert!(
            entry.get_text("message").is_some_and(|m| !m.is_empty()),
            "{entry:?}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

/// A minio bucket record `name` at `endpoint`, with `more` fields.
fn bucket(home: &std::path::Path, name: &str, endpoint: &str, more: &[(&str, SdkValue)]) {
    let mut fields = vec![
        ("name".to_owned(), SdkValue::text(name)),
        ("provider".to_owned(), SdkValue::text("minio")),
        ("endpoint".to_owned(), SdkValue::text(endpoint)),
        ("bucket".to_owned(), SdkValue::text("nothing")),
        ("prefix".to_owned(), SdkValue::text("")),
        ("region".to_owned(), SdkValue::text("us-east-1")),
    ];
    fields.extend(more.iter().map(|(k, v)| ((*k).to_owned(), v.clone())));
    write_record(&paths::bucket_record(home, name), &SdkValue::Map(fields)).unwrap();
}

/// An endpoint that answers each of `replies` requests with HTTP `status`
/// and the S3 error `code`, then stops.
fn refusing(status: u16, code: &'static str, replies: usize) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for _ in 0..replies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let mut request = [0u8; 8192];
            let _ = stream.read(&mut request);
            let body = format!("<Error><Code>{code}</Code></Error>");
            let reply = format!(
                "HTTP/1.1 {status} No\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    address
}

/// UI pass 2 (LF4): a `not_listed` entry carries `fix`, the call that
/// connects the bucket again at its URL with its key (V21-4), when its key
/// was missing (no key id in its record, no saved secret) or the bucket
/// refused it (HTTP 403); a bucket that refused for another reason, and
/// one whose record cannot be read, get none. The short form leaves an
/// absent fix out; the long form has it null.
#[test]
fn a_missing_or_refused_key_gives_the_connect_fix() {
    let home = tempdir("fix");
    home::open(&home).unwrap();
    let key = [("access_key", SdkValue::text("user"))];
    let secret = SdkValue::map([("secret", SdkValue::text("s3cret"))]);
    // no key id: a record written before 2.0
    bucket(&home, "noid", "http://127.0.0.1:9", &[]);
    // a key id and no saved secret
    bucket(&home, "nokey", "http://127.0.0.1:9", &key);
    // the bucket refuses the key, and one refuses for another reason
    let denied = refusing(403, "InvalidAccessKeyId", 2);
    bucket(&home, "denied", &denied, &key);
    let busy = refusing(503, "SlowDown", 2);
    bucket(&home, "busy", &busy, &key);
    for name in ["denied", "busy"] {
        home::secrets::save_secret(&home, "bucket", name, &secret).unwrap();
    }
    let bad = paths::bucket_record(&home, "bad");
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "not json").unwrap();

    let connect = |name: &str, endpoint: &str| {
        format!("layerfs backup connect {name} {endpoint}/nothing --access-key <key> --key-stdin")
    };
    let want = [
        ("bad", None),
        ("busy", None),
        ("denied", Some(connect("denied", &denied))),
        ("noid", Some(connect("noid", "http://127.0.0.1:9"))),
        ("nokey", Some(connect("nokey", "http://127.0.0.1:9"))),
    ];
    for detail in [false, true] {
        let args = SdkValue::map([("detail", SdkValue::Bool(detail))]);
        let listed = layerfs_api::call(Some(&home), "backup.list", args, &mut |_| {})
            .unwrap_or_else(|refusal| panic!("{refusal:?}"));
        let Some(SdkValue::List(skipped)) = listed.get("not_listed") else {
            panic!("{listed:?}");
        };
        let mut seen: Vec<_> = (skipped.iter())
            .map(|entry| {
                assert_eq!(entry.get_text("reason"), Some("refused"), "{entry:?}");
                if entry.get_text("fix").is_none() {
                    let absent = entry.get("fix");
                    match detail {
                        true => assert_eq!(absent, Some(&SdkValue::Null), "{entry:?}"),
                        false => assert_eq!(absent, None, "{entry:?}"),
                    }
                }
                let bucket = entry.get_text("bucket").unwrap().to_owned();
                (bucket, entry.get_text("fix").map(str::to_owned))
            })
            .collect();
        seen.sort();
        let want: Vec<_> = (want.iter())
            .map(|(name, fix)| ((*name).to_owned(), fix.clone()))
            .collect();
        assert_eq!(seen, want, "{listed:?}");
    }
    std::fs::remove_dir_all(home).unwrap();
}
