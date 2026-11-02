//! Merged roots across the merge's rule cases (#355 (d)). Each case forks
//! two Branches from one seeded tree, changes each, merges them and prints
//! one `MERGE_ORACLE` line: the merged root, or the conflict refusal. The
//! lines are compared across builds to show a change to how the merge keeps
//! its state leaves every result unchanged; the test itself asserts only that
//! each case ends as its rules say (merged or conflicted).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;
#[path = "support/view.rs"]
#[allow(dead_code)]
mod view;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{operation, write};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn mv(from: &str, to: &str) -> SdkValue {
    SdkValue::map([("op", text("mv")), ("from", text(from)), ("to", text(to))])
}

fn mkdir(path: &str) -> SdkValue {
    operation("mkdir", path)
}

fn rm(path: &str) -> SdkValue {
    operation("rm", path)
}

/// One case: its name, the left and right changes, and whether it merges.
type Case = (&'static str, Vec<SdkValue>, Vec<SdkValue>, bool);

fn cases() -> Vec<Case> {
    vec![
        (
            "disjoint",
            vec![write("/d1/a.txt", "a2\n")],
            vec![write("/d2/c.txt", "c2\n")],
            true,
        ),
        (
            "text-merge",
            vec![write("/top.txt", "one\n2\n3\n4\n5\n")],
            vec![write("/top.txt", "1\n2\n3\n4\nfive\n")],
            true,
        ),
        (
            "text-conflict",
            vec![write("/top.txt", "1\n2\nleft\n4\n5\n")],
            vec![write("/top.txt", "1\n2\nright\n4\n5\n")],
            false,
        ),
        (
            "modify-delete",
            vec![write("/d1/b.txt", "b2\n")],
            vec![rm("/d1/b.txt")],
            false,
        ),
        (
            "move-directory",
            vec![mv("/d1", "/moved")],
            vec![write("/d2/c.txt", "c2\n")],
            true,
        ),
        (
            // Merged by serial (#355 M1): the edit follows the moved file.
            "move-and-edit-inside",
            vec![mv("/d1", "/moved")],
            vec![write("/d1/a.txt", "edited\n")],
            true,
        ),
        (
            "revive-deleted-parent",
            vec![rm("/d2/c.txt"), rm("/d2")],
            vec![write("/d2/new.txt", "new\n")],
            true,
        ),
        (
            "directory-becomes-file",
            vec![
                rm("/d1/a.txt"),
                rm("/d1/b.txt"),
                rm("/d1"),
                write("/d1", "file\n"),
            ],
            // A path below a parent that stays a file is dropped.
            vec![write("/d1/z.txt", "z\n")],
            true,
        ),
        (
            "add-add-same",
            vec![write("/same.txt", "x\n")],
            vec![write("/same.txt", "x\n")],
            true,
        ),
        (
            "add-add-different",
            vec![write("/new.txt", "left\n")],
            vec![write("/new.txt", "right\n")],
            false,
        ),
        (
            "nested-creation-and-sort-edges",
            vec![
                mkdir("/n1"),
                mkdir("/n1/n2"),
                write("/n1/n2/deep.txt", "deep\n"),
                write("/n1.y", "dot\n"),
                write("/n1-z", "dash\n"),
            ],
            vec![write("/n1x.txt", "x\n"), write("/d1/a.b", "ab\n")],
            true,
        ),
        (
            "delete-directory-against-delete-inside",
            vec![rm("/d1/a.txt"), rm("/d1/b.txt"), rm("/d1")],
            vec![rm("/d1/a.txt")],
            true,
        ),
        (
            "both-move-elsewhere",
            vec![mv("/d2/c.txt", "/c-left.txt")],
            vec![mv("/d2/c.txt", "/c-right.txt")],
            true,
        ),
    ]
}

#[test]
fn merged_roots_of_every_rule_case() {
    let fixture = support::Fixture::new("merge-root-oracle", |source| {
        fs::create_dir(source.join("d1")).unwrap();
        fs::create_dir(source.join("d2")).unwrap();
        fs::write(source.join("d1/a.txt"), b"a\n").unwrap();
        fs::write(source.join("d1/b.txt"), b"b\n").unwrap();
        fs::write(source.join("d2/c.txt"), b"c\n").unwrap();
        fs::write(source.join("top.txt"), b"1\n2\n3\n4\n5\n").unwrap();
        6
    });
    for (index, (name, left, right, merges)) in cases().into_iter().enumerate() {
        let (l, r) = (format!("l{index}"), format!("r{index}"));
        fixture.fork(&l);
        fixture.fork(&r);
        fixture.apply(&l, left);
        fixture.apply(&r, right);
        let merged = fixture.try_call(
            "branch.merge",
            vec![
                ("branch", text(&l)),
                ("with", text(&r)),
                ("name", text(&format!("m{index}"))),
            ],
        );
        match merged {
            Ok(reply) => {
                assert!(merges, "{name} merged: {reply:?}");
                println!(
                    "MERGE_ORACLE {name} root={} lww={:?}",
                    reply.get_text("root").unwrap(),
                    reply.get("lww")
                );
                // With `LAYERFS_355_VIEWS`, the decoded view without mtimes:
                // compared across builds, whatever the root identities.
                if std::env::var_os("LAYERFS_355_VIEWS").is_some() {
                    let viewer = view::Viewer::open(
                        &fixture.host.config.path,
                        installed::BINDING,
                        installed::CURSOR,
                    );
                    for line in viewer.view(reply.get_text("root").unwrap(), false) {
                        println!("MERGE_VIEW {name} {line}");
                    }
                }
            }
            Err(refusal) => {
                assert!(!merges, "{name} refused: {refusal:?}");
                println!("MERGE_ORACLE {name} refused={refusal:?}");
            }
        }
    }
    fixture.cleanup();
}
