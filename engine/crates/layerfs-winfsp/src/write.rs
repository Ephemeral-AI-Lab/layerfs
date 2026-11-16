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
    let now = write_op::time(std::time::SystemTime::now()).ok_or("clock before the epoch")?;
    let input = write_op::write(offset, &data, false)
        .map_err(|declined| format!("write input: {declined:?}"))?;
    let request = MutationRequest {
        mount: context.mount,
        request: REQUEST.fetch_add(1, Ordering::Relaxed),
        protected: serial,
        handle: Some(handle),
        input,
        open: None,
        now,
    };
    let mutation = block_on(KernelMutation::perform(services, request))
        .map_err(|failure| format!("mutation: {failure}"))?;
    match mutation.value() {
        Ok(published) => Ok(offset + data.len() as u64),
        Err(declined) => Err(format!("declined: {declined:?}")),
    }
}

/// Truncates or grows `serial` to `size` (the attributes mutation).
pub fn truncate(context: &Context, serial: u64, size: u64) -> Result<(), String> {
    let services = context.request()?;
    let now = write_op::time(std::time::SystemTime::now()).ok_or("clock before the epoch")?;
    let input = write_op::set_attributes(
        serial,
        true,
        (0, 0),
        write_op::AttributeChange { mode: None, owner: None, group: None, size: Some(size), mtime: None },
        now,
    )
    .map_err(|declined| format!("truncate input: {declined:?}"))?;
    let request = MutationRequest {
        mount: context.mount,
        request: REQUEST.fetch_add(1, Ordering::Relaxed),
        protected: serial,
        handle: Some(serial),
        input,
        open: None,
        now: write_op::time(std::time::SystemTime::now()).ok_or("clock before the epoch")?,
    };
    let mutation = block_on(KernelMutation::perform(services, request))
        .map_err(|failure| format!("truncate: {failure}"))?;
    match mutation.value() {
        Ok(_published) => Ok(()),
        Err(declined) => Err(format!("declined: {declined:?}")),
    }
}