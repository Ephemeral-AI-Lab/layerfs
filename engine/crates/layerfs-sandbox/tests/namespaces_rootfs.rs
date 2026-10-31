//! The Sandbox image: one docker-archive in the bundle, unpacked once per
//! config digest. These tests need no root, no Engine and no namespaces: they
//! unpack archives into a scratch directory and read what a command would see.
//! What they prove is the contract the lifecycle tests build on — the digest
//! check, the archive's own whiteouts, one unpack per digest, and the entries
//! a Sandbox root refuses.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
mod support {
    pub mod image;
}
use layerfs_sandbox::{
    backend::namespaces::rootfs::{self, ImageRoot},
    Image,
};
use std::{
    fs, io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use support::image::{archive, hex, legacy_archive, Scratch, Tar, DIGEST, OTHER};

/// Two layers: the first holds the image, the second hides part of it and adds
/// to it, which is what a whiteout and an opaque directory mean.
fn two_layers() -> Vec<Vec<u8>> {
    let first = Tar::new()
        .directory("etc/", 0o755)
        .file("etc/marker", 0o644, b"layer one\n")
        .file("etc/gone", 0o644, b"replaced\n")
        .file("etc/kept", 0o644, b"kept\n")
        .directory("etc/opaque/", 0o755)
        .file("etc/opaque/old", 0o644, b"below\n")
        .finish();
    let second = Tar::new()
        // `.wh.` hides the entry the layer below placed at the same level
        .file("etc/.wh.gone", 0o644, b"")
        // `.wh..wh..opq` empties its directory, then this layer refills it
        .file("etc/opaque/.wh..wh..opq", 0o644, b"")
        .file("etc/opaque/new", 0o644, b"above\n")
        // and an ordinary file the first layer never had
        .file("etc/added", 0o644, b"added\n")
        .finish();
    vec![first, second]
}

fn image(digest: &str) -> Image {
    Image::parse(digest).unwrap()
}

/// The unpacked root of a digest, which must not exist yet.
fn unpacked(scratch: &Scratch, archive: &Path, digest: &str) -> ImageRoot {
    rootfs::ensure(archive, &image(digest), &scratch.cache).unwrap()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn the_layers_unpack_in_order_and_their_whiteouts_apply() {
    let scratch = Scratch::new("layers");
    let path = archive(&scratch.base, "image", DIGEST, two_layers());
    let root = unpacked(&scratch, &path, DIGEST);
    assert_eq!(root.image().as_str(), DIGEST);
    assert_eq!(root.directory(), scratch.cache.join(hex(DIGEST)).as_path());
    let directory = root.directory();
    // the first layer's files, and the second layer's
    assert_eq!(read(&directory.join("etc/marker")), "layer one\n");
    assert_eq!(read(&directory.join("etc/kept")), "kept\n");
    assert_eq!(read(&directory.join("etc/added")), "added\n");
    // `.wh.gone` hid what the layer below put there
    assert!(!directory.join("etc/gone").exists());
    // `.wh..wh..opq` emptied the directory: only the second layer's entry is left
    assert_eq!(read(&directory.join("etc/opaque/new")), "above\n");
    assert!(!directory.join("etc/opaque/old").exists());
    // the cache directory is not writable by the command identity
    let mode = fs::symlink_metadata(directory)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o555, "the image cache is writable");
}

#[test]
fn a_whiteout_with_nothing_below_it_creates_nothing() {
    let scratch = Scratch::new("whiteout");
    let layers = vec![
        Tar::new().file("etc/kept", 0o644, b"kept\n").finish(),
        // a whiteout under a directory no layer made: it hides nothing and
        // must not become the empty directory it names
        Tar::new()
            .file("etc/.wh.hidden", 0o644, b"")
            .file("missing/.wh..wh..opq", 0o644, b"")
            .finish(),
    ];
    let path = archive(&scratch.base, "image", DIGEST, layers);
    let root = unpacked(&scratch, &path, DIGEST);
    assert_eq!(read(&root.directory().join("etc/kept")), "kept\n");
    assert!(!root.directory().join("etc/hidden").exists());
    assert!(!root.directory().join("missing").exists());
}

#[test]
fn a_digest_the_archive_is_not_refuses_the_start_and_unpacks_nothing() {
    let scratch = Scratch::new("digest");
    let path = archive(&scratch.base, "image", DIGEST, two_layers());
    let error = rootfs::ensure(&path, &image(OTHER), &scratch.cache).unwrap_err();
    let message = format!("{error}");
    assert!(message.contains(hex(DIGEST)), "{message}");
    assert!(message.contains(hex(OTHER)), "{message}");
    assert!(message.contains("config digest"), "{message}");
    // nothing was published, and no half unpack was left behind
    assert!(!scratch.cache.join(hex(OTHER)).exists());
    assert_eq!(temporaries(&scratch.cache).len(), 0);
}

#[test]
fn an_older_writers_config_name_is_the_same_digest() {
    let scratch = Scratch::new("legacy");
    let path = legacy_archive(&scratch.base, "image", DIGEST, two_layers());
    let root = unpacked(&scratch, &path, DIGEST);
    assert_eq!(read(&root.directory().join("etc/marker")), "layer one\n");
}

#[test]
fn one_unpack_per_digest_however_many_sandboxes_ask_for_it() {
    let scratch = Scratch::new("once");
    let path = archive(&scratch.base, "image", DIGEST, two_layers());
    let first = unpacked(&scratch, &path, DIGEST);
    let published = fs::metadata(first.directory()).unwrap();
    // a second Sandbox of the same image finds the finished directory: the
    // same inode, and no new unpack
    let second = unpacked(&scratch, &path, DIGEST);
    assert_eq!(second.directory(), first.directory());
    let observed = fs::metadata(second.directory()).unwrap();
    assert_eq!(
        (observed.ino(), observed.mtime()),
        (published.ino(), published.mtime()),
        "the image was unpacked again"
    );
    assert_eq!(temporaries(&scratch.cache).len(), 0);
}

#[test]
fn a_second_sandbox_waits_for_the_unpack_that_is_running() {
    let scratch = Scratch::new("lock");
    let path = archive(&scratch.base, "image", DIGEST, two_layers());
    // the first unpack publishes the cache and its lock
    let published = unpacked(&scratch, &path, DIGEST);
    let directory = published.directory().to_owned();
    drop(published);
    fs::remove_dir_all(&directory).unwrap();
    // holding the digest's lock is what an unpack in progress looks like: a
    // second one neither starts nor publishes anything until it is released
    let held = nix::fcntl::Flock::lock(
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(scratch.cache.join(format!("{}.lock", hex(DIGEST))))
            .unwrap(),
        nix::fcntl::FlockArg::LockExclusive,
    )
    .unwrap();
    let waiting = {
        let (archive, cache) = (path.clone(), scratch.cache.clone());
        std::thread::spawn(move || {
            rootfs::ensure(&archive, &image(DIGEST), &cache).map(|root| root.directory().to_owned())
        })
    };
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        assert!(
            !directory.exists(),
            "a second unpack published while the first held the lock"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(held);
    let waited = waiting.join().expect("the waiting unpack").unwrap();
    assert_eq!(waited, directory);
    assert_eq!(read(&directory.join("etc/added")), "added\n");
}

#[test]
fn a_path_that_is_not_plain_and_relative_is_refused() {
    let scratch = Scratch::new("escape");
    for (name, layer) in [
        (
            "parent",
            Tar::new().file("../outside", 0o644, b"x").finish(),
        ),
        (
            "absolute",
            Tar::new().file("/etc/outside", 0o644, b"x").finish(),
        ),
        (
            "nested",
            Tar::new()
                .directory("a/", 0o755)
                .file("a/../../outside", 0o644, b"x")
                .finish(),
        ),
    ] {
        let path = archive(&scratch.base, name, DIGEST, vec![layer]);
        let error = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap_err();
        assert!(
            format!("{error}").contains("absolute or holds a `..` component"),
            "{name}: {error}"
        );
        assert!(!scratch.cache.join(hex(DIGEST)).exists(), "{name}");
        assert!(
            !scratch.base.join("outside").exists(),
            "{name} wrote outside"
        );
        assert!(!Path::new("/etc/outside").exists(), "{name} wrote outside");
    }
    assert_eq!(temporaries(&scratch.cache).len(), 0);
}

#[test]
fn a_symlink_cannot_redirect_a_later_layers_write() {
    let scratch = Scratch::new("symlink");
    let layers = vec![
        Tar::new()
            .directory("etc/", 0o755)
            .symlink("etc/escape", "/tmp")
            .finish(),
        // the parent of this entry is a symbolic link, so writing it would
        // leave the root entirely
        Tar::new().file("etc/escape/outside", 0o644, b"x").finish(),
    ];
    let path = archive(&scratch.base, "image", DIGEST, layers);
    let error = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap_err();
    assert!(
        format!("{error}").contains("passes through a symbolic link"),
        "{error}"
    );
    assert!(!Path::new("/tmp/outside").exists(), "the link was followed");
    assert!(!scratch.cache.join(hex(DIGEST)).exists());
}

#[test]
fn a_device_node_is_refused() {
    let scratch = Scratch::new("device");
    let layers = vec![Tar::new()
        .directory("dev/", 0o755)
        .raw("dev/sda", b'3', 0o660, &[])
        .finish()];
    let path = archive(&scratch.base, "image", DIGEST, layers);
    let error = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap_err();
    assert!(
        format!("{error}").contains("device node"),
        "a device node was extracted: {error}"
    );
    assert!(!scratch.cache.join(hex(DIGEST)).exists());
}

#[test]
fn a_compressed_layer_is_refused_with_the_reason() {
    let scratch = Scratch::new("gzip");
    let mut gzip = vec![0x1f, 0x8b];
    gzip.extend_from_slice(&[0u8; 64]);
    let path = archive(&scratch.base, "image", DIGEST, vec![gzip]);
    let error = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap_err();
    let message = format!("{error}");
    assert!(message.contains("gzip-compressed"), "{message}");
    assert!(message.contains("docker save"), "{message}");
}

#[test]
fn an_archive_without_its_manifest_or_a_layer_is_refused() {
    let scratch = Scratch::new("missing");
    let without_manifest = Tar::new()
        .file("1/layer.tar", 0o644, &Tar::new().finish())
        .finish();
    let path = scratch.base.join("no-manifest.tar");
    fs::write(&path, without_manifest).unwrap();
    let error = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap_err();
    assert!(format!("{error}").contains("manifest.json"), "{error}");
    // a manifest naming a layer the archive does not hold
    let mut tar = Tar::new().file(
        "manifest.json",
        0o644,
        format!(
            "[{{\"Config\":\"blobs/sha256/{}\",\"Layers\":[\"7/layer.tar\"]}}]",
            hex(DIGEST)
        )
        .as_bytes(),
    );
    tar = tar.file(&format!("blobs/sha256/{}", hex(DIGEST)), 0o644, b"{}");
    let path = scratch.base.join("no-layer.tar");
    fs::write(&path, tar.finish()).unwrap();
    let error = rootfs::ensure(&path, &image(DIGEST), &scratch.cache).unwrap_err();
    assert!(format!("{error}").contains("7/layer.tar"), "{error}");
}

#[test]
fn two_digests_share_one_cache_directory() {
    let scratch = Scratch::new("digests");
    let first = archive(&scratch.base, "first", DIGEST, two_layers());
    let second = archive(
        &scratch.base,
        "second",
        OTHER,
        vec![Tar::new().file("other", 0o644, b"other\n").finish()],
    );
    let one = unpacked(&scratch, &first, DIGEST);
    let two = unpacked(&scratch, &second, OTHER);
    assert_ne!(one.directory(), two.directory());
    assert_eq!(read(&one.directory().join("etc/marker")), "layer one\n");
    assert_eq!(read(&two.directory().join("other")), "other\n");
    // a record's image is found without unpacking anything again
    assert_eq!(
        rootfs::lookup(&image(OTHER), &scratch.cache)
            .unwrap()
            .directory(),
        two.directory()
    );
    // and one that is not there is refused rather than invented
    let missing = Image::parse(&format!("sha256:{}", "d".repeat(64))).unwrap();
    let error = rootfs::lookup(&missing, &scratch.cache).unwrap_err();
    assert!(format!("{error}").contains("cannot be found"), "{error}");
}

/// The temporary directories an unpack in progress leaves, if any.
fn temporaries(cache: &Path) -> Vec<PathBuf> {
    match fs::read_dir(cache) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("{}: {error}", cache.display()),
        Ok(entries) => (entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".tmp-"))
            })
            .collect(),
    }
}
