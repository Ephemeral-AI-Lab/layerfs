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
use layerfs_overlay::READ_WINDOW;

pub const PROGRAM: u32 = 100_003;
pub const VERSION: u32 = 3;
/// Largest READ or WRITE payload one call transfers, and what FSINFO declares
/// as `rtmax/rtpref/wtmax/wtpref`. It is exactly one engine write window
/// (`layerfs_overlay::WRITE_WINDOW`, asserted below): a WRITE of this size is
/// one owner job — one SQLite transaction and one publication — instead of a
/// loop of window jobs. A READ of it is filled from consecutive
/// `READ_WINDOW` windows inside this one call, so the engine never holds or
/// reads more than one read window for it.
///
/// 256 KiB, not larger: a macOS NFSv3 client sets its `nfsbuf` I/O block from
/// `max(rsize, wsize)`, and a wider one makes every small write of an
/// unshrunk page pool cost proportionally more in its own kernel (measured:
/// 4-byte writes 12 µs at 128 KiB, 18 µs at 256 KiB, 57 µs at 1 MiB — #77).
pub const TRANSFER_WINDOW: u32 = 256 * 1024;
// One transfer, one write job: the arithmetic that makes a 256 KiB WRITE one
// transaction cannot drift from the engine's window.
const _: () = assert!(TRANSFER_WINDOW as usize == layerfs_overlay::WRITE_WINDOW);
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
            // One owner job answers the entry and the directory it was named
            // in (RFC 1813 LOOKUP3resok's `obj_attributes` and
            // `dir_attributes`, and the failure's). The job had to read the
            // directory's own row to resolve the name, so its attributes cost
            // no second job. Deliberate change from the route before this: a
            // failure (`NFS3ERR_NOENT` included), `.`, `..` and a silly name
            // answer the directory facts this export last published or
            // observed for it, and none when it never saw the directory,
            // where each of them used to make a fresh GETATTR of the parent.
            // RFC 1813 keeps those attributes optional, so both are legal;
            // the answer is now at most one observation old.
            let (parent, found) = match parent {
                Ok(parent) => (parent, kernel.lookup(parent, name)),
                Err(status) => {
                    out.u32(status.code());
                    post_op_maybe(kernel.identity(), out, None);
                    return Ok(());
                }
            };
            let directory = match &found {
                Ok((_, directory)) => directory.clone(),
                Err(_) => None,
            }
            .or_else(|| kernel.directory_wcc(parent).0);
            match found {
                Ok((stat, _)) => {
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
            match serial.and_then(|serial| read(kernel, serial, offset, count)) {
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
                // A READDIRPLUS request carries the entries' own count
                // (`dircount`) before the reply's (`maxcount`). Only the
                // reply's bounds what this export sends: the client sized its
                // buffer by it, and names alone are never what fills a reply.
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
/// READ of up to one transfer window: consecutive engine windows fill it, so
/// one call carries `TRANSFER_WINDOW` (256 KiB) while the engine reads and holds one `READ_WINDOW`
/// at a time. The last window may be short (its file ended, or its engine call
/// failed after earlier windows were read), which is the short read RFC 1813
/// allows.
fn read(kernel: &Kernel, serial: u64, offset: u64, count: u32) -> Result<(Vec<u8>, bool), Status> {
    let target = count.min(TRANSFER_WINDOW) as usize;
    let mut data = Vec::with_capacity(target);
    loop {
        let want = (target - data.len()).min(READ_WINDOW) as u32;
        let at = offset.checked_add(data.len() as u64).ok_or(Status::FBig)?;
        let (window, eof) = match kernel.read(serial, at, want) {
            Ok(read) => read,
            Err(status) => {
                // A window the file did serve is answered as a short read.
                if !data.is_empty() {
                    return Ok((data, false));
                }
                return Err(status);
            }
        };
        let filled = window.len();
        data.extend_from_slice(&window);
        // A window short of what was asked ends the file or the fill.
        if eof || filled < want as usize || data.len() >= target {
            return Ok((data, eof));
        }
    }
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
            out.u32(TRANSFER_WINDOW)
                .u32(TRANSFER_WINDOW)
                .u32(BLOCK_BYTES as u32);
            out.u32(TRANSFER_WINDOW)
                .u32(TRANSFER_WINDOW)
                .u32(BLOCK_BYTES as u32);
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
