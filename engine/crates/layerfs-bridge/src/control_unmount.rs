//! Unmount with an explicit discard, and the Dirty refusal, on the control wire.
//!
//! Unmount refuses a Workspace that holds uncommitted changes unless the
//! caller discards them. The refusal names the changed paths while they fit
//! one control record and says when some were left out.
use crate::{
    control::{ControlError, WorkspaceToken},
    control_history::{boolean, put_token, token},
    control_merge::{lines, put_lines},
    wire::{Reader, Writer},
};

/// One Unmount request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnmountControl {
    /// Workspace token.
    pub token: WorkspaceToken,
    /// Close even when uncommitted changes would be lost.
    pub discard: bool,
}

/// Uncommitted changes that refused an Unmount; nothing was closed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirtyReport {
    /// Workspace token.
    pub token: WorkspaceToken,
    /// Changed paths, in path order.
    pub paths: Vec<String>,
    /// Changed inodes not yet captured by a Commit.
    pub inodes: u64,
    /// Changed names not yet captured by a Commit.
    pub entries: u64,
    /// True when an unpublished merge with conflict markers is installed.
    pub pending_merge: bool,
    /// True when some paths were not named.
    pub truncated: bool,
}

pub(crate) fn put_unmount(out: &mut Writer, value: &UnmountControl) -> Result<(), ControlError> {
    put_token(out, value.token)?;
    out.byte(u8::from(value.discard))
}

pub(crate) fn unmount(input: &mut Reader<'_>) -> Result<UnmountControl, ControlError> {
    Ok(UnmountControl {
        token: token(input)?,
        discard: boolean(input)?,
    })
}

pub(crate) fn put_dirty(out: &mut Writer, value: &DirtyReport) -> Result<(), ControlError> {
    put_token(out, value.token)?;
    out.put(&value.inodes.to_be_bytes())?;
    out.put(&value.entries.to_be_bytes())?;
    out.byte(u8::from(value.pending_merge))?;
    let cut = put_lines(out, &value.paths)?;
    out.byte(u8::from(value.truncated || cut))
}

pub(crate) fn dirty(input: &mut Reader<'_>) -> Result<DirtyReport, ControlError> {
    Ok(DirtyReport {
        token: token(input)?,
        inodes: u64::from_be_bytes(input.array()?),
        entries: u64::from_be_bytes(input.array()?),
        pending_merge: boolean(input)?,
        paths: lines(input)?,
        truncated: boolean(input)?,
    })
}
