//! The cursor reader of a log and its rotated file (layerfs-project#178,
//! `layerfs_home::tail::read`): the tail, the lines after a cursor, the
//! lines before one back into the rotated file, a line longer than a
//! window in pieces, an unfinished last line, escaped control characters,
//! and a cursor into a rotated or replaced file.
#![cfg(unix)]
use layerfs_home::log::tail::{self, Pair, CHUNK, DEFAULT_LINES, MAX_LINES};
use layerfs_protocol::sdk::SdkValue;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A scratch folder with one log, `worker.log`, removed when dropped.
struct Log {
    dir: PathBuf,
    pair: Pair,
}

impl Log {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "layerfs-tail-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pair = Pair::of(dir.join("worker.log"));
        assert_eq!(pair.rotated, dir.join("worker.1.log"));
        Self { dir, pair }
    }

    /// Appends `text` to the current file, making it if needed.
    fn append(&self, text: &str) {
        self.append_bytes(text.as_bytes());
    }

    fn append_bytes(&self, bytes: &[u8]) {
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.pair.current)
            .unwrap();
        file.write_all(bytes).unwrap();
    }

    /// What a worker's launcher does at a start: the current file becomes
    /// the rotated one.
    fn rotate(&self) {
        fs::rename(&self.pair.current, &self.pair.rotated).unwrap();
    }

    fn read(&self, cursor: Option<&str>, before: Option<&str>, limit: Option<i64>) -> Answer {
        Answer::of(tail::read(&self.pair, "machine.log", cursor, before, limit).unwrap())
    }

    fn refused(&self, cursor: Option<&str>, before: Option<&str>, limit: Option<i64>) -> String {
        let refusal = tail::read(&self.pair, "machine.log", cursor, before, limit).unwrap_err();
        format!("{}: {}", refusal.code, refusal.message)
    }

    fn tail(&self) -> Answer {
        self.read(None, None, None)
    }

    fn after(&self, cursor: &str) -> Answer {
        self.read(Some(cursor), None, None)
    }
}

impl Drop for Log {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// One answer, its fields checked to be exactly the call's.
#[derive(Debug)]
struct Answer {
    /// Each line's text and whether it is `continued`.
    lines: Vec<(String, bool)>,
    cursor: String,
    before: Option<String>,
    has_more: bool,
}

impl Answer {
    fn of(value: SdkValue) -> Self {
        let SdkValue::Map(fields) = &value else {
            panic!("{value:?}")
        };
        let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
        let before = value.get_text("before").map(str::to_owned);
        match before {
            Some(_) => assert_eq!(names, ["lines", "cursor", "before", "has_more"]),
            None => assert_eq!(names, ["lines", "cursor", "has_more"]),
        }
        let Some(SdkValue::List(lines)) = value.get("lines") else {
            panic!("{value:?}")
        };
        let lines = lines
            .iter()
            .map(|line| {
                let SdkValue::Map(fields) = line else {
                    panic!("{line:?}")
                };
                let continued = match fields.len() {
                    1 => false,
                    2 => {
                        assert_eq!(line.get("continued"), Some(&SdkValue::Bool(true)));
                        true
                    }
                    _ => panic!("{line:?}"),
                };
                (line.get_text("text").unwrap().to_owned(), continued)
            })
            .collect();
        let Some(SdkValue::Bool(has_more)) = value.get("has_more") else {
            panic!("{value:?}")
        };
        Self {
            lines,
            cursor: value.get_text("cursor").unwrap().to_owned(),
            before,
            has_more: *has_more,
        }
    }

    /// The whole lines of the answer, none of them a piece.
    fn texts(&self) -> Vec<&str> {
        self.lines
            .iter()
            .map(|(text, continued)| {
                assert!(!continued, "{text} is a piece");
                text.as_str()
            })
            .collect()
    }
}

/// Lines `from..to`, each `line <n>`, with their newlines.
fn numbered(from: usize, to: usize) -> String {
    let mut text = String::new();
    for n in from..to {
        text.push_str("line ");
        text.push_str(&n.to_string());
        text.push('\n');
    }
    text
}

fn names(from: usize, to: usize) -> Vec<String> {
    (from..to).map(|n| format!("line {n}")).collect()
}

/// The lines of `answers` (given oldest answer first) with each line's
/// pieces put together.
fn joined(answers: &[&Answer]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut open = false;
    for (text, continued) in answers.iter().flat_map(|answer| answer.lines.iter()) {
        match open {
            true => lines.last_mut().unwrap().push_str(text),
            false => lines.push(text.clone()),
        }
        open = *continued;
    }
    assert!(!open, "the last piece is not a line's end");
    lines
}

#[test]
fn a_read_with_no_cursor_is_the_last_lines_oldest_first() {
    let log = Log::new("tail");
    log.append(&numbered(0, 500));
    let tail = log.tail();
    assert_eq!(DEFAULT_LINES, 200);
    assert_eq!(tail.texts(), names(300, 500));
    assert!(tail.has_more, "earlier lines remain");
    assert!(tail.before.is_some());
    // a limit takes fewer, the newest ones
    let few = log.read(None, None, Some(3));
    assert_eq!(few.texts(), names(497, 500));
    assert_eq!(few.cursor, tail.cursor, "both end at the file's end");
    // a file smaller than the limit is answered whole, with nothing before
    let small = Log::new("tail-small");
    small.append(&numbered(0, 4));
    let all = small.tail();
    assert_eq!(all.texts(), names(0, 4));
    assert!(!all.has_more);
    assert_eq!(all.before, None);
    // blank lines are lines
    let blank = Log::new("tail-blank");
    blank.append("a\n\nb\n");
    assert_eq!(blank.tail().texts(), ["a", "", "b"]);
}

#[test]
fn a_cursor_reads_the_lines_written_after_it() {
    let log = Log::new("forward");
    log.append(&numbered(0, 10));
    let tail = log.tail();
    // nothing new: no lines, the same cursor
    let idle = log.after(&tail.cursor);
    assert!(idle.lines.is_empty());
    assert_eq!(idle.cursor, tail.cursor);
    assert!(!idle.has_more);
    assert_eq!(
        idle.before, None,
        "a read from a cursor answers no `before`"
    );
    log.append(&numbered(10, 15));
    let next = log.after(&tail.cursor);
    assert_eq!(next.texts(), names(10, 15));
    assert!(!next.has_more);
    assert!(log.after(&next.cursor).lines.is_empty());
    // a limit stops the answer early and says more is there
    log.append(&numbered(15, 20));
    let two = log.read(Some(&next.cursor), None, Some(2));
    assert_eq!(two.texts(), names(15, 17));
    assert!(two.has_more);
    let rest = log.after(&two.cursor);
    assert_eq!(rest.texts(), names(17, 20));
    assert!(!rest.has_more);
}

#[test]
fn before_pages_back_through_the_file_and_into_its_rotated_one() {
    let log = Log::new("before");
    log.append(&numbered(0, 300));
    log.rotate();
    log.append(&numbered(300, 700));
    let tail = log.tail();
    assert_eq!(tail.texts(), names(500, 700));
    let mut pages = vec![tail];
    // each page ends where the one after it starts, back to the first line
    while let Some(before) = pages.last().unwrap().before.clone() {
        assert!(pages.last().unwrap().has_more);
        let page = log.read(None, Some(&before), None);
        assert_eq!(
            page.cursor, before,
            "a page ends at the cursor it read before"
        );
        pages.push(page);
        assert!(pages.len() < 20, "paging back does not end");
    }
    assert!(!pages.last().unwrap().has_more);
    pages.reverse();
    let all: Vec<&Answer> = pages.iter().collect();
    assert_eq!(joined(&all), names(0, 700));
    // the page that crossed into the rotated file holds only its lines
    assert!(pages.iter().any(|page| page.texts() == names(100, 300)));
    // the end of a page in the rotated file reads on, into the current file
    let crossed = pages.iter().find(|p| p.texts() == names(100, 300)).unwrap();
    let on = log.after(&crossed.cursor);
    assert_eq!(on.texts(), names(300, 500));
    assert!(on.has_more);
}

#[test]
fn a_log_with_no_rotated_file_ends_at_its_first_line() {
    let log = Log::new("no-rotated");
    log.append(&numbered(0, 250));
    let tail = log.tail();
    assert_eq!(tail.texts(), names(50, 250));
    let first = log.read(None, tail.before.as_deref(), None);
    assert_eq!(first.texts(), names(0, 50));
    assert_eq!(first.before, None, "nothing is before the first line");
    assert!(!first.has_more);
    assert!(!log.pair.rotated.exists());
}

#[test]
fn a_log_that_is_not_there_is_empty_and_is_followed_once_it_is() {
    let log = Log::new("absent");
    let none = log.tail();
    assert!(none.lines.is_empty());
    assert!(!none.has_more);
    assert_eq!(none.before, None);
    // nothing was made by reading
    assert!(fs::read_dir(&log.dir).unwrap().next().is_none());
    assert!(log.after(&none.cursor).lines.is_empty());
    log.append(&numbered(0, 3));
    assert_eq!(log.after(&none.cursor).texts(), names(0, 3));
    // an empty current file beside a rotated one: the rotated file's tail,
    // and the cursor follows the current file
    let rotated = Log::new("absent-current");
    rotated.append(&numbered(0, 5));
    rotated.rotate();
    let old = rotated.tail();
    assert_eq!(old.texts(), names(0, 5));
    assert!(!old.has_more);
    rotated.append(&numbered(5, 7));
    assert_eq!(rotated.after(&old.cursor).texts(), names(5, 7));
}

#[test]
fn a_line_longer_than_a_window_comes_in_pieces_that_join_exactly() {
    // 200 KiB of two-, three- and four-byte characters, so windows end
    // inside characters unless the reader moves them
    let unit = "aé€😀";
    let long: String = unit.repeat((200 << 10) / unit.len());
    assert!(long.len() as u64 > 3 * CHUNK);
    let log = Log::new("long");
    log.append("first\n");
    let start = log.tail();
    log.append(&format!("{long}\nlast\n"));
    // forward, one window per call
    let mut pages = Vec::new();
    let mut cursor = start.cursor.clone();
    loop {
        let page = log.after(&cursor);
        cursor = page.cursor.clone();
        let more = page.has_more;
        pages.push(page);
        if !more {
            break;
        }
        assert!(pages.len() < 20, "following does not end");
    }
    let all: Vec<&Answer> = pages.iter().collect();
    assert_eq!(joined(&all), [long.as_str(), "last"]);
    let pieces: Vec<&(String, bool)> = pages.iter().flat_map(|page| page.lines.iter()).collect();
    assert!(pieces.len() >= 5, "{} pieces", pieces.len());
    // every piece but a line's last is `continued`, and each is a window
    assert!(pieces[..pieces.len() - 2].iter().all(|(_, more)| *more));
    assert!(!pieces[pieces.len() - 2].1 && !pieces[pieces.len() - 1].1);
    assert!(pieces.iter().all(|(text, _)| text.len() as u64 <= CHUNK));
    assert!(pieces.iter().all(|(text, _)| !text.contains('\u{fffd}')));
    // backward from the tail, the same lines
    let mut back = vec![log.tail()];
    while let Some(before) = back.last().unwrap().before.clone() {
        back.push(log.read(None, Some(&before), None));
        assert!(back.len() < 20, "paging back does not end");
    }
    back.reverse();
    let all: Vec<&Answer> = back.iter().collect();
    assert_eq!(joined(&all), ["first", long.as_str(), "last"]);
}

#[test]
fn an_unfinished_last_line_is_held_back_until_its_newline() {
    let log = Log::new("partial");
    log.append("a\nb");
    let tail = log.tail();
    assert_eq!(tail.texts(), ["a"]);
    assert!(log.after(&tail.cursor).lines.is_empty());
    log.append("c\n");
    assert_eq!(log.after(&tail.cursor).texts(), ["bc"]);
    // a first line still being written does not make its cursor stale
    let first = Log::new("partial-first");
    first.append("par");
    let none = first.tail();
    assert!(none.lines.is_empty());
    first.append("tial\nnext\n");
    assert_eq!(first.after(&none.cursor).texts(), ["partial", "next"]);
    // the rotated file is finished: its last line is whole as it stands
    let ended = Log::new("partial-rotated");
    ended.append("one\ncut off");
    ended.rotate();
    assert_eq!(ended.tail().texts(), ["one", "cut off"]);
}

#[test]
fn control_characters_are_escaped_and_bad_bytes_replaced() {
    let log = Log::new("escape");
    log.append_bytes(b"\x1b[31mred\x07\ttab\r\n\xff\xfe ok \xc3\xa9\n\x00nul\n");
    assert_eq!(
        log.tail().texts(),
        [
            "\\u{1b}[31mred\\u{7}\\ttab\\r",
            "\u{fffd}\u{fffd} ok é",
            "\\u{0}nul"
        ]
    );
}

#[test]
fn a_cursor_into_a_rotated_or_replaced_file_is_refused() {
    let stale = "InvalidInput: machine.log: the cursor is stale";
    // a rotation: the cursor's file is now the rotated one
    let log = Log::new("stale-rotated");
    log.append(&numbered(0, 300));
    let tail = log.tail();
    let before = tail.before.clone().unwrap();
    log.rotate();
    log.append(&numbered(300, 303));
    assert!(log
        .refused(Some(&tail.cursor), None, None)
        .starts_with(stale));
    assert!(log.refused(None, Some(&before), None).starts_with(stale));
    // a fresh read with no cursor follows the new file
    let fresh = log.tail();
    assert_eq!(fresh.texts(), names(300, 303));
    log.append(&numbered(303, 305));
    assert_eq!(log.after(&fresh.cursor).texts(), names(303, 305));
    // rotated again: a cursor into the first rotated file is stale too
    let old = log.read(None, fresh.before.as_deref(), None);
    assert_eq!(old.texts(), names(100, 300));
    let into_rotated = old.before.clone().unwrap();
    log.rotate();
    log.append("new\n");
    assert!(log
        .refused(None, Some(&into_rotated), None)
        .starts_with(stale));

    // a file removed and made again: another inode
    let log = Log::new("stale-replaced");
    log.append(&numbered(0, 5));
    let tail = log.tail();
    fs::rename(&log.pair.current, log.dir.join("kept")).unwrap();
    log.append(&numbered(0, 5));
    assert!(log
        .refused(Some(&tail.cursor), None, None)
        .starts_with(stale));
    // removed and not made again
    fs::remove_file(&log.pair.current).unwrap();
    assert!(log
        .refused(Some(&tail.cursor), None, None)
        .starts_with(stale));

    // the same file with another first line
    let log = Log::new("stale-first-line");
    log.append(&numbered(0, 5));
    let tail = log.tail();
    fs::write(&log.pair.current, format!("other\n{}", numbered(0, 9))).unwrap();
    assert!(log
        .refused(Some(&tail.cursor), None, None)
        .starts_with(stale));

    // the same file and first line, now smaller than the cursor's offset
    let log = Log::new("stale-smaller");
    log.append(&numbered(0, 50));
    let tail = log.tail();
    let file = OpenOptions::new()
        .write(true)
        .open(&log.pair.current)
        .unwrap();
    file.set_len(numbered(0, 2).len() as u64).unwrap();
    assert!(log
        .refused(Some(&tail.cursor), None, None)
        .starts_with(stale));
    assert_eq!(log.tail().texts(), names(0, 2));

    // text that is no cursor
    for text in [
        "",
        "c1",
        "c1.2.0.0.0",
        "c1.0.zz.0.0",
        "x1.0.1.1.1",
        "c1.0.1.1.1.1",
    ] {
        assert!(
            log.refused(Some(text), None, None).starts_with(stale),
            "{text}"
        );
        assert!(
            log.refused(None, Some(text), None).starts_with(stale),
            "{text}"
        );
    }
}

#[test]
fn a_window_is_checked_before_any_read() {
    let log = Log::new("limits");
    log.append(&numbered(0, 1500));
    assert_eq!(MAX_LINES, 1000);
    for limit in [0, -1, 1001, i64::MAX] {
        assert_eq!(
            log.refused(None, None, Some(limit)),
            "InvalidInput: machine.log: `limit` is 1 to 1000"
        );
    }
    let tail = log.tail();
    assert_eq!(
        log.refused(Some(&tail.cursor), tail.before.as_deref(), None),
        "InvalidInput: machine.log: give `cursor` or `before`, not both"
    );
    assert_eq!(log.read(None, None, Some(1)).texts(), names(1499, 1500));
    // 1000 short lines fit one window; a window bounds longer ones
    assert_eq!(log.read(None, None, Some(1000)).texts(), names(500, 1500));
    let wide = Log::new("limits-wide");
    let line = "x".repeat(999);
    for _ in 0..200 {
        wide.append(&format!("{line}\n"));
    }
    let window = wide.read(None, None, Some(1000));
    // whole lines only: the one the window's start cuts is on the page before
    assert_eq!(window.lines.len(), 65);
    assert!(window.texts().iter().all(|text| *text == line));
    assert!(window.has_more);
    let earlier = wide.read(None, window.before.as_deref(), Some(1000));
    assert_eq!(earlier.lines.len(), 65);
    assert!(earlier.texts().iter().all(|text| *text == line));
}

#[test]
fn a_link_in_the_log_s_place_is_not_followed() {
    let log = Log::new("link");
    let secret = log.dir.join("secret");
    fs::write(&secret, "key\n").unwrap();
    std::os::unix::fs::symlink(&secret, &log.pair.current).unwrap();
    let refusal = tail::read(&log.pair, "sandbox.log", None, None, None).unwrap_err();
    assert_eq!(refusal.code, "Failed", "{refusal:?}");
    assert!(!refusal.message.contains("key"), "{refusal:?}");
}
