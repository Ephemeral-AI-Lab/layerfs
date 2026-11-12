//! `store.create`'s directory (#570 P1): `make_empty` hands back whether this
//! call's own `mkdir` made `store/<name>`, and only that ownership may reclaim
//! the directory when nothing was published. The guard refuses tmpfs by
//! filesystem, after the `mkdir` succeeded, so it shows both sides.
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
