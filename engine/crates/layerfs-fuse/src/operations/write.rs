//! WRITE at the kernel's offset and SETATTR (truncate, chmod, utimens).
use super::{
    mutation::{Declined, MutationInput},
    unsupported,
};
use layerfs_overlay::WRITE_WINDOW;
use layerfs_workspace::{Operation, Refusal, Time};
use std::time::{SystemTime, UNIX_EPOCH};

/// Permission and special bits a mode request can carry; the file type bits
/// the kernel includes are not part of the change.
const MODE_BITS: u32 = 0o7777;

/// One WRITE window, copied once. The offset is always the kernel's, which has
/// already resolved `O_APPEND` under its inode lock; the engine never chooses
/// an end position on this path. `cached` marks a shared-mapping store.
pub fn write(offset: u64, data: &[u8], cached: bool) -> Result<MutationInput, Declined> {
    if data.len() > WRITE_WINDOW {
        return Err(Declined::Refused(Refusal::Invalid));
    }
    Ok(MutationInput::Write {
        offset,
        data: data.into(),
        cached,
    })
}
/// The fields of one SETATTR the portable metadata grammar can hold. Access
/// time is not stored (the mount is `noatime`) and change time is the engine's.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttributeChange {
    pub mode: Option<u32>,
    pub owner: Option<u32>,
    pub group: Option<u32>,
    pub size: Option<u64>,
    pub mtime: Option<Stamp>,
}
/// A requested modification time: a caller's value or the processing time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stamp {
    At(Time),
    Processing,
}
/// `identity` is the projected owner of every inode. `handle` is true when the
/// kernel named a descriptor: a size change through it keeps working after the
/// last name is gone. Set-id bits are refused by Workspace's mode grammar.
pub fn set_attributes(
    serial: u64,
    handle: bool,
    identity: (u32, u32),
    change: AttributeChange,
    now: Time,
) -> Result<MutationInput, Declined> {
    unsupported::ownership(identity, change.owner, change.group)?;
    let mode = change.mode.map(|mode| mode & MODE_BITS);
    let mtime = change.mtime.map(|stamp| match stamp {
        Stamp::At(time) => time,
        Stamp::Processing => now,
    });
    Ok(if handle && change.size.is_some() {
        MutationInput::Attributes {
            mode,
            mtime,
            size: change.size,
        }
    } else {
        MutationInput::Named(Operation::SetAttributes {
            serial,
            mode,
            mtime,
            size: change.size,
        })
    })
}
/// Signed portable time of one wall-clock instant; None when unrepresentable.
pub fn time(instant: SystemTime) -> Option<Time> {
    match instant.duration_since(UNIX_EPOCH) {
        Ok(after) => Some(Time {
            seconds: i64::try_from(after.as_secs()).ok()?,
            nanoseconds: after.subsec_nanos(),
        }),
        Err(before) => {
            let before = before.duration();
            let whole = i64::try_from(before.as_secs()).ok()?;
            let nanos = before.subsec_nanos();
            Some(if nanos == 0 {
                Time {
                    seconds: whole.checked_neg()?,
                    nanoseconds: 0,
                }
            } else {
                Time {
                    seconds: whole.checked_neg()?.checked_sub(1)?,
                    nanoseconds: 1_000_000_000 - nanos,
                }
            })
        }
    }
}
