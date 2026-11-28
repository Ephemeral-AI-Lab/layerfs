//! A call names a LayerFS root, and the home is the root itself only while
//! it holds `layerfs.json` (else `<root>/home`). Tests name a folder they
//! use as the home itself: an existing empty folder is made a home first,
//! as the first call into it made it before roots, so naming it as the
//! root resolves to it. A missing path or a folder that holds anything is
//! left as it is, and a call that names it uses `<it>/home`.
#![allow(dead_code)]
use std::path::Path;

/// `home` as the root that names it, made a home first when it is an
/// existing empty folder.
pub fn root_of<P: AsRef<Path> + ?Sized>(home: &P) -> &Path {
    let home = home.as_ref();
    let empty = std::fs::read_dir(home).is_ok_and(|mut entries| entries.next().is_none());
    if empty {
        let _ = layerfs_api::home::ensure(home);
    }
    home
}

/// `home` made a home if it is not one yet (its folder created when
/// missing), as the root that names it: for a test's `home.init` on a
/// fresh path, which then answers `unchanged`.
pub fn made<P: AsRef<Path> + ?Sized>(home: &P) -> &Path {
    let home = home.as_ref();
    if !home.join("layerfs.json").exists() {
        // private, as a call that makes a home makes it
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(home)
            .unwrap();
        layerfs_api::home::ensure(home).unwrap();
    }
    home
}
