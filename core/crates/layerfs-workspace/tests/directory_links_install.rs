//! Child-directory counts across a Commit. Nothing about them is added to
//! the canonical namespace: a captured row's count and the count derived
//! from the root the producer built from that row agree by construction.
//! This drives the real captured-namespace producer and the known local
//! install, and compares every directory before and after.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::Bench;
use layerfs_content::object::inode_leaf::InodeKind;
use producer::{drain, Drive};

/// The independent count: listed children that stat as a directory.
fn listed(b: &Bench, serial: u64) -> u64 {
    b.list(serial)
        .0
        .iter()
        .filter(|(_, child)| b.stat(*child).unwrap().kind == InodeKind::Directory)
        .count() as u64
}
/// The reported count of every path, each checked against the listing.
fn reported(b: &Bench, d: &Drive<'_>, paths: &[&str]) -> Vec<(u64, u64)> {
    paths
        .iter()
        .map(|path| {
            let serial = d.serial(path);
            let stat = b.stat(serial).unwrap();
            assert_eq!(stat.kind, InodeKind::Directory, "{path:?}");
            assert_eq!(stat.subdirs, listed(b, serial), "{path:?}");
            (serial, stat.subdirs)
        })
        .collect()
}
fn local(b: &Bench, serial: u64) -> Option<layerfs_overlay::Inode> {
    b.overlay.inode(b.route(), serial).unwrap()
}

#[test]
fn every_directory_reports_the_count_it_had_before_the_commit_from_the_installed_base() {
    let b = Bench::new("links-install");
    let mut d = Drive::new(&b);
    // mkdir -p tree/a/b tree/a/c tree/d, and a file beside them.
    d.mkdir("tree", 0o755);
    let a = d.mkdir("tree/a", 0o755);
    d.mkdir("tree/a/b", 0o755);
    d.mkdir("tree/a/c", 0o755);
    d.mkdir("tree/d", 0o700);
    d.create("tree/a/file", 0o644);
    // Directories made under a base directory.
    d.mkdir(".git/objects", 0o755);
    d.mkdir(".git/objects/aa", 0o755);
    d.mkdir(".git/objects/bb", 0o755);
    d.mkdir(".git/refs", 0o755);
    // rmdir of an emptied base directory.
    d.remove("cache/state");
    d.remove("cache");
    // A directory moved to another parent, one renamed in place, one made
    // and removed again, and a base directory moved over an emptied one.
    d.rename("tree/d", "node_modules/d");
    d.rename("tree/a/c", "tree/a/z");
    d.mkdir("output/tmp", 0o755);
    d.remove("output/tmp");
    d.remove("output/result");
    d.rename("node_modules", "output");

    let paths = [
        "",
        ".git",
        ".git/objects",
        ".git/objects/aa",
        ".git/refs",
        "tree",
        "tree/a",
        "tree/a/b",
        "tree/a/z",
        "output",
        "output/d",
    ];
    let before = reported(&b, &d, &paths);
    let stated: Vec<u64> = before.iter().map(|(_, count)| *count).collect();
    assert_eq!(stated, [3, 2, 2, 0, 0, 1, 2, 0, 0, 1, 0]);
    // The base directory that took the place of `output` kept its serial.
    assert_eq!(d.serial("output"), 5);
    // Every changed parent holds its count in a local row before the Commit.
    for (serial, count) in [
        before[0], before[1], before[2], before[5], before[6], before[9],
    ] {
        assert_eq!(local(&b, serial).unwrap().subdirs, count, "serial {serial}");
    }

    // Capture, construct through the real producer, install the built root.
    let scans = b.cache.diagnostics().unwrap().directory_count_scans;
    d.build("directory link counts").install(&b);
    drain(&b);
    // No local row is read any more: each count now comes from the new base,
    // derived once per directory content, and equals what the row reported.
    for (serial, _) in &before {
        assert_eq!(local(&b, *serial), None, "serial {serial}");
    }
    assert_eq!(reported(&b, &d, &paths), before);
    // The six directories that have a child were listed; the five empty
    // ones needed no listing at all.
    assert_eq!(
        b.cache.diagnostics().unwrap().directory_count_scans,
        scans + 6
    );

    // One more mkdir after the install: the first local row over the new
    // base starts from the derived count.
    d.mkdir("tree/a/late", 0o755);
    let row = local(&b, a).unwrap();
    assert_eq!((row.entries, row.subdirs, row.nlink), (4, 3, 1));
    let after = reported(&b, &d, &paths);
    for (index, path) in paths.iter().enumerate() {
        let more = u64::from(*path == "tree/a");
        assert_eq!(after[index], (before[index].0, before[index].1 + more));
    }
    // And the next Commit carries that count into its root the same way.
    d.build("directory link counts, second commit").install(&b);
    drain(&b);
    assert_eq!(local(&b, a), None);
    assert_eq!(reported(&b, &d, &paths), after);
}
