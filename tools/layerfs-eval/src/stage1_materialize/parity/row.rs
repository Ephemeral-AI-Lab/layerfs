use super::super::contract::EvalResult;
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::digest_file;
use super::super::evidence::process::fd_count;
use super::super::row::output::print_row;
use super::super::row::run::run_one;
use crate::legacy_full::LayerFs;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

pub fn parity_row(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    work: &Path,
    identity: &OsStr,
) -> EvalResult<()> {
    let size_mib = size_mib
        .to_str()
        .ok_or_else(|| "size-mib is not UTF-8".to_owned())?
        .parse::<u64>()
        .map_err(|error| format!("invalid size-mib: {error}"))?;
    let identity = identity
        .to_str()
        .filter(|value| {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .ok_or_else(|| "identity must be a nonempty ASCII identifier".to_owned())?;
    let expected_bytes = size_mib
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "fixture byte length overflow".to_owned())?;
    if !matches!(size_mib, 0 | 24 | 96) {
        return Err("size-mib must be exactly 0, 24, or 96".to_owned());
    }
    let source_metadata = fs::metadata(source).map_err(io_error)?;
    if !source_metadata.is_file() || source_metadata.len() != expected_bytes {
        return Err(format!(
            "source fixture length mismatch: expected {expected_bytes}, got {}",
            source_metadata.len()
        ));
    }
    let source_digest = digest_file(source)?;
    fs::create_dir(work).map_err(io_error)?;
    let store_clone = work.join("store");
    clone_store(store, &store_clone)?;

    let process_fd_baseline = fd_count()?;
    let opened = LayerFs::open(&store_clone).map_err(display_error)?;
    let root = opened.ref_state.root;
    let primer = run_one(
        &opened.fs,
        root,
        source,
        &source_digest,
        &source_metadata,
        expected_bytes,
        &work.join("primer"),
        process_fd_baseline,
    )?;
    print_row("warmup", identity, size_mib, root, &source_digest, &primer)?;
    std::io::stdout().flush().map_err(io_error)?;

    let mut measured = run_one(
        &opened.fs,
        root,
        source,
        &source_digest,
        &source_metadata,
        expected_bytes,
        &work.join("measured"),
        process_fd_baseline,
    )?;
    drop(opened);
    fs::remove_dir_all(&store_clone).map_err(io_error)?;
    fs::remove_dir(work).map_err(io_error)?;
    measured.fd_terminal = Some(fd_count()?);
    measured.connections_terminal = Some(0);
    measured.scratch_connections_terminal = Some(0);
    measured.total_connections_terminal = Some(0);
    print_row(
        "measured",
        identity,
        size_mib,
        root,
        &source_digest,
        &measured,
    )?;
    std::io::stdout().flush().map_err(io_error)?;
    Ok(())
}

pub(in crate::stage1_materialize) fn clone_store(
    source: &Path,
    destination: &Path,
) -> EvalResult<()> {
    let status = Command::new("/bin/cp")
        .arg("-cR")
        .arg(source)
        .arg(destination)
        .status()
        .map_err(io_error)?;
    if !status.success() {
        return Err(format!("APFS Store clone exited {status}"));
    }
    crate::stage1_fixture::make_writable(destination)
}
