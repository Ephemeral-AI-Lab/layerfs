//! `fs.blame` of a file past its line-by-line bound (2,000 lines or 48 KiB) compares two versions by
//! the bytes they share at both ends. A line of the newer version goes back to the older one only
//! where the older text starts a line at the same place: a line split in two, or one that lost its
//! first characters, is new text and keeps the Commit that wrote it.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::Fixture;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

/// `line a` to `line b`, one on each line.
fn lines(a: usize, b: usize) -> String {
    (a..=b).fold(String::new(), |mut text, n| {
        text.push_str(&format!("line {n}\n"));
        text
    })
}

/// Writes `before` as `/file.txt`, forks a Branch and rewrites the file on it through `steps`, one
/// Commit each: the new text and the lines of the last text that Commit wrote. Then checks `fs.blame`
/// of the last text, page by page: every line has its text and leads to the Commit that wrote it, or
/// to the base Layer when no step did.
fn check(label: &str, before: &str, steps: &[(&str, &[usize])], coarse: bool) {
    let f = Fixture::new(label, |source| {
        fs::write(source.join("file.txt"), before).unwrap();
        1
    });
    let fork = f.call("branch.fork", vec![("name", t("work"))]);
    let mut origins = vec![fork.get_text("base").unwrap().to_owned()];
    for (text, _) in steps {
        let reply = f.call(
            "fs.write",
            vec![
                ("branch", t("work")),
                ("path", t("/file.txt")),
                ("bytes", SdkValue::Bytes(text.as_bytes().to_vec())),
            ],
        );
        assert_eq!(reply.get_text("kind"), Some("Committed"), "{reply:?}");
        let commit = reply.get("commit").and_then(|c| c.get_text("id"));
        origins.push(commit.unwrap().to_owned());
    }
    let (mut blamed, mut compared_coarsely, mut next) = (Vec::new(), false, Some(1));
    for _ in 0..64 {
        let Some(from) = next else { break };
        let page = f.call(
            "fs.blame",
            vec![
                ("branch", t("work")),
                ("path", t("/file.txt")),
                ("fromLine", SdkValue::Int(from)),
            ],
        );
        compared_coarsely |= page.get_bool("comparisonCoarse");
        let Some(SdkValue::List(items)) = page.get("lines") else {
            panic!("{label}: no lines: {page:?}");
        };
        blamed.extend(items.iter().map(|line| {
            let text = line.get_text("text").unwrap().to_owned();
            (text, line.get_text("commit").unwrap().to_owned())
        }));
        next = page.get_int("nextLine");
    }
    assert_eq!(next, None, "{label}: more than 64 pages");
    assert_eq!(compared_coarsely, coarse, "{label}: comparisonCoarse");
    let text = steps.last().unwrap().0;
    let expected: Vec<&str> = text.split_inclusive('\n').collect();
    assert_eq!(blamed.len(), expected.len(), "{label}: blamed lines");
    let named = |id: &str| match origins.iter().position(|o| o == id) {
        Some(0) => "the base Layer".to_owned(),
        Some(step) => format!("Commit {step}"),
        None => "an unknown origin".to_owned(),
    };
    for (i, ((shown, origin), line)) in blamed.iter().zip(expected).enumerate() {
        let n = i + 1;
        let writer = steps.iter().position(|(_, written)| written.contains(&n));
        let want = &origins[writer.map_or(0, |step| step + 1)];
        assert_eq!(shown, line.trim_end_matches('\n'), "{label}: line {n}");
        assert_eq!(
            origin,
            want,
            "{label}: line {n} {shown:?} is credited to {}, but {} wrote it",
            named(origin),
            named(want)
        );
    }
    f.cleanup();
}

#[test]
fn a_split_line_keeps_the_commit_that_split_it() {
    let rest = lines(2, 2005);
    let before = format!("alpha beta\n{rest}");
    let after = format!("alpha\nbeta\n{rest}");
    check("split", &before, &[(&after, &[1, 2])], true);
}

#[test]
fn a_split_line_in_the_middle_keeps_the_commit_that_split_it() {
    let (head, tail) = (lines(1, 999), lines(1000, 2004));
    let before = format!("{head}one two\n{tail}");
    let after = format!("{head}one\ntwo\n{tail}");
    check("split-middle", &before, &[(&after, &[1000, 1001])], true);
}

#[test]
fn a_split_last_line_keeps_the_commit_that_split_it() {
    // The common suffix is the last line alone.
    let head = lines(1, 2004);
    let before = format!("{head}alpha beta\n");
    let after = format!("{head}alpha\nbeta\n");
    check("split-last", &before, &[(&after, &[2005, 2006])], true);
}

#[test]
fn a_split_last_line_without_a_final_newline_keeps_the_commit_that_split_it() {
    let head = lines(1, 2004);
    let before = format!("{head}alpha beta");
    let after = format!("{head}alpha\nbeta");
    check("split-bare", &before, &[(&after, &[2005, 2006])], true);
}

#[test]
fn a_line_that_lost_its_first_characters_keeps_the_commit_that_cut_it() {
    let rest = lines(2, 2005);
    let before = format!("xalpha\n{rest}");
    let after = format!("alpha\n{rest}");
    check("cut", &before, &[(&after, &[1])], true);
}

#[test]
fn a_split_line_is_not_credited_to_an_older_rewrite_of_the_line() {
    // Commit 1 wrote `alpha beta` and Commit 2 split it: no line of the result is Commit 1's.
    let rest = lines(2, 2005);
    let before = format!("alpha\n{rest}");
    let whole = format!("alpha beta\n{rest}");
    let split = format!("alpha\nbeta\n{rest}");
    check("twice", &before, &[(&whole, &[]), (&split, &[1, 2])], true);
}

#[test]
fn lines_behind_a_joined_line_keep_their_origin() {
    let rest = lines(2, 2005);
    let before = format!("alpha\nbeta\n{rest}");
    let after = format!("alpha beta\n{rest}");
    check("join", &before, &[(&after, &[1])], true);
}

#[test]
fn lines_behind_an_added_or_dropped_first_line_keep_their_origin() {
    // The common suffix is the whole older text, then the whole newer text.
    let rest = lines(2, 2005);
    let first = format!("first\n{rest}");
    check("add-first", &rest, &[(&first, &[1])], true);
    check("drop-first", &first, &[(&rest, &[])], true);
}

#[test]
fn a_file_without_a_final_newline_keeps_its_last_line_origin() {
    let bare = lines(2, 2005);
    let bare = bare.trim_end_matches('\n');
    let before = format!("alpha\n{bare}");
    let after = format!("alpha\nbeta\n{bare}");
    check("bare-add", &before, &[(&after, &[2])], true);
}

#[test]
fn an_added_final_newline_is_new_text() {
    // The last line shows the same, but the Commit wrote its newline.
    let bare = lines(2, 2005);
    let bare = bare.trim_end_matches('\n');
    let before = format!("alpha\n{bare}");
    let after = format!("alpha\n{bare}\n");
    check("bare-newline", &before, &[(&after, &[2005])], true);
}

#[test]
fn a_last_line_cut_short_without_a_final_newline_keeps_the_commit_that_cut_it() {
    // The newer text is all prefix, but its last line ends where the older line goes on.
    let head = lines(1, 2004);
    let before = format!("{head}alpha beta\n");
    let after = format!("{head}alpha");
    check("cut-last", &before, &[(&after, &[2005])], true);
    let before = format!("{head}alpha beta");
    check("cut-last-bare", &before, &[(&after, &[2005])], true);
}

#[test]
fn a_dropped_final_newline_is_new_text() {
    let head = lines(1, 2004);
    let before = format!("{head}alpha\n");
    let after = format!("{head}alpha");
    check("drop-newline", &before, &[(&after, &[2005])], true);
}

#[test]
fn lines_before_dropped_last_lines_keep_their_origin() {
    // The newer text is all prefix and ends with a newline, so every line is the older one.
    let head = lines(1, 2005);
    let before = format!("{head}alpha\nbeta");
    check("drop-last", &before, &[(&head, &[])], true);
}

#[test]
fn a_split_line_in_a_small_file_is_credited_to_the_commit_that_split_it() {
    let rest = lines(2, 10);
    let before = format!("alpha beta\n{rest}");
    let after = format!("alpha\nbeta\n{rest}");
    check("small", &before, &[(&after, &[1, 2])], false);
}
