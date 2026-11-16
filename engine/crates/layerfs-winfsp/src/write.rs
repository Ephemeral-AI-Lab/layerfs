//! The ports-backed Write face: one mutation over KernelMutation.
use crate::services::{block_on, Context};
use layerfs_mount_ports::operations::{KernelMutation, MutationRequest};
use layerfs_mount_ports::operations::write as write_op;
use layerfs_workspace::Time;
use std::sync::atomic::{AtomicU64, Ordering};

/// Fresh request ids for the mutation face.
static REQUEST: AtomicU64 = AtomicU64::new(1);

/// Writes `data` at `offset` of `handle` (serial), answering the end offset.
pub fn write(context: &Context, serial: u64, handle: u64, offset: u64, data: Vec<u8>) -> Result<u64, String> {
    let services = context.request()?;
    let input = write_op::write(offset, &data, false)
        .map_err(|declined| format!("write input: {declined:?}"))?;
    let request = MutationRequest {
        mount: context.mount,
        request: REQUEST.fetch_add(1, Ordering::Relaxed),
        protected: serial,
        handle: Some(handle),
        input,
        open: None,
        now: write_op::time(std::time::SystemTime::now()).ok_or("clock before the epoch")?,
    };
    let mutation = block_on(KernelMutation::perform(services, request))
        .map_err(|failure| format!("mutation: {failure}"))?;
    match mutation.value() {
        Ok(published) => Ok(offset + data.len() as u64),
        Err(declined) => Err(format!("declined: {declined:?}")),
    }
}
