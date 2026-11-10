//! ONC RPC v2 (RFC 5531) over TCP record marking: calls in, replies out.
use crate::xdr::{Garbage, Reader, Writer};
use std::io::{self, IoSlice, Read, Write};

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

/// Bytes one connection receives ahead of a record's length: one `read` of
/// this much carries any metadata call whole, so such a call costs one read.
const READ_AHEAD: usize = 8 * 1024;

/// One connection's received records.
///
/// A call the client sent in one piece is framed from the bytes one `read`
/// delivered, so it costs one read instead of the header read plus the body
/// read a plain `read_exact` pair makes. A record larger than the read-ahead
/// (a WRITE of a transfer window) is read into its own buffer with
/// `read_exact`, so its bytes are not copied twice.
pub struct Records {
    /// Received bytes, of which `valid` are data not yet consumed.
    buffer: Vec<u8>,
    valid: usize,
}
impl Default for Records {
    fn default() -> Self {
        Self::new()
    }
}
impl Records {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            valid: 0,
        }
    }
    /// Reads one whole record (all fragments), bounded by [`RECORD_LIMIT`].
    /// `Ok(None)` is an orderly end of stream before a record started.
    pub fn read(&mut self, stream: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
        loop {
            if self.valid < 4 {
                if !self.receive(stream)? {
                    return Ok(None);
                }
                continue;
            }
            let header = u32::from_be_bytes([
                self.buffer[0],
                self.buffer[1],
                self.buffer[2],
                self.buffer[3],
            ]);
            let length = (header & !LAST_FRAGMENT) as usize;
            if length > RECORD_LIMIT {
                return Err(too_large());
            }
            if header & LAST_FRAGMENT == 0 {
                // A fragment chain: its bodies are not contiguous.
                return self.chain(stream, length);
            }
            if self.valid >= 4 + length {
                let record = self.buffer[4..4 + length].to_vec();
                self.consume(4 + length);
                return Ok(Some(record));
            }
            if 4 + length > READ_AHEAD {
                return self.large(stream, length);
            }
            if !self.receive(stream)? {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
        }
    }
    /// Receives more bytes into the read-ahead buffer, allocated once on the
    /// first read; `false` is an orderly end of stream before any. A blocked
    /// `read` a signal interrupts is retried, as `read_exact` and
    /// [`write_record`] do, and the bytes already received stay buffered.
    fn receive(&mut self, stream: &mut impl Read) -> io::Result<bool> {
        if self.buffer.len() < READ_AHEAD {
            self.buffer.resize(READ_AHEAD, 0);
        }
        loop {
            let read = match stream.read(&mut self.buffer[self.valid..]) {
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if read == 0 {
                return if self.valid == 0 {
                    Ok(false)
                } else {
                    Err(io::Error::from(io::ErrorKind::UnexpectedEof))
                };
            }
            self.valid += read;
            return Ok(true);
        }
    }
    /// Drops the first `consumed` bytes; the read-ahead buffer stays.
    fn consume(&mut self, consumed: usize) {
        self.buffer.copy_within(consumed..self.valid, 0);
        self.valid -= consumed;
    }
    /// One final fragment larger than the read-ahead, read into its own
    /// buffer: the bulk of it is never copied.
    fn large(&mut self, stream: &mut impl Read, length: usize) -> io::Result<Option<Vec<u8>>> {
        let buffered = self.valid - 4;
        let mut record = Vec::with_capacity(length);
        record.extend_from_slice(&self.buffer[4..self.valid]);
        self.valid = 0;
        record.resize(length, 0);
        stream.read_exact(&mut record[buffered..])?;
        Ok(Some(record))
    }
    /// A fragment chain: each fragment's body follows its own header.
    fn chain(&mut self, stream: &mut impl Read, mut length: usize) -> io::Result<Option<Vec<u8>>> {
        self.consume(4); // the first fragment's header, already framed
        let mut record = Vec::new();
        let mut header = [0; 4];
        loop {
            if record.len() + length > RECORD_LIMIT {
                return Err(too_large());
            }
            let start = record.len();
            record.resize(start + length, 0);
            self.take(stream, &mut record[start..])?;
            self.take(stream, &mut header)?;
            let word = u32::from_be_bytes(header);
            length = (word & !LAST_FRAGMENT) as usize;
            if length > RECORD_LIMIT {
                return Err(too_large());
            }
            if word & LAST_FRAGMENT != 0 {
                if record.len() + length > RECORD_LIMIT {
                    return Err(too_large());
                }
                let start = record.len();
                record.resize(start + length, 0);
                self.take(stream, &mut record[start..])?;
                return Ok(Some(record));
            }
        }
    }
    /// Fills `into` from the buffered bytes first, then from the stream.
    fn take(&mut self, stream: &mut impl Read, into: &mut [u8]) -> io::Result<()> {
        let buffered = self.valid.min(into.len());
        into[..buffered].copy_from_slice(&self.buffer[..buffered]);
        self.consume(buffered);
        if buffered < into.len() {
            stream.read_exact(&mut into[buffered..])?;
        }
        Ok(())
    }
}
fn too_large() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "RPC record exceeds its bound")
}
/// Writes one reply as a single last fragment. The four-byte fragment header
/// and the reply go out as two writes of one vectored write, so a reply is
/// never copied to be framed; a stream that writes less than it was offered
/// (or a plain `Write` that writes only its first slice) continues where it
/// stopped.
pub fn write_record(stream: &mut impl Write, reply: &[u8]) -> io::Result<()> {
    let header = (reply.len() as u32 | LAST_FRAGMENT).to_be_bytes();
    let mut head = &header[..];
    let mut body = reply;
    while !head.is_empty() || !body.is_empty() {
        let written = match (head.is_empty(), body.is_empty()) {
            (false, false) => stream.write_vectored(&[IoSlice::new(head), IoSlice::new(body)]),
            (false, true) => stream.write(head),
            (true, false) => stream.write(body),
            (true, true) => break,
        };
        match written {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
            Ok(count) => {
                let from_head = count.min(head.len());
                head = &head[from_head..];
                body = &body[count - from_head..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
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
