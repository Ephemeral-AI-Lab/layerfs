//! The fresh file's size during the §7 bulk load.
//!
//! SQLite writes the load one page per `pwrite`. On APFS a page write that
//! also moves the end of the file costs about four times one that lands inside
//! an already sized, already allocated file (measured: 1 GiB of 4 KiB writes
//! in about 1.05 s extending, 0.58 s into a sized hole, 0.28 s into a sized and
//! preallocated file). The rebuild knows the retained payload before it copies
//! a byte, so it sizes the file once instead of 250,000 times per gigabyte.
//!
//! Ext4 showed no difference between the three shapes, so only macOS sizes the
//! file. Nothing here changes a byte SQLite reads back: the sized region is a
//! hole the load then fills, and [`trim`] cuts whatever the load did not reach.

use super::types::io_error;
use layerfs_history::error::HistoryResult;
use std::path::Path;

/// Sizes the fresh file to `bytes` before the bulk load, and on macOS asks
/// APFS for that much space in one allocation.
///
/// `bytes` is rounded down to whole pages and never shrinks the file. The
/// allocation request is advisory: a volume that refuses it leaves a sparse
/// file of the same size, and a load that really is out of space still meets
/// its own `Capacity` refusal at the write.
#[cfg(target_os = "macos")]
pub fn reserve(target: &Path, bytes: u64, page_size: u64) -> HistoryResult<()> {
    use nix::fcntl::fcntl;
    use nix::fcntl::FcntlArg;

    let bytes = bytes - bytes % page_size.max(1);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(target)
        .map_err(io_error)?;
    let present = file.metadata().map_err(io_error)?.len();
    if bytes <= present {
        return Ok(());
    }
    if let Ok(length) = i64::try_from(bytes - present) {
        let mut request = nix::libc::fstore_t {
            fst_flags: nix::libc::F_ALLOCATEALL,
            fst_posmode: nix::libc::F_PEOFPOSMODE,
            fst_offset: 0,
            fst_length: length,
            fst_bytesalloc: 0,
        };
        let _ = fcntl(&file, FcntlArg::F_PREALLOCATE(&mut request));
    }
    file.set_len(bytes).map_err(io_error)
}

/// Other platforms leave the file to grow with the load.
#[cfg(not(target_os = "macos"))]
pub fn reserve(_target: &Path, _bytes: u64, _page_size: u64) -> HistoryResult<()> {
    Ok(())
}

/// Cuts the closed fresh file back to the `pages` its header counts.
///
/// A load that wrote less than [`reserve`] sized leaves zero pages past the
/// database's own end; SQLite ignores them, but they would be installed and
/// counted as Store bytes. Returns the file's final size.
pub fn trim(target: &Path, pages: u64, page_size: u64) -> HistoryResult<u64> {
    let length = std::fs::metadata(target).map_err(io_error)?.len();
    let counted = pages.saturating_mul(page_size);
    if counted == 0 || length <= counted {
        return Ok(length);
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(target)
        .and_then(|file| file.set_len(counted))
        .map_err(io_error)?;
    Ok(counted)
}
