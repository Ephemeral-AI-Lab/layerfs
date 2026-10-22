//! MOUNT program 100005 version 3 (RFC 1813 appendix I): one export.
use crate::{
    kernel::Kernel,
    rpc::Accept,
    xdr::{Reader, Writer},
};

pub const PROGRAM: u32 = 100_005;
pub const VERSION: u32 = 3;
const MNTPATHLEN: usize = 1024;
const MNT3_OK: u32 = 0;
const MNT3ERR_NOENT: u32 = 2;
const AUTH_UNIX: u32 = 1;

/// Decodes and answers one MOUNT call for the export named `path`.
pub fn call(
    kernel: &Kernel,
    path: &str,
    procedure: u32,
    arguments: &mut Reader<'_>,
    out: &mut Writer,
) -> Result<(), Accept> {
    match procedure {
        0 | 4 => Ok(()),
        1 => {
            let asked = arguments
                .opaque(MNTPATHLEN)
                .map_err(|_| Accept::GarbageArguments)?;
            if exported(asked, path) {
                out.u32(MNT3_OK);
                out.opaque(&kernel.encode(kernel.root()));
                out.u32(1).u32(AUTH_UNIX);
            } else {
                out.u32(MNT3ERR_NOENT);
            }
            Ok(())
        }
        2 => {
            out.bool(false);
            Ok(())
        }
        3 => {
            arguments
                .opaque(MNTPATHLEN)
                .map_err(|_| Accept::GarbageArguments)?;
            Ok(())
        }
        5 => {
            out.bool(true).opaque(path.as_bytes()).bool(false);
            out.bool(false);
            Ok(())
        }
        _ => Err(Accept::ProcedureUnavailable),
    }
}
/// The export path, compared without trailing slashes ("/" names the root).
fn exported(asked: &[u8], path: &str) -> bool {
    let trim = |bytes: &[u8]| -> Vec<u8> {
        let mut bytes = bytes.to_vec();
        while bytes.len() > 1 && bytes.last() == Some(&b'/') {
            bytes.pop();
        }
        bytes
    };
    trim(asked) == trim(path.as_bytes())
}
