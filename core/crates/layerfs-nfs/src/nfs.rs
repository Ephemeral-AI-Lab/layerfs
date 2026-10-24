//! NFS program 100003 version 3: dispatch of every procedure.
//!
//! The reading procedures are answered here; the changing ones in
//! [`crate::changing`]. A read-only export answers every changing procedure
//! `NFS3ERR_ROFS` with its empty weak cache consistency data.
use crate::{
    attributes::{access, post_op_maybe, Status},
    change::Tickets,
    changing,
    directory::{list, Listing},
    kernel::Kernel,
    rpc::{Accept, Credentials},
    xdr::{Garbage, Reader, Writer},
};
use layerfs_overlay::{READ_WINDOW, WRITE_WINDOW};

pub const PROGRAM: u32 = 100_003;
pub const VERSION: u32 = 3;
/// Procedure names by number, for diagnostics.
pub const PROCEDURES: [&str; 22] = [
    "NULL",
    "GETATTR",
    "SETATTR",
    "LOOKUP",
    "ACCESS",
    "READLINK",
    "READ",
    "WRITE",
    "CREATE",
    "MKDIR",
    "SYMLINK",
    "MKNOD",
    "REMOVE",
    "RMDIR",
    "RENAME",
    "LINK",
    "READDIR",
    "READDIRPLUS",
    "FSSTAT",
    "FSINFO",
    "PATHCONF",
    "COMMIT",
];
/// Longest handle (`NFS3_FHSIZE`) and the longest name a client may send.
const FHSIZE: usize = 64;
const NAME_LIMIT: usize = 4096;
/// Declared capacity: fixed values, never an admission signal (as for FUSE statfs).
const DECLARED_BLOCKS: u64 = 1 << 30;
const DECLARED_FILES: u64 = 1 << 30;
const BLOCK_BYTES: u64 = 4096;
/// FSINFO properties: FSF3_LINK | FSF3_SYMLINK | FSF3_HOMOGENEOUS | FSF3_CANSETTIME.
const PROPERTIES: u32 = 0x01 | 0x02 | 0x08 | 0x10;
/// Preferred READDIR reply size the client is told to ask for.
const DIRECTORY_PREFERENCE: u32 = 32 * 1024;

/// The export's fixed answers.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub read_only: bool,
}

/// Decodes the arguments of `procedure`, runs it and appends its results.
/// `Err` is an RPC-level refusal of the call.
#[allow(clippy::too_many_arguments)]
pub fn call(
    kernel: &Kernel,
    policy: Policy,
    procedure: u32,
    credentials: &Credentials,
    arguments: &mut Reader<'_>,
    out: &mut Writer,
    tickets: &mut Tickets,
    verifier: [u8; 8],
) -> Result<(), Accept> {
    let garbage = |_: Garbage| Accept::GarbageArguments;
    match procedure {
        0 => Ok(()),
        1 => {
            let serial = handle(kernel, arguments).map_err(garbage)?;
            match serial.and_then(|serial| kernel.getattr(serial)) {
                Ok(stat) => {
                    out.u32(Status::Ok.code());
                    kernel.identity().fattr3(out, &stat);
                }
                Err(status) => {
                    out.u32(status.code());
                }
            }
            Ok(())
        }
        3 => {
            let parent = handle(kernel, arguments).map_err(garbage)?;
            let name = arguments.opaque(NAME_LIMIT).map_err(garbage)?;
            // Both answers carry the directory's attributes (RFC 1813
            // LOOKUP3resok and LOOKUP3resfail), so the client keeps its
            // cached view of the directory without a GETATTR of its own.
            let directory = parent
                .as_ref()
                .ok()
                .and_then(|parent| kernel.getattr(*parent).ok());
            match parent.and_then(|parent| kernel.lookup(parent, name)) {
                Ok(stat) => {
                    out.u32(Status::Ok.code());
                    out.opaque(&kernel.encode(stat.serial));
                    kernel.identity().post_op(out, &stat);
                    post_op_maybe(kernel.identity(), out, directory.as_ref());
                }
                Err(status) => {
                    out.u32(status.code());
                    post_op_maybe(kernel.identity(), out, directory.as_ref());
                }
            }
            Ok(())
        }
        4 => {
            let serial = handle(kernel, arguments).map_err(garbage)?;
            let requested = arguments.u32().map_err(garbage)?;
            match serial.and_then(|serial| kernel.getattr(serial)) {
                Ok(stat) => {
                    let caller = credentials
                        .unix
                        .as_ref()
                        .map(|unix| (unix.uid, unix.gid, unix.groups.as_slice()));
                    let granted = access(
                        kernel.identity(),
                        &stat,
                        caller,
                        requested,
                        policy.read_only,
                    );
                    out.u32(Status::Ok.code());
                    kernel.identity().post_op(out, &stat);
                    out.u32(granted);
                }
                Err(status) => {
                    out.u32(status.code()).bool(false);
                }
            }
            Ok(())
        }
        5 => {
            let serial = handle(kernel, arguments).map_err(garbage)?;
            match serial.and_then(|serial| kernel.readlink(serial)) {
                Ok(target) => {
                    out.u32(Status::Ok.code()).bool(false).opaque(&target);
                }
                Err(status) => {
                    out.u32(status.code()).bool(false);
                }
            }
            Ok(())
        }
        6 => {
            let serial = handle(kernel, arguments).map_err(garbage)?;
            let offset = arguments.u64().map_err(garbage)?;
            let count = arguments.u32().map_err(garbage)?;
            match serial.and_then(|serial| kernel.read(serial, offset, count)) {
                Ok((data, eof)) => {
                    out.u32(Status::Ok.code()).bool(false);
                    out.u32(data.len() as u32).bool(eof).opaque(&data);
                }
                Err(status) => {
                    out.u32(status.code()).bool(false);
                }
            }
            Ok(())
        }
        16 | 17 => {
            let plus = procedure == 17;
            let serial = handle(kernel, arguments).map_err(garbage)?;
            let cookie = arguments.u64().map_err(garbage)?;
            let verifier = arguments.fixed(8).map_err(garbage)?;
            let mut budget = arguments.u32().map_err(garbage)?;
            if plus {
                // dircount bounds the names alone; maxcount the whole reply.
                let _dircount = budget;
                budget = arguments.u32().map_err(garbage)?;
            }
            let mut word = [0; 8];
            word.copy_from_slice(verifier);
            let listing = |serial| Listing {
                serial,
                cookie,
                verifier: u64::from_be_bytes(word),
                budget: budget as usize,
                plus,
            };
            match serial.and_then(|serial| list(kernel, listing(serial))) {
                Ok(body) => {
                    out.u32(Status::Ok.code()).fixed(&body);
                }
                Err(status) => {
                    out.u32(status.code()).bool(false);
                }
            }
            Ok(())
        }
        18..=20 => {
            let serial = handle(kernel, arguments).map_err(garbage)?;
            match serial.and_then(|serial| kernel.getattr(serial)) {
                Ok(stat) => {
                    out.u32(Status::Ok.code());
                    kernel.identity().post_op(out, &stat);
                    filesystem(procedure, out);
                }
                Err(status) => {
                    out.u32(status.code()).bool(false);
                }
            }
            Ok(())
        }
        2 | 7..=15 | 21 if !policy.read_only => {
            changing::call(kernel, procedure, arguments, out, tickets, verifier).map_err(garbage)
        }
        2 | 7..=15 | 21 => {
            // Each changing procedure's failure carries wcc_data (two absent
            // optional fields) per directory it names; LINK also its object's
            // post_op_attr.
            let absent = match procedure {
                14 => 4,
                15 => 3,
                _ => 2,
            };
            out.u32(Status::RoFs.code());
            for _ in 0..absent {
                out.bool(false);
            }
            Ok(())
        }
        _ => Err(Accept::ProcedureUnavailable),
    }
}
/// The serial of the call's handle argument; a handle that is not accepted is
/// the procedure's own failure status, not an RPC refusal.
fn handle(kernel: &Kernel, arguments: &mut Reader<'_>) -> Result<Result<u64, Status>, Garbage> {
    let bytes = arguments.opaque(FHSIZE)?;
    Ok(kernel.decode(bytes))
}
/// FSSTAT, FSINFO and PATHCONF results after the object's attributes.
fn filesystem(procedure: u32, out: &mut Writer) {
    match procedure {
        18 => {
            out.u64(DECLARED_BLOCKS * BLOCK_BYTES);
            out.u64(DECLARED_BLOCKS * BLOCK_BYTES);
            out.u64(DECLARED_BLOCKS * BLOCK_BYTES);
            out.u64(DECLARED_FILES)
                .u64(DECLARED_FILES)
                .u64(DECLARED_FILES);
            out.u32(0); // invarsec
        }
        19 => {
            let (read, write) = (READ_WINDOW as u32, WRITE_WINDOW as u32);
            out.u32(read).u32(read).u32(BLOCK_BYTES as u32);
            out.u32(write).u32(write).u32(BLOCK_BYTES as u32);
            out.u32(DIRECTORY_PREFERENCE);
            out.u64(i64::MAX as u64);
            out.u32(0).u32(1); // time_delta: 1 ns
            out.u32(PROPERTIES);
        }
        _ => {
            out.u32(u32::MAX); // linkmax
            out.u32(255); // name_max
            out.bool(true); // no_trunc
            out.bool(true); // chown_restricted
            out.bool(false); // case_insensitive
            out.bool(true); // case_preserving
        }
    }
}
