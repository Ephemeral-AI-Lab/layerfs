//! The build identity `home.get` compares the Linux bundle and the native
//! daemon with: `LAYERFS_BUILD_COMMIT` and `LAYERFS_BUILD_DIRTY` as the
//! build tools set them (`tools/worktree.py`), else unknown. Unlike the
//! `layerfs` command's, it never asks Git, so editing a file does not
//! rebuild the library.
fn main() {
    for name in ["LAYERFS_BUILD_COMMIT", "LAYERFS_BUILD_DIRTY"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let commit = std::env::var("LAYERFS_BUILD_COMMIT")
        .ok()
        .filter(|c| c.len() == 40 && c.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or_default();
    let dirty = std::env::var("LAYERFS_BUILD_DIRTY").map_or(false, |d| d == "true");
    println!("cargo:rustc-env=LAYERFS_API_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=LAYERFS_API_BUILD_DIRTY={dirty}");
}
