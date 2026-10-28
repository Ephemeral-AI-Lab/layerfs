//! The original result of one kernel mutation and the kernel custody that
//! its own reply hands over.
use crate::{JobOutcome, Operation, WorkspaceResult};
use layerfs_overlay::{KernelEffect, OpenFile};

/// The original owner result. A descriptor is present only for an applied
/// create-and-open; it is usable only when `result` is the applied outcome.
#[derive(Debug)]
pub struct KernelMutationOutcome {
    pub result: WorkspaceResult<JobOutcome>,
    pub file: Option<OpenFile>,
}
/// The kernel custody an applied reply creates. Every entry-bearing
/// operation takes exactly one lookup reference on the bound inode.
pub(crate) fn effect(
    operation: &Operation,
    fresh: Option<u64>,
    open: Option<bool>,
) -> KernelEffect {
    let entry = |serial, parent, directory| KernelEffect::Entry {
        serial,
        parent,
        directory,
    };
    match (operation, fresh) {
        (Operation::Create { parent, .. }, Some(serial)) => match open {
            Some(writable) => KernelEffect::Open {
                serial,
                parent: *parent,
                writable,
            },
            None => entry(serial, *parent, false),
        },
        (Operation::Mkdir { parent, .. }, Some(serial)) => entry(serial, *parent, true),
        (Operation::Symlink { parent, .. }, Some(serial)) => entry(serial, *parent, false),
        (Operation::Link { serial, parent, .. }, _) => entry(*serial, *parent, false),
        _ => KernelEffect::None,
    }
}
