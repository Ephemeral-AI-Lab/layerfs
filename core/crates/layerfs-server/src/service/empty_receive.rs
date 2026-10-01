//! Actual v1 tag/EOF proof with no native receive spool or directory effects.
use super::error::content;
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::{
    rows::{SpoolPreparation, VerifiedEmptyRows},
    state::VerifiedEmptyState,
};
use std::{io::Read, time::Instant};
struct RefuseRows;
impl PreparedBindingSink for RefuseRows {
    fn begin_directory(&mut self, _parent: u64, _bindings: u32) -> Result<(), Failure> {
        Err(Code::InvalidInput.into())
    }
    fn binding(&mut self, _name: &[u8], _child: Option<u64>) -> Result<(), Failure> {
        Err(Code::InvalidInput.into())
    }
    fn end_directory(&mut self, _completion: PreparedDirectoryCompletion) -> Result<(), Failure> {
        Err(Code::InvalidInput.into())
    }
    fn identity(&mut self, _row: PreparedIdentity) -> Result<(), Failure> {
        Err(Code::InvalidInput.into())
    }
}
/// Preserve the original source issuer while the ordinary decoder checks tag/EOF.
pub(crate) fn receive(
    changes: &PreparedChanges,
    input: &mut dyn Read,
    preparation: SpoolPreparation,
    state: &mut VerifiedEmptyState,
    deadline: Instant,
) -> Result<VerifiedEmptyRows, Failure> {
    if !super::empty_admission::is_empty(changes)?
        || !preparation.declares_empty()
        || preparation.source_id() != state.subject().source_id()
        || state.subject().base().map(|root| *root.0.as_bytes()) != Some(changes.base)
        || state.subject().namespace().object().as_bytes() != &changes.scope
        || state.subject().root_serial() != changes.root_serial
    {
        return Err(Code::InvalidInput.into());
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    read_prepared_bindings(&changes.totals, changes.root_serial, input, &mut RefuseRows)?;
    let rows = preparation.verify_empty_eof(input).map_err(content)?;
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    state.confirm(&rows).map_err(content)?;
    Ok(rows)
}
