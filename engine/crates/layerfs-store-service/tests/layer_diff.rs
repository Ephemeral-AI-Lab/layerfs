//! Saved Layer/Commit comparisons retain the existing changed-entry plan.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;
use layerfs_bridge::control::SdkValue;
use support::{items, write, Fixture};
fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn pin(f: &Fixture, branch: &str) -> String {
    f.call("project.stats", vec![("branch", t(branch))])
        .get("at")
        .unwrap()
        .get_text("id")
        .unwrap()
        .to_owned()
}
#[test]
fn a_single_change_does_not_walk_unchanged_large_trees_for_layer_endpoints() {
    let mut demands = Vec::new();
    for files in [300, 12_000] {
        let f = Fixture::new(&format!("layer-diff-{files}"), |source| {
            std::fs::create_dir(source.join("bulk")).unwrap();
            for n in 0..files {
                std::fs::write(source.join(format!("bulk/f{n:05}")), "same").unwrap();
            }
            std::fs::write(source.join("changed"), "before").unwrap();
            files + 2
        });
        let base = pin(&f, "main");
        f.fork("work");
        f.apply("work", vec![write("/changed", "after")]);
        let commit = pin(&f, "work");
        let diff = |from: &str, to: &str| {
            f.call(
                "branch.diff_commits",
                vec![
                    ("from", t(from)),
                    ("to", t(to)),
                    ("hunks", SdkValue::Bool(false)),
                ],
            )
        };
        let before = f.work();
        let first = diff(&base, &commit);
        let work = f.since(before);
        assert_eq!(items(&first).len(), 1);
        assert_eq!(items(&first)[0].get_text("path"), Some("/changed"));
        f.call("branch.publish", vec![("branch", t("work"))]);
        let layer = pin(&f, "main");
        let before = f.work();
        let layers = diff(&base, &layer);
        let layer_work = f.since(before);
        assert_eq!(items(&layers).len(), 1);
        println!("LAYER_DIFF files={files} first={work:?} layers={layer_work:?}");
        demands.push((work.object_ids, layer_work.object_ids));
        f.cleanup();
    }
    let (small, large) = (demands[0], demands[1]);
    assert!(
        large.0 <= small.0 * 4 + 128,
        "unchanged namespace amplification: {demands:?}"
    );
    assert!(
        large.1 <= small.1 * 4 + 128,
        "unchanged namespace amplification: {demands:?}"
    );
    assert!(
        large.0 < 2000 && large.1 < 2000,
        "full-tree traversal: {demands:?}"
    );
}
