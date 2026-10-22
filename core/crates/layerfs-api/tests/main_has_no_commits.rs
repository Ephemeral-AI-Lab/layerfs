//! `branch.list_commits` on `main`: the stack's line of Layers holds no
//! Commits, so it answers one empty page; the page arguments are still read.
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

#[test]
fn main_answers_one_empty_commit_page() {
    let fixture = Fixture::new("main-has-no-commits", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    fixture.fork("feat");
    fixture.call(
        "fs.write",
        vec![
            ("branch", t("feat")),
            ("path", t("/b.txt")),
            ("content", t("b\n")),
            ("message", t("feat")),
        ],
    );

    let page = fixture.call("branch.list_commits", vec![("branch", t("main"))]);
    assert!(support::items(&page).is_empty());
    assert!(!page.get_bool("hasMore"));
    assert!(matches!(page.get("nextPagePos"), Some(SdkValue::Null)));

    // A Branch with a Commit still lists it.
    let feat = fixture.call("branch.list_commits", vec![("branch", t("feat"))]);
    assert_eq!(support::items(&feat).len(), 1);

    // `main` still reads the page arguments before it answers.
    let refusal = fixture
        .try_call(
            "branch.list_commits",
            vec![("branch", t("main")), ("pageSize", SdkValue::Int(0))],
        )
        .expect_err("pageSize 0 is refused");
    assert_eq!(refusal.code, "InvalidInput");
    assert_eq!(refusal.message, "pageSize must be in 1..=1000");

    fixture.cleanup();
}
