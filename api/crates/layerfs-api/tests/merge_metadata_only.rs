//! A metadata-only change on one side of a merge, against a content edit of
//! the same file on the other (#352 F-052). One side chmods file `f`; the
//! other rewrites its bytes. Both changes must survive, without a conflict:
//! the merged `f` has the edited content and the changed mode. Both sides
//! chmodding `f` to different modes stays a conflict.
//!
//! Each case runs both orientations through both merge routes: the SDK
//! `branch.merge`, and a Workspace Commit whose head moved after it bound
//! (merge-on-commit). Bases: the root form of a Store sealed before #360
//! (`fixtures/merge-forms-root.sqlite`), the inline form written by this
//! build, and inline rows in delta-stored inode branches (#360 item 3). One
//! case changes a hard-linked file through one name and edits it through the
//! other.
//!
//! An mtime-only change is not a change at Commit (#360 item 2, §4.8): the
//! Workspace keeps its live row and the Commit answers UpToDate, so such a
//! change never reaches a merge. The last test pins that rule.
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
use layerfs_content::compare::tree::{join, list_directory, TreeTable};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_content::inode_leaf::InodeKind;
use layerfs_content::merge::ConflictKind;
use layerfs_content::object::ObjectId;
use layerfs_history::CommitOutcome;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::Storage;
use std::collections::BTreeMap;
use std::path::Path;
use support::{write, Change, Fixture};

/// Entries below the root of the sealed base tree.
const ENTRIES: usize = 14;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/merge-forms-root.sqlite"
);

/// Files of the delta-stored table: several inode leaves under one branch.
const WIDE: usize = 400;

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        text.push_str(&format!("{byte:02x}"));
        text
    })
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn open(label: &str) -> Fixture {
    Fixture::open(installed::Fixture::cloned(
        label,
        Path::new(FIXTURE),
        ENTRIES,
    ))
}

/// A Branch's effective root (hex): its head's, else its base Layer's.
fn branch_root(fixture: &Fixture, branch: &str) -> String {
    let sql = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    let root: Vec<u8> = sql
        .query_row(
            "SELECT coalesce(
                 (SELECT root_id FROM \"commit\" WHERE commit_id = b.head_commit_id),
                 (SELECT root_id FROM layer WHERE layer_id = b.base_layer_id))
             FROM branch b WHERE b.name = ?1",
            [branch],
            |row| row.get(0),
        )
        .unwrap();
    hex(&root)
}

/// The decoded view of a root, path to the rest of its line.
fn view_of(fixture: &Fixture, root: &str) -> BTreeMap<String, String> {
    view::Viewer::open(
        &fixture.host.config.path,
        installed::BINDING,
        installed::CURSOR,
    )
    .view(root, true)
    .into_iter()
    .map(|line| {
        let (path, rest) = line.split_once(' ').unwrap();
        (path.to_owned(), rest.to_owned())
    })
    .collect()
}

fn view(fixture: &Fixture, branch: &str) -> BTreeMap<String, String> {
    view_of(fixture, &branch_root(fixture, branch))
}

/// Inode-branch deltas applied while every inode of the tree at `root` is
/// read through one fresh Store reader.
fn branch_delta_applies(fixture: &Fixture, root: &str) -> u64 {
    let config = PersistenceConfig::sqlite(&fixture.host.config.path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable);
    let handles = Handles::open_read_only(config, installed::BINDING, installed::CURSOR).unwrap();
    let storage = Storage::new(handles.storage).unwrap();
    let reader = storage.reader().unwrap();
    let root = FilesystemRootId(ObjectId::from_bytes(&unhex(root)).unwrap());
    let tree = TreeTable::open(&reader, root).unwrap();
    let mut stack = vec![("/".to_owned(), tree.root_serial())];
    while let Some((path, serial)) = stack.pop() {
        let value = tree.value(&reader, serial).unwrap().expect("bound serial");
        if value.kind == InodeKind::Directory {
            list_directory(&reader, value.content_root, |name, child| {
                stack.push((join(&path, name), child));
                Ok(true)
            })
            .unwrap();
        }
    }
    reader.pooled_read_counters().branch_delta_applies
}

fn fork(fixture: &Fixture, name: &str, from: &str) {
    fixture.call(
        "branch.fork",
        vec![("name", text(name)), ("from", text(from))],
    );
}

fn merge(fixture: &Fixture, branch: &str, with: &str, name: &str) -> Result<String, String> {
    fixture
        .try_call(
            "branch.merge",
            vec![
                ("branch", text(branch)),
                ("with", text(with)),
                ("name", text(name)),
            ],
        )
        .map(|merged| merged.get_text("root").unwrap().to_owned())
        .map_err(|refusal| format!("{refusal:?}"))
}

/// One view line's fields after the path, by name: `kind`, `content`,
/// `mode`, `mtime` and `link`.
fn field<'l>(line: &'l str, name: &str) -> Option<&'l str> {
    let mut parts = line.split(' ');
    match name {
        "kind" => parts.next(),
        "content" => parts.nth(1),
        _ => parts.find_map(|part| part.strip_prefix(&format!("{name}=")[..])),
    }
}

/// The base a case forks from, and the file (and its other name) it changes.
struct Case {
    name: &'static str,
    base: &'static str,
    path: String,
    other_name: Option<&'static str>,
}

const EDITED: &str = "edited\n2\n3\n";

/// The merged `f` must carry the edited content and the chmod's mode, and
/// any other name of `f` the same; the rest of the tree is the edit side's.
fn check(
    case: &Case,
    label: &str,
    merged: &BTreeMap<String, String>,
    edit: &BTreeMap<String, String>,
) {
    let names: Vec<&str> = std::iter::once(case.path.as_str())
        .chain(case.other_name)
        .collect();
    for name in &names {
        let line = merged
            .get(*name)
            .unwrap_or_else(|| panic!("{label}: {name} missing: {merged:?}"));
        assert_eq!(field(line, "kind"), Some("file"), "{label}: {line}");
        assert_eq!(
            field(line, "content"),
            field(&edit[*name], "content"),
            "{label}: {name} keeps the edit: {line}"
        );
        assert_eq!(
            field(line, "mode"),
            Some("600"),
            "{label}: {name} keeps the chmod: {line}"
        );
    }
    if let Some(other) = case.other_name {
        assert!(
            merged[other].ends_with(&format!(" link={}", case.path)),
            "{label}: the link group stays: {merged:?}"
        );
    }
    for (path, line) in edit {
        if !names.contains(&path.as_str()) {
            assert_eq!(merged.get(path), Some(line), "{label}: {path}");
        }
    }
}

/// A Commit route: its label, Workspace id, Workspace changes and the head
/// move made before the Commit.
type Route<'a> = (&'a str, u8, Vec<Change<'a>>, &'a dyn Fn(&str));

fn run(fixture: &Fixture, case: &Case) {
    let (chmod, edit) = (format!("{}-c", case.name), format!("{}-e", case.name));
    fork(fixture, &chmod, case.base);
    fork(fixture, &edit, case.base);
    fixture.chmods(&chmod, &[(case.path.as_str(), 0o600)], 81);
    let edited = case.other_name.unwrap_or(case.path.as_str());
    fixture.apply(&edit, vec![write(edited, EDITED)]);
    let edit_view = view(fixture, &edit);
    assert_eq!(
        field(&view(fixture, &chmod)[&case.path], "mode"),
        Some("600")
    );

    // The SDK merge, both orientations.
    for (label, branch, with) in [("sdk-ce", &chmod, &edit), ("sdk-ec", &edit, &chmod)] {
        let merged = format!("{}-{label}", case.name);
        let root = merge(fixture, branch, with, &merged)
            .unwrap_or_else(|refusal| panic!("{merged} conflicted: {refusal}"));
        check(case, &merged, &view_of(fixture, &root), &edit_view);
    }

    // A Workspace Commit that merges with a moved head, both orientations:
    // the Workspace chmods while the head takes the edit, and the reverse.
    let routes: [Route; 2] = [
        (
            "commit-c",
            83,
            vec![Change::Mode(&case.path, 0o600)],
            &|branch| {
                fixture.apply(branch, vec![write(edited, EDITED)]);
            },
        ),
        (
            "commit-e",
            89,
            vec![Change::Write(edited, EDITED.as_bytes())],
            &|branch| {
                fixture.chmods(branch, &[(case.path.as_str(), 0o600)], 82);
            },
        ),
    ];
    for (label, id, changes, before) in routes {
        let branch = format!("{}-{label}", case.name);
        fork(fixture, &branch, case.base);
        let committed = fixture.workspace_commit(&branch, &changes, id, || before(&branch));
        let report = committed
            .merge
            .as_ref()
            .expect("the head moved: a merge ran");
        assert!(
            report.conflicts.is_empty(),
            "{} {label} conflicted: {:?}",
            case.name,
            report.conflicts
        );
        assert!(
            matches!(committed.history, Some(CommitOutcome::Committed { .. })),
            "{} {label}: {:?}",
            case.name,
            committed.history
        );
        let merged = view(fixture, &branch);
        let names: Vec<&str> = std::iter::once(case.path.as_str())
            .chain(case.other_name)
            .collect();
        for name in names {
            let line = &merged[name];
            assert_eq!(
                field(line, "content"),
                field(&edit_view[name], "content"),
                "{} {label}: {name} keeps the edit: {line}",
                case.name
            );
            assert_eq!(
                field(line, "mode"),
                Some("600"),
                "{} {label}: {name} keeps the chmod: {line}",
                case.name
            );
        }
    }
}

#[test]
fn a_chmod_survives_a_content_edit_of_a_root_form_file() {
    let fixture = open("metadata-only-root");
    run(
        &fixture,
        &Case {
            name: "root",
            base: "main",
            path: "/f/same.txt".into(),
            other_name: None,
        },
    );
    fixture.cleanup();
}

#[test]
fn a_chmod_survives_a_content_edit_of_an_inline_file() {
    let fixture = open("metadata-only-inline");
    fork(&fixture, "inl", "main");
    fixture.apply("inl", vec![write("/f/edit.txt", "i\n2\n3\n")]);
    run(
        &fixture,
        &Case {
            name: "inline",
            base: "inl",
            path: "/f/edit.txt".into(),
            other_name: None,
        },
    );
    fixture.cleanup();
}

#[test]
fn a_chmod_survives_a_content_edit_in_delta_stored_branches() {
    let fixture = open("metadata-only-delta");
    fork(&fixture, "w", "main");
    fixture.apply(
        "w",
        std::iter::once(support::operation("mkdir", "/w"))
            .chain(
                (0..WIDE)
                    .map(|index| write(&format!("/w/f{index:03}.txt"), format!("w {index}\n"))),
            )
            .collect(),
    );
    // Commits on top of `w` store their inode branches as deltas.
    fork(&fixture, "wd", "w");
    fixture.apply("wd", vec![write("/w/f399.txt", "w 399, again\n")]);
    assert!(
        branch_delta_applies(&fixture, &branch_root(&fixture, "wd")) >= 1,
        "wd holds delta branches"
    );
    run(
        &fixture,
        &Case {
            name: "delta",
            base: "wd",
            path: "/w/f001.txt".into(),
            other_name: None,
        },
    );
    fixture.cleanup();
}

#[test]
fn a_chmod_through_one_link_survives_an_edit_through_the_other() {
    let fixture = open("metadata-only-link");
    run(
        &fixture,
        &Case {
            name: "link",
            base: "main",
            path: "/h/one".into(),
            other_name: Some("/h/two"),
        },
    );
    fixture.cleanup();
}

#[test]
fn chmods_to_different_modes_on_both_sides_conflict() {
    let fixture = open("metadata-only-both");
    for (side, mode, id) in [("bl", 0o600, 84), ("br", 0o700, 85)] {
        fork(&fixture, side, "main");
        fixture.chmods(side, &[("/f/rmode.txt", mode)], id);
    }
    for (branch, with) in [("bl", "br"), ("br", "bl")] {
        let refusal = merge(&fixture, branch, with, &format!("m-{branch}"))
            .expect_err("different modes on both sides conflict");
        assert!(refusal.contains("Conflict"), "{refusal}");
        assert!(refusal.contains("/f/rmode.txt"), "{refusal}");
        assert!(refusal.contains(r#"("kind", Text("mode"))"#), "{refusal}");
    }
    fork(&fixture, "bw", "main");
    let committed =
        fixture.workspace_commit("bw", &[Change::Mode("/f/rmode.txt", 0o600)], 86, || {
            fixture.chmods("bw", &[("/f/rmode.txt", 0o700)], 87);
        });
    let report = committed.merge.expect("the head moved: a merge ran");
    assert!(
        report
            .conflicts
            .iter()
            .any(|conflict| conflict.path == "/f/rmode.txt" && conflict.kind == ConflictKind::Mode),
        "{:?}",
        report.conflicts
    );
    fixture.cleanup();
}

#[test]
fn an_mtime_only_change_is_no_change_at_commit() {
    let fixture = open("metadata-only-mtime");
    fork(&fixture, "t", "main");
    let committed = fixture.workspace_commit(
        "t",
        &[Change::Mtime("/f/mode.txt", 2_000_000_000)],
        88,
        || {},
    );
    assert!(
        matches!(committed.history, Some(CommitOutcome::UpToDate { .. })),
        "{:?}",
        committed.history
    );
    fixture.cleanup();
}
