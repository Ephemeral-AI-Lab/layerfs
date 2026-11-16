//! The named mutations: create, rename, remove over the ports operations.
use crate::services::{block_on, Context};
use layerfs_mount_ports::operations::{create as create_op, remove as remove_op, rename as rename_op, KernelMutation, MutationRequest, MutationInput};
use layerfs_mount_ports::operations::write as write_op;
use layerfs_content::PathName;
use std::sync::atomic::{AtomicU64, Ordering};

static REQUEST: AtomicU64 = AtomicU64::new(1);

/// Runs one named mutation input to publication.
fn named(context: &Context, protected: u64, handle: Option<u64>, input: layerfs_workspace::Operation) -> Result<(), String> {
    let services = context.request()?;
    let request = MutationRequest {
        mount: context.mount,
        request: REQUEST.fetch_add(1, Ordering::Relaxed),
        protected,
        handle,
        input: MutationInput::Named(input),
        open: None,
        now: write_op::time(std::time::SystemTime::now()).ok_or("clock before the epoch")?,
    };
    let mutation = block_on(KernelMutation::perform(services, request))
        .map_err(|failure| format!("mutation: {failure}"))?;
    match mutation.value() {
        Ok(_published) => Ok(()),
        Err(declined) => Err(format!("declined: {declined:?}")),
    }
}

/// Creates a file or directory `name` under `parent`.
pub fn create(context: &Context, parent: u64, name: &str, directory: bool) -> Result<(), String> {
    let name = PathName::from_bytes(name.as_bytes()).map_err(|e| format!("name: {e:?}"))?;
    let operation = if directory {
        create_op::mkdir(parent, name, 0o755)
    } else {
        create_op::create(parent, name, 0o644)
    };
    named(context, parent, None, operation)
}

/// Renames `name` under `parent` to `new_name` under `new_parent`.
pub fn rename(context: &Context, parent: u64, name: &str, new_parent: u64, new_name: &str, flags: u32) -> Result<(), String> {
    let name = PathName::from_bytes(name.as_bytes()).map_err(|e| format!("name: {e:?}"))?;
    let new_name = PathName::from_bytes(new_name.as_bytes()).map_err(|e| format!("name: {e:?}"))?;
    let operation = rename_op::rename(parent, name, new_parent, new_name, flags)
        .map_err(|declined| format!("rename input: {declined:?}"))?;
    named(context, parent, None, operation)
}

/// Removes `name` under `parent` (file or directory).
pub fn remove(context: &Context, parent: u64, name: &str, directory: bool) -> Result<(), String> {
    let name = PathName::from_bytes(name.as_bytes()).map_err(|e| format!("name: {e:?}"))?;
    let operation = if directory {
        remove_op::rmdir(parent, name)
    } else {
        remove_op::unlink(parent, name)
    };
    named(context, parent, None, operation)
}
