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

fn view(fixture: &Fixture, root: &str) -> Vec<String> {
    view::Viewer::open(
        &fixture.host.config.path,
        installed::BINDING,
        installed::CURSOR,
    )
    .view(root, false)
}

fn merged_view(fixture: &Fixture, merged: Result<SdkValue, SdkRefusal>) -> Vec<String> {
    let reply = merged.unwrap_or_else(|refusal| panic!("merge refused: {refusal:?}"));
    view(fixture, reply.get_text("root").unwrap())
}

/// The view's line for `path`, if the path exists.
fn line<'v>(view: &'v [String], path: &str) -> Option<&'v str> {
    view.iter()
        .map(String::as_str)
        .find(|line| line.split(' ').next() == Some(path))
}

fn read(fixture: &Fixture, path: &str) -> Option<String> {
    fixture
        .try_call("fs.read", vec![("branch", text("m")), ("path", text(path))])
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
