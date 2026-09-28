//! View-lease terminal results shared by their distinct response tags.
use super::response::code;
use super::{Decoder, Encoder};
use crate::contract::*;

pub(super) fn put_entry(e: &mut Encoder, entry: &WorkspaceViewEntryWire) -> Result<(), Failure> {
    entry.validate()?;
    e.u64(entry.serial)?;
    e.u8(entry.kind)?;
    e.u64(entry.size)?;
    e.u64(entry.references)?;
    e.u32(entry.mode)?;
    e.u64(entry.mtime_seconds as u64)?;
    e.u32(entry.mtime_nanoseconds)?;
    Ok(())
}
pub(super) fn take_entry(d: &mut Decoder<'_>) -> Result<WorkspaceViewEntryWire, Failure> {
    let entry = WorkspaceViewEntryWire {
        serial: d.u64()?,
        kind: d.u8()?,
        size: d.u64()?,
        references: d.u64()?,
        mode: d.u32()?,
        mtime_seconds: d.u64()? as i64,
        mtime_nanoseconds: d.u32()?,
    };
    entry.validate()?;
    Ok(entry)
}
pub(super) fn put_lease(e: &mut Encoder, result: &WorkspaceViewLeaseWire) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    e.blob(&result.view)?;
    put_entry(e, &result.root)?;
    e.u64(result.generation)?;
    e.u64(result.revision)?;
    e.put(&result.base)?;
    Ok(())
}
pub(super) fn take_lease(d: &mut Decoder<'_>) -> Result<WorkspaceViewLeaseWire, Failure> {
    let result = WorkspaceViewLeaseWire {
        workspace: d.blob(WORKSPACE_ID_BYTES)?,
        incarnation: d.root()?,
        view: d.blob(VIEW_LEASE_TOKEN_BYTES)?,
        root: take_entry(d)?,
        generation: d.u64()?,
        revision: d.u64()?,
        base: d.root()?,
    };
    result.validate()?;
    Ok(result)
}
pub(super) fn put_list(e: &mut Encoder, result: &WorkspaceViewListWire) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    e.blob(&result.view)?;
    e.u16(result.entries.len() as u16)?;
    for (name, serial) in &result.entries {
        e.blob(name)?;
        e.u64(*serial)?;
    }
    match &result.continuation {
        Some(name) => {
            e.u8(1)?;
            e.blob(name)?;
        }
        None => e.u8(0)?,
    }
    Ok(())
}
pub(super) fn take_list(d: &mut Decoder<'_>) -> Result<WorkspaceViewListWire, Failure> {
    let result = WorkspaceViewListWire {
        workspace: d.blob(WORKSPACE_ID_BYTES)?,
        incarnation: d.root()?,
        view: d.blob(VIEW_LEASE_TOKEN_BYTES)?,
        entries: {
            let n = d.count(VIEW_LIST_ENTRIES, 10)?;
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push((d.blob(VIEW_NAME_BYTES)?, d.u64()?));
            }
            entries
        },
        continuation: match d.u8()? {
            0 => None,
            1 => Some(d.blob(VIEW_NAME_BYTES)?),
            _ => return Err(Code::InvalidInput.into()),
        },
    };
    result.validate()?;
    Ok(result)
}
pub(super) fn put_read(e: &mut Encoder, result: &WorkspaceViewReadWire) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    e.blob(&result.view)?;
    e.u64(result.serial)?;
    e.u16(result.bytes.len() as u16)?;
    e.put(&result.bytes)?;
    e.u8(u8::from(result.eof))?;
    e.u64(result.size)?;
    Ok(())
}
pub(super) fn take_read(d: &mut Decoder<'_>) -> Result<WorkspaceViewReadWire, Failure> {
    let workspace = d.blob(WORKSPACE_ID_BYTES)?;
    let incarnation = d.root()?;
    let view = d.blob(VIEW_LEASE_TOKEN_BYTES)?;
    let serial = d.u64()?;
    let length = d.u16()? as usize;
    let bytes = d.take(length)?.to_vec();
    let result = WorkspaceViewReadWire {
        workspace,
        incarnation,
        view,
        serial,
        bytes,
        eof: d.u8()? == 1,
        size: d.u64()?,
    };
    result.validate()?;
    Ok(result)
}
pub(super) fn put_readlink(
    e: &mut Encoder,
    result: &WorkspaceViewReadlinkWire,
) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    e.blob(&result.view)?;
    e.blob(&result.target)?;
    Ok(())
}
pub(super) fn take_readlink(d: &mut Decoder<'_>) -> Result<WorkspaceViewReadlinkWire, Failure> {
    let result = WorkspaceViewReadlinkWire {
        workspace: d.blob(WORKSPACE_ID_BYTES)?,
        incarnation: d.root()?,
        view: d.blob(VIEW_LEASE_TOKEN_BYTES)?,
        target: d.blob(VIEW_SYMLINK_BYTES)?,
    };
    result.validate()?;
    Ok(result)
}
pub(super) fn put_status(e: &mut Encoder, result: &WorkspaceViewStatusWire) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    e.blob(&result.view)?;
    e.u64(result.generation)?;
    e.u64(result.revision)?;
    e.u64(result.entries)?;
    e.u64(result.held_leases)?;
    Ok(())
}
pub(super) fn take_status(d: &mut Decoder<'_>) -> Result<WorkspaceViewStatusWire, Failure> {
    let result = WorkspaceViewStatusWire {
        workspace: d.blob(WORKSPACE_ID_BYTES)?,
        incarnation: d.root()?,
        view: d.blob(VIEW_LEASE_TOKEN_BYTES)?,
        generation: d.u64()?,
        revision: d.u64()?,
        entries: d.u64()?,
        held_leases: d.u64()?,
    };
    result.validate()?;
    Ok(result)
}
pub(super) fn put_release(
    e: &mut Encoder,
    result: &WorkspaceViewReleaseWire,
) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    e.blob(&result.view)?;
    match result.outcome {
        WorkspaceViewReleaseOutcome::Completed => e.u8(0)?,
        WorkspaceViewReleaseOutcome::Retained(cause) => {
            e.u8(1)?;
            e.u8(cause as u8)?;
        }
    }
    Ok(())
}
pub(super) fn take_release(d: &mut Decoder<'_>) -> Result<WorkspaceViewReleaseWire, Failure> {
    let result = WorkspaceViewReleaseWire {
        workspace: d.blob(WORKSPACE_ID_BYTES)?,
        incarnation: d.root()?,
        view: d.blob(VIEW_LEASE_TOKEN_BYTES)?,
        outcome: match d.u8()? {
            0 => WorkspaceViewReleaseOutcome::Completed,
            1 => WorkspaceViewReleaseOutcome::Retained(code(d.u8()?)?),
            _ => return Err(Code::InvalidInput.into()),
        },
    };
    result.validate()?;
    Ok(result)
}
