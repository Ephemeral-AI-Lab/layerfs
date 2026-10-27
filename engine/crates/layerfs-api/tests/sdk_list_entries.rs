//! A directory listing costs its own entries (#352 finding 011.a1, SPEC §4):
//! the same 40-entry directory lists with the same work in a 400-file and a
//! 30,000-file tree. Each entry's kind is looked up in the tree's inode table;
//! without a memo every lookup re-read the table's upper pages, so the work
//! per entry grew with the tree.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{items, Fixture};

const ENTRIES: usize = 40;

/// `/listed` with 40 files, beside `bulk` directories of 30 files each.
fn tree(source: &std::path::Path, bulk: usize) -> usize {
    let listed = source.join("listed");
    fs::create_dir_all(&listed).unwrap();
    for f in 0..ENTRIES {
        fs::write(
            listed.join(format!("f{f:02}.snap")),
            format!("snapshot {f}\n"),
        )
        .unwrap();
    }
    for b in 0..bulk {
        let dir = source.join(format!("bulk/u{b:04}"));
        fs::create_dir_all(&dir).unwrap();
        for f in 0..30 {
            fs::write(dir.join(format!("f{f:02}.txt")), format!("u{b} f{f}\n")).unwrap();
        }
    }
    // every inode below the root: the listed directory and its files, `bulk/`
    // and its directories and files
    1 + ENTRIES + 1 + bulk * 31
}

#[test]
fn a_listing_costs_the_same_in_a_small_and_a_large_tree() {
    let mut observed = Vec::new();
    for bulk in [12usize, 1000] {
        let fixture = Fixture::new(&format!("sdk-list-entries-{bulk}"), |s| tree(s, bulk));
        let list = || {
            fixture.call(
                "fs.list",
                vec![
                    ("branch", SdkValue::text("main")),
                    ("path", SdkValue::text("/listed")),
                    ("pageSize", SdkValue::Int(1000)),
                    ("pagePos", SdkValue::Int(0)),
                ],
            )
        };
        let before = fixture.work();
        let value = list();
        let work = fixture.since(before);
        assert_eq!(items(&value).len(), ENTRIES);
        println!(
            "SDK_LIST_ENTRIES files={} work={work:?}",
            ENTRIES + bulk * 30
        );
        observed.push(work);
        fixture.cleanup();
    }
    let (small, large) = (observed[0], observed[1]);
    assert!(
        large.statements <= small.statements + 16 && large.object_ids <= small.object_ids + 8,
        "listing 40 entries costs more in the larger tree: {observed:?}"
    );
}
