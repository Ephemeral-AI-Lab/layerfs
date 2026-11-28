//! Paging a Commit difference resumes where the last page stopped (#352
//! F-010): a later page does not walk the pairs earlier pages listed, and a
//! listing without line hunks fits its page by paths, not by file contents.
use crate::support;

use layerfs_protocol::sdk::SdkValue;
use std::fs;
use support::{items, paths, write, Fixture};

const DIRS: usize = 12;
const FILES: usize = 10;

fn body(d: usize, f: usize, round: usize) -> String {
    use std::fmt::Write;
    let mut text = String::new();
    for line in 0..60 {
        writeln!(text, "d{d} f{f} line {line} round {round}").unwrap();
    }
    text
}

fn fixture(label: &str) -> (Fixture, String, String) {
    let fixture = Fixture::new(label, |source| {
        for d in 0..DIRS {
            let dir = source.join(format!("d{d:02}"));
            fs::create_dir(&dir).unwrap();
            for f in 0..FILES {
                fs::write(dir.join(format!("f{f:02}.txt")), body(d, f, 0)).unwrap();
            }
        }
        DIRS * (FILES + 1)
    });
    fixture.fork("work");
    let first = fixture.apply("work", vec![write("/seed.txt", "seed\n")]);
    let changes = (0..DIRS)
        .flat_map(|d| (0..FILES).map(move |f| (d, f)))
        .map(|(d, f)| write(&format!("/d{d:02}/f{f:02}.txt"), body(d, f, 1)))
        .collect();
    let second = fixture.apply("work", changes);
    let head = |v: &SdkValue| v.get_text("head").unwrap().to_owned();
    (fixture, head(&first), head(&second))
}

fn page(fixture: &Fixture, from: &str, to: &str, cursor: &SdkValue, hunks: bool) -> SdkValue {
    super::commit_diff(fixture, from, to, 1000, cursor, hunks)
}

#[test]
fn cursor_pages_resume_and_list_every_change_once() {
    let (fixture, from, to) = fixture("sdk-diff-pages-cursor");
    let mut cursor = SdkValue::Null;
    let mut listed = Vec::new();
    let mut work = Vec::new();
    loop {
        let before = fixture.work();
        let value = page(&fixture, &from, &to, &cursor, true);
        work.push(fixture.since(before).statements);
        listed.extend(paths(&value).into_iter().map(str::to_owned));
        if !super::more(&value) {
            break;
        }
        cursor = value.get("next").cloned().expect("next");
        assert!(matches!(cursor, SdkValue::Text(_)), "{cursor:?}");
    }
    let expected: Vec<String> = (0..DIRS)
        .flat_map(|d| (0..FILES).map(move |f| format!("/d{d:02}/f{f:02}.txt")))
        .collect();
    assert_eq!(listed, expected, "every change once, in path order");
    assert!(work.len() >= 3, "the hunks fill several pages: {work:?}");
    // A resumed page pays for its own items, not for the pages before it.
    let (first, last) = (work[0], *work.last().unwrap());
    println!("SDK_DIFF_PAGES pages={} statements={work:?}", work.len());
    assert!(
        last <= first + first / 4,
        "the last page re-walks earlier pages: {work:?}"
    );
    fixture.cleanup();
}

#[test]
fn a_listing_without_hunks_fits_by_paths() {
    let (fixture, from, to) = fixture("sdk-diff-pages-paths");
    let value = page(&fixture, &from, &to, &SdkValue::Null, false);
    assert_eq!(
        items(&value).len(),
        DIRS * FILES,
        "one page lists every path"
    );
    assert!(!super::more(&value));
    let item = &items(&value)[0];
    assert!(item.get_bool("content"));
    assert_eq!(item.get("hunks"), Some(&SdkValue::Null));
    assert!(!item.get_bool("hunksOmitted"));
    fixture.cleanup();
}

/// Paths only, so a page is cheap and passing over earlier pages would show:
/// `WIDE_DIRS * WIDE_FILES` changes fill about six pages of about 240 paths.
const WIDE_DIRS: usize = 60;
const WIDE_FILES: usize = 24;

#[test]
fn cursor_pages_without_hunks_pay_for_their_own_entries() {
    let fixture = Fixture::new("sdk-diff-pages-wide", |source| {
        for d in 0..WIDE_DIRS {
            let dir = source.join(format!("d{d:02}"));
            fs::create_dir(&dir).unwrap();
            for f in 0..WIDE_FILES {
                fs::write(dir.join(format!("f{f:02}.txt")), format!("{d} {f}\n")).unwrap();
            }
        }
        WIDE_DIRS * (WIDE_FILES + 1)
    });
    fixture.fork("work");
    let first = fixture.apply("work", vec![write("/seed.txt", "seed\n")]);
    let changes = (0..WIDE_DIRS)
        .flat_map(|d| (0..WIDE_FILES).map(move |f| (d, f)))
        .map(|(d, f)| write(&format!("/d{d:02}/f{f:02}.txt"), format!("{d} {f} new\n")))
        .collect();
    let second = fixture.apply("work", changes);
    let (from, to) = (
        first.get_text("head").unwrap().to_owned(),
        second.get_text("head").unwrap().to_owned(),
    );
    let mut cursor = SdkValue::Null;
    let (mut listed, mut work) = (0, Vec::new());
    loop {
        let before = fixture.work();
        let value = page(&fixture, &from, &to, &cursor, false);
        work.push(fixture.since(before).statements);
        listed += items(&value).len();
        if !super::more(&value) {
            break;
        }
        cursor = value.get("next").cloned().expect("next");
    }
    println!(
        "SDK_DIFF_PAGES_WIDE pages={} statements={work:?}",
        work.len()
    );
    assert_eq!(listed, WIDE_DIRS * WIDE_FILES, "every change once");
    assert!(work.len() >= 5, "the paths fill several pages: {work:?}");
    // The first page also plans the difference; every later page reuses the
    // plan and pays for its own entries only, so none costs more than the
    // second page by more than a quarter, however many pages came before.
    let second = work[1];
    for (index, later) in work.iter().enumerate().skip(2) {
        assert!(
            *later <= second + second / 4,
            "page {index} walks the pages before it: {work:?}"
        );
    }
    fixture.cleanup();
}
