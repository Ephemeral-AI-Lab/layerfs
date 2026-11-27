//! `branch.publish` refuses a publication that would drop a Layer main gained
//! since the Branch forked (#691): main must still be the fork point, or its
//! head must be this Branch's own earlier publication. The caller may pin
//! main's head it read (`expected`). A refused publish writes nothing, leaves main
//! and the Branch alone, and `force` keeps the old overwrite-only behavior. A
//! merged Branch still fast-forwards, and a Branch may publish again over its
//! own Layer. A Commit already published stays `UpToDate` and a tree main
//! already holds stays `NoChanges` after main moved.
#[path = "../support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_protocol::sdk::SdkValue;
use support::{paths, write, Fixture};

/// One Branch's field from `project.list_branches`.
fn branch_field(fixture: &Fixture, name: &str, field: &str) -> Option<String> {
    let page = fixture.call(
        "project.list_branches",
        vec![("name", SdkValue::text(name))],
    );
    support::items(&page)
        .first()
        .and_then(|item| item.get_text(field))
        .map(str::to_owned)
}

/// main's current head Layer.
fn main_head(fixture: &Fixture) -> String {
    branch_field(fixture, "main", "base").expect("main is listed with its head Layer")
}

/// The paths in main's root.
fn main_paths(fixture: &Fixture) -> Vec<String> {
    let page = fixture.call(
        "fs.list",
        vec![
            ("branch", SdkValue::text("main")),
            ("path", SdkValue::text("/")),
        ],
    );
    paths(&page).into_iter().map(str::to_owned).collect()
}

/// One field of a publish answer's `publishedLayer`.
fn published_field(value: &SdkValue, field: &str) -> String {
    value
        .get("publishedLayer")
        .and_then(|layer| layer.get_text(field))
        .unwrap_or_else(|| panic!("no publishedLayer.{field} in {value:?}"))
        .to_owned()
}

/// A Store with one seed file, and two Branches forked from the same main.
fn two_branches(label: &str) -> Fixture {
    let fixture = Fixture::new(label, |source| {
        std::fs::write(source.join("seed.txt"), "seed\n").unwrap();
        1
    });
    fixture.fork("feat-a");
    fixture.fork("feat-b");
    fixture.apply("feat-a", vec![write("/a.txt", "from A\n")]);
    fixture.apply("feat-b", vec![write("/b.txt", "from B\n")]);
    fixture
}

#[test]
fn a_stale_base_publish_is_refused_and_main_is_unchanged() {
    let fixture = two_branches("publish-stale-base");
    let a = fixture.call("branch.publish", vec![("branch", SdkValue::text("feat-a"))]);
    assert_eq!(a.get_text("kind"), Some("Added"));
    let a_layer = published_field(&a, "id");
    let main_before = main_head(&fixture);
    assert_eq!(main_before, a_layer, "main is now A's Layer");
    let b_base = branch_field(&fixture, "feat-b", "base").expect("feat-b's base");
    let b_head = branch_field(&fixture, "feat-b", "head").expect("feat-b's head");

    let refusal = fixture
        .try_call("branch.publish", vec![("branch", SdkValue::text("feat-b"))])
        .expect_err("a Branch forked before main moved is refused");
    assert_eq!(refusal.code, "HeadMoved");
    assert!(
        refusal
            .message
            .contains("merge main into feat-b and publish again"),
        "the refusal says how to recover: {}",
        refusal.message
    );
    assert!(
        refusal.message.contains("force"),
        "the refusal names the escape: {}",
        refusal.message
    );
    assert_eq!(
        refusal.detail.get_text("expectedBase"),
        Some(b_base.as_str())
    );
    assert_eq!(
        refusal.detail.get_text("actualBase"),
        Some(a_layer.as_str())
    );

    // main did not move, and never received B's tree.
    assert_eq!(main_head(&fixture), main_before);
    let names = main_paths(&fixture);
    assert!(names.contains(&"/a.txt".to_owned()), "{names:?}");
    assert!(!names.iter().any(|path| path == "/b.txt"), "{names:?}");
    // The Branch itself did not move.
    assert_eq!(
        branch_field(&fixture, "feat-b", "base").as_deref(),
        Some(b_base.as_str())
    );
    assert_eq!(
        branch_field(&fixture, "feat-b", "head").as_deref(),
        Some(b_head.as_str())
    );
    fixture.cleanup();
}

#[test]
fn matching_expectations_publish_and_wrong_ones_are_refused() {
    let fixture = two_branches("publish-expected-base");
    let a = fixture.call("branch.publish", vec![("branch", SdkValue::text("feat-a"))]);
    let a_layer = published_field(&a, "id");

    // C forks after A published, so its base is main's current head.
    fixture.fork("feat-c");
    fixture.apply("feat-c", vec![write("/c.txt", "from C\n")]);
    let c_base = branch_field(&fixture, "feat-c", "base").expect("feat-c's base");
    let c_head = branch_field(&fixture, "feat-c", "head").expect("feat-c's head");
    assert_eq!(c_base, a_layer);

    let published = fixture.call(
        "branch.publish",
        vec![
            ("branch", SdkValue::text("feat-c")),
            ("expected", SdkValue::text(c_base.as_str())),
            ("at", SdkValue::text(c_head.as_str())),
        ],
    );
    assert_eq!(published.get_text("kind"), Some("Added"));
    assert_eq!(published.get_text("basedOn"), Some(c_base.as_str()));
    let c_layer = published_field(&published, "id");

    // D forks after C published; an expected base from before C is stale.
    fixture.fork("feat-d");
    fixture.apply("feat-d", vec![write("/d.txt", "from D\n")]);
    let d_base = branch_field(&fixture, "feat-d", "base").expect("feat-d's base");
    let d_head = branch_field(&fixture, "feat-d", "head").expect("feat-d's head");
    assert_eq!(d_base, c_layer);

    let moved_base = fixture
        .try_call(
            "branch.publish",
            vec![
                ("branch", SdkValue::text("feat-d")),
                ("expected", SdkValue::text(a_layer.as_str())),
            ],
        )
        .expect_err("a stale expected base is refused");
    assert_eq!(moved_base.code, "HeadMoved");
    assert_eq!(
        moved_base.detail.get_text("expectedBase"),
        Some(a_layer.as_str())
    );
    assert_eq!(
        moved_base.detail.get_text("actualBase"),
        Some(d_base.as_str())
    );
    // the refusal names main's head for the caller's next call
    assert_eq!(moved_base.detail.get_text("head"), Some(d_base.as_str()));

    // Neither refusal published anything: main is still C's Layer.
    assert_eq!(main_head(&fixture), c_layer);
    assert_eq!(
        branch_field(&fixture, "feat-d", "head").as_deref(),
        Some(d_head.as_str())
    );

    // main's head and the Commit the caller read publish as a fast-forward.
    let d = fixture.call(
        "branch.publish",
        vec![
            ("branch", SdkValue::text("feat-d")),
            ("expected", SdkValue::text(d_base.as_str())),
            ("at", SdkValue::text(d_head.as_str())),
        ],
    );
    assert_eq!(d.get_text("kind"), Some("Added"));
    assert_eq!(d.get_int("overwroteCount"), Some(0));
    fixture.cleanup();
}

#[test]
fn force_keeps_the_overwrite_behavior() {
    let fixture = two_branches("publish-force");
    let a = fixture.call("branch.publish", vec![("branch", SdkValue::text("feat-a"))]);
    let a_layer = published_field(&a, "id");

    let forced = fixture.call(
        "branch.publish",
        vec![
            ("branch", SdkValue::text("feat-b")),
            ("force", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(forced.get_text("kind"), Some("Added"));
    assert_eq!(forced.get_int("overwroteCount"), Some(1));
    let overwrote = match forced.get("overwrote") {
        Some(SdkValue::List(items)) => items.clone(),
        other => panic!("overwrote: {other:?}"),
    };
    assert_eq!(overwrote.len(), 1);
    assert_eq!(overwrote[0], SdkValue::text(a_layer.as_str()));
    // The forced Layer is a child of the Layer it replaced but carries B's root.
    assert_eq!(published_field(&forced, "parent"), a_layer);
    let names = main_paths(&fixture);
    assert!(names.contains(&"/b.txt".to_owned()), "{names:?}");
    assert!(!names.iter().any(|path| path == "/a.txt"), "{names:?}");

    // A Commit that is already published is UpToDate before the stale check,
    // as it was before the fix.
    let again = fixture.call("branch.publish", vec![("branch", SdkValue::text("feat-b"))]);
    assert_eq!(again.get_text("kind"), Some("UpToDate"));
    fixture.cleanup();
}

#[test]
fn publishing_a_merged_branch_is_a_fast_forward() {
    let fixture = two_branches("publish-merged");
    let a = fixture.call("branch.publish", vec![("branch", SdkValue::text("feat-a"))]);
    let a_layer = published_field(&a, "id");
    let merged = fixture.call(
        "branch.merge",
        vec![
            ("branch", SdkValue::text("main")),
            ("with", SdkValue::text("feat-b")),
            ("name", SdkValue::text("merged")),
        ],
    );
    assert_eq!(merged.get_text("kind"), Some("Merged"));
    // The merge base is main's Layer, so publishing it fast-forwards.
    let merged_base = branch_field(&fixture, "merged", "base").expect("merged's base");
    assert_eq!(merged_base, a_layer);
    let published = fixture.call(
        "branch.publish",
        vec![
            ("branch", SdkValue::text("merged")),
            ("expected", SdkValue::text(merged_base.as_str())),
        ],
    );
    assert_eq!(published.get_text("kind"), Some("Added"));
    assert_eq!(published.get_int("overwroteCount"), Some(0));
    assert_eq!(published_field(&published, "parent"), a_layer);
    let names = main_paths(&fixture);
    assert!(names.contains(&"/a.txt".to_owned()), "{names:?}");
    assert!(names.contains(&"/b.txt".to_owned()), "{names:?}");
    fixture.cleanup();
}

#[test]
fn a_branch_publishes_again_over_its_own_layer() {
    // Publishing at milestones is the normal flow: after a Branch's own Layer
    // reaches main, its next Commit still contains that Layer, so nothing is
    // skipped and the default publish stays allowed.
    let fixture = Fixture::new("publish-milestones", |source| {
        std::fs::write(source.join("seed.txt"), "seed\n").unwrap();
        1
    });
    fixture.fork("work");
    fixture.apply("work", vec![write("/work.txt", "one\n")]);
    let first = fixture.call("branch.publish", vec![("branch", SdkValue::text("work"))]);
    assert_eq!(first.get_text("kind"), Some("Added"));
    assert_eq!(first.get_int("overwroteCount"), Some(0));
    let first_layer = published_field(&first, "id");

    fixture.apply("work", vec![write("/work.txt", "two\n")]);
    let second = fixture.call("branch.publish", vec![("branch", SdkValue::text("work"))]);
    assert_eq!(second.get_text("kind"), Some("Added"));
    assert_eq!(second.get_int("overwroteCount"), Some(0));
    assert_eq!(published_field(&second, "parent"), first_layer);
    let names = main_paths(&fixture);
    assert!(names.contains(&"/work.txt".to_owned()), "{names:?}");
    fixture.cleanup();
}

#[test]
fn a_repeated_publish_after_main_moved_is_up_to_date() {
    // A lost reply leaves the publication in place; asking again after main
    // moved on reports that Layer, not a stale base.
    let fixture = two_branches("publish-repeat-after-move");
    let a = fixture.call("branch.publish", vec![("branch", SdkValue::text("feat-a"))]);
    let a_layer = published_field(&a, "id");
    let a_base = a.get_text("basedOn").expect("A's base").to_owned();
    fixture.fork("feat-c");
    fixture.apply("feat-c", vec![write("/c.txt", "from C\n")]);
    let c = fixture.call("branch.publish", vec![("branch", SdkValue::text("feat-c"))]);
    let c_layer = published_field(&c, "id");

    let again = fixture.call("branch.publish", vec![("branch", SdkValue::text("feat-a"))]);
    assert_eq!(again.get_text("kind"), Some("UpToDate"));
    assert_eq!(again.get_text("layer"), Some(a_layer.as_str()));
    // The base the caller read before its first attempt is stale now; the
    // answer is still the Layer that attempt published.
    let pinned = fixture.call(
        "branch.publish",
        vec![
            ("branch", SdkValue::text("feat-a")),
            ("expected", SdkValue::text(a_base.as_str())),
        ],
    );
    assert_eq!(pinned.get_text("kind"), Some("UpToDate"));
    assert_eq!(pinned.get_text("layer"), Some(a_layer.as_str()));
    assert_eq!(main_head(&fixture), c_layer, "nothing was published");
    fixture.cleanup();
}

#[test]
fn a_tree_main_already_holds_is_no_changes() {
    // A second Branch at the same Commit as the first: once the first is
    // published, the second drops nothing, so it answers NoChanges instead of
    // a stale-base refusal.
    let fixture = Fixture::new("publish-same-tree", |source| {
        std::fs::write(source.join("seed.txt"), "seed\n").unwrap();
        1
    });
    fixture.fork("same-a");
    fixture.apply("same-a", vec![write("/same.txt", "same\n")]);
    let a_head = branch_field(&fixture, "same-a", "head").expect("same-a's head");
    fixture.call(
        "branch.fork",
        vec![
            ("name", SdkValue::text("same-b")),
            ("from", SdkValue::text(a_head.as_str())),
        ],
    );
    let a = fixture.call("branch.publish", vec![("branch", SdkValue::text("same-a"))]);
    let a_layer = published_field(&a, "id");
    let b = fixture.call("branch.publish", vec![("branch", SdkValue::text("same-b"))]);
    assert_eq!(b.get_text("kind"), Some("NoChanges"), "{b:?}");
    assert_eq!(b.get_text("headLayer"), Some(a_layer.as_str()));
    assert_eq!(main_head(&fixture), a_layer);
    fixture.cleanup();
}

/// main's published Layers, newest first.
fn main_layers(fixture: &Fixture) -> Vec<String> {
    let page = fixture.call(
        "branch.list_commits",
        vec![
            ("branch", SdkValue::text("main")),
            ("limit", SdkValue::Int(100)),
        ],
    );
    support::items(&page)
        .iter()
        .map(|item| item.get_text("commit").expect("a Layer id").to_owned())
        .collect()
}

#[test]
fn two_threads_publishing_one_expected_base_race_cleanly() {
    use std::sync::Barrier;

    // Each round forks two Branches from main, then both threads publish with
    // main's head pinned as `expectedBase`. Whichever thread reaches the
    // Store's write transaction first adds the only Layer; the other finds the
    // stack head already moved, by the compare-and-swap in `add_layer` (or by
    // the read above it) and is refused `HeadMoved`. A barrier starts the two
    // calls together; no sleep orders them, and either thread may win.
    const ROUNDS: usize = 20;
    let fixture = Fixture::new("publish-two-threads", |source| {
        std::fs::write(source.join("seed.txt"), "seed\n").unwrap();
        1
    });
    for round in 0..ROUNDS {
        let a = format!("race-a-{round}");
        let b = format!("race-b-{round}");
        let a_file = format!("/a-{round}.txt");
        let b_file = format!("/b-{round}.txt");
        let a_text = format!("A {round}\n");
        let b_text = format!("B {round}\n");
        let base = main_head(&fixture);
        fixture.fork(&a);
        fixture.fork(&b);
        fixture.apply(&a, vec![write(&a_file, a_text.clone())]);
        fixture.apply(&b, vec![write(&b_file, b_text.clone())]);
        let a_head = branch_field(&fixture, &a, "head").expect("race-a's head");
        let b_head = branch_field(&fixture, &b, "head").expect("race-b's head");
        let layers_before = main_layers(&fixture);

        let barrier = Barrier::new(2);
        let publish = |branch: &str, head: &str| {
            fixture.try_call(
                "branch.publish",
                vec![
                    ("branch", SdkValue::text(branch)),
                    ("expected", SdkValue::text(base.as_str())),
                    ("at", SdkValue::text(head)),
                ],
            )
        };
        let (a_result, b_result) = std::thread::scope(|scope| {
            let a_thread = scope.spawn(|| {
                barrier.wait();
                publish(&a, &a_head)
            });
            let b_thread = scope.spawn(|| {
                barrier.wait();
                publish(&b, &b_head)
            });
            (a_thread.join().unwrap(), b_thread.join().unwrap())
        });

        // Exactly one publication wins; the other is refused before any write.
        let a_won = a_result.is_ok();
        assert!(
            a_won ^ b_result.is_ok(),
            "round {round}: exactly one publish must win: a={a_result:?} b={b_result:?}"
        );
        let (winner_answer, loser_refusal) = if a_won {
            (a_result.as_ref().unwrap(), b_result.as_ref().unwrap_err())
        } else {
            (b_result.as_ref().unwrap(), a_result.as_ref().unwrap_err())
        };
        let (winner_file, winner_text, loser_file, loser_head, loser_name) = if a_won {
            (&a_file, &a_text, &b_file, &b_head, &b)
        } else {
            (&b_file, &b_text, &a_file, &a_head, &a)
        };

        assert_eq!(
            winner_answer.get_text("kind"),
            Some("Added"),
            "round {round}"
        );
        assert_eq!(
            winner_answer.get_text("basedOn"),
            Some(base.as_str()),
            "round {round}"
        );
        assert_eq!(
            winner_answer.get_int("overwroteCount"),
            Some(0),
            "round {round}"
        );
        let winner_layer = published_field(winner_answer, "id");

        assert_eq!(loser_refusal.code, "HeadMoved", "round {round}");
        assert_eq!(
            loser_refusal.detail.get_text("expectedBase"),
            Some(base.as_str()),
            "round {round}"
        );
        assert_eq!(
            loser_refusal.detail.get_text("actualBase"),
            Some(winner_layer.as_str()),
            "round {round}"
        );

        // No write from the refused thread: main moved once, to the winner's
        // Layer, and has exactly one new Layer.
        assert_eq!(main_head(&fixture), winner_layer, "round {round}");
        let layers_after = main_layers(&fixture);
        assert_eq!(layers_after.len(), layers_before.len() + 1, "round {round}");
        assert_eq!(layers_after[0], winner_layer, "round {round}");
        assert_eq!(
            &layers_after[1..],
            &layers_before[..],
            "round {round}: main gained exactly the winner's Layer"
        );

        // The winner's file is in main; the loser's is not, and its Branch
        // did not move.
        let read = fixture.call(
            "fs.read",
            vec![
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text(winner_file.as_str())),
            ],
        );
        assert_eq!(read.get_text("text"), Some(winner_text.as_str()));
        let names = main_paths(&fixture);
        assert!(
            !names.iter().any(|path| path == loser_file),
            "round {round}: the refused Branch's file reached main: {names:?}"
        );
        assert_eq!(
            branch_field(&fixture, loser_name, "head").as_deref(),
            Some(loser_head.as_str()),
            "round {round}: the refused Branch did not move"
        );
    }
    fixture.cleanup();
}
