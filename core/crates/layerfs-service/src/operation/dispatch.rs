//! The closed logical dispatch has no socket or native configuration dependency.
//!
//! Classification is exhaustive and semantic: a read-only request, a content
//! mutation and a metadata-only history command each take their own path. A
//! metadata command never reaches the content save path, and no branch is
//! selected by comparing an opcode to a number.
use super::{history, read, write};
use layerfs_bridge::contract::*;
use layerfs_history::HistoryCatalog;
use layerfs_storage::Store;
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{
    io::{Read, Write},
    path::Path,
    time::Instant,
};

pub(crate) fn dispatch(
    store: &Store,
    catalog: Option<&dyn HistoryCatalog>,
    import_root: Option<&Path>,
    r: &Request,
    input: &mut dyn Read,
    output: &mut dyn Write,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<Response, Failure> {
    match &r.operation {
        Operation::WorkspaceStatus { .. }
        | Operation::WorkspaceUnmount { .. }
        | Operation::WorkspaceCloseClean { .. }
        | Operation::WorkspaceCommit { .. }
        | Operation::WorkspaceAttach { .. }
        | Operation::WorkspaceMount { .. } => Err(Code::Unsupported.into()),
        Operation::HistoryQuery(query) => {
            end_input(input)?;
            history::query(catalog.ok_or(Code::Unsupported)?, query, store)
        }
        Operation::HistoryCommand(command) => {
            end_input(input)?;
            history::command(
                catalog.ok_or(Code::Unsupported)?,
                store,
                import_root,
                command,
                deadline,
                scope,
            )
        }
        // A known successful C2 finish is never changed into a claimed abort.
        Operation::ConstructFile { .. }
        | Operation::ConstructSymlink { .. }
        | Operation::EditFile { .. }
        | Operation::UpdatePortableMetadata { .. }
        | Operation::ConstructPortableMetadata { .. }
        | Operation::UpdatePreparedFilesystem { .. } => {
            write::mutate(store, r, input, deadline, scope)
        }
        Operation::ReadFile { .. } | Operation::Inspect { .. } => {
            end_input(input)?;
            read::read(store, r, output, scope)
        }
    }
}

pub(crate) fn end_input(input: &mut dyn Read) -> Result<(), Failure> {
    let mut byte = [0; 1];
    if input.read(&mut byte)? != 0 {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}
