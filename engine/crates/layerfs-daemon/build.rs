//! The build identity the daemon binary carries, so a client can tell which
//! build it is without running it (`layerfs home get`):
//! `LAYERFS_BUILD_COMMIT` and `LAYERFS_BUILD_DIRTY` as the build tools set
//! them (`tools/worktree.py`), else `unknown`. It never asks Git, so editing
//! a file does not rebuild the daemon.
fn main() {
    for name in ["LAYERFS_BUILD_COMMIT", "LAYERFS_BUILD_DIRTY"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let commit = std::env::var("LAYERFS_BUILD_COMMIT")
        .ok()
        .filter(|c| c.len() == 40 && c.bytes().all(|b| b.is_ascii_hexdigit()));
    let identity = match (commit, std::env::var("LAYERFS_BUILD_DIRTY").as_deref()) {
        (Some(commit), Ok("true")) => format!("{commit}:dirty"),
        (Some(commit), Ok("false")) => format!("{commit}:clean"),
        _ => "unknown".to_owned(),
    };
    println!("cargo:rustc-env=LAYERFS_DAEMON_BUILD_IDENTITY={identity}");
}
