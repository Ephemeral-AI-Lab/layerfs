//! #360's two metadata forms through the merge by serial (#355 M1). The base
//! and Branch `r` come from a root-form Store sealed before #360 item 2
//! (`fixtures/merge-forms-root.sqlite`, written by `merge_forms_fixture`).
//! Branch `l` is changed here, so each inode it touches is rewritten in the
//! inline form:
//!
//! - form only: `/f/same.txt`, `/f/rmode.txt` (against `r`'s real mode
//!   change), `/f/other.txt` (against `r`'s edit) and the hard-linked
//!   `/h/one`;
//! - a real mode change: `/f/mode.txt`;
//! - content edits with the form change: `/f/edit.txt`, `/f/both.txt`
//!   (against `r`'s edit of another line) and `/d1/a.txt`, which `r` moved to
//!   `/moved`.
//!
//! Both merge orientations must equal, by decoded view, `r`'s tree with
//! `l`'s real changes applied: a form-only difference is neither a conflict
//! nor a change, and loses no change of the other side.
//!
//! The delta-form side (#360 item 3): the fixture's table is one leaf, so a
//! second case widens it to 400 files, then changes both sides in small
//! Commits, so each side's inode-table branches are stored as deltas against
//! the wide table's. Merges over delta-form branches equal the decoded
//! expectation the same way.
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
use layerfs_content::inode_leaf::{InodeKind, InodeMetadata};
use layerfs_content::object::ObjectId;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::Storage;
use std::collections::BTreeMap;
use std::path::Path;
use support::{write, Fixture};

/// Entries below the root of the sealed base tree.
const ENTRIES: usize = 14;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/merge-forms-root.sqlite"
);

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

/// Each path's metadata form, `inline` or `root`.
fn forms(fixture: &Fixture, root: &str) -> BTreeMap<String, &'static str> {
    let config = PersistenceConfig::sqlite(&fixture.host.config.path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable);
    let handles = Handles::open_read_only(config, installed::BINDING, installed::CURSOR).unwrap();
    let storage = Storage::new(handles.storage).unwrap();
    let reader = storage.reader().unwrap();
    let root = FilesystemRootId(ObjectId::from_bytes(&unhex(root)).unwrap());
    let tree = TreeTable::open(&reader, root).unwrap();
    let mut out = BTreeMap::new();
    let mut stack = vec![("/".to_owned(), tree.root_serial())];
    while let Some((path, serial)) = stack.pop() {
        let value = tree.value(&reader, serial).unwrap().expect("bound serial");
        let form = match value.metadata_root {
            InodeMetadata::Inline(_) => "inline",
            InodeMetadata::Root(_) => "root",
        };
        out.insert(path.clone(), form);
        if value.kind == InodeKind::Directory {
            list_directory(&reader, value.content_root, |name, child| {
                stack.push((join(&path, name), child));
                Ok(true)
            })
            .unwrap();
        }
    }
    out
}

/// The decoded view, as a map from path to the rest of its line.
fn view(fixture: &Fixture, root: &str) -> BTreeMap<String, String> {
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

fn read(fixture: &Fixture, branch: &str, path: &str) -> String {
    fixture
        .call(
            "fs.read",
            vec![("branch", text(branch)), ("path", text(path))],
        )
        .get_text("text")
        .unwrap()
        .to_owned()
}

/// `l`'s path under `r`'s move of `/d1` to `/moved`.
fn moved(path: &str) -> String {
    match path.strip_prefix("/d1/") {
        Some(rest) => format!("/moved/{rest}"),
        None => path.to_owned(),
    }
}

#[test]
fn inline_changes_merge_against_a_root_form_tree() {
    let fixture = Fixture::open(installed::Fixture::cloned(
        "merge-mixed-forms",
        Path::new(FIXTURE),
        ENTRIES,
    ));
    fixture.fork("l");
    // A chmod there and back in two Commits: the row is rewritten inline
    // with the base's mode and mtime (chmod leaves the mtime), a form-only
    // change. Within one Commit the round trip captures nothing.
    let paths = ["/f/same.txt", "/f/rmode.txt", "/f/other.txt", "/h/one"];
    let mut there: Vec<(&str, u32)> = paths.iter().map(|path| (*path, 0o600)).collect();
    there.push(("/f/mode.txt", 0o600));
    fixture.chmods("l", &there, 78);
    let back: Vec<(&str, u32)> = paths.iter().map(|path| (*path, 0o644)).collect();
    fixture.chmods("l", &back, 79);
    fixture.apply(
        "l",
        vec![
            write("/f/edit.txt", "1\n2\nedit, l\n"),
            write("/f/both.txt", "both, l\n2\n3\n"),
            write("/d1/a.txt", "a, l\n"),
        ],
    );
    let (base, l, r) = (
        branch_root(&fixture, "main"),
        branch_root(&fixture, "l"),
        branch_root(&fixture, "r"),
    );

    // The premise: the base and `r` are root form, and `l` rewrote what it
    // touched inline, form-only changes included.
    let (base_forms, l_forms, r_forms) = (
        forms(&fixture, &base),
        forms(&fixture, &l),
        forms(&fixture, &r),
    );
    assert!(
        base_forms.values().all(|form| *form == "root"),
        "{base_forms:?}"
    );
    for path in [
        "/f/same.txt",
        "/f/rmode.txt",
        "/f/other.txt",
        "/h/one",
        "/f/mode.txt",
        "/f/edit.txt",
        "/f/both.txt",
        "/d1/a.txt",
    ] {
        assert_eq!(l_forms[path], "inline", "l {path}: {l_forms:?}");
    }
    assert!(r_forms.values().all(|form| *form == "root"), "{r_forms:?}");

    // Decoded, `l`'s form-only changes are no changes.
    let (base_view, l_view, r_view) = (
        view(&fixture, &base),
        view(&fixture, &l),
        view(&fixture, &r),
    );
    let l_changed: Vec<&str> = l_view
        .iter()
        .filter(|(path, rest)| base_view.get(*path) != Some(*rest))
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(
        l_changed,
        ["/d1/a.txt", "/f/both.txt", "/f/edit.txt", "/f/mode.txt"],
        "{l_view:?}"
    );

    // Expected: `r`'s tree with `l`'s real changes, `/f/both.txt` merged.
    let mut expected = r_view.clone();
    for path in &l_changed {
        expected.insert(moved(path), l_view[*path].clone());
    }
    expected.remove("/f/both.txt");
    for (name, branch, with) in [("m-lr", "l", "r"), ("m-rl", "r", "l")] {
        let merged = fixture
            .try_call(
                "branch.merge",
                vec![
                    ("branch", text(branch)),
                    ("with", text(with)),
                    ("name", text(name)),
                ],
            )
            .unwrap_or_else(|refusal| panic!("{name} conflicted: {refusal:?}"));
        let mut merged_view = view(&fixture, merged.get_text("root").unwrap());
        // The text merge takes one side's metadata: checked by content.
        let both = merged_view.remove("/f/both.txt").expect("both merged");
        assert_eq!(merged_view, expected, "{name}");
        assert!(both.contains(" mode=644"), "{name}: {both}");
        assert_eq!(read(&fixture, name, "/f/both.txt"), "both, l\n2\nboth, r\n");
        assert!(
            merged_view["/h/two"].ends_with(" link=/h/one"),
            "{name}: {merged_view:?}"
        );
    }
    fixture.cleanup();
}

/// Files the wide table adds: several inode leaves under one root branch.
const WIDE: usize = 400;

fn wide(index: usize) -> String {
    format!("/w/f{index:03}.txt")
}

#[test]
fn changes_merge_over_delta_form_inode_branches() {
    let fixture = Fixture::open(installed::Fixture::cloned(
        "merge-delta-forms",
        Path::new(FIXTURE),
        ENTRIES,
    ));
    fixture.call(
        "branch.fork",
        vec![("name", text("w")), ("from", text("main"))],
    );
    fixture.apply(
        "w",
        std::iter::once(support::operation("mkdir", "/w"))
            .chain((0..WIDE).map(|index| write(&wide(index), format!("w {index}\n"))))
            .collect(),
    );
    for side in ["dl", "dr"] {
        fixture.call(
            "branch.fork",
            vec![("name", text(side)), ("from", text("w"))],
        );
    }
    // `dl`: a real mode change and an edit, in two Commits. `dr`: an edit at
    // the other end of the table and one in the middle, in two Commits.
    fixture.chmods("dl", &[(wide(1).as_str(), 0o600)], 80);
    fixture.apply("dl", vec![write(&wide(2), "2, dl\n")]);
    fixture.apply("dr", vec![write(&wide(WIDE - 1), "last, dr\n")]);
    fixture.apply("dr", vec![write(&wide(WIDE / 2), "middle, dr\n")]);
    let (w, dl, dr) = (
        branch_root(&fixture, "w"),
        branch_root(&fixture, "dl"),
        branch_root(&fixture, "dr"),
    );
    // The premise: both sides' branches are delta-stored.
    for (side, root) in [("dl", &dl), ("dr", &dr)] {
        assert!(
            branch_delta_applies(&fixture, root) >= 1,
            "{side} holds delta branches"
        );
    }
    let (w_view, dl_view, dr_view) = (view(&fixture, &w), view(&fixture, &dl), view(&fixture, &dr));
    let dl_changed: Vec<&str> = dl_view
        .iter()
        .filter(|(path, rest)| w_view.get(*path) != Some(*rest))
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(
        dl_changed,
        [wide(1).as_str(), wide(2).as_str()],
        "{dl_view:?}"
    );
    let mut expected = dr_view.clone();
    for path in &dl_changed {
        expected.insert(path.to_string(), dl_view[*path].clone());
    }
    for (name, branch, with) in [("dm-lr", "dl", "dr"), ("dm-rl", "dr", "dl")] {
        let merged = fixture
            .try_call(
                "branch.merge",
                vec![
                    ("branch", text(branch)),
                    ("with", text(with)),
                    ("name", text(name)),
                ],
            )
            .unwrap_or_else(|refusal| panic!("{name} conflicted: {refusal:?}"));
        let root = merged.get_text("root").unwrap().to_owned();
        assert_eq!(view(&fixture, &root), expected, "{name}");
        assert_eq!(read(&fixture, name, &wide(2)), "2, dl\n");
        assert_eq!(read(&fixture, name, &wide(WIDE - 1)), "last, dr\n");
    }
    fixture.cleanup();
}
