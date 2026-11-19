//! A byte range outside its file is refused in words, never as the Rust
//! `Debug` text of its `Span` (LF2 review FAIL 2).
use layerfs_bucket::bucket::driver::{open_span, Span};

#[test]
fn a_range_beyond_its_file_reads_as_words() {
    let dir = std::env::temp_dir().join(format!("layerfs-span-words-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("piece");
    std::fs::write(&file, b"0123456789").unwrap();
    let span = Span {
        offset: 8,
        length: 4,
    };
    let refused = open_span(&file, Some(span)).err().expect("beyond the file");
    let said = refused.to_string();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        said.contains("the range of 4 bytes at byte 8 lies beyond a 10-byte file"),
        "{said}"
    );
    assert!(!said.contains("Span {"), "{said}");
}
