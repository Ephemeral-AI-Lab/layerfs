//! A resumed Commit difference lists what the whole walk lists after its
//! cursor (#364 item 4). A difference of more than `MAXIMUM_CHANGED` inodes
//! takes the full walk on every page, which read each directory on the resume
//! path from its first name; it now looks the resume name up on each side and
//! lists after it, as the planned walk resumes. The edges: a name on one side
//! only, a file written again with the same bytes (a new inode), equal content
//! with another mode or time, symlinks, and directories removed, added,
//! recreated or changed in metadata only.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_content::compare::{ChangeWalk, Pair};
use layerfs_content::filesystem::PathName;
use layerfs_store_service::history::lookup;
use layerfs_store_service::read::snapshot;
use std::fs;
use support::{items, operation, write, Change, Fixture};

fn tree(source: &std::path::Path) -> usize {
    for dir in ["a/sub", "b", "b2", "c/deep"] {
        fs::create_dir_all(source.join(dir)).unwrap();
    }
    for f in 0..8 {
        fs::write(source.join(format!("a/f{f}")), format!("a {f}\n")).unwrap();
    }
    for g in 0..3 {
        fs::write(source.join(format!("a/sub/g{g}")), format!("g {g}\n")).unwrap();
        fs::write(source.join(format!("b/f{g}")), format!("b {g}\n")).unwrap();
        fs::write(source.join(format!("b2/f{g}")), format!("b2 {g}\n")).unwrap();
    }
    fs::write(source.join("c/deep/x"), b"x\n").unwrap();
    fs::write(source.join("z"), b"z\n").unwrap();
    std::os::unix::fs::symlink("f0", source.join("a/link")).unwrap();
    std::os::unix::fs::symlink("f1", source.join("a/still")).unwrap();
    std::os::unix::fs::symlink("..", source.join("a/sub/up")).unwrap();
    // a, sub, b, b2, c, deep: 6; files 8 + 9 + 1 + 1; links 3
    6 + 8 + 9 + 1 + 1 + 3
}

fn head(reply: &SdkValue) -> String {
    reply.get_text("head").unwrap().to_owned()
}

/// Every pair of the walk from `from` to `to` after `cursor`, resumed as a
/// page resumes: pairs at or before the cursor are passed over and only the
/// directories on its path are entered. With no cursor, the whole walk.
fn walk(fixture: &Fixture, from: &str, to: &str, cursor: Option<&str>) -> Vec<Pair> {
    let store = &fixture.service.store;
    let stack = lookup::project(store, "installed").unwrap();
    let reader = snapshot::reader(store, &stack);
    let left = snapshot::resolve(store, &stack, &reader, from).unwrap();
    let right = snapshot::resolve(store, &stack, &reader, to).unwrap();
    let component = |path: &str| -> Option<PathName> {
        let cursor = cursor?;
        let rest = if path == "/" {
            cursor.strip_prefix('/')
        } else {
            cursor.strip_prefix(path)?.strip_prefix('/')
        }?;
        Some(PathName::new(rest.split('/').next()?).unwrap())
    };
    let reader: &dyn layerfs_content::AuthenticatedObjects = reader.as_ref();
    let first = component("/");
    let mut walk = ChangeWalk::full(reader, &left.tree, &right.tree, first.as_ref()).unwrap();
    let mut out = Vec::new();
    while let Some(pair) = walk.next_pair().unwrap() {
        if let Some(cursor) = cursor {
            let path = pair.0.as_str();
            if cursor.split('/').cmp(path.split('/')) != std::cmp::Ordering::Less {
                let on_path = cursor == path
                    || cursor
                        .strip_prefix(path)
                        .is_some_and(|rest| rest.starts_with('/'));
                if on_path {
                    walk.enter_from(&pair, component(path).as_ref()).unwrap();
                }
                continue;
            }
        }
        walk.enter(&pair).unwrap();
        out.push(pair);
    }
    out
}

/// Every page of the difference at `size` entries a page, joined.
fn pages(fixture: &Fixture, from: &str, to: &str, size: i64) -> Vec<SdkValue> {
    let (mut out, mut cursor) = (Vec::new(), SdkValue::Null);
    loop {
        let page = fixture.call(
            "branch.diff_commits",
            vec![
                ("from", SdkValue::text(from)),
                ("to", SdkValue::text(to)),
                ("pageSize", SdkValue::Int(size)),
                ("cursor", cursor.clone()),
                ("hunks", SdkValue::Bool(false)),
            ],
        );
        out.extend(items(&page).iter().cloned());
        cursor = page.get("nextCursor").cloned().unwrap_or(SdkValue::Null);
        if matches!(cursor, SdkValue::Null) {
            return out;
        }
    }
}

fn flags(item: &SdkValue) -> (String, String, bool, bool, bool) {
    (
        item.get_text("path").unwrap().to_owned(),
        item.get_text("kind").unwrap().to_owned(),
        item.get_bool("content"),
        item.get_bool("identity"),
        item.get_bool("metadata"),
    )
}

#[test]
fn a_resumed_walk_lists_what_the_whole_walk_lists_after_its_cursor() {
    let fixture = Fixture::new("diff-resume", tree);
    fixture.fork("work");
    let c1 = head(&fixture.apply("work", vec![write("/seed", "seed\n")]));
    let c2 = head(&fixture.apply(
        "work",
        vec![
            // one side only: a removed file, an added file, a removed tree,
            // an added directory
            operation("rm", "/a/f3"),
            write("/a/f8", "a 8\n"),
            operation("rm", "/c"),
            operation("mkdir", "/d"),
            write("/d/x", "dx\n"),
            // the same bytes in a new inode
            operation("rm", "/a/f4"),
            write("/a/f4", "a 4\n"),
            // a symlink moved, a directory recreated with the same files
            SdkValue::map([
                ("op", SdkValue::text("mv")),
                ("from", SdkValue::text("/a/link")),
                ("to", SdkValue::text("/a/link2")),
            ]),
            operation("rm", "/b2"),
            operation("mkdir", "/b2"),
            write("/b2/f0", "b2 0\n"),
            write("/b2/f1", "b2 1\n"),
            write("/b2/f2", "b2 2\n"),
        ],
    ));
    // equal content with another mode or time; a directory's metadata alone;
    // one content change in place
    let changes = [
        Change::Mode("/a/f5", 0o600),
        Change::Mtime("/a/f6", 7),
        Change::Mode("/a/sub", 0o700),
        Change::Write("/b/f1", b"B 1\n"),
    ];
    let committed = fixture.workspace_commit("work", &changes, 31, || {});
    let Some(layerfs_history::CommitOutcome::Committed { record, .. }) = committed.history else {
        panic!("Workspace Commit: {:?}", committed.history);
    };
    let c3 = record.id.to_string();

    for (from, to) in [(&c1, &c2), (&c2, &c3), (&c1, &c3), (&c3, &c1)] {
        let whole = walk(&fixture, from, to, None);
        assert!(whole.len() > 20, "{whole:?}");
        for (at, (cursor, _, _)) in whole.iter().enumerate() {
            assert_eq!(
                walk(&fixture, from, to, Some(cursor)),
                whole[at + 1..],
                "resumed after {cursor}"
            );
        }
        // Paging by every small size lists what one page lists.
        let one = pages(&fixture, from, to, 1000);
        for size in [1, 2, 3] {
            assert_eq!(pages(&fixture, from, to, size), one, "page size {size}");
        }
    }

    let listed: Vec<_> = pages(&fixture, &c1, &c3, 1000).iter().map(flags).collect();
    let find = |path: &str| {
        listed
            .iter()
            .find(|item| item.0 == path)
            .unwrap_or_else(|| panic!("{path} not listed: {listed:?}"))
            .clone()
    };
    let flag = |path: &str| {
        let (_, kind, content, identity, metadata) = find(path);
        (kind, content, identity, metadata)
    };
    let modified =
        |content, identity, metadata| ("Modified".to_owned(), content, identity, metadata);
    assert_eq!(flag("/a/f3").0, "Removed");
    assert_eq!(flag("/a/f8").0, "Added");
    assert_eq!(flag("/c").0, "Removed");
    assert_eq!(flag("/c/deep/x").0, "Removed");
    assert_eq!(flag("/d").0, "Added");
    assert_eq!(flag("/a/link").0, "Removed");
    assert_eq!(flag("/a/link2").0, "Added");
    // the same bytes in a new inode (written now, so its time differs too)
    assert_eq!(flag("/a/f4"), modified(false, true, true));
    // equal content with another mode: metadata only
    assert_eq!(flag("/a/f5"), modified(false, false, true));
    assert_eq!(flag("/a/sub"), modified(false, false, true));
    // a write in place: content, and the write's time
    assert_eq!(flag("/b/f1"), modified(true, false, true));
    // a recreated directory and its files are new inodes
    assert!(flag("/b2").2 && flag("/b2/f0").2);
    // unchanged entries, symlinks included, are not listed, nor is a regular
    // file's time alone, which a Commit does not record
    for path in [
        "/a/f6",
        "/a/still",
        "/a/sub/up",
        "/a/sub/g0",
        "/a/f0",
        "/b/f0",
        "/z",
    ] {
        assert!(listed.iter().all(|item| item.0 != path), "{path} listed");
    }
    fixture.cleanup();
}
