//! Checked binary metadata; every vector count is bounded before reservation.
use crate::contract::*;

pub struct Decoder<'a> {
    bytes: &'a [u8],
}
impl<'a> Decoder<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, Failure> {
        if bytes.len() > METADATA_BYTES {
            return Err(Code::Capacity.into());
        }
        Ok(Self { bytes })
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Failure> {
        if n > self.bytes.len() {
            return Err(Code::InvalidInput.into());
        }
        let (v, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(v)
    }
    pub fn u8(&mut self) -> Result<u8, Failure> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, Failure> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| Code::InvalidInput)?,
        ))
    }
    pub fn u32(&mut self) -> Result<u32, Failure> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| Code::InvalidInput)?,
        ))
    }
    pub fn u64(&mut self) -> Result<u64, Failure> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| Code::InvalidInput)?,
        ))
    }
    pub fn root(&mut self) -> Result<Root, Failure> {
        self.take(32)?
            .try_into()
            .map_err(|_| Code::InvalidInput.into())
    }
    pub fn blob(&mut self, max: usize) -> Result<Vec<u8>, Failure> {
        let n = self.u16()? as usize;
        if n > max {
            return Err(Code::Capacity.into());
        }
        Ok(self.take(n)?.to_vec())
    }
    pub fn count(&mut self, max: usize, min_width: usize) -> Result<usize, Failure> {
        let n = self.u16()? as usize;
        if n > max || n.checked_mul(min_width).ok_or(Code::Capacity)? > self.bytes.len() {
            return Err(Code::Capacity.into());
        }
        Ok(n)
    }
    pub fn finish(self) -> Result<(), Failure> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(Code::InvalidInput.into())
        }
    }
}
#[derive(Default)]
pub struct Encoder {
    bytes: Vec<u8>,
}
impl Encoder {
    pub fn put(&mut self, b: &[u8]) -> Result<(), Failure> {
        if self
            .bytes
            .len()
            .checked_add(b.len())
            .ok_or(Code::Capacity)?
            > METADATA_BYTES
        {
            return Err(Code::Capacity.into());
        }
        self.bytes.extend_from_slice(b);
        Ok(())
    }
    pub fn u8(&mut self, n: u8) -> Result<(), Failure> {
        self.put(&[n])
    }
    pub fn u16(&mut self, n: u16) -> Result<(), Failure> {
        self.put(&n.to_be_bytes())
    }
    pub fn u32(&mut self, n: u32) -> Result<(), Failure> {
        self.put(&n.to_be_bytes())
    }
    pub fn u64(&mut self, n: u64) -> Result<(), Failure> {
        self.put(&n.to_be_bytes())
    }
    pub fn count(&mut self, n: usize) -> Result<(), Failure> {
        self.u16(u16::try_from(n).map_err(|_| Code::Capacity)?)
    }
    pub fn blob(&mut self, b: &[u8]) -> Result<(), Failure> {
        self.count(b.len())?;
        self.put(b)
    }
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}
pub fn encode_request(r: &Request) -> Result<Vec<u8>, Failure> {
    encode_request_with_budget(r, r.deadline_ms)
}
pub fn encode_request_with_budget(r: &Request, remaining_ms: u32) -> Result<Vec<u8>, Failure> {
    r.validate()?;
    if remaining_ms == 0 || remaining_ms > r.deadline_ms {
        return Err(Code::InvalidInput.into());
    }
    let mut e = Encoder::default();
    e.u64(r.generation)?;
    e.u32(r.store)?;
    e.u16(r.profile)?;
    e.u32(remaining_ms)?;
    e.u64(r.response_bytes)?;
    e.u8(r.operation.opcode())?;
    match &r.operation {
        Operation::ReadFile { root, start, end } => {
            e.put(root)?;
            e.u64(*start)?;
            e.u64(*end)?;
        }
        Operation::ConstructFile { length } => e.u64(*length)?,
        Operation::Inspect { root, query } => {
            e.put(root)?;
            match query {
                Inspect::File => e.u8(0)?,
                Inspect::Stat { path } => {
                    e.u8(1)?;
                    e.blob(path)?;
                }
                Inspect::List {
                    path,
                    after,
                    entries,
                    bytes,
                } => {
                    e.u8(2)?;
                    e.blob(path)?;
                    e.blob(after)?;
                    e.u16(*entries)?;
                    e.u32(*bytes)?;
                }
                Inspect::Readlink { path } => {
                    e.u8(3)?;
                    e.blob(path)?;
                }
            }
        }
        Operation::EditFile {
            root,
            base_length,
            edits,
        } => {
            e.put(root)?;
            e.u64(*base_length)?;
            e.count(edits.len())?;
            for v in edits {
                e.u64(v.start)?;
                e.u64(v.end)?;
                e.u64(v.replacement)?;
            }
        }
        Operation::UpdatePreparedFilesystem {
            base,
            scope,
            root_serial,
            directories,
            inodes,
        } => {
            e.put(base)?;
            e.put(scope)?;
            e.u64(*root_serial)?;
            e.count(directories.len())?;
            for d in directories {
                e.u64(d.parent)?;
                e.count(d.changes.len())?;
                for (name, serial) in &d.changes {
                    e.blob(name)?;
                    e.u64(serial.unwrap_or(0))?;
                }
            }
            e.count(inodes.len())?;
            for i in inodes {
                e.u64(i.serial)?;
                e.u8(i.kind)?;
                e.put(&i.content)?;
                e.put(&i.metadata)?;
            }
        }
    }
    Ok(e.finish())
}
pub fn decode_request(id: u64, b: &[u8]) -> Result<Request, Failure> {
    let mut d = Decoder::new(b)?;
    let generation = d.u64()?;
    let store = d.u32()?;
    let profile = d.u16()?;
    let deadline_ms = d.u32()?;
    let response_bytes = d.u64()?;
    let operation = match d.u8()? {
        1 => Operation::ReadFile {
            root: d.root()?,
            start: d.u64()?,
            end: d.u64()?,
        },
        2 => {
            let root = d.root()?;
            let query = match d.u8()? {
                0 => Inspect::File,
                1 => Inspect::Stat {
                    path: d.blob(4096)?,
                },
                2 => Inspect::List {
                    path: d.blob(4096)?,
                    after: d.blob(255)?,
                    entries: d.u16()?,
                    bytes: d.u32()?,
                },
                3 => Inspect::Readlink {
                    path: d.blob(4096)?,
                },
                _ => return Err(Code::Unsupported.into()),
            };
            Operation::Inspect { root, query }
        }
        3 => Operation::ConstructFile { length: d.u64()? },
        4 => {
            let root = d.root()?;
            let base_length = d.u64()?;
            let count = d.count(256, 24)?;
            let mut edits = Vec::with_capacity(count);
            for _ in 0..count {
                edits.push(Edit {
                    start: d.u64()?,
                    end: d.u64()?,
                    replacement: d.u64()?,
                });
            }
            Operation::EditFile {
                root,
                base_length,
                edits,
            }
        }
        5 => {
            let base = d.root()?;
            let scope = d.root()?;
            let root_serial = d.u64()?;
            let count = d.count(128, 10)?;
            let mut directories = Vec::with_capacity(count);
            let mut total = 0;
            for _ in 0..count {
                let parent = d.u64()?;
                let n = d.count(128 - total, 10)?;
                total += n;
                let mut changes = Vec::with_capacity(n);
                for _ in 0..n {
                    let name = d.blob(255)?;
                    let serial = d.u64()?;
                    changes.push((name, (serial != 0).then_some(serial)));
                }
                directories.push(DirectoryChange { parent, changes });
            }
            let n = d.count(128, 73)?;
            let mut inodes = Vec::with_capacity(n);
            for _ in 0..n {
                inodes.push(InodeChange {
                    serial: d.u64()?,
                    kind: d.u8()?,
                    content: d.root()?,
                    metadata: d.root()?,
                });
            }
            Operation::UpdatePreparedFilesystem {
                base,
                scope,
                root_serial,
                directories,
                inodes,
            }
        }
        _ => return Err(Code::Unsupported.into()),
    };
    d.finish()?;
    let r = Request {
        id,
        generation,
        store,
        profile,
        deadline_ms,
        response_bytes,
        operation,
    };
    r.validate()?;
    Ok(r)
}
