//! Arguments and results of the changing NFSv3 procedures (RFC 1813):
//! SETATTR, WRITE, CREATE, MKDIR, SYMLINK, MKNOD, REMOVE, RMDIR, RENAME,
//! LINK and COMMIT.
//!
//! Every directory a procedure changes is answered with `wcc_data`: its
//! attributes read just before the change and just after. The two reads are
//! separate visits, not atomic with the change; the connection serves one
//! call at a time, so they bracket it exactly unless another connection or
//! a Commit installs a change in between, in which case the client sees an
//! unexplained difference and drops its cache, which is safe.
//!
//! WRITE answers `FILE_SYNC`: every window was published to the Workspace
//! before the reply, as every FUSE write is. That is publication, not
//! durability; the write verifier changes with every export, so a client
//! that ever sent UNSTABLE data resends it to a restarted export.
use crate::{
    attributes::{post_op_maybe, pre_op, Status},
    change::{How, Sattr, Tickets},
    kernel::Kernel,
    xdr::{Garbage, Reader, Writer},
};
use layerfs_fuse::operations::write::Stamp;
use layerfs_workspace::{Time, ViewStat};

const FHSIZE: usize = 64;
const NAME_LIMIT: usize = 4096;
/// Longest symlink target a SYMLINK may carry.
const PATH_LIMIT: usize = 64 * 1024;
/// Longest WRITE payload accepted in one call (two engine windows of slack
/// over what FSINFO declares).
const WRITE_LIMIT: usize = 1 << 20;
/// `stable_how` FILE_SYNC.
const FILE_SYNC: u32 = 2;

fn handle(kernel: &Kernel, arguments: &mut Reader<'_>) -> Result<Result<u64, Status>, Garbage> {
    let bytes = arguments.opaque(FHSIZE)?;
    Ok(kernel.decode(bytes))
}
fn time_how(arguments: &mut Reader<'_>) -> Result<Option<Stamp>, Garbage> {
    Ok(match arguments.u32()? {
        0 => None,
        1 => Some(Stamp::Processing),
        2 => {
            let seconds = arguments.u32()?;
            let nanoseconds = arguments.u32()?;
            Some(Stamp::At(Time {
                seconds: i64::from(seconds),
                nanoseconds: nanoseconds.min(999_999_999),
            }))
        }
        _ => return Err(Garbage),
    })
}
/// `sattr3`. The access time is decoded and dropped.
pub fn sattr(arguments: &mut Reader<'_>) -> Result<Sattr, Garbage> {
    let mode = arguments.bool()?.then(|| arguments.u32()).transpose()?;
    let uid = arguments.bool()?.then(|| arguments.u32()).transpose()?;
    let gid = arguments.bool()?.then(|| arguments.u32()).transpose()?;
    let size = arguments.bool()?.then(|| arguments.u64()).transpose()?;
    let _atime = time_how(arguments)?;
    let mtime = time_how(arguments)?;
    Ok(Sattr {
        mode,
        uid,
        gid,
        size,
        mtime,
    })
}
/// `wcc_data` of one object around a change.
fn wcc(kernel: &Kernel, out: &mut Writer, before: Option<&ViewStat>, after: Option<&ViewStat>) {
    pre_op(out, before);
    post_op_maybe(kernel.identity(), out, after);
}
/// Attributes of `serial` when the handle was accepted.
fn attributes(kernel: &Kernel, serial: &Result<u64, Status>) -> Option<ViewStat> {
    serial
        .as_ref()
        .ok()
        .and_then(|serial| kernel.getattr(*serial).ok())
}
/// The results of CREATE, MKDIR and SYMLINK.
fn created(
    kernel: &Kernel,
    out: &mut Writer,
    result: Result<ViewStat, Status>,
    directory: &Result<u64, Status>,
    before: Option<&ViewStat>,
) {
    let after = attributes(kernel, directory);
    match result {
        Ok(stat) => {
            out.u32(Status::Ok.code());
            out.bool(true).opaque(&kernel.encode(stat.serial));
            kernel.identity().post_op(out, &stat);
        }
        Err(status) => {
            out.u32(status.code());
        }
    }
    wcc(kernel, out, before, after.as_ref());
}
/// Decodes the arguments of changing `procedure`, runs it and appends its
/// results. Mutations it decides go to `tickets`, released by the caller
/// after the reply attempt. `Err` is a garbage-arguments refusal.
pub fn call(
    kernel: &Kernel,
    procedure: u32,
    arguments: &mut Reader<'_>,
    out: &mut Writer,
    tickets: &mut Tickets,
    verifier: [u8; 8],
) -> Result<(), Garbage> {
    match procedure {
        2 => {
            let serial = handle(kernel, arguments)?;
            let change = sattr(arguments)?;
            let guard = if arguments.bool()? {
                Some((arguments.u32()?, arguments.u32()?))
            } else {
                None
            };
            let before = attributes(kernel, &serial);
            let result = serial.and_then(|serial| kernel.setattr(serial, change, guard, tickets));
            match result {
                Ok(stat) => {
                    out.u32(Status::Ok.code());
                    wcc(kernel, out, before.as_ref(), Some(&stat));
                }
                Err(status) => {
                    out.u32(status.code());
                    let after = attributes(kernel, &serial);
                    wcc(kernel, out, before.as_ref(), after.as_ref());
                }
            }
        }
        7 => {
            let serial = handle(kernel, arguments)?;
            let offset = arguments.u64()?;
            let count = arguments.u32()? as usize;
            let _stable = arguments.u32()?;
            let data = arguments.opaque(WRITE_LIMIT)?;
            let data = &data[..count.min(data.len())];
            let before = attributes(kernel, &serial);
            let result = serial.and_then(|serial| kernel.write(serial, offset, data, tickets));
            match result {
                Ok((written, stat)) => {
                    out.u32(Status::Ok.code());
                    wcc(kernel, out, before.as_ref(), Some(&stat));
                    out.u32(written).u32(FILE_SYNC).fixed(&verifier);
                }
                Err(status) => {
                    out.u32(status.code());
                    let after = attributes(kernel, &serial);
                    wcc(kernel, out, before.as_ref(), after.as_ref());
                }
            }
        }
        8..=10 => {
            let directory = handle(kernel, arguments)?;
            let name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let before = attributes(kernel, &directory);
            let result = match procedure {
                8 => {
                    let how = match arguments.u32()? {
                        0 => How::Unchecked(sattr(arguments)?),
                        1 => How::Guarded(sattr(arguments)?),
                        2 => {
                            let mut verifier = [0; 8];
                            verifier.copy_from_slice(arguments.fixed(8)?);
                            How::Exclusive(verifier)
                        }
                        _ => return Err(Garbage),
                    };
                    directory.and_then(|parent| kernel.create(parent, &name, how, tickets))
                }
                9 => {
                    let change = sattr(arguments)?;
                    directory.and_then(|parent| kernel.mkdir(parent, &name, change, tickets))
                }
                _ => {
                    let _attributes = sattr(arguments)?;
                    let target = arguments.opaque(PATH_LIMIT)?.to_vec();
                    directory.and_then(|parent| kernel.symlink(parent, &name, &target, tickets))
                }
            };
            created(kernel, out, result, &directory, before.as_ref());
        }
        11 => {
            // Only regular files, directories and symlinks exist in the
            // canonical format: no device, socket or FIFO node.
            let directory = handle(kernel, arguments)?;
            let _name = arguments.opaque(NAME_LIMIT)?;
            let before = attributes(kernel, &directory);
            let status = directory.err().unwrap_or(Status::NotSupp);
            created(kernel, out, Err(status), &directory, before.as_ref());
        }
        12 | 13 => {
            let directory = handle(kernel, arguments)?;
            let name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let before = attributes(kernel, &directory);
            let result = directory.and_then(|parent| {
                if procedure == 12 {
                    kernel.remove(parent, &name, tickets)
                } else {
                    kernel.rmdir(parent, &name, tickets)
                }
            });
            let after = attributes(kernel, &directory);
            out.u32(result.err().unwrap_or(Status::Ok).code());
            wcc(kernel, out, before.as_ref(), after.as_ref());
        }
        14 => {
            let from = handle(kernel, arguments)?;
            let name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let to = handle(kernel, arguments)?;
            let new_name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let before_from = attributes(kernel, &from);
            let before_to = attributes(kernel, &to);
            let result = match (&from, &to) {
                (Ok(from), Ok(to)) => kernel.rename(*from, &name, *to, &new_name, tickets),
                (Err(status), _) | (_, Err(status)) => Err(*status),
            };
            let after_from = attributes(kernel, &from);
            let after_to = attributes(kernel, &to);
            out.u32(result.err().unwrap_or(Status::Ok).code());
            wcc(kernel, out, before_from.as_ref(), after_from.as_ref());
            wcc(kernel, out, before_to.as_ref(), after_to.as_ref());
        }
        15 => {
            let file = handle(kernel, arguments)?;
            let directory = handle(kernel, arguments)?;
            let name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let before = attributes(kernel, &directory);
            let result = match (&file, &directory) {
                (Ok(file), Ok(parent)) => kernel.link(*file, *parent, &name, tickets),
                (Err(status), _) | (_, Err(status)) => Err(*status),
            };
            let after = attributes(kernel, &directory);
            let (status, object) = match result {
                Ok(stat) => (Status::Ok, Some(stat)),
                Err(status) => (status, attributes(kernel, &file)),
            };
            out.u32(status.code());
            post_op_maybe(kernel.identity(), out, object.as_ref());
            wcc(kernel, out, before.as_ref(), after.as_ref());
        }
        21 => {
            // Every WRITE was answered FILE_SYNC, so nothing is uncommitted.
            let serial = handle(kernel, arguments)?;
            let _offset = arguments.u64()?;
            let _count = arguments.u32()?;
            match serial.and_then(|serial| kernel.getattr(serial)) {
                Ok(stat) => {
                    out.u32(Status::Ok.code());
                    wcc(kernel, out, None, Some(&stat));
                    out.fixed(&verifier);
                }
                Err(status) => {
                    out.u32(status.code());
                    wcc(kernel, out, None, None);
                }
            }
        }
        _ => return Err(Garbage),
    }
    Ok(())
}
