//! A merge names its changed paths with work that follows the change (#352
//! F-025 fix 1): merging one file changed at depth 6 costs the same in a tree
//! with 1x, 10x and 100x the directories. The search that named changed inodes
//! walked the tree breadth first from the root, listing every directory
//! shallower than the deepest change, so its cost grew with the tree (225,
//! 852 and 7,584 statements at 20, 200 and 2,000 directories).
//!
//! Then the changed inodes were named from recorded bindings, climbed to the
//! root: no directory listed for naming. Merged by serial (#355 M1), these
//! one-sided changes name no path at all. Each climb step still looks a
//! serial and a name up in B-trees that gain a level as the tree grows by
//! about 10x, so the statements may grow by a fixed step per 10x
//! (logarithmically), never with the directory count.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{write, Fixture, Work};

/// The changed file: depth 6.
const DEEP: &str = "/d1/d2/d3/d4/d5/f.txt";

/// `DEEP`, beside `bulk` directories of 3 files each one level below the root.
fn tree(source: &std::path::Path, bulk: usize) -> usize {
    let deep = source.join("d1/d2/d3/d4/d5");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("f.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(source.join("other.txt"), "other\n").unwrap();
    for b in 0..bulk {
        let dir = source.join(format!("bulk/u{b:05}"));
        fs::create_dir_all(&dir).unwrap();
        for f in 0..3 {
            fs::write(dir.join(format!("f{f}.txt")), format!("u{b} f{f}\n")).unwrap();
        }
    }
    // d1..d5 and f.txt, other.txt, bulk/ and its directories and files
    6 + 1 + 1 + bulk * 4
}

fn merge_work(bulk: usize) -> Work {
    let fixture = Fixture::new(&format!("sdk-merge-paths-{bulk}"), |s| tree(s, bulk));
    fixture.fork("left");
    fixture.fork("right");
    fixture.apply("left", vec![write(DEEP, "one\ntwo\nthree\nfour\n")]);
    fixture.apply("right", vec![write("/other.txt", "other, changed\n")]);
    let before = fixture.work();
    let counted = fixture.service.store.save_counters().merge;
    let merged = fixture.call(
        "branch.merge",
        vec![
            ("branch", SdkValue::text("left")),
            ("with", SdkValue::text("right")),
            ("name", SdkValue::text("merged")),
        ],
    );
    let work = fixture.since(before);
    assert_eq!(merged.get_text("kind"), Some("Merged"), "{merged:?}");
    // The merge counters (#352): one merge. Merged by serial (#355 M1),
    // changes only one side made name no path at all, whatever the tree's
    // size; no breadth-first walk runs.
    let after = fixture.service.store.save_counters().merge;
    assert_eq!(after.merges - counted.merges, 1);
    assert_eq!(after.fallbacks - counted.fallbacks, 0);
    assert_eq!(after.naming.inodes - counted.naming.inodes, 0, "{after:?}");
    println!("SDK_MERGE_PATHS bulk={bulk} work={work:?}");
    fixture.cleanup();
    work
}

#[test]
fn a_merge_names_its_paths_without_listing_the_tree() {
    let observed: Vec<(usize, Work)> = [20usize, 200, 2000]
        .into_iter()
        .map(|bulk| (bulk, merge_work(bulk)))
        .collect();
    // One B-tree level per climb lookup per 10x: at most 24 statements per
    // step, and 100x within 125% of 1x. The breadth-first walk took 3.8x and
    // 34x.
    for pair in observed.windows(2) {
        let (a, b) = (pair[0].1.statements, pair[1].1.statements);
        assert!(
            b <= a + 24,
            "10x the directories added {} statements to the merge: {observed:?}",
            b.saturating_sub(a)
        );
    }
    let (small, large) = (observed[0].1.statements, observed[2].1.statements);
    assert!(
        large * 4 <= small * 5,
        "the merge with 100x the directories costs more than 125% of 1x: {observed:?}"
    );
}
