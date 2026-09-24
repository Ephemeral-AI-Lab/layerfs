//! Linux LFS2/LFE2 carrier; Workspace owns the projected range mutation.
use crate::{
    adapter::{Adapter, CALLBACK_BUDGET},
    replies::{errno, serial},
};
use fuser::{Errno, FileHandle, INodeNo, IoctlFlags, ReplyIoctl, Request};
use layerfs_workspace::{
    filesystem::projection_counters::ProjectionOp, RangeEdit, RangeStamp, RangeState,
};
use std::time::Instant;

const STATE_CMD: u32 = 0xc058_f540;
const EDIT_CMD: u32 = 0x5060_f541;
const STATE_BYTES: usize = 88;
const EDIT_BYTES: usize = 4192;

struct Edit<'a> {
    stamp: RangeStamp,
    start: u64,
    end: u64,
    data: &'a [u8],
}

fn u16_at(input: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(input[at..at + 2].try_into().unwrap())
}
fn u32_at(input: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(input[at..at + 4].try_into().unwrap())
}
fn u64_at(input: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(input[at..at + 8].try_into().unwrap())
}

fn state_input(input: &[u8], out_size: u32, flags: IoctlFlags) -> Result<(), Errno> {
    if !flags.is_empty() || out_size != STATE_BYTES as u32 {
        return Err(Errno::EOPNOTSUPP);
    }
    if input.len() != STATE_BYTES || &input[..4] != b"LFS2" {
        return Err(Errno::EINVAL);
    }
    if u16_at(input, 4) != 2 || u16_at(input, 6) != 1 {
        return Err(Errno::EOPNOTSUPP);
    }
    if input[8..].iter().any(|byte| *byte != 0) {
        return Err(Errno::EINVAL);
    }
    Ok(())
}

fn edit_input(input: &[u8], out_size: u32, flags: IoctlFlags) -> Result<Edit<'_>, Errno> {
    if !flags.is_empty() || out_size != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    if input.len() != EDIT_BYTES || &input[..4] != b"LFE2" {
        return Err(Errno::EINVAL);
    }
    if u16_at(input, 4) != 2 || u16_at(input, 6) != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    let count = u32_at(input, 80) as usize;
    if count > 4096
        || input[84..96].iter().any(|byte| *byte != 0)
        || input[96 + count..].iter().any(|byte| *byte != 0)
    {
        return Err(Errno::EINVAL);
    }
    let start = u64_at(input, 64);
    let deleted = u64_at(input, 72);
    let end = start.checked_add(deleted).ok_or(Errno::EINVAL)?;
    if count == 0 && deleted == 0 {
        return Err(Errno::EINVAL);
    }
    let mut incarnation = [0; 32];
    incarnation.copy_from_slice(&input[16..48]);
    Ok(Edit {
        stamp: RangeStamp {
            inode: u64_at(input, 8),
            incarnation,
            generation: u64_at(input, 48),
            revision: u64_at(input, 56),
        },
        start,
        end,
        data: &input[96..96 + count],
    })
}

fn state_output(state: RangeState) -> [u8; STATE_BYTES] {
    let mut output = [0; STATE_BYTES];
    output[..4].copy_from_slice(b"LFS2");
    output[4..6].copy_from_slice(&2u16.to_le_bytes());
    output[8..16].copy_from_slice(&state.stamp.inode.to_le_bytes());
    output[16..48].copy_from_slice(&state.stamp.incarnation);
    output[48..56].copy_from_slice(&state.stamp.generation.to_le_bytes());
    output[56..64].copy_from_slice(&state.stamp.revision.to_le_bytes());
    output[64..72].copy_from_slice(&state.length.to_le_bytes());
    output[72..80].copy_from_slice(&state.mtime_seconds.to_le_bytes());
    output[80..84].copy_from_slice(&state.mtime_nanoseconds.to_le_bytes());
    output
}

pub(crate) fn dispatch(
    adapter: &Adapter,
    req: &Request,
    ino: INodeNo,
    fh: FileHandle,
    flags: IoctlFlags,
    cmd: u32,
    input: &[u8],
    out_size: u32,
    reply: ReplyIoctl,
) {
    let op = match cmd {
        STATE_CMD => ProjectionOp::RangeState,
        EDIT_CMD => ProjectionOp::RangeEdit,
        _ => {
            adapter
                .workspace
                .record_projection_call(ProjectionOp::Other);
            reply.error(Errno::ENOTTY);
            return;
        }
    };
    adapter.workspace.record_projection_call(op);
    let deadline = Instant::now() + CALLBACK_BUDGET;
    match cmd {
        STATE_CMD => {
            let permit = state_input(input, out_size, flags)
                .and_then(|()| adapter.guard(req))
                .and_then(|()| adapter.handle(ino, fh))
                .and_then(|()| {
                    adapter
                        .workspace
                        .begin_projection_reply(deadline)
                        .map_err(errno)
                });
            let result = permit.as_ref().map_err(|error| *error).and_then(|_| {
                adapter
                    .workspace
                    .projected_range_state(fh.0, serial(ino, adapter.root()))
                    .map(state_output)
                    .map_err(errno)
            });
            match result {
                Ok(output) => reply.ioctl(0, &output),
                Err(error) => reply.error(error),
            }
        }
        EDIT_CMD => {
            let parsed = edit_input(input, out_size, flags).and_then(|edit| {
                adapter.guard(req)?;
                if !adapter.writable {
                    return Err(Errno::EROFS);
                }
                adapter.handle(ino, fh)?;
                if edit.stamp.inode != serial(ino, adapter.root()) {
                    return Err(Errno::EBADF);
                }
                Ok(edit)
            });
            let mut permit = parsed.as_ref().map_err(|error| *error).and_then(|_| {
                adapter
                    .workspace
                    .begin_projection_mutation(deadline)
                    .map_err(errno)
            });
            let result = permit.as_mut().map_err(|error| *error).and_then(|permit| {
                let edit = parsed.as_ref().map_err(|error| *error)?;
                let current = adapter
                    .workspace
                    .projected_range_state(fh.0, serial(ino, adapter.root()))
                    .map_err(errno)?;
                if !current.writable {
                    return Err(Errno::EBADF);
                }
                if current.stamp != edit.stamp {
                    return Err(Errno::ESTALE);
                }
                let payload = adapter
                    .workspace
                    .own_payload(edit.data.len() as u64, &mut &edit.data[..], deadline)
                    .map_err(errno)?;
                permit
                    .edit_file_range(
                        fh.0,
                        edit.stamp,
                        &RangeEdit {
                            start: edit.start,
                            end: edit.end,
                            replacement: payload,
                        },
                        deadline,
                    )
                    .map_err(errno)
            });
            match result {
                Ok(_) => reply.ioctl(0, &[]),
                Err(error) => reply.error(error),
            }
        }
        _ => unreachable!(),
    }
}
