//! A text scan reads a file's root once, whatever its size, and a size needs
//! no payload (#352 range 002: grep re-read each file's root, and its delta
//! chain, for its size and again for every 16 KiB window).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fmt::Write;
use std::fs;
use support::{write, Fixture, Work};

fn text(lines: usize, seed: &str) -> String {
    let mut out = String::new();
    for n in 0..lines {
        writeln!(out, "{seed} line {n:06} of a scanned file").unwrap();
    }
    out
}

fn grep(fixture: &Fixture, path: &str) -> Work {
    let call = || {
        fixture.call(
            "fs.grep",
            vec![
                ("branch", SdkValue::text("edited")),
                ("pattern", SdkValue::text("NEVER_PRESENT")),
                ("path", SdkValue::text(path)),
            ],
        )
    };
    let before = fixture.work();
    let found = call();
    assert!(matches!(found.get("items"), Some(SdkValue::List(items)) if items.is_empty()));
    fixture.since(before)
}

#[test]
fn grep_reads_each_root_once_whatever_its_size() {
    let fixture = Fixture::new("sdk-grep-reads", |source| {
        fs::write(source.join("small.txt"), text(20, "small")).unwrap();
        fs::write(source.join("large.txt"), text(2800, "large")).unwrap();
        2
    });
    fixture.fork("edited");
    // Edits give both files the same short delta chain.
    for round in 0..3 {
        fixture.apply(
            "edited",
            vec![
                write("/small.txt", text(20, &format!("small {round}"))),
                write("/large.txt", text(2800, &format!("large {round}"))),
            ],
        );
    }
    let small = grep(&fixture, "/small.txt");
    let large = grep(&fixture, "/large.txt");
    println!("SDK_GREP_READS small={small:?} large={large:?} large_bytes={} profile=Disposable cache=uncontrolled diagnostic_count_gate=true", text(2800, "large 2").len());
    assert!(text(2800, "large 2").len() > 6 * 16384 && text(2800, "large 2").len() < 131072);
    assert_eq!(
        (small.object_batches, small.object_ids),
        (large.object_batches, large.object_ids),
        "a larger whole-file root is read once per window: small {small:?}, large {large:?}"
    );
    fixture.cleanup();
}
