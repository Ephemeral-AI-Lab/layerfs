//! The Sandbox image keeps the archive's modification times: a file's, a
//! symbolic link's and a directory's (Python, for one, rejects a cached
//! `.pyc` whose source's time is not the one it recorded). And the unpack that
//! restores them is published under its own name, so a new Sandbox never uses
//! an older unpack that did not, while a reopened one keeps the root it was
//! built on. Like `namespaces_rootfs`, these need no root, no Engine and no
//! namespaces.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
mod support {
    pub mod image;
}
use layerfs_sandbox_backend::{namespaces::rootfs, Image};
use std::{fs, os::unix::fs::MetadataExt, path::Path};
use support::image::{archive, base256, hex, pax_record, unpacked_at, Scratch, Tar, DIGEST, OTHER};

fn image(digest: &str) -> Image {
    Image::parse(digest).unwrap()
}

/// The unpacked root of one archive of `layers`.
fn unpack(scratch: &Scratch, layers: Vec<Vec<u8>>) -> std::path::PathBuf {
    let path = archive(&scratch.base, "image", DIGEST, layers);
    let root = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap();
    root.directory().to_owned()
}

/// `path`'s own modification time (never a link's target's): seconds and
/// nanoseconds.
fn time(path: &Path) -> (i64, i64) {
    let metadata =
        fs::symlink_metadata(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    (metadata.mtime(), metadata.mtime_nsec())
}

#[test]
fn files_links_and_directories_carry_the_archives_times() {
    let scratch = Scratch::new("times-kinds");
    let layer = Tar::new()
        .at(1_000_000_000)
        .directory("etc/", 0o755)
        .at(1_100_000_000)
        .file("etc/file", 0o644, b"file\n")
        .raw(
            "PaxHeaders/etc/link",
            b'x',
            0o644,
            &pax_record("mtime", "1200000000.000000123"),
        )
        .at(5)
        .symlink("etc/link", "file")
        .raw(
            "PaxHeaders/etc/exact",
            b'x',
            0o644,
            &pax_record("mtime", "1300000000.5"),
        )
        .file("etc/exact", 0o644, b"exact\n")
        // a hard link shares its file's inode, and so that file's time, not
        // the time its own entry carries
        .at(1_400_000_000)
        .hard_link("etc/hard", "etc/file")
        .finish();
    let root = unpack(&scratch, vec![layer]);
    // the access time is the modification time; read before anything reads
    // the file's bytes (relatime would move it)
    let exact = fs::symlink_metadata(root.join("etc/exact")).unwrap();
    assert_eq!(
        (exact.atime(), exact.atime_nsec()),
        (1_300_000_000, 500_000_000)
    );
    assert_eq!(time(&root.join("etc/exact")), (1_300_000_000, 500_000_000));
    assert_eq!(time(&root.join("etc/file")), (1_100_000_000, 0));
    // the link's own time, and its target's left alone
    assert_eq!(time(&root.join("etc/link")), (1_200_000_000, 123));
    assert_eq!(time(&root.join("etc/hard")), (1_100_000_000, 0));
    // the directory keeps its entry's time, although its children came after
    assert_eq!(time(&root.join("etc")), (1_000_000_000, 0));
    assert_eq!(fs::read(root.join("etc/hard")).unwrap(), b"file\n");
}

#[test]
fn a_directorys_time_survives_later_layers_and_their_whiteouts() {
    let scratch = Scratch::new("times-layers");
    let layers = vec![
        Tar::new()
            .at(1_000)
            .directory("a/", 0o755)
            .file("a/x", 0o644, b"x\n")
            .at(2_000)
            .directory("b/", 0o755)
            .file("b/gone", 0o644, b"gone\n")
            .at(3_000)
            .directory("c/", 0o755)
            .finish(),
        Tar::new()
            .at(9_000)
            // a child added by a later layer, with no entry for its directory
            .file("a/added", 0o644, b"added\n")
            // a whiteout removing a child
            .file("b/.wh.gone", 0o644, b"")
            // a later entry for the same directory: its time is the one kept
            .at(4_000)
            .directory("c/", 0o755)
            .file("c/new", 0o644, b"new\n")
            .finish(),
    ];
    let root = unpack(&scratch, layers);
    assert_eq!(time(&root.join("a")), (1_000, 0));
    assert_eq!(time(&root.join("a/added")), (9_000, 0));
    assert!(!root.join("b/gone").exists());
    assert_eq!(time(&root.join("b")), (2_000, 0));
    assert_eq!(time(&root.join("c")), (4_000, 0));
}

#[test]
fn a_directory_a_later_layer_replaced_or_emptied_still_unpacks() {
    let scratch = Scratch::new("times-replaced");
    let layers = vec![
        Tar::new()
            .at(1_000)
            .directory("d/", 0o755)
            .directory("d/inner/", 0o755)
            .directory("s/", 0o755)
            .directory("s/inner/", 0o755)
            .directory("o/", 0o755)
            .directory("o/sub/", 0o755)
            .file("o/sub/f", 0o644, b"f\n")
            .finish(),
        Tar::new()
            .at(2_000)
            // a file, and a symbolic link out of the root, where directories
            // with a time to set were
            .file("d", 0o644, b"now a file\n")
            .symlink("s", "/tmp")
            // an opaque whiteout empties a directory with one below it
            .file("o/.wh..wh..opq", 0o644, b"")
            .finish(),
    ];
    let root = unpack(&scratch, layers);
    assert!(fs::symlink_metadata(root.join("d")).unwrap().is_file());
    assert_eq!(time(&root.join("d")), (2_000, 0));
    assert!(fs::symlink_metadata(root.join("s"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(time(&root.join("s")), (2_000, 0));
    // nothing was set through the link
    assert_ne!(time(Path::new("/tmp")), (1_000, 0));
    assert_eq!(time(&root.join("o")), (1_000, 0));
    assert!(!root.join("o/sub").exists());
}

#[test]
fn a_headers_time_is_octal_or_base_256_and_may_be_before_1970() {
    let scratch = Scratch::new("times-header");
    let layer = Tar::new()
        .at(1_234_567_890)
        .file("octal", 0o644, b"")
        .at_field(base256(1_700_000_000))
        .file("positive", 0o644, b"")
        .at_field(base256(-86_400))
        .file("negative", 0o644, b"")
        // a time that cannot be read is 0, never a refused image
        .at_field(*b"not a time\0\0")
        .file("unreadable", 0o644, b"")
        .finish();
    let root = unpack(&scratch, vec![layer]);
    assert_eq!(time(&root.join("octal")), (1_234_567_890, 0));
    assert_eq!(time(&root.join("positive")), (1_700_000_000, 0));
    assert_eq!(time(&root.join("negative")), (-86_400, 0));
    assert_eq!(time(&root.join("unreadable")), (0, 0));
}

#[test]
fn a_pax_time_overrides_the_header_for_its_entry_only() {
    let scratch = Scratch::new("times-pax");
    let pax = |tar: Tar, name: &str, value: &str| {
        tar.raw(
            &format!("PaxHeaders/{name}"),
            b'x',
            0o644,
            &pax_record("mtime", value),
        )
    };
    let mut tar = pax(Tar::new(), "over", "1700000000.25")
        .at(5)
        .file("over", 0o644, b"");
    // the next entry has the header's time again
    tar = tar.at(7).file("next", 0o644, b"");
    // -1.25 s is 2 s before 1970 plus 0.75 s
    tar = pax(tar, "before", "-1.25").file("before", 0o644, b"");
    // digits past nanoseconds are dropped
    tar = pax(tar, "long", "1.1234567891").file("long", 0o644, b"");
    // a pax time that cannot be read leaves the header's
    tar = pax(tar, "bad", "soon").at(9).file("bad", 0o644, b"");
    // a pax time for the skipped `./` entry is not the next entry's
    tar = pax(tar, ".", "1111").directory("./", 0o755);
    tar = tar.at(11).file("after-root", 0o644, b"");
    let root = unpack(&scratch, vec![tar.finish()]);
    assert_eq!(time(&root.join("over")), (1_700_000_000, 250_000_000));
    assert_eq!(time(&root.join("next")), (7, 0));
    assert_eq!(time(&root.join("before")), (-2, 750_000_000));
    assert_eq!(time(&root.join("long")), (1, 123_456_789));
    assert_eq!(time(&root.join("bad")), (9, 0));
    assert_eq!(time(&root.join("after-root")), (11, 0));

    // a pax record whose framing is wrong is still refused
    let scratch = Scratch::new("times-pax-framing");
    let layer = Tar::new()
        .raw("PaxHeaders/x", b'x', 0o644, b"garbage")
        .file("x", 0o644, b"x")
        .finish();
    let path = archive(&scratch.base, "image", DIGEST, vec![layer]);
    let error = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap_err();
    assert!(format!("{error}").contains("pax record"), "{error}");
}

#[test]
fn a_new_unpack_never_uses_an_older_layouts_directory() {
    let scratch = Scratch::new("times-layout");
    // what a build that did not restore times left: `images/<64 hex>`
    let old = scratch.cache.join(hex(DIGEST));
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("old-marker"), b"old\n").unwrap();
    let layer = Tar::new()
        .at(1_000)
        .file("marker", 0o644, b"new\n")
        .finish();
    let path = archive(&scratch.base, "image", DIGEST, vec![layer]);
    let root = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap();
    let new = unpacked_at(&scratch.cache, DIGEST);
    assert_eq!(root.directory(), new.as_path());
    assert_eq!(time(&new.join("marker")), (1_000, 0));
    assert!(!new.join("old-marker").exists());
    // the older unpack is left as it was
    assert_eq!(fs::read(old.join("old-marker")).unwrap(), b"old\n");

    // a reopen gets the root its spec names, whichever layout, even when the
    // new one exists
    for lower in [&old, &new] {
        let found = rootfs::lookup(&image(DIGEST), &scratch.cache, lower).unwrap();
        assert_eq!(found.directory(), lower.as_path());
        assert_eq!(found.image().as_str(), DIGEST);
    }
    // anything else is refused, even a directory that exists: one outside
    // the cache, another layout's name, another digest's root
    let outside = scratch.base.join("elsewhere");
    let unknown = scratch.cache.join(format!("{}.3", hex(DIGEST)));
    let other = unpacked_at(&scratch.cache, OTHER);
    for directory in [&outside, &unknown, &other] {
        fs::create_dir_all(directory).unwrap();
        let error = rootfs::lookup(&image(DIGEST), &scratch.cache, directory).unwrap_err();
        assert!(format!("{error}").contains("cannot be found"), "{error}");
    }
    let error = rootfs::lookup(&image(OTHER), &scratch.cache, &new).unwrap_err();
    assert!(format!("{error}").contains("cannot be found"), "{error}");
}
