use crate::{Attr, Kind, NodeId, PortError};
use std::io::{Read, Write};

const MAX_FRAME: usize = 2 * 1024 * 1024;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 16_384;
const MAX_UNLINKS: usize = 512;

pub(crate) type ClosedCreate = (
    NodeId,
    Vec<u8>,
    u32,
    NodeId,
    Vec<(u64, Vec<u8>)>,
    Option<(i64, u32)>,
);

pub(crate) enum Request {
    Lookup(NodeId, Vec<u8>),
    Attr(NodeId),
    Readlink(NodeId),
    Readdir(NodeId),
    CreateFile(NodeId, Vec<u8>, u32),
    Mkdir(NodeId, Vec<u8>, u32),
    Symlink(NodeId, Vec<u8>, Vec<u8>),
    Link(NodeId, NodeId, Vec<u8>),
    Unlink(NodeId, Vec<u8>, bool),
    Rename(NodeId, Vec<u8>, NodeId, Vec<u8>, bool),
    Pin(NodeId, bool, bool),
    Unpin(NodeId, bool),
    Read(NodeId, u64, usize),
    Write(NodeId, u64, Vec<u8>),
    Truncate(NodeId, u64),
    Chmod(NodeId, u32),
    SetMtime(NodeId, i64, u32),
    Fsync(Option<NodeId>),
    CreateFileOpen(NodeId, Vec<u8>, u32),
    ReserveNodes(u32),
    CreateFileOpenReserved(NodeId, Vec<u8>, u32, NodeId),
    CreateFilesClosedReserved(Vec<ClosedCreate>),
    UnlinkBatch(Vec<(NodeId, Vec<u8>)>),
    WriteZero(NodeId, u64, u32),
    Fence,
    PinRead(NodeId),
}

impl Request {
    pub(crate) const fn no_reply(&self) -> bool {
        matches!(
            self,
            Self::Write(..)
                | Self::Unpin(..)
                | Self::CreateFileOpenReserved(..)
                | Self::CreateFilesClosedReserved(..)
                | Self::UnlinkBatch(..)
                | Self::WriteZero(..)
                | Self::PinRead(..)
        )
    }

    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Write(..) => "Write",
            Self::SetMtime(..) => "SetMtime",
            Self::Unpin(..) => "Unpin",
            Self::CreateFileOpenReserved(..) => "CreateFileOpenReserved",
            Self::CreateFilesClosedReserved(..) => "CreateFilesClosedReserved",
            Self::UnlinkBatch(..) => "UnlinkBatch",
            Self::WriteZero(..) => "WriteZero",
            Self::PinRead(..) => "PinRead",
            _ => "request",
        }
    }
}

pub(crate) enum Response {
    Attr(Attr),
    Bytes(Vec<u8>),
    Entries(Vec<(NodeId, Kind, Vec<u8>)>),
    Size(usize),
    Unit,
    Error(PortError),
    Node(NodeId),
}

pub(crate) fn write_request(output: &mut impl Write, request: &Request) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    match request {
        Request::Lookup(node, name) => {
            bytes.push(0);
            put_node(&mut bytes, *node);
            put_bytes(&mut bytes, name)?;
        }
        Request::Attr(node) => unary(&mut bytes, 1, *node),
        Request::Readlink(node) => unary(&mut bytes, 2, *node),
        Request::Readdir(node) => unary(&mut bytes, 3, *node),
        Request::CreateFile(node, name, mode) | Request::Mkdir(node, name, mode) => {
            bytes.push(if matches!(request, Request::CreateFile(..)) {
                4
            } else {
                5
            });
            put_node(&mut bytes, *node);
            put_bytes(&mut bytes, name)?;
            bytes.extend_from_slice(&mode.to_be_bytes());
        }
        Request::Symlink(node, name, target) => {
            bytes.push(6);
            put_node(&mut bytes, *node);
            put_bytes(&mut bytes, name)?;
            put_bytes(&mut bytes, target)?;
        }
        Request::Link(node, parent, name) => {
            bytes.push(7);
            put_node(&mut bytes, *node);
            put_node(&mut bytes, *parent);
            put_bytes(&mut bytes, name)?;
        }
        Request::Unlink(node, name, directory) => {
            bytes.push(8);
            put_node(&mut bytes, *node);
            put_bytes(&mut bytes, name)?;
            put_bool(&mut bytes, *directory);
        }
        Request::Rename(parent, name, target, target_name, no_replace) => {
            bytes.push(9);
            put_node(&mut bytes, *parent);
            put_bytes(&mut bytes, name)?;
            put_node(&mut bytes, *target);
            put_bytes(&mut bytes, target_name)?;
            put_bool(&mut bytes, *no_replace);
        }
        Request::Pin(node, truncate, writable) => {
            bytes.push(10);
            put_node(&mut bytes, *node);
            put_bool(&mut bytes, *truncate);
            put_bool(&mut bytes, *writable);
        }
        Request::Unpin(node, writable) => {
            bytes.push(11);
            put_node(&mut bytes, *node);
            put_bool(&mut bytes, *writable);
        }
        Request::Read(node, offset, size) => {
            bytes.push(12);
            put_node(&mut bytes, *node);
            bytes.extend_from_slice(&offset.to_be_bytes());
            bytes.extend_from_slice(&u32::try_from(*size).map_err(invalid)?.to_be_bytes());
        }
        Request::Write(node, offset, value) => {
            bytes.push(13);
            put_node(&mut bytes, *node);
            bytes.extend_from_slice(&offset.to_be_bytes());
            put_bytes(&mut bytes, value)?;
        }
        Request::Truncate(node, size) => {
            bytes.push(14);
            put_node(&mut bytes, *node);
            bytes.extend_from_slice(&size.to_be_bytes());
        }
        Request::Chmod(node, mode) => {
            bytes.push(15);
            put_node(&mut bytes, *node);
            bytes.extend_from_slice(&mode.to_be_bytes());
        }
        Request::SetMtime(node, seconds, nanos) => {
            bytes.push(16);
            put_node(&mut bytes, *node);
            bytes.extend_from_slice(&seconds.to_be_bytes());
            bytes.extend_from_slice(&nanos.to_be_bytes());
        }
        Request::Fsync(node) => {
            bytes.push(17);
            put_bool(&mut bytes, node.is_some());
            if let Some(node) = node {
                put_node(&mut bytes, *node);
            }
        }
        Request::CreateFileOpen(node, name, mode) => {
            bytes.push(18);
            put_node(&mut bytes, *node);
            put_bytes(&mut bytes, name)?;
            bytes.extend_from_slice(&mode.to_be_bytes());
        }
        Request::ReserveNodes(count) => {
            bytes.push(19);
            bytes.extend_from_slice(&count.to_be_bytes());
        }
        Request::CreateFileOpenReserved(parent, name, mode, node) => {
            bytes.push(20);
            put_node(&mut bytes, *parent);
            put_bytes(&mut bytes, name)?;
            bytes.extend_from_slice(&mode.to_be_bytes());
            put_node(&mut bytes, *node);
        }
        Request::CreateFilesClosedReserved(entries) => {
            if entries.len() > 128
                || entries.iter().map(|entry| entry.4.len()).sum::<usize>() > MAX_ENTRIES
                || entries
                    .iter()
                    .flat_map(|entry| &entry.4)
                    .map(|(_, bytes)| bytes.len())
                    .sum::<usize>()
                    > MAX_BYTES
            {
                return Err(invalid("create batch"));
            }
            bytes.push(21);
            bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes());
            for (parent, name, mode, node, writes, mtime) in entries {
                put_node(&mut bytes, *parent);
                put_bytes(&mut bytes, name)?;
                bytes.extend_from_slice(&mode.to_be_bytes());
                put_node(&mut bytes, *node);
                bytes.extend_from_slice(&(writes.len() as u32).to_be_bytes());
                for (offset, value) in writes {
                    bytes.extend_from_slice(&offset.to_be_bytes());
                    put_bytes(&mut bytes, value)?;
                }
                bytes.push(u8::from(mtime.is_some()));
                if let Some((seconds, nanos)) = mtime {
                    bytes.extend_from_slice(&seconds.to_be_bytes());
                    bytes.extend_from_slice(&nanos.to_be_bytes());
                }
            }
        }
        Request::UnlinkBatch(entries) => {
            if entries.len() > MAX_UNLINKS
                || entries.iter().map(|(_, name)| name.len()).sum::<usize>() > MAX_BYTES
            {
                return Err(invalid("unlink batch"));
            }
            bytes.push(22);
            bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes());
            for (parent, name) in entries {
                put_node(&mut bytes, *parent);
                put_bytes(&mut bytes, name)?;
            }
        }
        Request::WriteZero(node, offset, len) => {
            bytes.push(23);
            put_node(&mut bytes, *node);
            bytes.extend_from_slice(&offset.to_be_bytes());
            bytes.extend_from_slice(&len.to_be_bytes());
        }
        Request::Fence => bytes.push(24),
        Request::PinRead(node) => unary(&mut bytes, 25, *node),
    }
    write_frame(output, &bytes)
}

pub(crate) fn read_request(input: &mut impl Read) -> std::io::Result<Request> {
    let bytes = read_frame(input)?;
    let mut input = Input::new(&bytes);
    let request = match input.u8()? {
        0 => Request::Lookup(input.node()?, input.bytes()?.to_vec()),
        1 => Request::Attr(input.node()?),
        2 => Request::Readlink(input.node()?),
        3 => Request::Readdir(input.node()?),
        4 => Request::CreateFile(input.node()?, input.bytes()?.to_vec(), input.u32()?),
        5 => Request::Mkdir(input.node()?, input.bytes()?.to_vec(), input.u32()?),
        6 => Request::Symlink(
            input.node()?,
            input.bytes()?.to_vec(),
            input.bytes()?.to_vec(),
        ),
        7 => Request::Link(input.node()?, input.node()?, input.bytes()?.to_vec()),
        8 => Request::Unlink(input.node()?, input.bytes()?.to_vec(), input.boolean()?),
        9 => Request::Rename(
            input.node()?,
            input.bytes()?.to_vec(),
            input.node()?,
            input.bytes()?.to_vec(),
            input.boolean()?,
        ),
        10 => Request::Pin(input.node()?, input.boolean()?, input.boolean()?),
        11 => Request::Unpin(input.node()?, input.boolean()?),
        12 => Request::Read(input.node()?, input.u64()?, input.u32()? as usize),
        13 => Request::Write(input.node()?, input.u64()?, input.bytes()?.to_vec()),
        14 => Request::Truncate(input.node()?, input.u64()?),
        15 => Request::Chmod(input.node()?, input.u32()?),
        16 => Request::SetMtime(input.node()?, input.i64()?, input.u32()?),
        17 => Request::Fsync(input.boolean()?.then(|| input.node()).transpose()?),
        18 => Request::CreateFileOpen(input.node()?, input.bytes()?.to_vec(), input.u32()?),
        19 => Request::ReserveNodes(input.u32()?),
        20 => Request::CreateFileOpenReserved(
            input.node()?,
            input.bytes()?.to_vec(),
            input.u32()?,
            input.node()?,
        ),
        21 => {
            let count = input.u32()? as usize;
            if count > 128 {
                return Err(invalid("create batch"));
            }
            let mut entries = Vec::with_capacity(count);
            let mut write_count = 0_usize;
            let mut write_bytes = 0_usize;
            for _ in 0..count {
                let parent = input.node()?;
                let name = input.bytes()?.to_vec();
                let mode = input.u32()?;
                let node = input.node()?;
                let count = input.u32()? as usize;
                if count > 128 {
                    return Err(invalid("create batch"));
                }
                let mut writes = Vec::with_capacity(count);
                for _ in 0..count {
                    let offset = input.u64()?;
                    let bytes = input.bytes()?.to_vec();
                    write_count = write_count.saturating_add(1);
                    write_bytes = write_bytes.saturating_add(bytes.len());
                    writes.push((offset, bytes));
                }
                let mtime = if input.boolean()? {
                    Some((input.i64()?, input.u32()?))
                } else {
                    None
                };
                entries.push((parent, name, mode, node, writes, mtime));
            }
            if write_count > MAX_ENTRIES || write_bytes > MAX_BYTES {
                return Err(invalid("create batch"));
            }
            Request::CreateFilesClosedReserved(entries)
        }
        22 => {
            let count = input.u32()? as usize;
            if count > MAX_UNLINKS {
                return Err(invalid("unlink batch"));
            }
            let mut entries = Vec::with_capacity(count);
            let mut name_bytes = 0_usize;
            for _ in 0..count {
                let parent = input.node()?;
                let name = input.bytes()?.to_vec();
                name_bytes = name_bytes.saturating_add(name.len());
                entries.push((parent, name));
            }
            if name_bytes > MAX_BYTES {
                return Err(invalid("unlink batch"));
            }
            Request::UnlinkBatch(entries)
        }
        23 => Request::WriteZero(input.node()?, input.u64()?, input.u32()?),
        24 => Request::Fence,
        25 => Request::PinRead(input.node()?),
        _ => return Err(invalid("request tag")),
    };
    input.done()?;
    Ok(request)
}

pub(crate) fn write_response(output: &mut impl Write, response: &Response) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    match response {
        Response::Attr(attr) => {
            bytes.push(0);
            put_attr(&mut bytes, *attr);
        }
        Response::Bytes(value) => {
            bytes.push(1);
            put_bytes(&mut bytes, value)?;
        }
        Response::Entries(entries) => {
            if entries.len() > MAX_ENTRIES {
                return Err(invalid("entry count"));
            }
            bytes.push(2);
            bytes.extend_from_slice(&(entries.len() as u32).to_be_bytes());
            for (node, kind, name) in entries {
                put_node(&mut bytes, *node);
                put_kind(&mut bytes, *kind);
                put_bytes(&mut bytes, name)?;
            }
        }
        Response::Size(value) => {
            bytes.push(3);
            bytes.extend_from_slice(&u64::try_from(*value).map_err(invalid)?.to_be_bytes());
        }
        Response::Unit => bytes.push(4),
        Response::Error(error) => {
            bytes.push(5);
            bytes.push(error_code(*error));
        }
        Response::Node(node) => {
            bytes.push(6);
            put_node(&mut bytes, *node);
        }
    }
    write_frame(output, &bytes)
}

pub(crate) fn read_response(input: &mut impl Read) -> std::io::Result<Response> {
    let bytes = read_frame(input)?;
    let mut input = Input::new(&bytes);
    let response = match input.u8()? {
        0 => Response::Attr(input.attr()?),
        1 => Response::Bytes(input.bytes()?.to_vec()),
        2 => {
            let count = input.u32()? as usize;
            if count > MAX_ENTRIES {
                return Err(invalid("entry count"));
            }
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                entries.push((input.node()?, input.kind()?, input.bytes()?.to_vec()));
            }
            Response::Entries(entries)
        }
        3 => Response::Size(input.u64()?.try_into().map_err(invalid)?),
        4 => Response::Unit,
        5 => Response::Error(port_error(input.u8()?)?),
        6 => Response::Node(input.node()?),
        _ => return Err(invalid("response tag")),
    };
    input.done()?;
    Ok(response)
}

fn write_frame(output: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    if bytes.len() > MAX_FRAME {
        return Err(invalid("frame length"));
    }
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(bytes)?;
    output.flush()
}

fn read_frame(input: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(invalid("frame length"));
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn unary(output: &mut Vec<u8>, tag: u8, node: NodeId) {
    output.push(tag);
    put_node(output, node);
}

fn put_node(output: &mut Vec<u8>, node: NodeId) {
    output.extend_from_slice(&node.0.to_be_bytes());
}

fn put_bool(output: &mut Vec<u8>, value: bool) {
    output.push(u8::from(value));
}

fn put_bytes(output: &mut Vec<u8>, value: &[u8]) -> std::io::Result<()> {
    if value.len() > MAX_BYTES {
        return Err(invalid("byte length"));
    }
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
    Ok(())
}

fn put_kind(output: &mut Vec<u8>, kind: Kind) {
    output.push(match kind {
        Kind::File => 0,
        Kind::Directory => 1,
        Kind::Symlink => 2,
    });
}

fn put_attr(output: &mut Vec<u8>, attr: Attr) {
    put_node(output, attr.node);
    output.extend_from_slice(&attr.size.to_be_bytes());
    put_kind(output, attr.kind);
    output.extend_from_slice(&attr.mode.to_be_bytes());
    output.extend_from_slice(&attr.links.to_be_bytes());
    output.extend_from_slice(&attr.mtime_seconds.to_be_bytes());
    output.extend_from_slice(&attr.mtime_nanoseconds.to_be_bytes());
}

fn error_code(error: PortError) -> u8 {
    match error {
        PortError::NotFound => 0,
        PortError::NotEmpty => 1,
        PortError::Exists => 2,
        PortError::NoSpace => 3,
        PortError::ReadOnly => 4,
        PortError::Busy => 5,
        PortError::Invalid => 6,
        PortError::Io => 7,
    }
}

fn port_error(value: u8) -> std::io::Result<PortError> {
    Ok(match value {
        0 => PortError::NotFound,
        1 => PortError::NotEmpty,
        2 => PortError::Exists,
        3 => PortError::NoSpace,
        4 => PortError::ReadOnly,
        5 => PortError::Busy,
        6 => PortError::Invalid,
        7 => PortError::Io,
        _ => return Err(invalid("error tag")),
    })
}

struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Input<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn raw(&mut self, len: usize) -> std::io::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| invalid("truncated frame"))?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> std::io::Result<u8> {
        Ok(self.raw(1)?[0])
    }

    fn u32(&mut self) -> std::io::Result<u32> {
        Ok(u32::from_be_bytes(self.raw(4)?.try_into().expect("length")))
    }

    fn u64(&mut self) -> std::io::Result<u64> {
        Ok(u64::from_be_bytes(self.raw(8)?.try_into().expect("length")))
    }

    fn i64(&mut self) -> std::io::Result<i64> {
        Ok(i64::from_be_bytes(self.raw(8)?.try_into().expect("length")))
    }

    fn boolean(&mut self) -> std::io::Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid("boolean")),
        }
    }

    fn node(&mut self) -> std::io::Result<NodeId> {
        Ok(NodeId(self.u64()?))
    }

    fn bytes(&mut self) -> std::io::Result<&'a [u8]> {
        let len = self.u32()? as usize;
        if len > MAX_BYTES {
            return Err(invalid("byte length"));
        }
        self.raw(len)
    }

    fn kind(&mut self) -> std::io::Result<Kind> {
        Ok(match self.u8()? {
            0 => Kind::File,
            1 => Kind::Directory,
            2 => Kind::Symlink,
            _ => return Err(invalid("kind")),
        })
    }

    fn attr(&mut self) -> std::io::Result<Attr> {
        Ok(Attr {
            node: self.node()?,
            size: self.u64()?,
            kind: self.kind()?,
            mode: self.u32()?,
            links: self.u32()?,
            mtime_seconds: self.i64()?,
            mtime_nanoseconds: self.u32()?,
        })
    }

    fn done(self) -> std::io::Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid("trailing frame"))
        }
    }
}

fn invalid(_: impl std::fmt::Debug) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "LayerFS FUSE protocol")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_size_round_trip() {
        let mut bytes = Vec::new();
        write_request(&mut bytes, &Request::Write(NodeId(2), 7, b"bytes".to_vec())).unwrap();
        let Request::Write(node, offset, value) = read_request(&mut bytes.as_slice()).unwrap()
        else {
            panic!("write")
        };
        assert_eq!((node, offset, value), (NodeId(2), 7, b"bytes".to_vec()));

        bytes.clear();
        write_response(&mut bytes, &Response::Size(5)).unwrap();
        let Response::Size(size) = read_response(&mut bytes.as_slice()).unwrap() else {
            panic!("size")
        };
        assert_eq!(size, 5);

        bytes.clear();
        let entries = vec![(NodeId(1), b"one".to_vec()), (NodeId(2), b"two".to_vec())];
        write_request(&mut bytes, &Request::UnlinkBatch(entries.clone())).unwrap();
        let Request::UnlinkBatch(decoded) = read_request(&mut bytes.as_slice()).unwrap() else {
            panic!("unlink batch")
        };
        assert_eq!(decoded, entries);

        bytes.clear();
        write_request(&mut bytes, &Request::WriteZero(NodeId(3), 11, 4096)).unwrap();
        let Request::WriteZero(node, offset, len) = read_request(&mut bytes.as_slice()).unwrap()
        else {
            panic!("zero write")
        };
        assert_eq!((node, offset, len), (NodeId(3), 11, 4096));

        bytes.clear();
        write_request(&mut bytes, &Request::Fence).unwrap();
        assert!(matches!(
            read_request(&mut bytes.as_slice()).unwrap(),
            Request::Fence
        ));

        bytes.clear();
        write_request(&mut bytes, &Request::PinRead(NodeId(9))).unwrap();
        let Request::PinRead(node) = read_request(&mut bytes.as_slice()).unwrap() else {
            panic!("read pin")
        };
        assert_eq!(node, NodeId(9));

        bytes.clear();
        let entries = vec![(
            NodeId(1),
            b"file".to_vec(),
            0o600,
            NodeId(2),
            vec![(0, b"contents".to_vec())],
            Some((7, 11)),
        )];
        write_request(
            &mut bytes,
            &Request::CreateFilesClosedReserved(entries.clone()),
        )
        .unwrap();
        let Request::CreateFilesClosedReserved(decoded) =
            read_request(&mut bytes.as_slice()).unwrap()
        else {
            panic!("create batch")
        };
        assert_eq!(decoded, entries);
    }
}
