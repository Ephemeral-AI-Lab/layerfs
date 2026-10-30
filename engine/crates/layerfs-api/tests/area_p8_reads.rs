//! P8 Commit/Layer snapshot reads and binary windows over a real disposable Store.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use support::{write, Fixture};

const BUDGET: usize = 48 * 1024;

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn read(
    fixture: &Fixture,
    reference: &str,
    path: &str,
    start: i64,
    end: i64,
    binary: Option<bool>,
) -> SdkValue {
    let mut fields = vec![
        ("branch", text(reference)),
        ("path", text(path)),
        ("startLine", SdkValue::Int(start)),
        ("endLine", SdkValue::Int(end)),
    ];
    if let Some(binary) = binary {
        fields.push(("binary", SdkValue::Bool(binary)));
    }
    fixture.call("fs.read", fields)
}

fn refusal(fixture: &Fixture, reference: &str, path: &str) -> SdkRefusal {
    fixture
        .try_call(
            "fs.read",
            vec![("branch", text(reference)), ("path", text(path))],
        )
        .expect_err("the read is refused")
}

fn bytes(value: &SdkValue) -> &[u8] {
    match value.get("bytes") {
        Some(SdkValue::Bytes(bytes)) => bytes,
        other => panic!("bytes: {other:?}"),
    }
}

fn int(value: &SdkValue, key: &str) -> Option<i64> {
    match value.get(key) {
        Some(SdkValue::Int(number)) => Some(*number),
        Some(SdkValue::Null) | None => None,
        other => panic!("{key}: {other:?}"),
    }
}

fn boolean(value: &SdkValue, key: &str) -> bool {
    match value.get(key) {
        Some(SdkValue::Bool(value)) => *value,
        other => panic!("{key}: {other:?}"),
    }
}

#[derive(Debug)]
struct ContentIo {
    bytes: u64,
    reads: u64,
}

fn content_io(fixture: &Fixture) -> ContentIo {
    let mut total = ContentIo { bytes: 0, reads: 0 };
    for index in 0..256 {
        if let Some(work) = fixture.service.store.reader_storage_diagnostics(index) {
            total.bytes +=
                work.pack_read_bytes + work.payload_read_bytes + work.range_acquired_bytes;
            total.reads += work.read_packs + work.payload_reads;
        }
    }
    total
}

fn huge_line(length: usize) -> Vec<u8> {
    let mut state = 0x4d59_5df4_d0f3_3173u64;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            match (state >> 32) as u8 {
                b'\n' => 0,
                byte => byte,
            }
        })
        .collect()
}

fn assert_huge_next_line_stops(terminated: bool) {
    let fixture = Fixture::new(
        if terminated {
            "area-p8-huge-terminated"
        } else {
            "area-p8-huge-unterminated"
        },
        |source| {
            let mut content = b"first\n".to_vec();
            content.extend_from_slice(&huge_line(4_000_000));
            if terminated {
                content.extend_from_slice(b"\nthird\n");
            }
            fs::write(source.join("huge.bin"), content).unwrap();
            1
        },
    );
    let before = content_io(&fixture);
    let reply = read(&fixture, "main", "/huge.bin", 1, 3, Some(true));
    let after = content_io(&fixture);
    let io = ContentIo {
        bytes: after.bytes - before.bytes,
        reads: after.reads - before.reads,
    };
    assert_eq!(bytes(&reply), b"first\n");
    assert_eq!(int(&reply, "endLine"), Some(1));
    assert_eq!(int(&reply, "nextLine"), Some(2));
    assert!(!boolean(&reply, "lineTruncated"));
    assert!(boolean(&reply, "truncated"));
    assert!(
        io.bytes < 2_000_000,
        "huge next line was read past the reply budget: terminated={terminated} {io:?}"
    );
    assert!(
        io.reads < 16,
        "huge next line caused unbounded content reads: terminated={terminated} {io:?}"
    );
    println!("P8_EARLY_STOP terminated={terminated} {io:?}");
    fixture.cleanup();
}

#[test]
fn huge_terminated_next_line_stops_at_the_reply_budget() {
    assert_huge_next_line_stops(true);
}

#[test]
fn huge_unterminated_next_line_stops_at_the_reply_budget() {
    assert_huge_next_line_stops(false);
}

fn list(fixture: &Fixture, reference: &str, path: &str) -> SdkValue {
    fixture.call(
        "fs.list",
        vec![
            ("branch", text(reference)),
            ("path", text(path)),
            ("pageSize", SdkValue::Int(50)),
            ("pagePos", SdkValue::Int(0)),
        ],
    )
}

fn stat(fixture: &Fixture, reference: &str, path: &str) -> SdkValue {
    fixture.call(
        "fs.stat",
        vec![("branch", text(reference)), ("path", text(path))],
    )
}

fn commits(fixture: &Fixture, branch: &str) -> SdkValue {
    fixture.call(
        "branch.list_commits",
        vec![
            ("branch", text(branch)),
            ("pageSize", SdkValue::Int(1000)),
            ("pagePos", SdkValue::Int(0)),
        ],
    )
}

fn assert_text(value: &SdkValue, expected: &str) {
    assert_eq!(value.get_text("text"), Some(expected), "{value:?}");
    assert!(value.get("bytes").is_none(), "text and bytes: {value:?}");
}

fn assert_refusal(fixture: &Fixture, reference: &str, path: &str, code: &str, message: &str) {
    let refusal = refusal(fixture, reference, path);
    assert_eq!(
        (refusal.code.as_str(), refusal.message.as_str()),
        (code, message)
    );
}

#[test]
fn commit_and_layer_reads_cover_binary_windows_and_exact_refusals() {
    let fixture = Fixture::new("area-p8-reads", |source| {
        fs::create_dir(source.join("dir")).unwrap();
        fs::write(source.join("dir/inside.txt"), b"inside\n").unwrap();
        std::os::unix::fs::symlink("file.txt", source.join("link")).unwrap();
        3
    });
    fixture.fork("p8");
    let mut long = vec![b'x'; 300_000];
    long[20_000] = 0xff;
    long[BUDGET - 1] = 0xe2;
    let mut window = b"first\n".to_vec();
    window.extend_from_slice(&long);
    window.extend_from_slice(b"\nthird\n");
    let mut cut_text = vec![b'x'; BUDGET - 1];
    cut_text.extend_from_slice("é\n".as_bytes());
    let first = fixture.apply(
        "p8",
        vec![
            write("/file.txt", b"v1\n".to_vec()),
            write("/same.txt", b"same\n".to_vec()),
            write("/nul.bin", b"a\0b\n".to_vec()),
            write("/invalid.bin", b"a\xffb\n".to_vec()),
            write("/window.bin", window),
            write("/cut.txt", cut_text),
        ],
    );
    let commit = first
        .get("commit")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();
    let second = fixture.call(
        "fs.write",
        vec![
            ("branch", text("p8")),
            ("path", text("/file.txt")),
            ("content", text("v2\n")),
        ],
    );
    assert_ne!(
        second.get("commit").unwrap().get_text("id"),
        Some(commit.as_str())
    );
    let published = fixture.call("branch.publish", vec![("branch", text("p8"))]);
    let layer = published
        .get("publishedLayer")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();

    let before = commits(&fixture, "p8");
    for (reference, version) in [(&commit, "v1\n"), (&layer, "v2\n")] {
        let root = list(&fixture, reference, "/");
        assert_eq!(root.get_text("snapshot"), Some(&reference[2..14]));
        assert!(support::paths(&root).contains(&"/file.txt"));
        let subdir = list(&fixture, reference, "/dir");
        assert_eq!(support::paths(&subdir), ["/dir/inside.txt"]);
        let stat = stat(&fixture, reference, "/file.txt");
        assert_eq!(stat.get_text("snapshot"), Some(&reference[2..14]));
        assert_eq!(int(&stat, "size"), Some(3));
        let changed = read(&fixture, reference, "/file.txt", 1, 1, Some(false));
        assert_text(&changed, version);
        assert_eq!(changed.get_text("snapshot"), Some(&reference[2..14]));
        let same = read(&fixture, reference, "/same.txt", 1, 1, Some(false));
        assert_text(&same, "same\n");
        let as_bytes = read(&fixture, reference, "/file.txt", 1, 1, Some(true));
        assert_eq!(bytes(&as_bytes), version.as_bytes());
        assert!(as_bytes.get("text").is_none());
    }
    assert_ne!(
        read(&fixture, &commit, "/file.txt", 1, 1, Some(false)).get("root"),
        read(&fixture, &layer, "/file.txt", 1, 1, Some(false)).get("root")
    );

    for (path, expected) in [
        ("/nul.bin", &b"a\0b\n"[..]),
        ("/invalid.bin", &b"a\xffb\n"[..]),
    ] {
        let reply = read(&fixture, &commit, path, 1, 1, Some(true));
        assert_eq!(bytes(&reply), expected);
        assert_eq!(int(&reply, "size"), Some(expected.len() as i64));
        assert!(reply.get("text").is_none());
        for binary in [None, Some(false)] {
            let mut fields = vec![
                ("branch", text(&commit)),
                ("path", text(path)),
                ("startLine", SdkValue::Int(1)),
            ];
            if let Some(binary) = binary {
                fields.push(("binary", SdkValue::Bool(binary)));
            }
            let refusal = fixture
                .try_call("fs.read", fields)
                .expect_err("text mode refuses binary content");
            assert_eq!(
                (refusal.code.as_str(), refusal.message.as_str()),
                ("Binary", "text contains NUL or invalid UTF-8")
            );
        }
    }

    let first_window = read(&fixture, &commit, "/window.bin", 1, 3, Some(true));
    assert_eq!(bytes(&first_window), b"first\n");
    assert_eq!(int(&first_window, "endLine"), Some(1));
    assert_eq!(int(&first_window, "nextLine"), Some(2));
    assert!(!boolean(&first_window, "lineTruncated"));
    assert!(boolean(&first_window, "truncated"));

    let cut = read(&fixture, &commit, "/window.bin", 2, 3, Some(true));
    assert_eq!(bytes(&cut), &long[..BUDGET]);
    assert_eq!(bytes(&cut).len(), BUDGET);
    assert_eq!(int(&cut, "endLine"), Some(2));
    assert_eq!(int(&cut, "nextLine"), Some(3));
    assert_eq!(int(&cut, "size"), Some(300_013));
    assert!(boolean(&cut, "lineTruncated"));
    assert!(boolean(&cut, "truncated"));

    let text_cut = read(&fixture, &commit, "/cut.txt", 1, 1, Some(false));
    assert_eq!(
        text_cut.get_text("text"),
        Some("x".repeat(BUDGET - 1).as_str())
    );
    assert!(boolean(&text_cut, "lineTruncated"));
    assert!(boolean(&text_cut, "truncated"));
    let raw_cut = read(&fixture, &commit, "/cut.txt", 1, 1, Some(true));
    let mut expected_cut = vec![b'x'; BUDGET - 1];
    expected_cut.push(0xc3);
    assert_eq!(bytes(&raw_cut).len(), expected_cut.len());
    assert_eq!(&bytes(&raw_cut)[..BUDGET - 1], &expected_cut[..BUDGET - 1]);
    assert_eq!(bytes(&raw_cut)[BUDGET - 1], expected_cut[BUDGET - 1]);
    assert_eq!(int(&raw_cut, "size"), Some((BUDGET + 2) as i64));
    assert!(boolean(&raw_cut, "lineTruncated"));
    assert!(boolean(&raw_cut, "truncated"));

    assert_refusal(
        &fixture,
        "p8-unknown-branch",
        "/file.txt",
        "NoSuchBranch",
        "Branch p8-unknown-branch does not exist",
    );
    let nonhex = "z".repeat(66);
    assert_refusal(
        &fixture,
        &nonhex,
        "/file.txt",
        "NoSuchBranch",
        &format!("Branch {nonhex} does not exist"),
    );
    let bad_tag = format!("99{}", "e".repeat(64));
    assert_refusal(
        &fixture,
        &bad_tag,
        "/file.txt",
        "InvalidInput",
        &format!("`{bad_tag}` is not a Commit or Layer identity"),
    );
    let unknown_commit = format!("12{}", "c".repeat(64));
    assert_refusal(
        &fixture,
        &unknown_commit,
        "/file.txt",
        "NoSuchCommit",
        &format!("Commit {unknown_commit} does not exist"),
    );
    let unknown_layer = format!("32{}", "d".repeat(64));
    assert_refusal(
        &fixture,
        &unknown_layer,
        "/file.txt",
        "NotFound",
        &format!("Layer {unknown_layer} does not exist"),
    );
    let missing = refusal(&fixture, &commit, "/absent");
    assert_eq!(missing.code, "NoSuchPath");
    assert_eq!(
        missing.message,
        format!("/absent does not exist at {}", &commit[2..14])
    );
    assert_eq!(missing.detail, SdkValue::map([("path", text("/absent"))]));
    assert_refusal(
        &fixture,
        &commit,
        "/dir",
        "IsDirectory",
        "expected a regular file",
    );
    assert_refusal(
        &fixture,
        &commit,
        "/link",
        "NotFile",
        "expected a regular file",
    );
    assert_refusal(
        &fixture,
        &commit,
        "relative",
        "InvalidInput",
        "expected an absolute path without empty, dot or parent components",
    );

    let after = commits(&fixture, "p8");
    assert_eq!(after, before, "snapshot reads moved a Branch head");
    fixture.cleanup();
}
