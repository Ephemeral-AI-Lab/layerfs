use super::super::contract::EvalResult;
use super::super::error::{display_error, io_error};
use super::super::evidence::digest::{ascii_argument, digest_file};
use super::super::evidence::process::fd_count;
use super::super::parity::row::clone_store;
use super::super::row::run::run_one;
use super::contract::AttributionArm;
use super::equations::attribution_row_json;
use super::native::validate_attribution_observation;
use super::observation::{run_attribution_one, AttributionObservation};
use crate::legacy_full::{IntegrityMode, LayerFs};
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn attribution_block(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    arm: &OsStr,
    work: &Path,
    identity: &OsStr,
) -> EvalResult<()> {
    attribution_block_with_mode(
        store,
        source,
        size_mib,
        arm,
        work,
        identity,
        IntegrityMode::Verified,
    )
}

pub fn trusted_block(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    work: &Path,
    identity: &OsStr,
) -> EvalResult<()> {
    attribution_block_with_mode(
        store,
        source,
        size_mib,
        OsStr::new("complete"),
        work,
        identity,
        IntegrityMode::TrustedLocalDev,
    )
}

pub(in crate::stage1_materialize) fn attribution_block_with_mode(
    store: &Path,
    source: &Path,
    size_mib: &OsStr,
    arm: &OsStr,
    work: &Path,
    identity: &OsStr,
    mode: IntegrityMode,
) -> EvalResult<()> {
    let size_mib = size_mib
        .to_str()
        .ok_or_else(|| "size-mib is not UTF-8".to_owned())?
        .parse::<u64>()
        .map_err(|error| format!("invalid size-mib: {error}"))?;
    if !matches!(size_mib, 0 | 24 | 96) {
        return Err("size-mib must be exactly 0, 24, or 96".to_owned());
    }
    let arm = AttributionArm::parse(arm)?;
    let identity = ascii_argument(identity, "identity")?;
    let expected_bytes = size_mib
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "fixture byte length overflow".to_owned())?;
    let source_metadata = fs::metadata(source).map_err(io_error)?;
    if !source_metadata.is_file() || source_metadata.len() != expected_bytes {
        return Err("source fixture length mismatch".to_owned());
    }
    let source_digest = digest_file(source)?;
    fs::create_dir(work).map_err(io_error)?;
    let store_clone = work.join("store");
    clone_store(store, &store_clone)?;
    let process_fd_baseline = fd_count()?;
    let opened = LayerFs::open_with_integrity(&store_clone, mode).map_err(display_error)?;
    let root = opened.ref_state.root;

    let primer = AttributionObservation {
        row: run_one(
            &opened.fs,
            root,
            source,
            &source_digest,
            &source_metadata,
            expected_bytes,
            &work.join("primer"),
            process_fd_baseline,
        )?,
        sink_write_calls: 0,
        sink_write_ns: 0,
        digest_sink_hash_bytes: None,
    };
    validate_attribution_observation(AttributionArm::Complete, expected_bytes, &primer, mode)?;
    println!(
        "{}",
        attribution_row_json(
            "warmup",
            arm,
            AttributionArm::Complete,
            0,
            identity,
            size_mib,
            root,
            &source_digest,
            &primer,
            mode,
        )?
    );
    std::io::stdout().flush().map_err(io_error)?;

    let mut measured = Vec::with_capacity(3);
    for ordinal in 1..=3 {
        let observation = run_attribution_one(
            &opened.fs,
            root,
            arm,
            source,
            &source_digest,
            &source_metadata,
            expected_bytes,
            &work.join(format!("measured-{ordinal}")),
            process_fd_baseline,
        )?;
        validate_attribution_observation(arm, expected_bytes, &observation, mode)?;
        measured.push(observation);
    }
    drop(opened);
    fs::remove_dir_all(&store_clone).map_err(io_error)?;
    fs::remove_dir(work).map_err(io_error)?;
    let terminal_fd = fd_count()?;
    let last = measured
        .last_mut()
        .ok_or_else(|| "missing measured rows".to_owned())?;
    last.row.fd_terminal = Some(terminal_fd);
    last.row.connections_terminal = Some(0);
    last.row.scratch_connections_terminal = Some(0);
    last.row.total_connections_terminal = Some(0);
    for (index, observation) in measured.iter().enumerate() {
        println!(
            "{}",
            attribution_row_json(
                "measured",
                arm,
                arm,
                index + 1,
                identity,
                size_mib,
                root,
                &source_digest,
                observation,
                mode,
            )?
        );
    }
    std::io::stdout().flush().map_err(io_error)
}
