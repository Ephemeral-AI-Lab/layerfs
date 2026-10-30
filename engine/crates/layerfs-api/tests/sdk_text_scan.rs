//! `fs.grep` and `fs.read` give the window-at-a-time scan's results (#354,
//! #352 F-035). The whole-buffer scan reads blocks of 16 windows and searches
//! a block at once; its lines, Binary skips, truncation and stops are checked
//! against that scan, kept here as the reference, on files built to put a NUL,
//! invalid UTF-8, a character, a match or a long line on every window and
//! block boundary.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use support::Fixture;

const WINDOW: usize = 16 * 1024;
const BLOCK: usize = 16 * WINDOW;
const BUDGET: usize = 48 * 1024;

/// The window-at-a-time scan before #354, over the file's bytes.
fn reference(
    bytes: &[u8],
    prefix: usize,
    pattern: &[u8],
    mut visit: impl FnMut(u64, &str, u64, bool, bool) -> bool,
) -> Result<(), ()> {
    let mut failure = vec![0; pattern.len()];
    for i in 1..pattern.len() {
        let mut j = failure[i - 1];
        while j > 0 && pattern[i] != pattern[j] {
            j = failure[j - 1];
        }
        if pattern[i] == pattern[j] {
            j += 1;
        }
        failure[i] = j;
    }
    let (mut offset, mut number, mut length) = (0, 1, 0usize);
    let (mut shown, mut utf8) = (Vec::new(), Vec::new());
    let (mut matched, mut state) = (pattern.is_empty(), 0);
    let mut emit = |number: u64, shown: &mut Vec<u8>, length: usize, matched: bool| {
        let end = std::str::from_utf8(shown).map_or_else(|e| e.valid_up_to(), |_| shown.len());
        let text = std::str::from_utf8(&shown[..end]).unwrap().to_owned();
        shown.clear();
        visit(number, &text, length as u64, end < length, matched)
    };
    while offset < bytes.len() {
        let end = bytes.len().min(offset + WINDOW);
        let window = &bytes[offset..end];
        utf8.extend_from_slice(window);
        if utf8.contains(&0) {
            return Err(());
        }
        let valid = match std::str::from_utf8(&utf8) {
            Ok(_) => utf8.len(),
            Err(e) if e.error_len().is_none() && end < bytes.len() => e.valid_up_to(),
            Err(_) => return Err(()),
        };
        utf8.drain(..valid);
        for &byte in window {
            length += 1;
            if shown.len() < prefix {
                shown.push(byte);
            }
            if !matched && !pattern.is_empty() {
                while state > 0 && byte != pattern[state] {
                    state = failure[state - 1];
                }
                if byte == pattern[state] {
                    state += 1;
                }
                if state == pattern.len() {
                    matched = true;
                }
            }
            if byte == b'\n' {
                if !emit(number, &mut shown, length, matched) {
                    return Ok(());
                }
                number += 1;
                length = 0;
                state = 0;
                matched = pattern.is_empty();
            }
        }
        offset = end;
    }
    if length > 0 {
        emit(number, &mut shown, length, matched);
    }
    Ok(())
}

/// One grep item as the reply carries it.
type Item = (String, i64, String, bool);

/// The reference grep over files in walk order: items, skipped, more.
fn reference_grep(
    files: &[(String, Vec<u8>)],
    pattern: &str,
    limit: usize,
) -> (Vec<Item>, i64, bool) {
    let (mut items, mut spent, mut more, mut skipped) = (Vec::new(), 0, false, 0);
    for (path, bytes) in files {
        let scan = reference(
            bytes,
            1024,
            pattern.as_bytes(),
            |n, text, _, truncated, matched| {
                if !matched {
                    return true;
                }
                let cost = path.len() + text.len() + 160;
                if items.len() == limit || spent + cost > BUDGET {
                    more = true;
                    return false;
                }
                spent += cost;
                items.push((path.clone(), n as i64, text.to_owned(), truncated));
                true
            },
        );
        skipped += scan.is_err() as i64;
        if more {
            break;
        }
    }
    (items, skipped, more)
}

/// The reference read: text, last line, truncation, next line, or Binary.
fn reference_read(
    bytes: &[u8],
    start: u64,
    end: Option<u64>,
) -> Option<(String, Option<u64>, bool, Option<u64>)> {
    let requested = end.unwrap_or(9_007_199_254_740_991);
    let end = requested.min(start + 1999);
    let (mut content, mut next, mut clipped, mut last) = (String::new(), None, false, None);
    reference(bytes, BUDGET, b"", |n, text, _, truncated, _| {
        if n < start {
            return true;
        }
        if n > requested {
            return false;
        }
        if n > end || (!content.is_empty() && content.len() + text.len() > BUDGET) {
            next = Some(n);
            return false;
        }
        clipped |= truncated;
        last = Some(n);
        content.push_str(text);
        true
    })
    .ok()?;
    Some((content, last, clipped || next.is_some(), next))
}

/// Lines of `width` bytes, each with `TOKEN` somewhere, to `size` bytes.
fn lines(size: usize, width: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(size);
    let mut n = 0usize;
    while out.len() < size {
        let mut line = format!("{n:07} ");
        if n % 3 == 0 {
            line.push_str("TOKEN ");
        }
        while line.len() + 1 < width {
            line.push(if n % 5 == 0 { 'é' } else { 'x' });
        }
        while line.len() + 1 > width {
            line.pop();
        }
        line.push('\n');
        out.extend_from_slice(line.as_bytes());
        n += 1;
    }
    out.truncate(size);
    out
}

/// `bytes` with `insert` written over it at `at`.
fn put(mut bytes: Vec<u8>, at: usize, insert: &[u8]) -> Vec<u8> {
    bytes[at..at + insert.len()].copy_from_slice(insert);
    bytes
}

fn files() -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    // Window and block edges, and the last byte of the window after the first
    // block, which a part handed on early would have passed.
    let edges = [
        0,
        1,
        WINDOW - 2,
        WINDOW - 1,
        WINDOW,
        WINDOW + 1,
        3 * WINDOW - 1,
        BLOCK - 2,
        BLOCK - 1,
        BLOCK,
        BLOCK + 3,
        BLOCK + WINDOW - 2,
        BLOCK + WINDOW - 1,
        300_000,
    ];
    for at in edges {
        files.push((format!("nul-{at:06}"), put(lines(310_000, 61), at, b"\0")));
        for (name, bad) in [
            ("lone", &b"\x80"[..]),
            ("cut", b"\xE2\x82A"),
            ("short", b"\xF0\x9F\x98\n"),
            ("ff", b"\xFF"),
        ] {
            files.push((
                format!("bad-{name}-{at:06}"),
                put(lines(310_000, 47), at, bad),
            ));
        }
        // A valid character and a match laid across the boundary.
        files.push((
            format!("char-{at:06}"),
            put(lines(310_000, 53), at.saturating_sub(2), "😀".as_bytes()),
        ));
        files.push((
            format!("match-{at:06}"),
            put(lines(310_000, 4099), at.saturating_sub(3), b"TOKEN"),
        ));
    }
    // A part is handed on where the read pass's chunks reach a block; a NUL
    // just past that point, in the same window, must still fail the window.
    for at in (0..32).map(|k| BLOCK + 1000 + k * 1500) {
        files.push((
            format!("nul-sweep-{at:06}"),
            put(lines(310_000, 61), at, b"\0"),
        ));
    }
    for at in [
        2 * BLOCK - 1,
        2 * BLOCK + WINDOW - 1,
        2 * BLOCK + 2 * WINDOW - 1,
    ] {
        files.push((
            format!("nul-big-{at:06}"),
            put(lines(700_000, 61), at, b"\0"),
        ));
    }
    files.push((
        "cut-at-end".into(),
        [lines(300_000, 40), b"\xE2\x82".to_vec()].concat(),
    ));
    files.push(("cut-at-end-small".into(), b"TOKEN \xE2\x82".to_vec()));
    let mut long = vec![b'y'; 600_000];
    long[BLOCK - 2..BLOCK + 3].copy_from_slice(b"TOKEN");
    long.extend_from_slice("é\nTOKEN\n".as_bytes());
    files.push(("long-line".into(), long));
    for width in [1023, 1024, 1025, 1026, 49_151, 49_152, 49_153] {
        let mut line = vec![b'z'; width - 1];
        line[width / 2..width / 2 + 5].copy_from_slice(b"TOKEN");
        let cut = 1022.min(width - 3);
        line[cut..cut + 2].copy_from_slice("é".as_bytes());
        line.push(b'\n');
        files.push((format!("width-{width:06}"), line.repeat(3)));
    }
    files.push(("empty".into(), Vec::new()));
    files.push(("newlines".into(), b"\n\n\n".to_vec()));
    files.push(("no-final-lf".into(), b"a TOKEN\nb TOKEN".to_vec()));
    files.push(("small".into(), "é😀€ TOKEN\nTOKEN\n€ TOKEN\n".into()));
    files.push(("many".into(), lines(600_000, 30)));
    let duplicate = lines(70_000, 33);
    for name in ["dup-a", "dup-b", "dup-c"] {
        files.push((name.into(), duplicate.clone()));
    }
    files.push(("dup-bin-a".into(), put(lines(70_000, 33), 40_000, b"\0")));
    files.push(("dup-bin-b".into(), put(lines(70_000, 33), 40_000, b"\0")));
    files.sort();
    files
}

fn text(value: &SdkValue, key: &str) -> String {
    match value.get(key) {
        Some(SdkValue::Text(text)) => text.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

fn int(value: &SdkValue, key: &str) -> Option<u64> {
    match value.get(key) {
        Some(SdkValue::Int(n)) => Some(*n as u64),
        _ => None,
    }
}

fn boolean(value: &SdkValue, key: &str) -> bool {
    matches!(value.get(key), Some(SdkValue::Bool(true)))
}

fn grep(fixture: &Fixture, path: &str, pattern: &str, limit: i64) -> (Vec<Item>, i64, bool) {
    let reply = fixture.call(
        "fs.grep",
        vec![
            ("branch", SdkValue::text("main")),
            ("pattern", SdkValue::text(pattern)),
            ("path", SdkValue::text(path)),
            ("pageSize", SdkValue::Int(limit)),
        ],
    );
    let Some(SdkValue::List(items)) = reply.get("items") else {
        panic!("grep items");
    };
    let items = items
        .iter()
        .map(|item| {
            (
                text(item, "path"),
                int(item, "line").unwrap() as i64,
                text(item, "text"),
                boolean(item, "lineTruncated"),
            )
        })
        .collect();
    let skipped = int(&reply, "skipped").unwrap() as i64;
    (items, skipped, boolean(&reply, "hasMore"))
}

#[test]
fn grep_and_read_give_the_window_scan_results() {
    let files = files();
    let fixture = Fixture::new("sdk-text-scan", |source| {
        for (name, bytes) in &files {
            std::fs::write(source.join(name), bytes).unwrap();
        }
        files.len()
    });
    let rooted: Vec<(String, Vec<u8>)> = files
        .iter()
        .map(|(name, bytes)| (format!("/{name}"), bytes.clone()))
        .collect();
    // `00043`, `00044` and `00086` match only lines 4300-4499 and 8600-8699:
    // in 61-byte lines, the bytes around the first and second blocks' ends,
    // past where a full page stops.
    let late = ["TOKEN", "TOKEN\n", "😀", "00043", "00044", "00086"];
    for pattern in [
        "TOKEN",
        "TOKEN\n",
        "N\nT",
        "é",
        "😀",
        "\n",
        "x",
        "€ TOKEN",
        "NEVER",
        "00043",
        "00044",
        "00086",
    ] {
        for limit in [1000, 7] {
            assert_eq!(
                grep(&fixture, "/", pattern, limit),
                reference_grep(&rooted, pattern, limit as usize),
                "grep / {pattern:?} {limit}"
            );
        }
        for file in rooted.iter().filter(|_| late.contains(&pattern)) {
            assert_eq!(
                grep(&fixture, &file.0, pattern, 1000),
                reference_grep(std::slice::from_ref(file), pattern, 1000),
                "grep {} {pattern:?}",
                file.0
            );
        }
    }
    for (path, bytes) in &rooted {
        for (start, end) in [
            (1, None),
            (2, Some(3)),
            (5, Some(9000)),
            (2001, None),
            (4200, None),
            (4290, Some(4400)),
            (7000, None),
        ] {
            let mut fields = vec![
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text(path.as_str())),
                ("startLine", SdkValue::Int(start as i64)),
            ];
            if let Some(end) = end {
                fields.push(("endLine", SdkValue::Int(end as i64)));
            }
            let got = fixture.try_call("fs.read", fields).map(|reply| {
                (
                    text(&reply, "text"),
                    int(&reply, "endLine"),
                    boolean(&reply, "truncated"),
                    int(&reply, "nextLine"),
                )
            });
            let want = reference_read(bytes, start, end);
            match (got, want) {
                (Ok(got), Some(want)) => assert_eq!(got, want, "read {path} {start} {end:?}"),
                (Err(refusal), None) => assert_eq!(refusal.code, "Binary", "read {path}"),
                (got, want) => panic!(
                    "read {path} {start} {end:?}: {:?} against {:?}",
                    got.map(|g| g.1),
                    want.map(|w| w.1)
                ),
            }
        }
    }
}

fn read_offset(
    fixture: &Fixture,
    path: &str,
    start: Option<u64>,
    binary: bool,
    offset: Option<&str>,
) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    let mut fields = vec![
        ("branch", SdkValue::text("main")),
        ("path", SdkValue::text(path)),
        ("binary", SdkValue::Bool(binary)),
    ];
    if let Some(start) = start {
        fields.push(("startLine", SdkValue::Int(start as i64)));
    }
    if let Some(offset) = offset {
        fields.push(("offset", SdkValue::text(offset)));
    }
    fixture.try_call("fs.read", fields)
}

fn cursor(value: &SdkValue) -> Option<&str> {
    match value.get("nextOffset") {
        Some(SdkValue::Text(cursor)) => Some(cursor),
        Some(SdkValue::Null) => None,
        other => panic!("nextOffset must be present and text or null: {other:?}"),
    }
}

fn plain_lines(size: usize, width: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(size);
    for n in 0.. {
        if out.len() >= size {
            break;
        }
        let mut line = format!("{n:07} ");
        while line.len() + 1 < width {
            line.push('x');
        }
        line.push('\n');
        out.extend_from_slice(line.as_bytes());
    }
    out.truncate(size);
    out
}

#[test]
fn read_cursors_match_reference_and_preserve_delayed_binary_classification() {
    let valid = [
        (
            "valid-window",
            put(plain_lines(400_000, 61), WINDOW - 2, "😀".as_bytes()),
        ),
        (
            "valid-block",
            put(plain_lines(400_000, 61), BLOCK - 2, "😀".as_bytes()),
        ),
        (
            "valid-budget",
            put(plain_lines(400_000, 61), BUDGET - 2, "😀".as_bytes()),
        ),
    ];
    let bad = [
        ("nul-late", put(plain_lines(400_000, 61), 150_000, b"\0")),
        (
            "invalid-late",
            put(plain_lines(400_000, 61), 170_000, b"\xE2\x82A"),
        ),
    ];
    let fixture = Fixture::new("sdk-text-scan-cursors", |source| {
        for (name, bytes) in valid.iter().chain(&bad) {
            std::fs::write(source.join(name), bytes).unwrap();
        }
        valid.len() + bad.len()
    });
    for (name, _) in &valid {
        let path = format!("/{name}");
        let (mut offset, mut start, mut pages) = (None, 1, 0);
        loop {
            pages += 1;
            assert!(pages < 100, "cursor traversal did not terminate for {path}");
            let reply = read_offset(&fixture, &path, None, false, offset.as_deref()).unwrap();
            let numeric = read_offset(&fixture, &path, Some(start), false, None).unwrap();
            let got = (
                text(&reply, "text"),
                int(&reply, "endLine"),
                boolean(&reply, "truncated"),
                int(&reply, "nextLine"),
            );
            let want = (
                text(&numeric, "text"),
                int(&numeric, "endLine"),
                boolean(&numeric, "truncated"),
                int(&numeric, "nextLine"),
            );
            assert_eq!(got, want, "cursor/reference mismatch for {path} at {start}");
            let next = int(&reply, "nextLine");
            assert_eq!(cursor(&reply).is_some(), next.is_some());
            if next.is_none() {
                break;
            }
            start = next.unwrap();
            offset = cursor(&reply).map(str::to_owned);
        }
        assert!(pages > 1);
    }
    for (name, _) in &bad {
        let path = format!("/{name}");
        let (mut offset, mut pages) = (None, 0);
        let refused = loop {
            assert!(pages < 100, "cursor traversal did not terminate for {path}");
            match read_offset(&fixture, &path, None, false, offset.as_deref()) {
                Ok(reply) => {
                    pages += 1;
                    let next = int(&reply, "nextLine");
                    assert_eq!(cursor(&reply).is_some(), next.is_some());
                    assert!(
                        next.is_some(),
                        "complete traversal unexpectedly succeeded for {path}"
                    );
                    offset = cursor(&reply).map(str::to_owned);
                }
                Err(refusal) => break refusal,
            }
        };
        assert!(pages >= 2, "valid early pages must succeed for {path}");
        assert_eq!(refused.code, "Binary");
        assert_eq!(refused.message, "text contains NUL or invalid UTF-8");
        assert_eq!(refused.detail, SdkValue::Null);
    }
    let binary = read_offset(&fixture, "/nul-late", None, true, None).unwrap();
    let offset = cursor(&binary).unwrap().to_owned();
    let refusal = read_offset(&fixture, "/nul-late", None, false, Some(&offset)).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    assert_eq!(refusal.message, "offset does not match this file read");
    assert_eq!(refusal.detail, SdkValue::Null);
    fixture.cleanup();
}
