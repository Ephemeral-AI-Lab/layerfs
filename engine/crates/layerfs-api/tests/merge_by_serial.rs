//! The merge by serial (#355 M1): the cases a name-only merge cannot see.
//! Each test forks two Branches from one seeded tree, changes each through
//! the SDK, merges them and checks the merged tree's decoded view.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;
#[path = "support/view.rs"]
#[allow(dead_code)]
mod view;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use support::{operation, write, Change, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn mv(from: &str, to: &str) -> SdkValue {
    SdkValue::map([("op", text("mv")), ("from", text(from)), ("to", text(to))])
}

fn rm(path: &str) -> SdkValue {
    operation("rm", path)
}

/// `/d1/{a,b}.txt`, `/d2/c.txt`, `/top.txt` and the hard-linked
/// `/h/one` and `/h/two`.
fn seeded(label: &str) -> Fixture {
    Fixture::new(label, |source| {
        fs::create_dir(source.join("d1")).unwrap();
        fs::create_dir(source.join("d2")).unwrap();
        fs::create_dir(source.join("h")).unwrap();
        fs::write(source.join("d1/a.txt"), b"a\n").unwrap();
        fs::write(source.join("d1/b.txt"), b"b\n").unwrap();
        fs::write(source.join("d2/c.txt"), b"c\n").unwrap();
        fs::write(source.join("top.txt"), b"1\n2\n3\n").unwrap();
        fs::write(source.join("h/one"), b"linked\n").unwrap();
        fs::hard_link(source.join("h/one"), source.join("h/two")).unwrap();
        9
    })
}

/// Forks `l` and `r`, applies each side's changes and merges them into `m`,
/// returning the left root and the merge's outcome.
fn merge(
    fixture: &Fixture,
    left: Vec<SdkValue>,
    right: Vec<SdkValue>,
) -> (String, Result<SdkValue, SdkRefusal>) {
    fixture.fork("l");
    fixture.fork("r");
    let applied = fixture.apply("l", left);
    let left_root = applied.get("commit").unwrap().get_text("root").unwrap();
    fixture.apply("r", right);
    let merged = fixture.try_call(
        "branch.merge",
        vec![
            ("branch", text("l")),
            ("with", text("r")),
            ("name", text("m")),
        ],
    );
    (left_root.to_owned(), merged)
}

fn viewer(fixture: &Fixture) -> view::Viewer {
    view::Viewer::open(
        &fixture.host.config.path,
        installed::BINDING,
        installed::CURSOR,
    )
}

fn view(fixture: &Fixture, root: &str) -> Vec<String> {
    viewer(fixture).view(root, false)
}

fn merged_root(merged: Result<SdkValue, SdkRefusal>) -> String {
    let reply = merged.unwrap_or_else(|refusal| panic!("merge refused: {refusal:?}"));
    reply.get_text("root").unwrap().to_owned()
}

fn merged_view(fixture: &Fixture, merged: Result<SdkValue, SdkRefusal>) -> Vec<String> {
    view(fixture, &merged_root(merged))
}

/// The view's line for `path`, if the path exists.
fn line<'v>(view: &'v [String], path: &str) -> Option<&'v str> {
    view.iter()
        .map(String::as_str)
        .find(|line| line.split(' ').next() == Some(path))
}

fn read(fixture: &Fixture, path: &str) -> Option<String> {
    read_at(fixture, "m", path)
}

/// The text at `path` on `branch`, if the path exists.
fn read_at(fixture: &Fixture, branch: &str, path: &str) -> Option<String> {
    fixture
        .try_call(
            "fs.read",
            vec![("branch", text(branch)), ("path", text(path))],
        )
        .ok()
        .map(|reply| reply.get_text("text").unwrap().to_owned())
}

fn conflicted(merged: Result<SdkValue, SdkRefusal>) -> String {
    let refusal = merged.expect_err("the merge conflicts");
    assert_eq!(refusal.code, "Conflict", "{refusal:?}");
    format!("{refusal:?}")
}

#[test]
fn an_edit_follows_a_moved_directory() {
    let fixture = seeded("serial-move-edit");
    let (_, merged) = merge(
        &fixture,
        vec![mv("/d1", "/moved")],
        vec![write("/d1/a.txt", "edited\n")],
    );
    let merged_view = merged_view(&fixture, merged);
    assert_eq!(line(&merged_view, "/d1"), None);
    assert_eq!(read(&fixture, "/moved/a.txt").as_deref(), Some("edited\n"));
    assert_eq!(read(&fixture, "/moved/b.txt").as_deref(), Some("b\n"));
    fixture.cleanup();
}

#[test]
fn an_edit_follows_a_move_into_a_directory() {
    let fixture = seeded("serial-move-nested");
    let (_, merged) = merge(
        &fixture,
        vec![mv("/d1", "/d2/inner")],
        vec![write("/d1/b.txt", "edited\n"), write("/d2/c.txt", "c2\n")],
    );
    let merged_view = merged_view(&fixture, merged);
    assert_eq!(line(&merged_view, "/d1"), None);
    assert_eq!(
        read(&fixture, "/d2/inner/b.txt").as_deref(),
        Some("edited\n")
    );
    assert_eq!(read(&fixture, "/d2/c.txt").as_deref(), Some("c2\n"));
    fixture.cleanup();
}

#[test]
fn a_file_renamed_on_both_sides_keeps_both_names_as_links() {
    let fixture = seeded("serial-rename-file");
    let (_, merged) = merge(
        &fixture,
        vec![mv("/d2/c.txt", "/left.txt")],
        vec![mv("/d2/c.txt", "/right.txt")],
    );
    let merged_view = merged_view(&fixture, merged);
    assert_eq!(line(&merged_view, "/d2/c.txt"), None);
    for path in ["/left.txt", "/right.txt"] {
        let found = line(&merged_view, path).expect("both names bound");
        assert!(found.ends_with(" link=/left.txt"), "{found}");
    }
    fixture.cleanup();
}

#[test]
fn a_directory_renamed_on_both_sides_conflicts() {
    let fixture = seeded("serial-rename-dir");
    let (_, merged) = merge(
        &fixture,
        vec![mv("/d1", "/left")],
        vec![mv("/d1", "/right")],
    );
    conflicted(merged);
    fixture.cleanup();
}

#[test]
fn crossed_moves_that_would_make_a_cycle_conflict() {
    let fixture = seeded("serial-cycle");
    let (_, merged) = merge(
        &fixture,
        vec![mv("/d1", "/d2/d1")],
        vec![mv("/d2", "/d1/d2")],
    );
    conflicted(merged);
    fixture.cleanup();
}

#[test]
fn a_deleted_directory_against_an_edit_below_conflicts() {
    let fixture = seeded("serial-delete-edit");
    let (_, merged) = merge(
        &fixture,
        vec![rm("/d1/a.txt"), rm("/d1/b.txt"), rm("/d1")],
        vec![write("/d1/a.txt", "edited\n")],
    );
    let refusal = conflicted(merged);
    assert!(refusal.contains("/d1/a.txt"), "{refusal}");
    fixture.cleanup();
}

fn delete_d1() -> Vec<SdkValue> {
    vec![rm("/d1/a.txt"), rm("/d1/b.txt"), rm("/d1")]
}

fn move_and_edit_d1() -> Vec<SdkValue> {
    vec![mv("/d1", "/moved"), write("/moved/a.txt", "edited\n")]
}

/// (b): a deletion against a move with an edit inside conflicts on the edited
/// file, whichever side deleted, instead of silently keeping the moved copy.
#[test]
fn a_deleted_directory_against_a_move_and_edit_conflicts() {
    for (name, left, right) in [
        ("serial-delete-move-edit", delete_d1(), move_and_edit_d1()),
        ("serial-move-edit-delete", move_and_edit_d1(), delete_d1()),
    ] {
        let fixture = seeded(name);
        let (_, merged) = merge(&fixture, left, right);
        let refusal = conflicted(merged);
        assert!(refusal.contains("/moved/a.txt"), "{name}: {refusal}");
        assert!(refusal.contains("modify"), "{name}: {refusal}");
        fixture.cleanup();
    }
}

/// A deletion against a move alone keeps the moved directory, whichever side
/// deleted, as the merge by name did.
#[test]
fn a_deleted_directory_against_a_move_keeps_the_moved_directory() {
    for (name, left, right) in [
        ("serial-delete-move", delete_d1(), vec![mv("/d1", "/moved")]),
        ("serial-move-delete", vec![mv("/d1", "/moved")], delete_d1()),
    ] {
        let fixture = seeded(name);
        let (_, merged) = merge(&fixture, left, right);
        let merged_view = merged_view(&fixture, merged);
        assert_eq!(line(&merged_view, "/d1"), None, "{name}");
        assert_eq!(
            read(&fixture, "/moved/a.txt").as_deref(),
            Some("a\n"),
            "{name}"
        );
        assert_eq!(
            read(&fixture, "/moved/b.txt").as_deref(),
            Some("b\n"),
            "{name}"
        );
        fixture.cleanup();
    }
}

#[test]
fn a_deleted_directory_against_an_add_below_keeps_only_the_add() {
    let fixture = seeded("serial-delete-add");
    let (_, merged) = merge(
        &fixture,
        vec![rm("/d1/a.txt"), rm("/d1/b.txt"), rm("/d1")],
        vec![write("/d1/new.txt", "new\n")],
    );
    let merged_view = merged_view(&fixture, merged);
    let below: Vec<_> = merged_view
        .iter()
        .filter(|line| line.starts_with("/d1/"))
        .collect();
    assert_eq!(below.len(), 1, "{below:?}");
    assert_eq!(read(&fixture, "/d1/new.txt").as_deref(), Some("new\n"));
    fixture.cleanup();
}

#[test]
fn hard_links_keep_their_group_across_a_one_sided_edit() {
    let fixture = seeded("serial-links-edit");
    let (left, merged) = merge(
        &fixture,
        vec![write("/h/one", "edited\n")],
        vec![write("/top.txt", "1\n2\nthree\n")],
    );
    let left_view = view(&fixture, &left);
    let merged_view = merged_view(&fixture, merged);
    for path in ["/h/one", "/h/two"] {
        let found = line(&merged_view, path).expect("both links stay");
        assert!(found.ends_with(" link=/h/one"), "{found}");
        assert_eq!(Some(found), line(&left_view, path), "{path}");
    }
    assert_eq!(read(&fixture, "/h/two").as_deref(), Some("edited\n"));
    assert_eq!(read(&fixture, "/top.txt").as_deref(), Some("1\n2\nthree\n"));
    fixture.cleanup();
}

#[test]
fn removing_one_link_leaves_the_other_without_a_group() {
    let fixture = seeded("serial-links-count");
    let (_, merged) = merge(
        &fixture,
        vec![rm("/h/two")],
        vec![write("/top.txt", "1\n2\nthree\n")],
    );
    let merged_view = merged_view(&fixture, merged);
    assert_eq!(line(&merged_view, "/h/two"), None);
    let one = line(&merged_view, "/h/one").expect("the other link stays");
    assert!(!one.contains(" link="), "{one}");
    assert_eq!(read(&fixture, "/h/one").as_deref(), Some("linked\n"));
    fixture.cleanup();
}

#[test]
fn an_edit_through_one_link_meets_the_removal_of_the_other() {
    let fixture = seeded("serial-links-both");
    let (_, merged) = merge(
        &fixture,
        vec![write("/h/one", "edited\n")],
        vec![rm("/h/two")],
    );
    let merged_view = merged_view(&fixture, merged);
    assert_eq!(line(&merged_view, "/h/two"), None);
    let one = line(&merged_view, "/h/one").expect("the edited link stays");
    assert!(!one.contains(" link="), "{one}");
    assert_eq!(read(&fixture, "/h/one").as_deref(), Some("edited\n"));
    fixture.cleanup();
}

/// A file new on one side with a second link (as cargo links each crate's
/// `.rmeta` into its new incremental session directory) loses its first name
/// to the other side's newer file by a `.lww` rule: the link the merge keeps
/// still binds it, with its value.
#[test]
fn a_new_link_outlives_its_other_name_lost_to_lww() {
    let fixture = Fixture::new("serial-link-lost", |source| {
        fs::write(source.join(".lww"), b"/t\n").unwrap();
        fs::create_dir(source.join("t")).unwrap();
        2
    });
    fixture.fork("l");
    fixture.fork("r");
    fixture.workspace_commit(
        "l",
        &[
            Change::Create("/t/x", b"ours\n"),
            Change::Mkdir("/t/s"),
            Change::Link("/t/x", "/t/s/x"),
        ],
        1,
        || {},
    );
    fixture.workspace_commit(
        "r",
        &[
            Change::Create("/t/x", b"theirs\n"),
            Change::Mtime("/t/x", 456),
        ],
        2,
        || {},
    );
    let merged = fixture.try_call(
        "branch.merge",
        vec![
            ("branch", text("l")),
            ("with", text("r")),
            ("name", text("m")),
        ],
    );
    let merged_view = merged_view(&fixture, merged);
    let kept = line(&merged_view, "/t/s/x").expect("the other link stays");
    assert!(!kept.contains(" link="), "{kept}");
    assert_eq!(read(&fixture, "/t/x").as_deref(), Some("theirs\n"));
    assert_eq!(read(&fixture, "/t/s/x").as_deref(), Some("ours\n"));
    fixture.cleanup();
}

/// The same new file loses both its names: one to the other side's newer
/// file, the other with the new directory holding it, which the other
/// side's newer file replaces. With no link left the merge leaves it
/// unbound and no name binds it, whichever name it loses last: the file's
/// own (`/t/x`, after `/t/s`) or the one below the directory (`/t/a`
/// before it).
#[test]
fn a_new_file_is_dropped_once_every_link_is_lost() {
    for (label, file) in [
        ("serial-links-lost-name", "/t/x"),
        ("serial-links-lost-below", "/t/a"),
    ] {
        let fixture = Fixture::new(label, |source| {
            fs::write(source.join(".lww"), b"/t\n").unwrap();
            fs::create_dir(source.join("t")).unwrap();
            2
        });
        fixture.fork("l");
        fixture.fork("r");
        fixture.workspace_commit(
            "l",
            &[
                Change::Create(file, b"ours\n"),
                Change::Mkdir("/t/s"),
                Change::Link(file, "/t/s/x"),
            ],
            1,
            || {},
        );
        fixture.workspace_commit(
            "r",
            &[
                Change::Create(file, b"theirs\n"),
                Change::Mtime(file, 456),
                Change::Create("/t/s", b"theirs s\n"),
                Change::Mtime("/t/s", 456),
            ],
            2,
            || {},
        );
        let merged = fixture.try_call(
            "branch.merge",
            vec![
                ("branch", text("l")),
                ("with", text("r")),
                ("name", text("m")),
            ],
        );
        let merged_view = merged_view(&fixture, merged);
        assert_eq!(line(&merged_view, "/t/s/x"), None, "{merged_view:?}");
        assert_eq!(read(&fixture, file).as_deref(), Some("theirs\n"));
        assert_eq!(read(&fixture, "/t/s").as_deref(), Some("theirs s\n"));
        let below: Vec<_> = merged_view
            .iter()
            .filter_map(|line| line.split(' ').next())
            .filter(|path| path.starts_with("/t/"))
            .collect();
        let mut names = [file, "/t/s"];
        names.sort();
        assert_eq!(below, names, "{merged_view:?}");
        for path in below {
            let found = line(&merged_view, path).unwrap();
            assert!(found.starts_with(&format!("{path} file ")), "{found}");
            assert!(!found.contains(" link="), "{found}");
            let bytes = read(&fixture, path).unwrap_or_else(|| panic!("{path} reads"));
            assert_ne!(bytes, "ours\n", "{path}");
        }
        fixture.cleanup();
    }
}

/// Both orders of one merge: `l` into `r`, then `r` into `l`.
const ORDERS: [[&str; 2]; 2] = [["l", "r"], ["r", "l"]];

/// `/t`, with `/.lww` = `/t` when `rules`, and `/t/d/old` when `old`.
fn link_tree(label: &str, rules: bool, old: bool) -> Fixture {
    Fixture::new(label, |source| {
        let mut count = 1;
        if rules {
            fs::write(source.join(".lww"), b"/t\n").unwrap();
            count += 1;
        }
        fs::create_dir(source.join("t")).unwrap();
        if old {
            fs::create_dir(source.join("t/d")).unwrap();
            fs::write(source.join("t/d/old"), b"old\n").unwrap();
            count += 2;
        }
        count
    })
}

/// Commits `left` on `l`, then `right` on `r` (so `r`'s files are newer at
/// equal mtimes) and merges `branch` with `with` into `m`.
fn merge_changes(
    fixture: &Fixture,
    left: &[Change],
    right: &[Change],
    [branch, with]: [&str; 2],
) -> Result<SdkValue, SdkRefusal> {
    fixture.fork("l");
    fixture.fork("r");
    fixture.workspace_commit("l", left, 1, || {});
    fixture.workspace_commit("r", right, 2, || {});
    fixture.try_call(
        "branch.merge",
        vec![
            ("branch", text(branch)),
            ("with", text(with)),
            ("name", text("m")),
        ],
    )
}

/// Runs one link case in both orders: `check` gets the merged view.
fn both_orders(
    label: &str,
    rules: bool,
    old: bool,
    left: &[Change],
    right: &[Change],
    check: impl Fn(&Fixture, &[String], [&'static str; 2]),
) {
    each_order(
        label,
        rules,
        old,
        left,
        right,
        |fixture, _, merged_view, order| check(fixture, merged_view, order),
    );
}

/// Runs one case in both orders: `check` gets the merge's reply and view.
fn each_order(
    label: &str,
    rules: bool,
    old: bool,
    left: &[Change],
    right: &[Change],
    check: impl Fn(&Fixture, &SdkValue, &[String], [&'static str; 2]),
) {
    let mut failed = Vec::new();
    for order in ORDERS {
        let fixture = link_tree(&format!("{label}-{}", order[0]), rules, old);
        let merged = merge_changes(&fixture, left, right, order);
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let merged = merged.unwrap_or_else(|refusal| panic!("merge refused: {refusal:?}"));
            let merged_view = view(&fixture, merged.get_text("root").unwrap());
            check(&fixture, &merged, &merged_view, order);
        }));
        if checked.is_err() {
            failed.push(order);
        }
        fixture.cleanup();
    }
    assert!(failed.is_empty(), "{label}: failed in {failed:?}");
}

/// Every path below `/t/` in the view, in view order.
fn below_t(merged_view: &[String]) -> Vec<&str> {
    merged_view
        .iter()
        .filter_map(|line| line.split(' ').next())
        .filter(|path| path.starts_with("/t/"))
        .collect()
}

/// A file view line without a link group.
fn single(merged_view: &[String], path: &str) {
    let found = line(merged_view, path).unwrap_or_else(|| panic!("{path}: {merged_view:?}"));
    assert!(found.starts_with(&format!("{path} file ")), "{found}");
    assert!(!found.contains(" link="), "{found}");
}

const NEWER: i64 = 456;

/// The new two-link file of the `.lww` case, merged in each direction: the
/// file `l` hard-linked keeps the name the merge keeps, whichever side the
/// merge reads changes from.
#[test]
fn a_new_link_outlives_its_lost_name_in_either_direction() {
    both_orders(
        "serial-link-lost-both",
        true,
        false,
        &[
            Change::Create("/t/x", b"ours\n"),
            Change::Mkdir("/t/s"),
            Change::Link("/t/x", "/t/s/x"),
        ],
        &[
            Change::Create("/t/x", b"theirs\n"),
            Change::Mtime("/t/x", NEWER),
        ],
        |fixture, merged_view, _| {
            single(merged_view, "/t/s/x");
            assert_eq!(read(fixture, "/t/x").as_deref(), Some("theirs\n"));
            assert_eq!(read(fixture, "/t/s/x").as_deref(), Some("ours\n"));
        },
    );
}

/// A new file with three links loses two names to newer files: the third
/// keeps it (the stored lost count is read back once). Losing the third
/// with its new directory as well drops it.
#[test]
fn a_three_link_file_is_dropped_only_with_its_last_name() {
    let left = [
        Change::Create("/t/x", b"ours\n"),
        Change::Link("/t/x", "/t/y"),
        Change::Mkdir("/t/s"),
        Change::Link("/t/x", "/t/s/z"),
    ];
    let lost_two = [
        Change::Create("/t/x", b"theirs x\n"),
        Change::Mtime("/t/x", NEWER),
        Change::Create("/t/y", b"theirs y\n"),
        Change::Mtime("/t/y", NEWER),
    ];
    both_orders(
        "serial-three-links-kept",
        true,
        false,
        &left,
        &lost_two,
        |fixture, merged_view, _| {
            single(merged_view, "/t/s/z");
            assert_eq!(read(fixture, "/t/x").as_deref(), Some("theirs x\n"));
            assert_eq!(read(fixture, "/t/y").as_deref(), Some("theirs y\n"));
            assert_eq!(read(fixture, "/t/s/z").as_deref(), Some("ours\n"));
        },
    );
    let mut lost_three = lost_two.to_vec();
    lost_three.extend([
        Change::Create("/t/s", b"theirs s\n"),
        Change::Mtime("/t/s", NEWER),
    ]);
    both_orders(
        "serial-three-links-lost",
        true,
        false,
        &left,
        &lost_three,
        |fixture, merged_view, _| {
            assert_eq!(below_t(merged_view), ["/t/s", "/t/x", "/t/y"]);
            for path in ["/t/x", "/t/y", "/t/s"] {
                single(merged_view, path);
            }
            assert_eq!(read(fixture, "/t/s").as_deref(), Some("theirs s\n"));
        },
    );
}

/// Without a `.lww` rule the new linked file meets the other side's new
/// file in a conflict, which the merge reports at that name.
#[test]
fn a_new_linked_file_in_a_conflict_is_reported() {
    for order in ORDERS {
        let fixture = link_tree(&format!("serial-link-conflict-{}", order[0]), false, false);
        let merged = merge_changes(
            &fixture,
            &[
                Change::Create("/t/x", b"ours\n"),
                Change::Mkdir("/t/s"),
                Change::Link("/t/x", "/t/s/x"),
            ],
            &[Change::Create("/t/x", b"theirs\n")],
            order,
        );
        let refusal = conflicted(merged);
        assert!(refusal.contains("/t/x"), "{order:?}: {refusal}");
        fixture.cleanup();
    }
}

/// What one plain new file `l` creates at `path` gives in each order, read
/// at `at`: the outcome a second link of that file must not change.
fn plain_outcome(
    label: &str,
    path: &str,
    right: &[Change],
    at: &'static str,
) -> Vec<([&'static str; 2], Option<String>)> {
    let seen = std::cell::RefCell::new(Vec::new());
    both_orders(
        label,
        true,
        true,
        &[Change::Create(path, b"ours\n")],
        right,
        |fixture, _, order| seen.borrow_mut().push((order, read(fixture, at))),
    );
    seen.into_inner()
}

fn outcome(seen: &[([&str; 2], Option<String>)], order: [&str; 2]) -> Option<String> {
    seen.iter()
        .find(|(known, _)| *known == order)
        .and_then(|(_, read)| read.clone())
}

/// `r` replaces the directory `/t/d` with a new one at the same name, so
/// one side's copy is merged into the other's by name; `l`'s new file
/// there meets `r`'s newer file. Its second link, outside the directory or
/// inside it, changes nothing at `/t/d/x` and keeps `l`'s file, though the
/// directory's names are read twice.
#[test]
fn a_new_link_in_a_merged_directory_is_lost_once() {
    let right = [
        Change::Unlink("/t/d/old"),
        Change::Rmdir("/t/d"),
        Change::Mkdir("/t/d"),
        Change::Create("/t/d/x", b"theirs\n"),
        Change::Mtime("/t/d/x", NEWER),
    ];
    let plain = plain_outcome("serial-absorbed-plain", "/t/d/x", &right, "/t/d/x");
    for other in ["/t/k", "/t/d/y"] {
        both_orders(
            &format!("serial-absorbed-link{}", other.replace('/', "-")),
            true,
            true,
            &[
                Change::Create("/t/d/x", b"ours\n"),
                Change::Link("/t/d/x", other),
            ],
            &right,
            |fixture, merged_view, order| {
                let at_x = read(fixture, "/t/d/x");
                assert_eq!(at_x, outcome(&plain, order));
                assert_eq!(read(fixture, other).as_deref(), Some("ours\n"));
                if at_x.as_deref() == Some("theirs\n") {
                    single(merged_view, other);
                }
            },
        );
    }
}

/// `r` replaces the existing directory `/t/d` with a newer file. Where that
/// file wins, `l`'s new file below the directory loses that name with it,
/// which `drop_new` does not see. Its other link keeps it when that name is
/// kept, and nothing holds it when that name is lost to a newer file too.
#[test]
fn a_new_link_below_a_replaced_directory() {
    let replace = [
        Change::Unlink("/t/d/old"),
        Change::Rmdir("/t/d"),
        Change::Create("/t/d", b"theirs d\n"),
        Change::Mtime("/t/d", NEWER),
    ];
    let plain = plain_outcome("serial-replaced-plain", "/t/d/y", &replace, "/t/d/y");
    // Whether `l`'s directory survives at `/t/d` in each order.
    let survives = |order: [&'static str; 2]| outcome(&plain, order).is_some();
    both_orders(
        "serial-replaced-dir-kept",
        true,
        true,
        &[
            Change::Create("/t/d/y", b"ours\n"),
            Change::Link("/t/d/y", "/t/k"),
        ],
        &replace,
        |fixture, merged_view, order| {
            assert_eq!(read(fixture, "/t/k").as_deref(), Some("ours\n"));
            if survives(order) {
                assert_eq!(read(fixture, "/t/d/y").as_deref(), Some("ours\n"));
            } else {
                assert_eq!(read(fixture, "/t/d").as_deref(), Some("theirs d\n"));
                single(merged_view, "/t/k");
            }
        },
    );
    let mut both_lost = replace.to_vec();
    both_lost.extend([
        Change::Create("/t/w", b"theirs w\n"),
        Change::Mtime("/t/w", NEWER),
    ]);
    both_orders(
        "serial-replaced-dir-lost",
        true,
        true,
        &[
            Change::Create("/t/d/y", b"ours\n"),
            Change::Link("/t/d/y", "/t/w"),
        ],
        &both_lost,
        |fixture, merged_view, order| {
            assert_eq!(read(fixture, "/t/w").as_deref(), Some("theirs w\n"));
            single(merged_view, "/t/w");
            if survives(order) {
                assert_eq!(read(fixture, "/t/d/y").as_deref(), Some("ours\n"));
                single(merged_view, "/t/d/y");
                assert_eq!(below_t(merged_view), ["/t/d", "/t/d/y", "/t/w"]);
            } else {
                assert_eq!(read(fixture, "/t/d").as_deref(), Some("theirs d\n"));
                single(merged_view, "/t/d");
                assert_eq!(below_t(merged_view), ["/t/d", "/t/w"]);
            }
        },
    );
}

/// The paths of a merge reply's `/.lww` decisions.
fn lww_paths(reply: &SdkValue) -> Vec<&str> {
    match reply.get("lww") {
        Some(SdkValue::List(lww)) => lww
            .iter()
            .map(|decision| decision.get_text("path").unwrap())
            .collect(),
        other => panic!("lww: {other:?}"),
    }
}

/// `r` replaces the directory `/t/d` with a file.
const REPLACED_BY_FILE: [Change<'static>; 4] = [
    Change::Unlink("/t/d/old"),
    Change::Rmdir("/t/d"),
    Change::Create("/t/d", b"theirs d\n"),
    Change::Mtime("/t/d", NEWER),
];

/// `l` added to `/t/d`, which `r` replaced with a file. Two directories are
/// the same entry, so `l` did not change the entry at `/t/d`: `r`'s file
/// wins in either order, with no `/.lww` decision and no conflict, and
/// `l`'s new file goes with the directory. That holds with `l`'s directory
/// newer and without a `/.lww` rule.
#[test]
fn a_directory_added_to_loses_to_the_file_that_replaced_it() {
    let added = [Change::Create("/t/d/y", b"ours\n")];
    let newer = [
        Change::Create("/t/d/y", b"ours\n"),
        Change::Mtime("/t/d", NEWER + 1),
    ];
    for (label, rules, left) in [
        ("serial-dir-file", true, &added[..]),
        ("serial-dir-file-newer-dir", true, &newer[..]),
        ("serial-dir-file-no-rule", false, &added[..]),
    ] {
        each_order(
            label,
            rules,
            true,
            left,
            &REPLACED_BY_FILE,
            |fixture, reply, merged_view, _| {
                assert_eq!(below_t(merged_view), ["/t/d"]);
                assert_eq!(read(fixture, "/t/d").as_deref(), Some("theirs d\n"));
                assert!(lww_paths(reply).is_empty(), "{reply:?}");
            },
        );
    }
}

/// `r` replaces `/t/d` with a new directory holding a newer `x`; `l` adds
/// an older `x` and a `y` to the old one. In either order the two
/// directories are merged by name: `/.lww` keeps `r`'s `x`, `l`'s `y`
/// stays and `r`'s removal of `old` holds.
#[test]
fn a_directory_added_to_against_a_new_directory_merges_by_name() {
    each_order(
        "serial-dir-dir-lww",
        true,
        true,
        &[
            Change::Create("/t/d/x", b"ours\n"),
            Change::Create("/t/d/y", b"ours y\n"),
        ],
        &[
            Change::Unlink("/t/d/old"),
            Change::Rmdir("/t/d"),
            Change::Mkdir("/t/d"),
            Change::Create("/t/d/x", b"theirs\n"),
            Change::Mtime("/t/d/x", NEWER),
        ],
        |fixture, reply, merged_view, _| {
            assert_eq!(below_t(merged_view), ["/t/d", "/t/d/x", "/t/d/y"]);
            assert_eq!(read(fixture, "/t/d/x").as_deref(), Some("theirs\n"));
            assert_eq!(read(fixture, "/t/d/y").as_deref(), Some("ours y\n"));
            assert_eq!(lww_paths(reply), ["/t/d/x"]);
        },
    );
}

/// As above, but `l` saves a new two-link file over `/t/d/old`, a name the
/// base held, and `r`'s new directory holds a newer `old`. The merged
/// directories decide `old` once in either order: `/.lww` keeps `r`'s, and
/// `l`'s file stays at its other name, outside the directory or inside it.
#[test]
fn a_new_link_over_a_base_name_in_a_merged_directory_is_lost_once() {
    let right = [
        Change::Unlink("/t/d/old"),
        Change::Rmdir("/t/d"),
        Change::Mkdir("/t/d"),
        Change::Create("/t/d/old", b"theirs\n"),
        Change::Mtime("/t/d/old", NEWER),
    ];
    for other in ["/t/k", "/t/d/z"] {
        each_order(
            &format!("serial-absorbed-base-link{}", other.replace('/', "-")),
            true,
            true,
            &[
                Change::SaveOver("/t/d/old", b"ours\n"),
                Change::Link("/t/d/old", other),
            ],
            &right,
            |fixture, reply, merged_view, _| {
                assert_eq!(read(fixture, "/t/d/old").as_deref(), Some("theirs\n"));
                assert_eq!(read(fixture, other).as_deref(), Some("ours\n"));
                single(merged_view, "/t/d/old");
                single(merged_view, other);
                assert_eq!(lww_paths(reply), ["/t/d/old"]);
            },
        );
    }
}

/// `l` replaces `/shared.txt` with a new file that it also links at
/// `/uncontested.txt`; `r` edits `/shared.txt` and never touches the other
/// name. Merging `branch` with `with` gives `/shared.txt` the merged text, a
/// file of its own, and leaves `/uncontested.txt` the bytes `l` wrote, as one
/// link.
fn merge_beside_a_new_link(branch: &str, with: &str) {
    let fixture = Fixture::new(&format!("serial-link-beside-{branch}"), |source| {
        fs::write(source.join("shared.txt"), b"line1\nline2\nline3\n").unwrap();
        1
    });
    fixture.fork("l");
    fixture.fork("r");
    fixture.apply("l", vec![rm("/shared.txt")]);
    fixture.workspace_commit(
        "l",
        &[
            Change::Create("/uncontested.txt", b"OUR_LINE1\nline2\nline3\n"),
            Change::Link("/uncontested.txt", "/shared.txt"),
        ],
        1,
        || {},
    );
    fixture.apply(
        "r",
        vec![write("/shared.txt", "line1\nline2\nTHEIR_LINE3\n")],
    );
    let merged = fixture.try_call(
        "branch.merge",
        vec![
            ("branch", text(branch)),
            ("with", text(with)),
            ("name", text("m")),
        ],
    );
    let root = merged_root(merged);
    let merged_view = view(&fixture, &root);
    assert_eq!(
        read(&fixture, "/shared.txt").as_deref(),
        Some("OUR_LINE1\nline2\nTHEIR_LINE3\n")
    );
    assert_eq!(
        read(&fixture, "/uncontested.txt").as_deref(),
        Some("OUR_LINE1\nline2\nline3\n")
    );
    for path in ["/shared.txt", "/uncontested.txt"] {
        let found = line(&merged_view, path).expect("both names stay");
        assert!(!found.contains(" link="), "{found}");
        assert_eq!(viewer(&fixture).links(&root, path), 1, "{path}");
    }
    fixture.cleanup();
}

/// `l` is merged with `r`: the merge reads the changes of the branch that
/// made the new file.
#[test]
fn text_merged_at_one_link_of_a_new_file_leaves_its_other_link_alone() {
    merge_beside_a_new_link("l", "r");
}

/// `r` is merged with `l`: the merge reads the changes of the branch that
/// only edited.
#[test]
fn text_merged_at_one_link_of_a_new_file_leaves_its_other_link_alone_in_the_other_order() {
    merge_beside_a_new_link("r", "l");
}

/// The new file linked at `/a.txt` and `/b.txt`, both edited by `r`: each
/// name holds its own merged text and the file keeps none of its own bytes.
/// Two merged texts that differ are two files; two that are equal stay one
/// file with two links.
#[test]
fn a_new_file_with_every_name_merged_keeps_none_of_its_own_bytes() {
    for (label, edits, merged_texts, links) in [
        (
            "serial-links-merged-apart",
            ["1\n2\nA3\n", "1\n2\nB3\n"],
            ["O1\n2\nA3\n", "O1\n2\nB3\n"],
            1,
        ),
        (
            "serial-links-merged-alike",
            ["1\n2\nT3\n", "1\n2\nT3\n"],
            ["O1\n2\nT3\n", "O1\n2\nT3\n"],
            2,
        ),
    ] {
        let fixture = Fixture::new(label, |source| {
            fs::write(source.join("a.txt"), b"1\n2\n3\n").unwrap();
            fs::write(source.join("b.txt"), b"1\n2\n3\n").unwrap();
            2
        });
        fixture.fork("l");
        fixture.fork("r");
        fixture.apply("l", vec![rm("/a.txt"), rm("/b.txt")]);
        fixture.workspace_commit(
            "l",
            &[
                Change::Create("/a.txt", b"O1\n2\n3\n"),
                Change::Link("/a.txt", "/b.txt"),
            ],
            1,
            || {},
        );
        fixture.apply(
            "r",
            vec![write("/a.txt", edits[0]), write("/b.txt", edits[1])],
        );
        let merged = fixture.try_call(
            "branch.merge",
            vec![
                ("branch", text("l")),
                ("with", text("r")),
                ("name", text("m")),
            ],
        );
        let root = merged_root(merged);
        let merged_view = view(&fixture, &root);
        for (path, merged_text) in ["/a.txt", "/b.txt"].into_iter().zip(merged_texts) {
            let case = format!("{label} {path}");
            assert_eq!(read(&fixture, path).as_deref(), Some(merged_text), "{case}");
            let found = line(&merged_view, path).expect("both names stay");
            assert_eq!(found.contains(" link="), links == 2, "{case}: {found}");
            assert_eq!(viewer(&fixture).links(&root, path), links, "{case}");
        }
        fixture.cleanup();
    }
}

/// A Workspace creates `/conflict.txt` and links it at `/clean_alias.txt`
/// while another writer adds a different `/conflict.txt` to the Branch. The
/// Commit conflicts at `/conflict.txt` alone and publishes nothing. The root
/// it installs in the Workspace holds the markers at `/conflict.txt` and
/// leaves `/clean_alias.txt` the bytes the Workspace wrote, as one link.
#[test]
fn a_conflict_at_one_link_of_a_new_file_leaves_its_other_link_alone() {
    let fixture = Fixture::new("serial-link-conflict", |source| {
        fs::write(source.join("readme.txt"), b"base\n").unwrap();
        1
    });
    fixture.fork("work");
    let committed = fixture.workspace_commit(
        "work",
        &[
            Change::Create("/conflict.txt", b"our new file\n"),
            Change::Link("/conflict.txt", "/clean_alias.txt"),
        ],
        1,
        || {
            fixture.apply("work", vec![write("/conflict.txt", "their new file\n")]);
        },
    );
    let report = committed
        .merge
        .as_ref()
        .expect("the head moved: a merge ran");
    let conflicts: Vec<_> = report.conflicts.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(conflicts, ["/conflict.txt"]);
    // Nothing is published: the Branch keeps only the other writer's file.
    assert!(committed.history.is_none(), "{:?}", committed.history);
    assert_eq!(
        read_at(&fixture, "work", "/conflict.txt").as_deref(),
        Some("their new file\n")
    );
    assert_eq!(read_at(&fixture, "work", "/clean_alias.txt"), None);
    // The Workspace's own root binds one file at both names; the root the
    // merge installed in its place binds a file of its own at each.
    let written = report.written.clone();
    let (shown, local) = committed
        .replaced
        .expect("a conflicted merge installs a root");
    let (shown, local) = (view::hex(shown.as_bytes()), view::hex(local.as_bytes()));
    let store = viewer(&fixture);
    let shown_view = store.view(&shown, false);
    for path in ["/conflict.txt", "/clean_alias.txt"] {
        let found = line(&shown_view, path).expect("both names bound");
        assert!(found.ends_with(" link=/clean_alias.txt"), "{found}");
    }
    let local_view = store.view(&local, false);
    assert_eq!(store.text(&local, "/clean_alias.txt"), "our new file\n");
    let marked = store.text(&local, "/conflict.txt");
    assert!(
        marked
            .starts_with("<<<<<<< workspace\nour new file\n=======\ntheir new file\n>>>>>>> work@"),
        "{marked}"
    );
    for path in ["/conflict.txt", "/clean_alias.txt"] {
        let found = line(&local_view, path).expect("both names stay");
        assert!(!found.contains(" link="), "{found}");
        assert_eq!(store.links(&local, path), 1, "{path}");
    }
    // Only the conflicted name is reported as written into the Workspace.
    assert_eq!(written, ["/conflict.txt"]);
    fixture.cleanup();
}

#[test]
fn a_one_sided_mode_change_is_kept() {
    let fixture = seeded("serial-chmod");
    fixture.fork("l");
    fixture.fork("r");
    fixture.chmod("l", "/top.txt", 0o700);
    fixture.apply("r", vec![write("/d2/c.txt", "c2\n")]);
    let merged = fixture.try_call(
        "branch.merge",
        vec![
            ("branch", text("l")),
            ("with", text("r")),
            ("name", text("m")),
        ],
    );
    let merged_view = merged_view(&fixture, merged);
    let top = line(&merged_view, "/top.txt").expect("top stays");
    assert!(top.ends_with(" mode=700"), "{top}");
    assert_eq!(read(&fixture, "/d2/c.txt").as_deref(), Some("c2\n"));
    fixture.cleanup();
}
