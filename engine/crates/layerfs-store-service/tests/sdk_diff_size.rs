//! A Commit difference reads the entries that changed, not the tree or the
//! history (#352 range 002: diff_commits of one changed file walked both whole
//! trees, its work growing with the tree's size).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{items, paths, write, Fixture, Work};

/// A fixed `/hot` directory and `dirs` bulk directories of 30 files each.
fn tree(source: &std::path::Path, dirs: usize) -> usize {
    fs::create_dir(source.join("hot")).unwrap();
    for name in ["edit-a", "edit-b", "edit-c", "edit-d", "steady"] {
        fs::write(
            source.join(format!("hot/{name}.txt")),
            format!("{name} 0\n"),
        )
        .unwrap();
    }
    fs::create_dir(source.join("lane")).unwrap();
    fs::write(source.join("lane/churn.txt"), b"churn\n").unwrap();
    fs::create_dir(source.join("bulk")).unwrap();
    for d in 0..dirs {
        let dir = source.join(format!("bulk/d{d:03}"));
        fs::create_dir(&dir).unwrap();
        for f in 0..30 {
            fs::write(dir.join(format!("f{f:02}.txt")), format!("bulk {d} {f}\n")).unwrap();
        }
    }
    1 + 5 + 1 + 1 + 1 + dirs * 31
}

fn head(reply: &SdkValue) -> String {
    reply.get_text("head").unwrap().to_owned()
}

/// One Commit changing `/hot/<name>.txt`, then its difference from the
/// Commit before it; returns the difference's work.
fn diff_one(fixture: &Fixture, previous: &str, name: &str) -> (Work, String) {
    let next = head(&fixture.apply(
        "work",
        vec![write(&format!("/hot/{name}.txt"), format!("{name} 1\n"))],
    ));
    let before = fixture.work();
    let diff = fixture.call(
        "branch.diff_commits",
        vec![
            ("from", SdkValue::text(previous)),
            ("to", SdkValue::text(next.as_str())),
        ],
    );
    let work = fixture.since(before);
    assert_eq!(
        paths(&diff),
        [format!("/hot/{name}.txt").as_str()],
        "{diff:?}"
    );
    assert!(items(&diff)[0].get_bool("content"));
    (work, next)
}

#[test]
fn diff_commits_is_independent_of_tree_size_and_history_depth() {
    let mut observed = Vec::new();
    for dirs in [10usize, 100] {
        let fixture = Fixture::new(&format!("sdk-diff-size-{dirs}"), |source| {
            tree(source, dirs)
        });
        fixture.fork("work");
        let mut last = head(&fixture.apply("work", vec![write("/lane/churn.txt", "0\n")]));
        let mut depth = 1;
        for (target, warm, measured) in [(2usize, "edit-a", "edit-b"), (60, "edit-c", "edit-d")] {
            while depth < target {
                last = head(
                    &fixture.apply("work", vec![write("/lane/churn.txt", format!("{depth}\n"))]),
                );
                depth += 1;
            }
            // A first difference in the same directory warms the caches, so
            // every cell measures the same state.
            let (_, warmed) = diff_one(&fixture, &last, warm);
            let (work, measured_head) = diff_one(&fixture, &warmed, measured);
            last = measured_head;
            depth += 2;
            println!("SDK_DIFF_SIZE files={} commits={target} diff={work:?} profile=Disposable cache=warm-after-sibling-diff diagnostic_count_gate=true", dirs * 30);
            observed.push((dirs, target, work));
        }
        fixture.cleanup();
    }
    // Object demands are the difference's own reads and must be equal in
    // every cell (the full walk read 98 at 300 files and 720 at 3,000). SQL
    // statements and rows also pay to decode those objects' metadata delta
    // chains, capped at 8 but at a position that depends on each tree's page
    // layout and Commit count; they are printed, not compared.
    // #352 F-012: the comparison's memo reads each B-tree page once, so the
    // count is now the pages themselves (36 -> 10), and a tree one level
    // taller reads one more. Equal within that one level.
    let first = observed[0].2;
    for (dirs, depth, work) in &observed {
        assert!(
            work.object_ids.abs_diff(first.object_ids) <= 1
                && work.object_batches.abs_diff(first.object_batches) <= 1,
            "diff_commits of one file reads objects by tree size or depth ({dirs} directories, {depth} Commits): {observed:?}"
        );
    }
}
