//! `fs.read_bytes` (#417): a file's raw bytes at a saved snapshot, one
//! bounded window at a time, through the public method table.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_service::read::window;
use support::Fixture;

fn field<'a>(value: &'a SdkValue, name: &str) -> &'a SdkValue {
    value.get(name).unwrap_or_else(|| panic!("missing {name}"))
}

fn bytes(value: &SdkValue) -> &[u8] {
    match field(value, "bytes") {
        SdkValue::Bytes(bytes) => bytes,
        other => panic!("bytes: {other:?}"),
    }
}

fn integer(value: &SdkValue, name: &str) -> i64 {
    match field(value, name) {
        SdkValue::Int(n) => *n,
        other => panic!("{name}: {other:?}"),
    }
}

fn boolean(value: &SdkValue, name: &str) -> bool {
    match field(value, name) {
        SdkValue::Bool(b) => *b,
        other => panic!("{name}: {other:?}"),
    }
}

fn call(
    fixture: &Fixture,
    path: &str,
    offset: Option<i64>,
    length: Option<i64>,
) -> Result<SdkValue, SdkRefusal> {
    let mut fields = vec![
        ("branch", SdkValue::text("main")),
        ("path", SdkValue::text(path)),
    ];
    fields.extend(offset.map(|n| ("offset", SdkValue::Int(n))));
    fields.extend(length.map(|n| ("length", SdkValue::Int(n))));
    fixture.try_call("fs.read_bytes", fields)
}

#[test]
fn a_file_reads_back_in_bounded_windows() {
    let content: Vec<u8> = (0..window::BYTES + 1000).map(|n| (n % 251) as u8).collect();
    let fixture = Fixture::new("sdk-read-bytes", |source| {
        std::fs::write(source.join("big.bin"), &content).unwrap();
        1
    });
    let read = |offset, length| call(&fixture, "/big.bin", offset, length).unwrap();

    let first = read(None, None);
    assert_eq!(bytes(&first), &content[..window::BYTES]);
    assert_eq!(integer(&first, "size"), content.len() as i64);
    assert!(boolean(&first, "hasMore"));

    let rest = read(Some(window::BYTES as i64), None);
    assert_eq!(bytes(&rest), &content[window::BYTES..]);
    assert_eq!(integer(&rest, "size"), content.len() as i64);
    assert!(!boolean(&rest, "hasMore"));

    // A longer request is capped at one window.
    let capped = read(Some(0), Some(1_000_000));
    assert_eq!(bytes(&capped), &content[..window::BYTES]);
    assert!(boolean(&capped, "hasMore"));

    // An offset past the end names no bytes and no continuation.
    let past = read(Some(content.len() as i64 + 5), None);
    assert!(bytes(&past).is_empty());
    assert_eq!(integer(&past, "size"), content.len() as i64);
    assert!(!boolean(&past, "hasMore"));

    let empty = read(Some(0), Some(0));
    assert!(bytes(&empty).is_empty());
    assert!(boolean(&empty, "hasMore"));
}

#[test]
fn a_directory_a_symlink_and_a_missing_path_are_refused() {
    let fixture = Fixture::new("sdk-read-bytes-refusals", |source| {
        std::fs::write(source.join("file"), b"x").unwrap();
        std::os::unix::fs::symlink("file", source.join("link")).unwrap();
        2
    });
    assert_eq!(
        call(&fixture, "/", None, None).unwrap_err().code,
        "IsDirectory"
    );
    assert_eq!(
        call(&fixture, "/link", None, None).unwrap_err().code,
        "NotFile"
    );
    assert_eq!(
        call(&fixture, "/nope", None, None).unwrap_err().code,
        "NoSuchPath"
    );
    assert_eq!(
        call(&fixture, "/file", Some(-1), None).unwrap_err().code,
        "InvalidInput"
    );
    assert_eq!(
        call(&fixture, "/file", None, Some(-1)).unwrap_err().code,
        "InvalidInput"
    );
}
