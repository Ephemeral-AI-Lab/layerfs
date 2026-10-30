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

const MAX_LINE: i64 = 9_007_199_254_740_991;
const WINDOW: usize = 16 * 1024;
const BLOCK: usize = 16 * WINDOW;

fn read_wire(
    fixture: &Fixture,
    reference: &str,
    path: &str,
    start: Option<i64>,
    end: Option<i64>,
    binary: Option<bool>,
    offset: Option<SdkValue>,
) -> Result<SdkValue, SdkRefusal> {
    let mut fields = vec![("branch", text(reference)), ("path", text(path))];
    if let Some(start) = start {
        fields.push(("startLine", SdkValue::Int(start)));
    }
    if let Some(end) = end {
        fields.push(("endLine", SdkValue::Int(end)));
    }
    if let Some(binary) = binary {
        fields.push(("binary", SdkValue::Bool(binary)));
    }
    if let Some(offset) = offset {
        fields.push(("offset", offset));
    }
    fixture.try_call("fs.read", fields)
}

fn next_offset(value: &SdkValue) -> Option<&str> {
    match value.get("nextOffset") {
        Some(SdkValue::Text(offset)) => Some(offset),
        Some(SdkValue::Null) => None,
        other => panic!("nextOffset must be present and text or null: {other:?}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    layerfs_bridge::sdk_values::hex(bytes)
}

fn assert_cursor(value: &SdkValue, line: i64, offset: usize, binary: bool, path: &str) {
    let cursor = next_offset(value).expect("a continued page has a cursor");
    let fields: Vec<_> = cursor.split('.').collect();
    assert_eq!(fields.len(), 6, "{cursor}");
    assert_eq!(fields[0], "lfr1");
    assert_eq!(fields[1], if binary { "b" } else { "t" });
    assert_eq!(fields[2], line.to_string());
    assert_eq!(fields[3], offset.to_string());
    assert_eq!(fields[4].len(), 64);
    assert!(fields[4]
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    assert_eq!(fields[5], hex(path.as_bytes()));
}

fn ascii_line(number: usize, width: usize) -> Vec<u8> {
    let mut line = format!("{number:016} ");
    while line.len() + 1 < width {
        line.push(char::from(b'a' + (number % 26) as u8));
    }
    line.truncate(width - 1);
    line.push('\n');
    assert_eq!(line.len(), width);
    line.into_bytes()
}

fn fixed_lines(size: usize) -> Vec<u8> {
    assert_eq!(size % 128, 0);
    (0..size / 128).flat_map(|n| ascii_line(n, 128)).collect()
}

#[derive(Debug)]
struct Traversal {
    bytes: u64,
    reads: u64,
    pages: usize,
    final_page: ContentIo,
}

fn traverse_fixed(fixture: &Fixture, path: &str, expected: &[u8]) -> Traversal {
    let overall_before = content_io(fixture);
    let expected_pages = expected.len().div_ceil(BUDGET);
    let (mut rebuilt, mut offset, mut pages, mut start) = (Vec::new(), None, 0, 1);
    let final_page = loop {
        pages += 1;
        assert!(
            pages <= expected_pages + 1,
            "cursor traversal did not terminate"
        );
        let page_before = content_io(fixture);
        let reply = read_wire(
            fixture,
            "main",
            path,
            None,
            None,
            Some(false),
            offset.as_deref().map(text),
        )
        .unwrap();
        let after = content_io(fixture);
        let page_io = ContentIo {
            bytes: after.bytes - page_before.bytes,
            reads: after.reads - page_before.reads,
        };
        let content = reply.get_text("text").unwrap().as_bytes();
        assert_eq!(content.len() % 128, 0);
        assert_eq!(int(&reply, "startLine"), Some(start));
        assert_eq!(
            int(&reply, "endLine"),
            Some(start + (content.len() / 128) as i64 - 1)
        );
        assert!(!boolean(&reply, "lineTruncated"));
        let next = int(&reply, "nextLine");
        assert_eq!(boolean(&reply, "truncated"), next.is_some());
        assert_eq!(next_offset(&reply).is_some(), next.is_some());
        if let Some(next) = next {
            assert_cursor(&reply, next, (next as usize - 1) * 128, false, path);
        }
        rebuilt.extend_from_slice(content);
        let Some(next) = next else {
            break page_io;
        };
        start = next;
        offset = next_offset(&reply).map(str::to_owned);
    };
    assert_eq!(rebuilt, expected);
    assert_eq!(rebuilt.chunks_exact(128).count(), expected.len() / 128);
    for (n, chunk) in rebuilt.chunks_exact(128).enumerate() {
        assert_eq!(chunk, ascii_line(n, 128));
    }
    let after = content_io(fixture);
    Traversal {
        bytes: after.bytes - overall_before.bytes,
        reads: after.reads - overall_before.reads,
        pages,
        final_page,
    }
}

#[test]
fn cursor_traversal_is_linear_at_two_sizes_and_bounded_on_the_late_page() {
    let small = fixed_lines(2 * 1024 * 1024);
    let large = fixed_lines(4 * 1024 * 1024);
    let fixture = Fixture::new("area-p8-cursor-two-sizes", |source| {
        fs::write(source.join("two.txt"), &small).unwrap();
        fs::write(source.join("four.txt"), &large).unwrap();
        2
    });
    let two = traverse_fixed(&fixture, "/two.txt", &small);
    let four = traverse_fixed(&fixture, "/four.txt", &large);
    println!("P8_CURSOR_TWO_SIZE size=2097152 {two:?}");
    println!("P8_CURSOR_TWO_SIZE size=4194304 {four:?}");
    assert_eq!(two.pages, small.len().div_ceil(BUDGET));
    assert_eq!(four.pages, large.len().div_ceil(BUDGET));
    assert!(
        four.bytes <= 3 * two.bytes,
        "byte scaling: {two:?} {four:?}"
    );
    assert!(
        four.reads <= 3 * two.reads,
        "read scaling: {two:?} {four:?}"
    );
    assert!(
        four.final_page.bytes < 2_000_000,
        "the late 4 MiB page rescanned a prefix: {four:?}"
    );
    fixture.cleanup();
}

fn boundary_bytes(terminated: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for n in 0.. {
        if bytes.len() >= BLOCK + 2 * WINDOW {
            break;
        }
        bytes.extend_from_slice(&ascii_line(n, [127, 128, 129][n % 3]));
    }
    bytes.extend_from_slice(&ascii_line(9000, BUDGET + 257));
    bytes.extend_from_slice(b"after-huge\n");
    bytes.extend_from_slice(if terminated { b"tail\n" } else { b"tail" });
    bytes
}

fn line_offset(bytes: &[u8], line: i64) -> usize {
    if line == 1 {
        return 0;
    }
    let mut seen = 1;
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            seen += 1;
            if seen == line {
                return at + 1;
            }
        }
    }
    panic!("line {line} has no start")
}

fn assert_cursor_reference(fixture: &Fixture, path: &str, expected: &[u8]) {
    let (mut offset, mut start, mut pages, mut saw_truncated) = (None, 1, 0, false);
    let final_reply = loop {
        pages += 1;
        assert!(pages < 100, "cursor reference traversal did not terminate");
        let cursor = read_wire(
            fixture,
            "main",
            path,
            None,
            None,
            Some(false),
            offset.as_deref().map(text),
        )
        .unwrap();
        let numeric = read(fixture, "main", path, start, MAX_LINE, Some(false));
        assert_eq!(
            cursor, numeric,
            "cursor and numeric reference differ at line {start}"
        );
        saw_truncated |= boolean(&cursor, "lineTruncated");
        let next = int(&cursor, "nextLine");
        assert_eq!(next_offset(&cursor).is_some(), next.is_some());
        if let Some(next) = next {
            assert_cursor(&cursor, next, line_offset(expected, next), false, path);
        }
        if next.is_none() {
            break cursor;
        }
        start = next.unwrap();
        offset = next_offset(&cursor).map(str::to_owned);
    };
    assert!(saw_truncated, "the returned huge line was truncated");
    let expected_lines = expected.iter().filter(|&&byte| byte == b'\n').count() as i64
        + i64::from(!expected.ends_with(b"\n"));
    assert_eq!(int(&final_reply, "endLine"), Some(expected_lines));
    assert_eq!(int(&final_reply, "nextLine"), None);
    assert_eq!(next_offset(&final_reply), None);
}

#[test]
fn cursor_pages_are_exactly_once_across_windows_blocks_and_truncation() {
    let terminated = boundary_bytes(true);
    let unterminated = boundary_bytes(false);
    let fixture = Fixture::new("area-p8-cursor-boundaries", |source| {
        fs::write(source.join("terminated.txt"), &terminated).unwrap();
        fs::write(source.join("unterminated.txt"), &unterminated).unwrap();
        2
    });
    assert_cursor_reference(&fixture, "/terminated.txt", &terminated);
    assert_cursor_reference(&fixture, "/unterminated.txt", &unterminated);
    fixture.cleanup();
}

#[test]
fn cursor_precedence_and_numeric_validation_keep_their_contract() {
    let content: Vec<u8> = (0..2500)
        .flat_map(|n| format!("line-{n}\n").into_bytes())
        .collect();
    let fixture = Fixture::new("area-p8-cursor-precedence", |source| {
        fs::write(source.join("lines.txt"), content).unwrap();
        1
    });
    let first = read(&fixture, "main", "/lines.txt", 1, MAX_LINE, Some(false));
    let cursor = next_offset(&first).unwrap().to_owned();
    assert_eq!(int(&first, "nextLine"), Some(2001));
    let continued = read_wire(
        &fixture,
        "main",
        "/lines.txt",
        None,
        None,
        Some(false),
        Some(text(&cursor)),
    )
    .unwrap();
    let both = read_wire(
        &fixture,
        "main",
        "/lines.txt",
        Some(7),
        None,
        Some(false),
        Some(text(&cursor)),
    )
    .unwrap();
    assert_eq!(both, continued);
    assert_eq!(int(&both, "startLine"), Some(2001));
    for (start, end, message) in [
        (None, Some(2000), "endLine must be at least startLine"),
        (Some(0), None, "startLine must be a positive safe integer"),
    ] {
        let refusal = read_wire(
            &fixture,
            "main",
            "/lines.txt",
            start,
            end,
            Some(false),
            Some(text(&cursor)),
        )
        .unwrap_err();
        assert_eq!(refusal.code, "InvalidInput");
        assert_eq!(refusal.message, message);
        assert_eq!(refusal.detail, SdkValue::Null);
    }
    fixture.cleanup();
}

#[test]
fn legacy_numeric_window_is_a_non_discriminating_compatibility_guard() {
    let fixture = Fixture::new("area-p8-numeric-compat", |source| {
        fs::write(source.join("lines.txt"), b"one\ntwo\nthree\n").unwrap();
        1
    });
    let reply = read(&fixture, "main", "/lines.txt", 2, 3, Some(false));
    assert_text(&reply, "two\nthree\n");
    assert_eq!(int(&reply, "startLine"), Some(2));
    assert_eq!(int(&reply, "endLine"), Some(3));
    assert!(!boolean(&reply, "lineTruncated"));
    assert!(!boolean(&reply, "truncated"));
    assert_eq!(int(&reply, "nextLine"), None);
    assert_eq!(int(&reply, "size"), Some(14));
    fixture.cleanup();
}

fn exact_refusal(result: Result<SdkValue, SdkRefusal>, message: &str) {
    let refusal = result.expect_err("the cursor read must be refused");
    assert_eq!(refusal.code, "InvalidInput");
    assert_eq!(refusal.message, message);
    assert_eq!(refusal.detail, SdkValue::Null);
}

fn commit_id(value: &SdkValue) -> String {
    value
        .get("commit")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned()
}

#[test]
fn cursor_format_binding_and_cross_snapshot_rules_are_exact() {
    let content: Vec<u8> = (0..2500)
        .flat_map(|n| format!("line-{n}\n").into_bytes())
        .collect();
    let fixture = Fixture::new("area-p8-cursor-refusals", |source| {
        for name in ["left.txt", "same.txt"] {
            fs::write(source.join(name), &content).unwrap();
        }
        2
    });
    fixture.fork("p8");
    let first_commit =
        commit_id(&fixture.apply("p8", vec![write("/unrelated.txt", b"one\n".to_vec())]));
    let first = read(
        &fixture,
        &first_commit,
        "/left.txt",
        1,
        MAX_LINE,
        Some(false),
    );
    let cursor = next_offset(&first).unwrap().to_owned();
    let binary_cursor = next_offset(&read(
        &fixture,
        &first_commit,
        "/left.txt",
        1,
        MAX_LINE,
        Some(true),
    ))
    .unwrap()
    .to_owned();
    let replace = |cursor: &str, index: usize, value: &str| {
        let mut fields: Vec<String> = cursor.split('.').map(str::to_owned).collect();
        fields[index] = value.to_owned();
        fields.join(".")
    };
    let malformed = vec![
        SdkValue::Int(7),
        SdkValue::Bool(true),
        SdkValue::map([("offset", text(&cursor))]),
        text(""),
        text("not-a-cursor"),
        text("lfr2.t.2001.1.00.2f"),
        text(&replace(&cursor, 2, "02001")),
        text(&replace(&cursor, 2, "0")),
        text(&replace(&cursor, 2, "9007199254740992")),
        text(&replace(&cursor, 3, "01")),
        text(&replace(&cursor, 3, "-1")),
        text(&replace(&cursor, 3, "9007199254740992")),
        text(&replace(&cursor, 4, &"A".repeat(64))),
        text(&replace(&cursor, 4, "xyz")),
        text(&replace(&cursor, 5, "")),
        text(&replace(&cursor, 5, "f")),
        text(&replace(&cursor, 5, "FF")),
        text(&replace(&cursor, 5, &hex(b"/left.txt").to_uppercase())),
        text(&replace(&cursor, 5, &hex(b"relative"))),
        text(&replace(&cursor, 5, &hex(b"/bad//path"))),
        text(&format!("{cursor}.extra")),
    ];
    for offset in malformed {
        exact_refusal(
            read_wire(
                &fixture,
                &first_commit,
                "/left.txt",
                None,
                None,
                Some(false),
                Some(offset),
            ),
            "offset is not a valid read cursor",
        );
    }
    let beyond = replace(&cursor, 3, &(content.len() + 1).to_string());
    for (reference, path, offset, binary) in [
        (
            first_commit.as_str(),
            "/same.txt",
            cursor.as_str(),
            Some(false),
        ),
        (
            first_commit.as_str(),
            "/left.txt",
            cursor.as_str(),
            Some(true),
        ),
        (
            first_commit.as_str(),
            "/left.txt",
            binary_cursor.as_str(),
            Some(false),
        ),
        (
            first_commit.as_str(),
            "/left.txt",
            binary_cursor.as_str(),
            None,
        ),
        (
            first_commit.as_str(),
            "/left.txt",
            beyond.as_str(),
            Some(false),
        ),
    ] {
        exact_refusal(
            read_wire(
                &fixture,
                reference,
                path,
                None,
                None,
                binary,
                Some(text(offset)),
            ),
            "offset does not match this file read",
        );
    }
    let second_commit =
        commit_id(&fixture.apply("p8", vec![write("/other.txt", b"two\n".to_vec())]));
    let continued = read_wire(
        &fixture,
        &second_commit,
        "/left.txt",
        None,
        None,
        Some(false),
        Some(text(&cursor)),
    )
    .unwrap();
    assert_eq!(int(&continued, "startLine"), Some(2001));
    assert!(continued
        .get_text("text")
        .unwrap()
        .starts_with("line-2000\n"));
    let published = fixture.call("branch.publish", vec![("branch", text("p8"))]);
    let layer = published
        .get("publishedLayer")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned();
    let at_layer = read_wire(
        &fixture,
        &layer,
        "/left.txt",
        None,
        None,
        Some(false),
        Some(text(&cursor)),
    )
    .unwrap();
    assert_eq!(int(&at_layer, "startLine"), Some(2001));
    fixture.chmod("p8", "/left.txt", 0o600);
    let metadata_only = read_wire(
        &fixture,
        "p8",
        "/left.txt",
        None,
        None,
        Some(false),
        Some(text(&cursor)),
    )
    .unwrap();
    assert_eq!(int(&metadata_only, "startLine"), Some(2001));
    fixture.apply("p8", vec![write("/left.txt", b"changed\n".to_vec())]);
    exact_refusal(
        read_wire(
            &fixture,
            "p8",
            "/left.txt",
            None,
            None,
            Some(false),
            Some(text(&cursor)),
        ),
        "offset does not match this file read",
    );
    let new_commit = commit_id(&fixture.apply("p8", vec![write("/later.txt", content.clone())]));
    let later_cursor = next_offset(&read(
        &fixture,
        &new_commit,
        "/later.txt",
        1,
        MAX_LINE,
        Some(false),
    ))
    .unwrap()
    .to_owned();
    let expected_missing = refusal(&fixture, "main", "/later.txt");
    for offset in [text(&later_cursor), text("malformed")] {
        let missing = read_wire(
            &fixture,
            "main",
            "/later.txt",
            None,
            None,
            Some(false),
            Some(offset),
        )
        .unwrap_err();
        assert_eq!(missing, expected_missing);
    }
    fixture.cleanup();
}
