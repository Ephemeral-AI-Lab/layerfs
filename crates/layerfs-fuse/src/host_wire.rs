//! Bounded host-authoritative filesystem requests over the authenticated backing
//! transport. A successful transport reply can contain a known operation error.
use crate::live_wire::{bytes_out, invalid, u64_out, Input};
use crate::{Attr, Kind, NodeId, PortResult};
use std::io;

pub const OPCODE: u8 = 64;
pub const VERSION: u8 = 1;
pub const MAX_IO: usize = 1024 * 1024;
pub const MAX_PAGE: usize = 128;
pub const MAX_REPLAY_REPLY: usize = 64 * 1024;

#[derive(Clone, Copy, Debug)]
pub enum Operation<'a> {
    Lookup {
        parent: NodeId,
        name: &'a [u8],
        kernel: bool,
    },
    Attr(NodeId),
    Readlink(NodeId),
    Directory {
        node: NodeId,
        after: u64,
        kernel: bool,
    },
    Create {
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
        open: bool,
        kernel: bool,
    },
    Mkdir {
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
        kernel: bool,
    },
    Symlink {
        parent: NodeId,
        name: &'a [u8],
        target: &'a [u8],
        kernel: bool,
    },
    Link {
        node: NodeId,
        parent: NodeId,
        name: &'a [u8],
        kernel: bool,
    },
    Unlink {
        parent: NodeId,
        name: &'a [u8],
        directory: bool,
    },
    Rename {
        parent: NodeId,
        name: &'a [u8],
        new_parent: NodeId,
        new_name: &'a [u8],
        no_replace: bool,
    },
    Pin {
        node: NodeId,
        directory: bool,
        truncate: bool,
        writable: bool,
    },
    Unpin {
        node: NodeId,
        directory: bool,
        writable: bool,
    },
    Forget {
        node: NodeId,
        count: u64,
    },
    Read {
        node: NodeId,
        offset: u64,
        size: u32,
    },
    Write {
        node: NodeId,
        offset: u64,
        bytes: &'a [u8],
    },
    Truncate {
        node: NodeId,
        size: u64,
    },
    Chmod {
        node: NodeId,
        mode: u32,
    },
    Mtime {
        node: NodeId,
        seconds: i64,
        nanos: u32,
    },
    Fsync(Option<NodeId>),
    Detach,
    /// Fixed 16-byte (node, count) records, at most one directory page.
    ForgetBatch {
        entries: &'a [u8],
    },
}

impl Operation<'_> {
    pub fn request_bound(self) -> io::Result<usize> {
        let payload = match self {
            Self::Lookup { name, .. }
            | Self::Create { name, .. }
            | Self::Mkdir { name, .. }
            | Self::Link { name, .. }
            | Self::Unlink { name, .. } => name.len(),
            Self::Symlink { name, target, .. } => {
                name.len().checked_add(target.len()).ok_or_else(invalid)?
            }
            Self::Rename { name, new_name, .. } => {
                name.len().checked_add(new_name.len()).ok_or_else(invalid)?
            }
            Self::Write { bytes, .. } => bytes.len(),
            Self::ForgetBatch { entries } => entries.len(),
            _ => 0,
        };
        payload
            .checked_add(128)
            .filter(|bound| *bound <= crate::live_wire::MAX_FRAME)
            .ok_or_else(invalid)
    }
    pub fn needs_replay(self) -> bool {
        !matches!(
            self,
            Self::Attr(_)
                | Self::Readlink(_)
                | Self::Read { .. }
                | Self::Lookup { kernel: false, .. }
                | Self::Directory { kernel: false, .. }
        )
    }
    pub fn response_bound(self) -> usize {
        match self {
            Self::Read { size, .. } => size as usize + 10,
            Self::Readlink(_) => 4096 + 10,
            Self::Directory { .. } => MAX_REPLAY_REPLY,
            _ => 64,
        }
    }
}

pub struct Request<'a> {
    pub session: [u8; 16],
    pub sequence: u64,
    pub acknowledged: u64,
    pub operation: Operation<'a>,
}

/// Session/sequence identify the replay slot. A newer cumulative acknowledgment
/// settles earlier replies without changing this slot's filesystem operation.
pub fn replay_digest(bytes: &[u8]) -> io::Result<[u8; 32]> {
    decode_request(bytes)?;
    Ok(layerfs_content::ObjectId::for_bytes(&bytes[34..]).to_bytes())
}

fn node(input: &mut Input<'_>) -> io::Result<NodeId> {
    let id = input.u64()?;
    if id == 0 {
        return Err(invalid());
    }
    Ok(NodeId(id))
}
fn boolean(input: &mut Input<'_>) -> io::Result<bool> {
    match input.byte()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid()),
    }
}
fn name<'a>(input: &mut Input<'a>) -> io::Result<&'a [u8]> {
    let bytes = input.bytes()?;
    layerfs_content::CanonicalName::from_bytes(bytes).map_err(|_| invalid())?;
    Ok(bytes)
}
pub fn decode_request(bytes: &[u8]) -> io::Result<Request<'_>> {
    if bytes.len() > crate::live_wire::MAX_FRAME {
        return Err(invalid());
    }
    let mut input = Input(bytes);
    if input.byte()? != OPCODE || input.byte()? != VERSION {
        return Err(invalid());
    }
    let session = input.raw(16)?.try_into().unwrap();
    let sequence = input.u64()?;
    let acknowledged = input.u64()?;
    let operation = match input.byte()? {
        1 => Operation::Lookup {
            parent: node(&mut input)?,
            name: name(&mut input)?,
            kernel: boolean(&mut input)?,
        },
        2 => Operation::Attr(node(&mut input)?),
        3 => Operation::Readlink(node(&mut input)?),
        4 => {
            let node = node(&mut input)?;
            let after = input.u64()?;
            Operation::Directory {
                node,
                after,
                kernel: boolean(&mut input)?,
            }
        }
        5 => Operation::Create {
            parent: node(&mut input)?,
            name: name(&mut input)?,
            mode: input.u32()?,
            open: boolean(&mut input)?,
            kernel: boolean(&mut input)?,
        },
        6 => Operation::Mkdir {
            parent: node(&mut input)?,
            name: name(&mut input)?,
            mode: input.u32()?,
            kernel: boolean(&mut input)?,
        },
        7 => {
            let parent = node(&mut input)?;
            let name = name(&mut input)?;
            let target = input.bytes()?;
            if target.len() > 4096 || target.contains(&0) {
                return Err(invalid());
            }
            Operation::Symlink {
                parent,
                name,
                target,
                kernel: boolean(&mut input)?,
            }
        }
        8 => Operation::Link {
            node: node(&mut input)?,
            parent: node(&mut input)?,
            name: name(&mut input)?,
            kernel: boolean(&mut input)?,
        },
        9 => Operation::Unlink {
            parent: node(&mut input)?,
            name: name(&mut input)?,
            directory: boolean(&mut input)?,
        },
        10 => Operation::Rename {
            parent: node(&mut input)?,
            name: name(&mut input)?,
            new_parent: node(&mut input)?,
            new_name: name(&mut input)?,
            no_replace: boolean(&mut input)?,
        },
        11 => Operation::Pin {
            node: node(&mut input)?,
            directory: boolean(&mut input)?,
            truncate: boolean(&mut input)?,
            writable: boolean(&mut input)?,
        },
        12 => Operation::Unpin {
            node: node(&mut input)?,
            directory: boolean(&mut input)?,
            writable: boolean(&mut input)?,
        },
        13 => Operation::Forget {
            node: node(&mut input)?,
            count: input.u64()?,
        },
        14 => {
            let node = node(&mut input)?;
            let offset = input.u64()?;
            let size = input.u32()?;
            if size as usize > MAX_IO || offset.checked_add(size as u64).is_none() {
                return Err(invalid());
            }
            Operation::Read { node, offset, size }
        }
        15 => {
            let node = node(&mut input)?;
            let offset = input.u64()?;
            let bytes = input.bytes()?;
            if bytes.len() > MAX_IO || offset.checked_add(bytes.len() as u64).is_none() {
                return Err(invalid());
            }
            Operation::Write {
                node,
                offset,
                bytes,
            }
        }
        16 => Operation::Truncate {
            node: node(&mut input)?,
            size: input.u64()?,
        },
        17 => Operation::Chmod {
            node: node(&mut input)?,
            mode: input.u32()?,
        },
        18 => {
            let node = node(&mut input)?;
            let seconds = input.u64()? as i64;
            let nanos = input.u32()?;
            if nanos >= 1_000_000_000 {
                return Err(invalid());
            }
            Operation::Mtime {
                node,
                seconds,
                nanos,
            }
        }
        19 => {
            let id = input.u64()?;
            Operation::Fsync((id != 0).then_some(NodeId(id)))
        }
        20 => Operation::Detach,
        21 => {
            let entries = input.bytes()?;
            validate_forget_batch(entries)?;
            Operation::ForgetBatch { entries }
        }
        _ => return Err(invalid()),
    };
    input.done()?;
    if operation.needs_replay() != (sequence != 0) {
        return Err(invalid());
    }
    Ok(Request {
        session,
        sequence,
        acknowledged,
        operation,
    })
}

pub fn encode_request(request: Request<'_>) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    out.try_reserve_exact(request.operation.request_bound()?)
        .map_err(io::Error::other)?;
    out.extend_from_slice(&[OPCODE, VERSION]);
    out.extend_from_slice(&request.session);
    u64_out(&mut out, request.sequence);
    u64_out(&mut out, request.acknowledged);
    macro_rules! id {
        ($node:expr) => {
            u64_out(&mut out, $node.0)
        };
    }
    macro_rules! bytes {
        ($bytes:expr) => {
            bytes_out(&mut out, $bytes)?
        };
    }
    match request.operation {
        Operation::Lookup {
            parent,
            name,
            kernel,
        } => {
            out.push(1);
            id!(parent);
            bytes!(name);
            out.push(kernel.into());
        }
        Operation::Attr(node) => {
            out.push(2);
            id!(node);
        }
        Operation::Readlink(node) => {
            out.push(3);
            id!(node);
        }
        Operation::Directory {
            node,
            after,
            kernel,
        } => {
            out.push(4);
            id!(node);
            u64_out(&mut out, after);
            out.push(kernel.into());
        }
        Operation::Create {
            parent,
            name,
            mode,
            open,
            kernel,
        } => {
            out.push(5);
            id!(parent);
            bytes!(name);
            out.extend_from_slice(&mode.to_be_bytes());
            out.extend_from_slice(&[open.into(), kernel.into()]);
        }
        Operation::Mkdir {
            parent,
            name,
            mode,
            kernel,
        } => {
            out.push(6);
            id!(parent);
            bytes!(name);
            out.extend_from_slice(&mode.to_be_bytes());
            out.push(kernel.into());
        }
        Operation::Symlink {
            parent,
            name,
            target,
            kernel,
        } => {
            out.push(7);
            id!(parent);
            bytes!(name);
            bytes!(target);
            out.push(kernel.into());
        }
        Operation::Link {
            node,
            parent,
            name,
            kernel,
        } => {
            out.push(8);
            id!(node);
            id!(parent);
            bytes!(name);
            out.push(kernel.into());
        }
        Operation::Unlink {
            parent,
            name,
            directory,
        } => {
            out.push(9);
            id!(parent);
            bytes!(name);
            out.push(directory.into());
        }
        Operation::Rename {
            parent,
            name,
            new_parent,
            new_name,
            no_replace,
        } => {
            out.push(10);
            id!(parent);
            bytes!(name);
            id!(new_parent);
            bytes!(new_name);
            out.push(no_replace.into());
        }
        Operation::Pin {
            node,
            directory,
            truncate,
            writable,
        } => {
            out.push(11);
            id!(node);
            out.extend_from_slice(&[directory.into(), truncate.into(), writable.into()]);
        }
        Operation::Unpin {
            node,
            directory,
            writable,
        } => {
            out.push(12);
            id!(node);
            out.extend_from_slice(&[directory.into(), writable.into()]);
        }
        Operation::Forget { node, count } => {
            out.push(13);
            id!(node);
            u64_out(&mut out, count);
        }
        Operation::Read { node, offset, size } => {
            out.push(14);
            id!(node);
            u64_out(&mut out, offset);
            out.extend_from_slice(&size.to_be_bytes());
        }
        Operation::Write {
            node,
            offset,
            bytes,
        } => {
            out.push(15);
            id!(node);
            u64_out(&mut out, offset);
            bytes!(bytes);
        }
        Operation::Truncate { node, size } => {
            out.push(16);
            id!(node);
            u64_out(&mut out, size);
        }
        Operation::Chmod { node, mode } => {
            out.push(17);
            id!(node);
            out.extend_from_slice(&mode.to_be_bytes());
        }
        Operation::Mtime {
            node,
            seconds,
            nanos,
        } => {
            out.push(18);
            id!(node);
            u64_out(&mut out, seconds as u64);
            out.extend_from_slice(&nanos.to_be_bytes());
        }
        Operation::Fsync(node) => {
            out.push(19);
            u64_out(&mut out, node.map_or(0, |node| node.0));
        }
        Operation::Detach => out.push(20),
        Operation::ForgetBatch { entries } => {
            out.push(21);
            bytes!(entries);
        }
    }
    decode_request(&out)?;
    Ok(out)
}

fn validate_forget_batch(bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty()
        || bytes.len() % 16 != 0
        || bytes.len() / 16 > MAX_PAGE
        || bytes.chunks_exact(16).any(|entry| entry[..8] == [0; 8])
    {
        return Err(invalid());
    }
    Ok(())
}
pub fn forget_batch_out(entries: &[(NodeId, u64)]) -> io::Result<Vec<u8>> {
    if entries.is_empty() || entries.len() > MAX_PAGE {
        return Err(invalid());
    }
    let mut out = Vec::with_capacity(entries.len() * 16);
    for (node, count) in entries {
        u64_out(&mut out, node.0);
        u64_out(&mut out, *count);
    }
    validate_forget_batch(&out)?;
    Ok(out)
}
pub fn forget_batch_in(bytes: &[u8]) -> io::Result<Vec<(NodeId, u64)>> {
    validate_forget_batch(bytes)?;
    Ok(bytes
        .chunks_exact(16)
        .map(|entry| {
            (
                NodeId(u64::from_be_bytes(entry[..8].try_into().unwrap())),
                u64::from_be_bytes(entry[8..].try_into().unwrap()),
            )
        })
        .collect())
}

pub enum Reply<'a> {
    Known(PortResult<&'a [u8]>),
    Unknown,
}
pub fn reply(sequence: u64, result: PortResult<&[u8]>) -> io::Result<Vec<u8>> {
    let mut out = vec![VERSION];
    u64_out(&mut out, sequence);
    match result {
        Ok(bytes) => {
            out.push(0);
            out.extend_from_slice(bytes);
        }
        Err(error) => {
            out.push(1);
            out.push(crate::protocol::error_code(error));
        }
    }
    if out.len() > crate::live_wire::MAX_FRAME {
        return Err(invalid());
    }
    Ok(out)
}
pub fn unknown(sequence: u64) -> Vec<u8> {
    let mut out = vec![VERSION];
    u64_out(&mut out, sequence);
    out.push(2);
    out
}
pub fn decode_reply(bytes: &[u8], sequence: u64) -> io::Result<Reply<'_>> {
    if bytes.len() > crate::live_wire::MAX_FRAME {
        return Err(invalid());
    }
    let mut input = Input(bytes);
    if input.byte()? != VERSION || input.u64()? != sequence {
        return Err(invalid());
    }
    Ok(match input.byte()? {
        0 => Reply::Known(Ok(input.0)),
        1 => {
            let error = crate::protocol::port_error(input.byte()?)?;
            input.done()?;
            Reply::Known(Err(error))
        }
        2 => {
            input.done()?;
            Reply::Unknown
        }
        _ => return Err(invalid()),
    })
}
pub fn attr_out(attr: Attr) -> Vec<u8> {
    let mut out = Vec::with_capacity(37);
    u64_out(&mut out, attr.node.0);
    u64_out(&mut out, attr.size);
    out.push(match attr.kind {
        Kind::File => 1,
        Kind::Directory => 2,
        Kind::Symlink => 3,
    });
    out.extend_from_slice(&attr.mode.to_be_bytes());
    out.extend_from_slice(&attr.links.to_be_bytes());
    u64_out(&mut out, attr.mtime_seconds as u64);
    out.extend_from_slice(&attr.mtime_nanoseconds.to_be_bytes());
    out
}
fn attr_input(input: &mut Input<'_>) -> io::Result<Attr> {
    let node = node(input)?;
    let size = input.u64()?;
    let kind = match input.byte()? {
        1 => Kind::File,
        2 => Kind::Directory,
        3 => Kind::Symlink,
        _ => return Err(invalid()),
    };
    let mode = input.u32()?;
    let links = input.u32()?;
    let mtime_seconds = input.u64()? as i64;
    let mtime_nanoseconds = input.u32()?;
    if mode & !0o7777 != 0 || mtime_nanoseconds >= 1_000_000_000 {
        return Err(invalid());
    }
    Ok(Attr {
        node,
        size,
        kind,
        mode,
        links,
        mtime_seconds,
        mtime_nanoseconds,
    })
}
pub fn attr_in(bytes: &[u8]) -> io::Result<Attr> {
    let mut input = Input(bytes);
    let attr = attr_input(&mut input)?;
    input.done()?;
    Ok(attr)
}
pub type Page = Vec<(u64, Attr, Vec<u8>)>;
fn page_name(cookie: u64, name: &[u8]) -> io::Result<()> {
    match (cookie, name) {
        (1, b".") | (2, b"..") => Ok(()),
        (3.., _) => layerfs_content::CanonicalName::from_bytes(name)
            .map(|_| ())
            .map_err(|_| invalid()),
        _ => Err(invalid()),
    }
}
pub fn page_out(entries: &[(u64, Attr, Vec<u8>)]) -> io::Result<Vec<u8>> {
    if entries.len() > MAX_PAGE {
        return Err(invalid());
    }
    let mut out = (entries.len() as u32).to_be_bytes().to_vec();
    let mut previous = 0;
    for (cookie, attr, name) in entries {
        if *cookie <= previous {
            return Err(invalid());
        }
        page_name(*cookie, name)?;
        previous = *cookie;
        u64_out(&mut out, *cookie);
        out.extend_from_slice(&attr_out(*attr));
        bytes_out(&mut out, name)?;
    }
    if out.len() + 10 > MAX_REPLAY_REPLY {
        return Err(invalid());
    }
    Ok(out)
}
pub fn page_in(bytes: &[u8]) -> io::Result<Page> {
    let mut input = Input(bytes);
    let count = input.u32()? as usize;
    if count > MAX_PAGE {
        return Err(invalid());
    }
    let mut entries = Vec::with_capacity(count);
    let mut previous = 0;
    for _ in 0..count {
        let cookie = input.u64()?;
        if cookie <= previous {
            return Err(invalid());
        }
        previous = cookie;
        let attr = attr_input(&mut input)?;
        let name = input.bytes()?;
        page_name(cookie, name)?;
        entries.push((cookie, attr, name.to_vec()));
    }
    input.done()?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledged_operation_errors_and_unknown_are_distinct() {
        let response = reply(19, Err(crate::PortError::NoSpace)).unwrap();
        assert!(matches!(
            decode_reply(&response, 19).unwrap(),
            Reply::Known(Err(crate::PortError::NoSpace))
        ));
        assert!(matches!(
            decode_reply(&unknown(19), 19).unwrap(),
            Reply::Unknown
        ));
        assert!(decode_reply(&response, 20).is_err());
        let mut malformed = response;
        malformed.push(0);
        assert!(decode_reply(&malformed, 19).is_err());
    }

    #[test]
    fn mutation_replay_identity_and_full_write_bounds_are_validated() {
        let body = vec![1; MAX_IO];
        let operation = Operation::Write {
            node: NodeId(2),
            offset: 7,
            bytes: &body,
        };
        let envelope = encode_request(Request {
            session: [9; 16],
            sequence: 23,
            acknowledged: 22,
            operation,
        })
        .unwrap();
        let decoded = decode_request(&envelope).unwrap();
        assert_eq!(
            (decoded.session, decoded.sequence, decoded.acknowledged),
            ([9; 16], 23, 22)
        );
        assert!(
            matches!(decoded.operation, Operation::Write { node: NodeId(2), offset: 7, bytes } if bytes == body)
        );
        assert!(encode_request(Request {
            session: [9; 16],
            sequence: 0,
            acknowledged: 22,
            operation
        })
        .is_err());
        let too_large = vec![0; MAX_IO + 1];
        assert!(encode_request(Request {
            session: [9; 16],
            sequence: 24,
            acknowledged: 23,
            operation: Operation::Write {
                node: NodeId(2),
                offset: 0,
                bytes: &too_large
            }
        })
        .is_err());
        assert!(encode_request(Request {
            session: [9; 16],
            sequence: 0,
            acknowledged: 23,
            operation: Operation::Read {
                node: NodeId(2),
                offset: u64::MAX,
                size: 2
            }
        })
        .is_err());
        assert!(encode_request(Request {
            session: [9; 16],
            sequence: 0,
            acknowledged: 23,
            operation: Operation::Lookup {
                parent: NodeId(1),
                name: b"../escape",
                kernel: false
            }
        })
        .is_err());
    }
}
