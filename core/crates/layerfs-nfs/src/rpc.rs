//! ONC RPC v2 (RFC 5531) over TCP record marking: calls in, replies out.
use crate::xdr::{Garbage, Reader, Writer};
use std::io::{self, Read, Write};

const CALL: u32 = 0;
const REPLY: u32 = 1;
const MSG_ACCEPTED: u32 = 0;
const MSG_DENIED: u32 = 1;
const RPC_MISMATCH: u32 = 0;
const AUTH_ERROR: u32 = 1;
const AUTH_NONE: u32 = 0;
const AUTH_UNIX: u32 = 1;
/// AUTH_TOOWEAK: the credential flavor is not accepted.
const AUTH_TOOWEAK: u32 = 5;
const LAST_FRAGMENT: u32 = 1 << 31;
/// Largest credential or verifier body RFC 5531 allows.
const AUTH_BODY: usize = 400;
/// Largest whole record accepted. A1 serves no WRITE, so every call is small;
/// the bound still admits a full-window WRITE for the later write path.
pub const RECORD_LIMIT: usize = 2 * 1024 * 1024;

/// The accepted-reply status of one call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Accept {
    Success,
    ProgramUnavailable,
    /// The program exists in versions `low..=high` only.
    ProgramMismatch {
        low: u32,
        high: u32,
    },
    ProcedureUnavailable,
    GarbageArguments,
}
/// The caller's identity as AUTH_UNIX states it. AUTH_NONE has none.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Credentials {
    pub unix: Option<UnixCredentials>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnixCredentials {
    pub uid: u32,
    pub gid: u32,
    pub groups: Vec<u32>,
}
/// One decoded call header; its arguments follow in the same record.
#[derive(Debug)]
pub struct Call {
    pub xid: u32,
    pub program: u32,
    pub version: u32,
    pub procedure: u32,
    pub credentials: Credentials,
}
/// A record that is not an answerable call.
#[derive(Debug)]
pub enum Refused {
    /// Not a call message, or not decodable: no reply is possible.
    Undecodable,
    /// A call whose header can be answered only with a rejection.
    Reply(Vec<u8>),
}

/// Reads one whole record (all fragments), bounded by [`RECORD_LIMIT`].
/// `Ok(None)` is an orderly end of stream before a record started.
pub fn read_record(stream: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut record = Vec::new();
    loop {
        let mut header = [0; 4];
        if let Err(error) = stream.read_exact(&mut header) {
            if error.kind() == io::ErrorKind::UnexpectedEof && record.is_empty() {
                return Ok(None);
            }
            return Err(error);
        }
        let header = u32::from_be_bytes(header);
        let length = (header & !LAST_FRAGMENT) as usize;
        if record.len() + length > RECORD_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "RPC record exceeds its bound",
            ));
        }
        let start = record.len();
        record.resize(start + length, 0);
        stream.read_exact(&mut record[start..])?;
        if header & LAST_FRAGMENT != 0 {
            return Ok(Some(record));
        }
    }
}
/// Writes one reply as a single last fragment.
pub fn write_record(stream: &mut impl Write, reply: &[u8]) -> io::Result<()> {
    let mut framed = Vec::with_capacity(reply.len() + 4);
    framed.extend_from_slice(&(reply.len() as u32 | LAST_FRAGMENT).to_be_bytes());
    framed.extend_from_slice(reply);
    stream.write_all(&framed)
}

/// Decodes the call header; the returned reader is positioned at the arguments.
pub fn parse_call(record: &[u8]) -> Result<(Call, Reader<'_>), Refused> {
    let mut reader = Reader::new(record);
    let header = (|| -> Result<(u32, u32, u32), Garbage> {
        Ok((reader.u32()?, reader.u32()?, reader.u32()?))
    })();
    let (xid, kind, rpcvers) = header.map_err(|_| Refused::Undecodable)?;
    if kind != CALL {
        return Err(Refused::Undecodable);
    }
    if rpcvers != 2 {
        let mut reply = Writer::new();
        reply.u32(xid).u32(REPLY).u32(MSG_DENIED).u32(RPC_MISMATCH);
        reply.u32(2).u32(2);
        return Err(Refused::Reply(reply.into_bytes()));
    }
    let body = (|| -> Result<(u32, u32, u32, u32, &[u8]), Garbage> {
        let program = reader.u32()?;
        let version = reader.u32()?;
        let procedure = reader.u32()?;
        let flavor = reader.u32()?;
        let credential = reader.opaque(AUTH_BODY)?;
        let _verifier_flavor = reader.u32()?;
        let _verifier = reader.opaque(AUTH_BODY)?;
        Ok((program, version, procedure, flavor, credential))
    })();
    let (program, version, procedure, flavor, credential) =
        body.map_err(|_| Refused::Undecodable)?;
    let credentials = match flavor {
        AUTH_NONE => Credentials::default(),
        AUTH_UNIX => match unix_credentials(credential) {
            Ok(unix) => Credentials { unix: Some(unix) },
            Err(Garbage) => return Err(Refused::Reply(denied_auth(xid))),
        },
        _ => return Err(Refused::Reply(denied_auth(xid))),
    };
    Ok((
        Call {
            xid,
            program,
            version,
            procedure,
            credentials,
        },
        reader,
    ))
}
fn unix_credentials(body: &[u8]) -> Result<UnixCredentials, Garbage> {
    let mut reader = Reader::new(body);
    let _stamp = reader.u32()?;
    let _machine = reader.opaque(255)?;
    let uid = reader.u32()?;
    let gid = reader.u32()?;
    let count = reader.u32()? as usize;
    if count > 16 {
        return Err(Garbage);
    }
    let groups = (0..count)
        .map(|_| reader.u32())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(UnixCredentials { uid, gid, groups })
}
fn denied_auth(xid: u32) -> Vec<u8> {
    let mut reply = Writer::new();
    reply.u32(xid).u32(REPLY).u32(MSG_DENIED).u32(AUTH_ERROR);
    reply.u32(AUTH_TOOWEAK);
    reply.into_bytes()
}
/// The accepted-reply header for `xid`; procedure results follow a success.
pub fn accepted(xid: u32, status: Accept) -> Writer {
    let mut reply = Writer::with_capacity(256);
    reply.u32(xid).u32(REPLY).u32(MSG_ACCEPTED);
    reply.u32(AUTH_NONE).u32(0);
    match status {
        Accept::Success => reply.u32(0),
        Accept::ProgramUnavailable => reply.u32(1),
        Accept::ProgramMismatch { low, high } => reply.u32(2).u32(low).u32(high),
        Accept::ProcedureUnavailable => reply.u32(3),
        Accept::GarbageArguments => reply.u32(4),
    };
    reply
}
