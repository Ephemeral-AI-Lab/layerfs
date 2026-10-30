//! Modification time and committed link count on the Branch file reads
//! (#486). `fs.stat` reports `mtimeSeconds`, `mtimeNanoseconds` and `nlink`
//! beside `mode`. Each `fs.list` entry reports the same four fields, so a
//! listing does not need one `fs.stat` per name. Hard-linked names share
//! `inode` and carry the namespace name count as `nlink`. Seconds and
//! nanoseconds stay separate so a JavaScript Number keeps them exactly.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};
use support::{items, Change, Fixture};

const FILE_SECONDS: i64 = 1_700_000_123;
const FILE_NANOS: u32 = 123_456_789;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn stamp(path: &Path, seconds: i64, nanos: u32) {
    let times =
        fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::new(seconds as u64, nanos));
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(times)
        .unwrap();
}

fn source(root: &Path) -> usize {
    fs::write(root.join("a.txt"), b"a\n").unwrap();
    fs::set_permissions(root.join("a.txt"), fs::Permissions::from_mode(0o640)).unwrap();
    stamp(&root.join("a.txt"), FILE_SECONDS, FILE_NANOS);
    fs::hard_link(root.join("a.txt"), root.join("alias.txt")).unwrap();
    fs::create_dir(root.join("private")).unwrap();
    fs::set_permissions(root.join("private"), fs::Permissions::from_mode(0o750)).unwrap();
    fs::write(root.join("private/b.txt"), b"b\n").unwrap();
    std::os::unix::fs::symlink("a.txt", root.join("link")).unwrap();
    5
}

fn int(value: &SdkValue, name: &str) -> i64 {
    value
        .get_int(name)
        .unwrap_or_else(|| panic!("{name} missing on {value:?}"))
}

fn stat(f: &Fixture, branch: &str, path: &str) -> SdkValue {
    f.call("fs.stat", vec![("branch", t(branch)), ("path", t(path))])
}

fn listed(f: &Fixture, branch: &str, path: &str) -> Vec<SdkValue> {
    items(&f.call("fs.list", vec![("branch", t(branch)), ("path", t(path))])).to_vec()
}

fn named<'a>(entries: &'a [SdkValue], name: &str) -> &'a SdkValue {
    entries
        .iter()
        .find(|entry| entry.get_text("name") == Some(name))
        .unwrap_or_else(|| panic!("no list entry {name}"))
}

/// Init keeps host mode, mtime and hard links; list entries match stat.
#[test]
fn stat_and_list_report_mtime_mode_and_nlink() {
    let f = Fixture::new("fs-stat-metadata", source);

    let file = stat(&f, "main", "/a.txt");
    let alias = stat(&f, "main", "/alias.txt");
    assert_eq!(file.get_text("type"), Some("file"));
    assert_eq!(int(&file, "mode"), 0o640);
    assert_eq!(int(&file, "mtimeSeconds"), FILE_SECONDS);
    assert_eq!(int(&file, "mtimeNanoseconds"), i64::from(FILE_NANOS));
    assert_eq!(int(&file, "nlink"), 2);
    assert_eq!(int(&alias, "inode"), int(&file, "inode"));
    assert_eq!(int(&alias, "nlink"), 2);
    assert_eq!(int(&alias, "mode"), 0o640);
    assert_eq!(int(&alias, "mtimeSeconds"), FILE_SECONDS);

    let dir = stat(&f, "main", "/private");
    assert_eq!(dir.get_text("type"), Some("dir"));
    assert_eq!(int(&dir, "mode"), 0o750);
    assert_eq!(int(&dir, "nlink"), 1);

    let root = stat(&f, "main", "/");
    assert_eq!(root.get_text("type"), Some("dir"));
    assert_eq!(int(&root, "nlink"), 0);

    let link = stat(&f, "main", "/link");
    assert_eq!(link.get_text("type"), Some("symlink"));
    assert_eq!(int(&link, "mode"), 0o777);
    assert_eq!(int(&link, "nlink"), 1);

    let top = listed(&f, "main", "/");
    assert_eq!(
        top.iter()
            .map(|entry| entry.get_text("name").unwrap())
            .collect::<Vec<_>>(),
        ["a.txt", "alias.txt", "link", "private"]
    );
    for name in ["a.txt", "alias.txt", "link", "private"] {
        let path = if name == "private" {
            "/private".to_owned()
        } else {
            format!("/{name}")
        };
        let from_stat = stat(&f, "main", &path);
        let from_list = named(&top, name);
        for field in [
            "type",
            "inode",
            "mode",
            "mtimeSeconds",
            "mtimeNanoseconds",
            "nlink",
        ] {
            assert_eq!(from_list.get(field), from_stat.get(field), "{name} {field}");
        }
        assert_eq!(from_list.get_text("path"), Some(path.as_str()));
    }
    assert_eq!(
        int(named(&top, "a.txt"), "inode"),
        int(named(&top, "alias.txt"), "inode")
    );

    f.fork("feat");
    // A content write records the mutation clock (123 s in this fixture).
    f.workspace_commit(
        "feat",
        &[Change::Write("/private/b.txt", b"bb\n")],
        41,
        || {},
    );
    let touched = stat(&f, "feat", "/private/b.txt");
    assert_eq!(int(&touched, "mtimeSeconds"), 123);
    assert_eq!(int(&touched, "mtimeNanoseconds"), 0);
    assert_eq!(int(&touched, "nlink"), 1);
    let inner = listed(&f, "feat", "/private");
    assert_eq!(int(named(&inner, "b.txt"), "mtimeSeconds"), 123);
    assert_eq!(int(named(&inner, "b.txt"), "mtimeNanoseconds"), 0);
    assert_ne!(
        int(&stat(&f, "main", "/private/b.txt"), "mtimeSeconds"),
        123
    );

    f.cleanup();
}
