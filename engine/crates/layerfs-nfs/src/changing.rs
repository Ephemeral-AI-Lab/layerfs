//! Arguments and results of the changing NFSv3 procedures (RFC 1813):
//! SETATTR, WRITE, CREATE, MKDIR, SYMLINK, MKNOD, REMOVE, RMDIR, RENAME,
//! LINK and COMMIT.
//!
//! Every directory a procedure changes is answered with `wcc_data`: the
//! attributes that the change's own publishing transaction read just before
//! it and wrote just after, so the pair is one atomic window and no attribute
//! is read for it outside that job. COMMIT answers the file's attributes from
//! what this export last published or observed for it.
//!
//! WRITE answers `FILE_SYNC`: every window was published to the Workspace
//! before the reply, as every FUSE write is. That is publication, not
//! durability; the write verifier changes with every export, so a client
//! that ever sent UNSTABLE data resends it to a restarted export.
use crate::{
    attributes::{post_op_maybe, pre_op, Status},
    change::{How, Sattr, Tickets, Wcc},
    kernel::Kernel,
    xdr::{Garbage, Reader, Writer},
};
use layerfs_mount_ports::operations::write::Stamp;
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
/// The results of CREATE, MKDIR and SYMLINK.
fn created(kernel: &Kernel, out: &mut Writer, result: Result<(ViewStat, Wcc), Status>) {
    match result {
        Ok((stat, directory)) => {
            out.u32(Status::Ok.code());
            out.bool(true).opaque(&kernel.encode(stat.serial));
            kernel.identity().post_op(out, &stat);
            wcc(kernel, out, directory.0.as_ref(), directory.1.as_ref());
        }
        Err(status) => {
            out.u32(status.code());
            wcc(kernel, out, None, None);
        }
    }
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
            let result = serial.and_then(|serial| kernel.setattr(serial, change, guard, tickets));
            match result {
                Ok((_, changed)) => {
                    out.u32(Status::Ok.code());
                    wcc(kernel, out, changed.0.as_ref(), changed.1.as_ref());
                }
                Err(status) => {
                    out.u32(status.code());
                    wcc(kernel, out, None, None);
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
            let result = serial.and_then(|serial| kernel.write(serial, offset, data, tickets));
            match result {
                Ok((written, changed)) => {
                    out.u32(Status::Ok.code());
                    wcc(kernel, out, changed.0.as_ref(), changed.1.as_ref());
                    out.u32(written).u32(FILE_SYNC).fixed(&verifier);
                }
                Err(status) => {
                    out.u32(status.code());
                    wcc(kernel, out, None, None);
                }
            }
        }
        8..=10 => {
            let directory = handle(kernel, arguments)?;
            let name = arguments.opaque(NAME_LIMIT)?.to_vec();
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
            created(kernel, out, result);
        }
        11 => {
            // Only regular files, directories and symlinks exist in the
            // canonical format: no device, socket or FIFO node.
            let directory = handle(kernel, arguments)?;
            let _name = arguments.opaque(NAME_LIMIT)?;
            let status = directory.err().unwrap_or(Status::NotSupp);
            created(kernel, out, Err(status));
        }
        12 | 13 => {
            let directory = handle(kernel, arguments)?;
            let name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let result = directory.and_then(|parent| {
                if procedure == 12 {
                    kernel.remove(parent, &name, tickets)
                } else {
                    kernel.rmdir(parent, &name, tickets)
                }
            });
            match result {
                Ok(changed) => {
                    out.u32(Status::Ok.code());
                    wcc(kernel, out, changed.0.as_ref(), changed.1.as_ref());
                }
                Err(status) => {
                    out.u32(status.code());
                    wcc(kernel, out, None, None);
                }
            }
        }
        14 => {
            let from = handle(kernel, arguments)?;
            let name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let to = handle(kernel, arguments)?;
            let new_name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let changed = match (&from, &to) {
                (Ok(from), Ok(to)) => kernel.rename(*from, &name, *to, &new_name, tickets),
                (Err(status), _) | (_, Err(status)) => Err(*status),
            };
            let (status, from_wcc, to_wcc) = match changed {
                Ok([from_wcc, to_wcc]) => (Status::Ok, from_wcc, to_wcc),
                Err(status) => (status, (None, None), (None, None)),
            };
            out.u32(status.code());
            wcc(kernel, out, from_wcc.0.as_ref(), from_wcc.1.as_ref());
            wcc(kernel, out, to_wcc.0.as_ref(), to_wcc.1.as_ref());
        }
        15 => {
            let file = handle(kernel, arguments)?;
            let directory = handle(kernel, arguments)?;
            let name = arguments.opaque(NAME_LIMIT)?.to_vec();
            let result = match (&file, &directory) {
                (Ok(file), Ok(parent)) => kernel.link(*file, *parent, &name, tickets),
                (Err(status), _) | (_, Err(status)) => Err(*status),
            };
            let (status, object, changed) = match result {
                Ok((stat, changed)) => (Status::Ok, Some(stat), changed),
                Err(status) => (status, None, (None, None)),
            };
            out.u32(status.code());
            post_op_maybe(kernel.identity(), out, object.as_ref());
            wcc(kernel, out, changed.0.as_ref(), changed.1.as_ref());
        }
        21 => {
            // Every WRITE was answered FILE_SYNC, so nothing is uncommitted.
            let serial = handle(kernel, arguments)?;
            let _offset = arguments.u64()?;
            let _count = arguments.u32()?;
            match serial.and_then(|serial| kernel.commit(serial)) {
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
