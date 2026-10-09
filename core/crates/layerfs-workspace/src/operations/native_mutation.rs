//! The original result of one native mutation and the kernel custody that
//! its own reply hands over.
use crate::{JobOutcome, Operation, WorkspaceResult};
use layerfs_overlay::{NativeEffect, OpenFile};

/// The original owner result. A descriptor is present only for an applied
/// create-and-open; it is usable only when `result` is the applied outcome.
#[derive(Debug)]
pub struct NativeMutationOutcome {
    pub result: WorkspaceResult<JobOutcome>,
    pub file: Option<OpenFile>,
}
/// The kernel custody an applied reply creates. Every entry-bearing
/// operation takes exactly one lookup reference on the bound inode.
pub(crate) fn effect(
    operation: &Operation,
    fresh: Option<u64>,
    open: Option<bool>,
) -> NativeEffect {
    let entry = |serial, parent, directory| NativeEffect::Entry {
        serial,
        parent,
        directory,
    };
    match (operation, fresh) {
        (Operation::Create { parent, .. }, Some(serial)) => match open {
            Some(writable) => NativeEffect::Open {
                serial,
                parent: *parent,
                writable,
            },
            None => entry(serial, *parent, false),
        },
        (Operation::Mkdir { parent, .. }, Some(serial)) => entry(serial, *parent, true),
        (Operation::Symlink { parent, .. }, Some(serial)) => entry(serial, *parent, false),
        (Operation::Link { serial, parent, .. }, _) => entry(*serial, *parent, false),
        _ => NativeEffect::None,
    }
}
