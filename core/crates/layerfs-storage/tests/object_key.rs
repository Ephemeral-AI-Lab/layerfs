//! Independent SHA-256 known answers protect sealed-key byte compatibility.
use layerfs_storage::port::ObjectKey;
fn hex(key: ObjectKey) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(64);
    for b in key.as_bytes() {
        write!(text, "{b:02x}").unwrap();
    }
    text
}

#[test]
fn sealed_keys_keep_standard_sha256_bytes_for_empty_short_and_many_blocks() {
    assert_eq!(
        hex(ObjectKey::for_bytes(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(ObjectKey::for_bytes(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(ObjectKey::for_bytes(&vec![b'a'; 1_000_000])),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}
