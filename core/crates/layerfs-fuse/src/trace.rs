//! Env-gated per-callback diagnostics for the mounted adapter.
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

/// Prints one line per callback the adapter admitted.
///
/// The aggregate Status counters cannot report this sequence, because a
/// namespace mutation is counted in the same `write` slot as a data write, and
/// a caller that only sees the receipt cannot separate them. Off unless
/// `LAYERFS_FUSE_TRACE` is set, and allocation-free when it is not.
pub(crate) fn trace(operation: &str, name: Option<&OsStr>, detail: &str) {
    if std::env::var_os("LAYERFS_FUSE_TRACE").is_none() {
        return;
    }
    let name = name.map_or_else(String::new, |name| {
        format!(" name={}", String::from_utf8_lossy(name.as_bytes()))
    });
    eprintln!("LFS_FUSE_CALLBACK v=1 op={operation}{name} {detail}");
}
