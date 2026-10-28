//! A Commit difference costs its own changed entries (#352 F-012, SPEC §4):
//! the same changes cost the same in a 300-file and a 30,000-file tree, and
//! a paged difference plans its changes once, so a later page costs what its
//! own entries cost and not the whole difference again.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{items, operation, write, Fixture, Work};

const CHANGED_DIRS: usize = 5;
const CHANGED_FILES: usize = 20;

fn head(reply: &SdkValue) -> String {
    reply.get_text("head").unwrap().to_owned()
}

/// `/src/a/c0..c4` with 20 files each (the changed paths), beside `bulk`
/// unchanged sibling directories of 30 files each.
fn tree(source: &std::path::Path, bulk: usize) -> usize {
    let a = source.join("src/a");
    fs::create_dir_all(&a).unwrap();
    for c in 0..CHANGED_DIRS {
        let dir = a.join(format!("c{c}"));
        fs::create_dir(&dir).unwrap();
        for f in 0..CHANGED_FILES {
            fs::write(dir.join(format!("f{f:02}.txt")), format!("c{c} f{f} 0\n")).unwrap();
        }
    }
    for b in 0..bulk {
        let dir = a.join(format!("u{b:04}"));
        fs::create_dir(&dir).unwrap();
        for f in 0..30 {
            fs::write(dir.join(format!("f{f:02}.txt")), format!("u{b} f{f}\n")).unwrap();
        }
    }
    2 + CHANGED_DIRS * (CHANGED_FILES + 1) + bulk * 31
}

/// Every changed file rewritten, one added and one removed.
fn changes() -> Vec<SdkValue> {
    let mut out: Vec<SdkValue> = (0..CHANGED_DIRS)
        .flat_map(|c| (0..CHANGED_FILES).map(move |f| (c, f)))
        .map(|(c, f)| {
            write(
                &format!("/src/a/c{c}/f{f:02}.txt"),
                format!("c{c} f{f} 1\n"),
            )
        })
        .collect();
    out.push(write("/src/a/c0/added.txt", "added\n"));
    out.push(operation("rm", "/src/a/c1/f00.txt"));
    out
}

fn diff(fixture: &Fixture, from: &str, to: &str, size: i64, cursor: &SdkValue) -> SdkValue {
    fixture.call(
        "branch.diff_commits",
        vec![
            ("from", SdkValue::text(from)),
            ("to", SdkValue::text(to)),
            ("pageSize", SdkValue::Int(size)),
            ("cursor", cursor.clone()),
            ("hunks", SdkValue::Bool(false)),
        ],
    )
}

#[test]
fn the_same_changes_cost_the_same_in_a_small_and_a_large_tree() {
    let mut observed: Vec<(usize, Work, usize)> = Vec::new();
    for bulk in [10usize, 1000] {
        let fixture = Fixture::new(&format!("sdk-diff-entries-{bulk}"), |s| tree(s, bulk));
        fixture.fork("work");
        let from = head(&fixture.apply("work", vec![write("/seed.txt", "seed\n")]));
        let to = head(&fixture.apply("work", changes()));
        let before = fixture.work();
        let value = diff(&fixture, &from, &to, 1000, &SdkValue::Null);
        let work = fixture.since(before);
        assert!(!value.get_bool("hasMore"));
        let listed = items(&value).len();
        println!(
            "SDK_DIFF_ENTRIES files={} listed={listed} diff={work:?}",
            bulk * 30 + 100
        );
        observed.push((bulk, work, listed));
        fixture.cleanup();
    }
    let ((_, small, a), (_, large, b)) = (observed[0], observed[1]);
    assert_eq!(a, b, "the same changes list the same entries");
    // A B-tree descent may be one page deeper in the large tree's
    // directories and inode table; nothing else may differ.
    let near = |x: u64, y: u64, slack: u64| x.abs_diff(y) <= slack;
    assert!(
        near(small.object_ids, large.object_ids, 32)
            && near(small.statements, large.statements, 128)
            && near(small.rows, large.rows, 512),
        "the difference reads by tree size: {observed:?}"
    );
}

/// Pages of 20 through a difference of `files` changed files; each page's
/// statements in order.
fn paged(files: usize) -> Vec<u64> {
    let dirs = files / 100;
    let fixture = Fixture::new(&format!("sdk-diff-entries-pages-{files}"), |source| {
        for d in 0..dirs {
            let dir = source.join(format!("p/d{d:02}"));
            fs::create_dir_all(&dir).unwrap();
            for f in 0..100 {
                fs::write(dir.join(format!("f{f:02}.txt")), format!("{d} {f} 0\n")).unwrap();
            }
        }
        1 + dirs * 101
    });
    fixture.fork("work");
    let from = head(&fixture.apply("work", vec![write("/seed.txt", "seed\n")]));
    let all: Vec<SdkValue> = (0..dirs)
        .flat_map(|d| (0..100).map(move |f| (d, f)))
        .map(|(d, f)| write(&format!("/p/d{d:02}/f{f:02}.txt"), format!("{d} {f} 1\n")))
        .collect();
    // Several Commits, each within one control record.
    let mut to = from.clone();
    for batch in all.chunks(400) {
        to = head(&fixture.apply("work", batch.to_vec()));
    }
    let (mut cursor, mut pages, mut listed) = (SdkValue::Null, Vec::new(), 0);
    loop {
        let before = fixture.work();
        let value = diff(&fixture, &from, &to, 20, &cursor);
        pages.push(fixture.since(before).statements);
        listed += items(&value).len();
        if !value.get_bool("hasMore") {
            break;
        }
        cursor = value.get("nextCursor").cloned().unwrap();
    }
    assert_eq!(listed, files, "every change once");
    fixture.cleanup();
    pages
}

#[test]
fn a_paged_difference_plans_once() {
    let mut later = Vec::new();
    for files in [100usize, 2000] {
        let pages = paged(files);
        let rest = &pages[1..];
        let mean = |s: &[u64]| s.iter().sum::<u64>() / s.len().max(1) as u64;
        let quarter = rest.len() / 4;
        println!(
            "SDK_DIFF_ENTRIES_PAGES files={files} pages={} first={} later_mean={} later_max={} total={}",
            pages.len(),
            pages[0],
            mean(rest),
            rest.iter().max().unwrap(),
            pages.iter().sum::<u64>()
        );
        // A later page's work does not grow with its index.
        assert!(
            mean(&rest[rest.len() - quarter..]) <= mean(&rest[..quarter]) * 5 / 4 + 64,
            "{files} files: later pages cost more: {pages:?}"
        );
        later.push(mean(rest));
    }
    // A later page costs its own 20 entries, whatever the difference's size.
    assert!(
        later[1] <= later[0] * 3 / 2 + 64,
        "a later page re-plans the whole difference: {later:?}"
    );
}
