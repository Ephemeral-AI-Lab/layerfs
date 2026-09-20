//! Fixed-width local frames; network records authenticate these exact bytes.
use crate::contract::*;
use std::io::{Read, Write};
pub const HEADER: usize = 20;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Hello = 1,
    Begin = 2,
    Body = 3,
    EndInput = 4,
    ResultData = 5,
    Success = 6,
    Failure = 7,
}
#[derive(Debug)]
pub struct Frame {
    pub kind: Kind,
    pub id: u64,
    pub bytes: Vec<u8>,
}
impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, Failure> {
        let mut out = Vec::new();
        self.encode_into(&mut out)?;
        Ok(out)
    }
    /// Encodes into a caller-owned buffer so a hot path can reuse its scratch.
    /// The buffer is cleared first and left holding exactly this frame.
    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<(), Failure> {
        check(self.kind, self.bytes.len())?;
        out.clear();
        out.reserve(HEADER + self.bytes.len());
        out.extend_from_slice(b"LFB1");
        out.extend_from_slice(&[self.kind as u8, 0, 0, 0]);
        out.extend_from_slice(&self.id.to_be_bytes());
        out.extend_from_slice(&(self.bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.bytes);
        Ok(())
    }
    pub fn read(reader: &mut impl Read) -> Result<Self, Failure> {
        Self::read_optional(reader)?.ok_or_else(|| Code::Io.into())
    }
    /// Clean EOF is accepted only between complete local submission frames.
    pub fn read_optional(reader: &mut impl Read) -> Result<Option<Self>, Failure> {
        let mut h = [0u8; HEADER];
        // The fixed header normally arrives in a single read; a clean end of stream
        // is legal only before its first byte, so a zero-length first read is the
        // one end-of-stream signal accepted here.
        let first = reader.read(&mut h)?;
        if first == 0 {
            return Ok(None);
        }
        if first < HEADER {
            reader.read_exact(&mut h[first..])?;
        }
        if &h[..4] != b"LFB1" || h[5..8] != [0, 0, 0] {
            return Err(Code::Unsupported.into());
        }
        let kind = match h[4] {
            1 => Kind::Hello,
            2 => Kind::Begin,
            3 => Kind::Body,
            4 => Kind::EndInput,
            5 => Kind::ResultData,
            6 => Kind::Success,
            7 => Kind::Failure,
            _ => return Err(Code::Unsupported.into()),
        };
        let id = u64::from_be_bytes(h[8..16].try_into().map_err(|_| Code::InvalidInput)?);
        let n = u32::from_be_bytes(h[16..20].try_into().map_err(|_| Code::InvalidInput)?) as usize;
        check(kind, n)?;
        let mut bytes = vec![0; n];
        reader.read_exact(&mut bytes)?;
        Ok(Some(Self { kind, id, bytes }))
    }
    pub fn write(&self, writer: &mut impl Write) -> Result<(), Failure> {
        writer.write_all(&self.encode()?)?;
        writer.flush()?;
        Ok(())
    }
}
fn check(kind: Kind, n: usize) -> Result<(), Failure> {
    let valid = match kind {
        Kind::Body | Kind::ResultData => n > 0 && n <= FRAME_BYTES,
        Kind::EndInput => n == 8,
        Kind::Hello => n == 2,
        Kind::Failure => n == 3,
        _ => n <= METADATA_BYTES,
    };
    if valid {
        Ok(())
    } else {
        Err(Code::Capacity.into())
    }
}
