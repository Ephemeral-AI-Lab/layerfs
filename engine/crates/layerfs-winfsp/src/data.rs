//! The ports-backed Read face: one data window over KernelData.
use crate::services::{block_on, Context};
use layerfs_mount_ports::operations::{KernelData, ReadDataInput};
use std::sync::atomic::{AtomicU64, Ordering};

/// Fresh request ids for the data face.
static REQUEST: AtomicU64 = AtomicU64::new(1);

/// Reads one window of `handle` at `offset`, answering its bytes.
pub fn data(context: &Context, serial: u64, handle: u64, offset: u64, length: u32) -> Result<Vec<u8>, String> {
    let services = context.request()?;
    let request = REQUEST.fetch_add(1, Ordering::Relaxed);
    let outcome = block_on(KernelData::read(
        services,
        context.mount,
        request,
        serial,
        Some(handle),
        ReadDataInput::File { offset, length },
    ))
    .map_err(|failure| format!("data read: {failure}"))?;
    match outcome {
        Ok(data) => Ok(data.bytes().to_vec()),
        Err(_refusal) => Ok(Vec::new()),
    }
}
