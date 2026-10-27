//! A mount finds its Branch by name without listing the project's Branches
//! (#352 finding 009.a1). `mount_branch` resolved the name by paging through
//! `project.list_branches`, so a one-shot mount's VM steps grew with every
//! Branch the replay made (about 7 per range). `project.list_branches` with
//! `name` answers from the name index: the same work at 10 and 1,000 Branches.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{items, Fixture};

fn named(fixture: &Fixture, name: &str) -> SdkValue {
    fixture.call(
        "project.list_branches",
        vec![
            ("name", SdkValue::text(name)),
            ("pageSize", SdkValue::Int(1000)),
            ("pagePos", SdkValue::Int(0)),
        ],
    )
}

#[test]
fn a_branch_is_listed_by_name_with_the_same_work_at_any_branch_count() {
    let fixture = Fixture::new("sdk-mount-lookup", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    let mut made = 0;
    let mut observed = Vec::new();
    for count in [10usize, 1000] {
        while made < count {
            fixture.fork(&format!("lane-{made:05}"));
            made += 1;
        }
        // The same call first, so both counts measure the same warm state.
        named(&fixture, "lane-00005");
        let before = fixture.work();
        let value = named(&fixture, "lane-00005");
        let work = fixture.since(before);
        let listed = items(&value);
        assert_eq!(listed.len(), 1, "{value:?}");
        assert_eq!(listed[0].get_text("name"), Some("lane-00005"));
        assert!(listed[0].get_text("id").is_some());
        assert!(!value.get_bool("hasMore"));
        println!("SDK_MOUNT_LOOKUP branches={count} work={work:?}");
        observed.push(work);
    }
    let (few, many) = (observed[0], observed[1]);
    assert_eq!(
        (few.statements, few.rows, few.vm_steps),
        (many.statements, many.rows, many.vm_steps),
        "a Branch looked up by name reads the other Branches: {observed:?}"
    );
    // main and an absent name: one item and none, never a listing.
    let main = named(&fixture, "main");
    assert_eq!(items(&main).len(), 1);
    assert_eq!(items(&main)[0].get_text("name"), Some("main"));
    assert!(items(&named(&fixture, "no-such-lane")).is_empty());
    fixture.cleanup();
}
