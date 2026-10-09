//! Real kernel mounts: a directory's `st_nlink` is two plus its child
//! directories, as a native filesystem reports it. Inherited and local
//! directories alike, after every mkdir, rmdir and rename, through ordinary
//! syscalls of processes the daemon never registered.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use mounted::COMMAND;
use mutating::{forced, harness, path_only, quiet, shell, unmounted};
use std::{fs, os::unix::fs::MetadataExt, path::Path};

/// Two plus the directory's children that `lstat` as directories: what an
/// ordinary process counts with `readdir` and `lstat`, never a `d_type`.
fn counted(directory: &Path) -> u64 {
    let children = fs::read_dir(directory)
        .unwrap()
        .filter(|entry| {
            let path = entry.as_ref().unwrap().path();
            fs::symlink_metadata(path).unwrap().is_dir()
        })
        .count();
    2 + children as u64
}
/// Three readings of one directory agree with the stated count: the
/// attributes `lstat` returns, which the kernel may have cached; attributes
/// the kernel fetches from the daemon for this call; and the count above.
#[track_caller]
fn links(directory: &Path, stated: u64, what: &str) {
    let cached = fs::symlink_metadata(directory).unwrap().nlink();
    let fetched = u64::from(forced(&path_only(directory)).unwrap().links);
    assert_eq!(
        (cached, fetched, counted(directory)),
        (stated, stated, stated),
        "{what}: lstat, forced statx, readdir+lstat of {}",
        directory.display()
    );
}
/// The directories the fixture declares directly beneath `path`.
fn declared(path: &str) -> u64 {
    fixture::DIRS
        .iter()
        .filter(|other| !other.is_empty())
        .filter(|other| other.rsplit_once('/').map_or("", |(parent, _)| parent) == path)
        .count() as u64
}
/// Every directory of the tree reports two plus its child directories.
/// Returns the number of directories visited.
fn every(directory: &Path) -> u64 {
    let mut visited = 1;
    let mut children = 0;
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            children += 1;
            visited += every(&path);
        }
    }
    assert_eq!(
        fs::symlink_metadata(directory).unwrap().nlink(),
        2 + children,
        "{}",
        directory.display()
    );
    visited
}

#[test]
fn a_directory_reports_two_links_plus_its_child_directories() {
    let (f, h) = harness("-links");
    let ready = h.mount(2);
    let root = Path::new(&ready.directory);
    let at = |path: &str| {
        if path.is_empty() {
            root.to_path_buf()
        } else {
            root.join(path)
        }
    };

    // The committed tree, before any local row exists: each count is derived
    // from the base, and files, hard links and symlinks are not in it.
    assert_eq!(
        (declared(""), declared(".git/objects"), declared(".cache")),
        (5, 1, 2)
    );
    for path in fixture::DIRS {
        links(&at(path), 2 + declared(path), "inherited");
    }
    assert_eq!(every(root), fixture::DIRS.len() as u64);

    // mkdir -p by an ordinary process that was never registered.
    let made = shell(
        COMMAND,
        &format!("cd {} && mkdir -p a/b/c a/d", root.display()),
    );
    assert!(made.status.success(), "{made:?}");
    links(root, 8, "mkdir -p");
    links(&at("a"), 4, "mkdir -p");
    links(&at("a/b"), 3, "mkdir -p");
    links(&at("a/b/c"), 2, "mkdir -p");
    links(&at("a/d"), 2, "mkdir -p");
    // A file, a hard link and a symlink to a directory are not directories.
    fs::write(at("a/file"), b"f").unwrap();
    fs::hard_link(at("a/file"), at("a/alias")).unwrap();
    std::os::unix::fs::symlink("b", at("a/to-b")).unwrap();
    links(&at("a"), 4, "file, hard link and symlink");

    // rmdir, and a removed directory that is still referenced.
    fs::remove_dir(at("a/d")).unwrap();
    links(&at("a"), 3, "rmdir");
    let held = path_only(&at("a/b/c"));
    fs::remove_dir(at("a/b/c")).unwrap();
    links(&at("a/b"), 2, "rmdir of the last child directory");
    assert_eq!(forced(&held).unwrap().links, 0, "removed, still referenced");
    drop(held);

    // mv a/b x: one fewer in the old parent, one more in the new one.
    fs::rename(at("a/b"), at("x")).unwrap();
    links(&at("a"), 2, "moved away");
    links(root, 9, "moved in");
    links(&at("x"), 2, "the moved directory itself");
    // A rename inside one parent changes nothing.
    fs::create_dir(at("x/y")).unwrap();
    fs::create_dir(at("x/z")).unwrap();
    links(&at("x"), 4, "two more");
    fs::rename(at("x/y"), at("x/w")).unwrap();
    links(&at("x"), 4, "renamed in place");
    // Over an empty directory of another parent: the destination gains the
    // moved directory and loses the replaced one.
    fs::create_dir(at("a/t")).unwrap();
    links(&at("a"), 3, "before the replacement");
    fs::rename(at("x/z"), at("a/t")).unwrap();
    links(&at("x"), 3, "moved over an empty directory elsewhere");
    links(&at("a"), 3, "replaced by the moved directory");
    // Over an empty directory of the same parent: one fewer.
    fs::create_dir(at("a/u")).unwrap();
    links(&at("a"), 4, "before the replacement in place");
    fs::rename(at("a/t"), at("a/u")).unwrap();
    links(&at("a"), 3, "replaced in place");

    // Inherited directories: the first local row starts from the base count.
    fs::create_dir(at(".git/refs")).unwrap();
    links(&at(".git"), 4, "mkdir in an inherited directory");
    fs::create_dir(at(".git/objects/bb")).unwrap();
    links(&at(".git/objects"), 4, "mkdir two levels down");
    links(&at(".git"), 4, "a grandchild is not counted");
    fs::remove_dir(at(".cache/empty")).unwrap();
    links(&at(".cache"), 3, "rmdir of an inherited subdirectory");
    fs::rename(at("output/build"), at("node_modules/build")).unwrap();
    links(&at("output"), 3, "an inherited subdirectory moved away");
    links(&at("node_modules"), 4, "an inherited subdirectory moved in");
    fs::remove_dir(at("output/empty")).unwrap();
    links(&at("output"), 2, "no child directory left");
    links(root, 9, "the root at the end");

    // The whole tree once more: eleven inherited directories remain, and
    // a, a/u, x, x/w, .git/refs and .git/objects/bb were made here.
    let directories = every(root);
    assert_eq!(directories, 17);

    quiet(&h, ready.token);
    let work = h.status(ready.token).native.unwrap().work.unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    println!("MOUNTED_LINKS directories={directories} work={work:?}");
    unmounted(&h, &ready);
    h.stop();
    f.cleanup();
}
