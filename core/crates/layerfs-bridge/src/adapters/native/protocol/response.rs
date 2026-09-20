//! Closed terminal result encoding.
use super::{Decoder, Encoder};
use crate::contract::*;
pub fn encode_response(r: &Response) -> Result<Vec<u8>, Failure> {
    let mut e = Encoder::default();
    match r {
        Response::Read { length } => {
            e.u8(1)?;
            e.u64(*length)?;
        }
        Response::Saved {
            root,
            length,
            inserted,
            reused,
        } => {
            e.u8(2)?;
            e.put(root)?;
            e.u64(*length)?;
            e.u64(*inserted)?;
            e.u64(*reused)?;
        }
        Response::FilesystemSaved {
            root,
            inserted,
            reused,
        } => {
            e.u8(7)?;
            e.put(root)?;
            e.u64(*inserted)?;
            e.u64(*reused)?;
        }
        Response::File {
            length,
            representation,
        } => {
            e.u8(3)?;
            e.u64(*length)?;
            e.u8(*representation)?;
        }
        Response::Stat {
            serial,
            kind,
            references,
            content,
            metadata,
            mode,
            mtime,
            nanoseconds,
        } => {
            e.u8(4)?;
            e.u64(*serial)?;
            e.u8(*kind)?;
            e.u64(*references)?;
            e.put(content)?;
            e.put(metadata)?;
            e.u32(*mode)?;
            e.u64(*mtime as u64)?;
            e.u32(*nanoseconds)?;
        }
        Response::List {
            entries,
            continuation,
        } => {
            if entries.len() > 128 {
                return Err(Code::Capacity.into());
            }
            e.u8(5)?;
            e.blob(continuation.as_deref().unwrap_or(&[]))?;
            e.count(entries.len())?;
            for (name, serial) in entries {
                e.blob(name)?;
                e.u64(*serial)?;
            }
        }
        Response::Link(bytes) => {
            e.u8(6)?;
            e.blob(bytes)?;
        }
    }
    Ok(e.finish())
}
pub fn decode_response(b: &[u8]) -> Result<Response, Failure> {
    let mut d = Decoder::new(b)?;
    let r = match d.u8()? {
        1 => Response::Read { length: d.u64()? },
        2 => Response::Saved {
            root: d.root()?,
            length: d.u64()?,
            inserted: d.u64()?,
            reused: d.u64()?,
        },
        3 => Response::File {
            length: d.u64()?,
            representation: d.u8()?,
        },
        4 => Response::Stat {
            serial: d.u64()?,
            kind: d.u8()?,
            references: d.u64()?,
            content: d.root()?,
            metadata: d.root()?,
            mode: d.u32()?,
            mtime: d.u64()? as i64,
            nanoseconds: d.u32()?,
        },
        5 => {
            let after = d.blob(255)?;
            let continuation = (!after.is_empty()).then_some(after);
            let n = d.count(128, 10)?;
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push((d.blob(255)?, d.u64()?));
            }
            Response::List {
                entries,
                continuation,
            }
        }
        6 => Response::Link(d.blob(4096)?),
        7 => Response::FilesystemSaved {
            root: d.root()?,
            inserted: d.u64()?,
            reused: d.u64()?,
        },
        _ => return Err(Code::Unsupported.into()),
    };
    d.finish()?;
    Ok(r)
}
pub fn encode_failure(f: Failure) -> [u8; 3] {
    [
        f.code as u8,
        u8::from(f.unknown),
        f.cleanup.map_or(0, |c| c as u8),
    ]
}
pub fn decode_failure(b: &[u8]) -> Result<Failure, Failure> {
    if b.len() != 3 || b[1] > 1 {
        return Err(Code::InvalidInput.into());
    }
    if b[0] == Code::Unknown as u8 && b[1] == 0 {
        return Err(Code::InvalidInput.into());
    }
    Ok(Failure {
        code: code(b[0])?,
        unknown: b[1] != 0,
        cleanup: if b[2] == 0 { None } else { Some(code(b[2])?) },
    })
}
fn code(n: u8) -> Result<Code, Failure> {
    Ok(match n {
        1 => Code::InvalidInput,
        2 => Code::Unsupported,
        3 => Code::Denied,
        4 => Code::Capacity,
        5 => Code::Ownership,
        6 => Code::MissingObject,
        7 => Code::PathNotFound,
        8 => Code::Provider,
        9 => Code::Integrity,
        10 => Code::Io,
        11 => Code::Deadline,
        12 => Code::Unknown,
        _ => return Err(Code::InvalidInput.into()),
    })
}
