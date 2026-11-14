//! `store.create`'s directory (#570 P1): `make_empty` hands back whether this
//! call's own `mkdir` made `store/<name>`, and only that ownership may reclaim
//! the directory when nothing was published. The guard refuses tmpfs by
//! filesystem, after the `mkdir` succeeded, so it shows both sides; a `mkdir`
//! that fails on a taken name claims nothing and removes nothing.
use layerfs_home::home::paths;
use layerfs_home::names::StoreName;
use std::path::{Path, PathBuf};

fn scratch(parent: &Path, label: &str) -> PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let directory = parent.join(format!("layerfs-create-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .unwrap();
    directory
}

/// A `mkdir` that fails removes nothing: the Store's directory name is taken
/// already, by a dangling symbolic link here — `Path::exists` looks through
/// the link and sees nothing, so `make_empty` reaches its own `mkdir`, which
/// returns `AlreadyExists` and so claims no directory. The link, its target
/// and its surroundings stay as they were, and no Store is created.
#[cfg(unix)]
#[test]
fn a_mkdir_failure_removes_nothing() {
    let home = scratch(&std::env::temp_dir(), "taken-name");
    layerfs_home::home::layout::create(&home).unwrap();
    let name = StoreName::parse("taken").unwrap();
    let directory = paths::store_dir(&home, &name);
    let absent_target = home.join("link-target");
    std::fs::create_dir(home.join("store")).unwrap();
    std::os::unix::fs::symlink(&absent_target, &directory).unwrap();
    let refusal = layerfs_store::sqlite::create::create(&home, &name).unwrap_err();
    assert_eq!(refusal.code, "Failed", "{}", refusal.message);
    assert!(
        refusal.message.contains("File exists"),
        "{}",
        refusal.message
    );
    let link = std::fs::symlink_metadata(&directory).unwrap();
    assert!(link.file_type().is_symlink(), "the taken name stays");
    assert_eq!(std::fs::read_link(&directory).unwrap(), absent_target);
    assert!(
        std::fs::symlink_metadata(&absent_target).is_err(),
        "the call creates no directory of its own"
    );
    std::fs::remove_dir_all(&home).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_refused_create_removes_only_the_directory_this_call_made() {
    use std::os::unix::fs::DirBuilderExt;
    // a home on tmpfs: `make_empty`'s own `0700` mkdir succeeds and the kernel
    // guard then refuses the filesystem
    let home = scratch(Path::new("/dev/shm"), "tmpfs-home");
    layerfs_home::home::layout::create(&home).unwrap();
    let name = StoreName::parse("tmp").unwrap();
    let refusal = layerfs_store::sqlite::create::create(&home, &name).unwrap_err();
    assert_eq!(refusal.code, "Unsupported", "{}", refusal.message);
    assert!(
        refusal.message.contains("ext4, xfs or btrfs"),
        "{}",
        refusal.message
    );
    let directory = paths::store_dir(&home, &name);
    assert!(
        !directory.exists(),
        "the empty directory this call made is reclaimed"
    );
    assert!(
        paths::store_dir(&home, &name).parent().unwrap().is_dir(),
        "store/ itself stays"
    );

    // a directory this call did not make, and what it holds, stay
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let marker = directory.join("another callers file");
    std::fs::write(&marker, b"another caller's file").unwrap();
    let refusal = layerfs_store::sqlite::create::create(&home, &name).unwrap_err();
    assert_eq!(refusal.code, "Unsupported", "{}", refusal.message);
    assert!(directory.is_dir(), "the other caller's directory stays");
    assert_eq!(std::fs::read(&marker).unwrap(), b"another caller's file");
    std::fs::remove_dir_all(home).unwrap();
}
