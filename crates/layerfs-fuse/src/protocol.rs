use crate::{Attr, Kind, NodeId, PortError};
use std::io::{Read, Write};

const MAX_FRAME: usize = 17 * 1024 * 1024;
const MAX_BYTES: usize = 16 * 1024 * 1024;
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
    MkdirReserved(NodeId, Vec<u8>, u32, NodeId),
    ReaddirPlus(NodeId),
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
                | Self::MkdirReserved(..)
        )
    }

    pub(crate) const fn acknowledges_deferred_error(&self) -> bool {
        matches!(self, Self::Fence | Self::Fsync(_))
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
            Self::MkdirReserved(..) => "MkdirReserved",
            _ => "request",
        }
    }

    pub(crate) fn write_logical_bytes(&self) -> u64 {
        match self {
            Self::Write(_, _, bytes) => bytes.len() as u64,
            Self::WriteZero(_, _, len) => u64::from(*len),
            Self::CreateFilesClosedReserved(entries) => entries
                .iter()
                .flat_map(|entry| &entry.4)
                .fold(0_u64, |total, (_, bytes)| {
                    total.saturating_add(bytes.len() as u64)
                }),
            _ => 0,
        }
    }

    pub(crate) fn write_payload_bytes(&self) -> u64 {
        match self {
            Self::Write(_, _, bytes) => bytes.len() as u64,
            Self::CreateFilesClosedReserved(entries) => entries
                .iter()
                .flat_map(|entry| &entry.4)
                .fold(0_u64, |total, (_, bytes)| {
                    total.saturating_add(bytes.len() as u64)
                }),
            _ => 0,
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
    EntriesPlus(Vec<(Attr, Vec<u8>)>),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RequestWriteMeasurement {
    pub(crate) frame_bytes: u64,
    pub(crate) logical_bytes: u64,
    pub(crate) payload_copy_bytes: u64,
    pub(crate) encode_ns: u64,
    pub(crate) socket_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RequestReadMeasurement {
    pub(crate) frame_bytes: u64,
    pub(crate) logical_bytes: u64,
    pub(crate) payload_copy_bytes: u64,
    pub(crate) socket_ns: u64,
    pub(crate) decode_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResponseWriteMeasurement {
    pub(crate) frame_count: u64,
    pub(crate) frame_bytes: u64,
    pub(crate) logical_bytes: u64,
    pub(crate) payload_copy_bytes: u64,
    pub(crate) encode_ns: u64,
    pub(crate) socket_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResponseReadMeasurement {
    pub(crate) frame_count: u64,
    pub(crate) frame_bytes: u64,
    pub(crate) logical_bytes: u64,
    pub(crate) payload_copy_bytes: u64,
    pub(crate) socket_ns: u64,
    pub(crate) decode_ns: u64,
}

#[cfg(test)]
pub(crate) fn write_request(output: &mut impl Write, request: &Request) -> std::io::Result<()> {
    write_request_measured(output, request).map(|_| ())
}

pub(crate) fn write_request_measured(
    output: &mut impl Write,
    request: &Request,
) -> std::io::Result<RequestWriteMeasurement> {
    if let Request::Write(node, offset, value) = request {
        if value.len() > MAX_BYTES {
            return Err(invalid("byte length"));
        }
        let started = std::time::Instant::now();
        let body_len = 21_usize
            .checked_add(value.len())
            .filter(|length| *length <= MAX_FRAME)
            .ok_or_else(|| invalid("frame length"))?;
        let mut header = [0_u8; 25];
        header[..4].copy_from_slice(&u32::try_from(body_len).map_err(invalid)?.to_be_bytes());
        header[4] = 13;
        header[5..13].copy_from_slice(&node.0.to_be_bytes());
        header[13..21].copy_from_slice(&offset.to_be_bytes());
        header[21..25].copy_from_slice(&u32::try_from(value.len()).map_err(invalid)?.to_be_bytes());
        let encode_ns = elapsed_ns(started);
        let started = std::time::Instant::now();
        output.write_all(&header)?;
        output.write_all(value)?;
        output.flush()?;
        return Ok(RequestWriteMeasurement {
            frame_bytes: (header.len() as u64).saturating_add(value.len() as u64),
            logical_bytes: value.len() as u64,
            payload_copy_bytes: 0,
            encode_ns,
            socket_ns: elapsed_ns(started),
        });
    }
    let started = std::time::Instant::now();
    let bytes = encode_request(request)?;
    let encode_ns = elapsed_ns(started);
    let started = std::time::Instant::now();
    write_frame(output, &bytes)?;
    let socket_ns = elapsed_ns(started);
    Ok(RequestWriteMeasurement {
        frame_bytes: (bytes.len() as u64).saturating_add(4),
        logical_bytes: request.write_logical_bytes(),
        payload_copy_bytes: request.write_payload_bytes(),
        encode_ns,
        socket_ns,
    })
}

fn encode_request(request: &Request) -> std::io::Result<Vec<u8>> {
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
        Request::MkdirReserved(parent, name, mode, node) => {
            bytes.push(26);
            put_node(&mut bytes, *parent);
            put_bytes(&mut bytes, name)?;
            bytes.extend_from_slice(&mode.to_be_bytes());
            put_node(&mut bytes, *node);
        }
        Request::ReaddirPlus(node) => unary(&mut bytes, 27, *node),
    }
    Ok(bytes)
}

#[cfg(test)]
pub(crate) fn read_request(input: &mut impl Read) -> std::io::Result<Request> {
    read_request_measured(input).map(|(request, _)| request)
}

pub(crate) fn read_request_measured(
    input: &mut impl Read,
) -> std::io::Result<(Request, RequestReadMeasurement)> {
    let started = std::time::Instant::now();
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(invalid("frame length"));
    }
    let mut tag = [0];
    input.read_exact(&mut tag)?;
    let mut socket_ns = elapsed_ns(started);
    if tag == [13] {
        if length < 21 {
            return Err(invalid("frame length"));
        }
        let started = std::time::Instant::now();
        let mut header = [0; 20];
        input.read_exact(&mut header)?;
        socket_ns = socket_ns.saturating_add(elapsed_ns(started));
        let started = std::time::Instant::now();
        let node = NodeId(u64::from_be_bytes(header[..8].try_into().expect("node")));
        let offset = u64::from_be_bytes(header[8..16].try_into().expect("offset"));
        let payload_len =
            u32::from_be_bytes(header[16..20].try_into().expect("payload length")) as usize;
        if payload_len > MAX_BYTES || length != 21 + payload_len {
            return Err(invalid("byte length"));
        }
        let mut payload = vec![0; payload_len];
        let decode_ns = elapsed_ns(started);
        let started = std::time::Instant::now();
        input.read_exact(&mut payload)?;
        socket_ns = socket_ns.saturating_add(elapsed_ns(started));
        return Ok((
            Request::Write(node, offset, payload),
            RequestReadMeasurement {
                frame_bytes: (length as u64).saturating_add(4),
                logical_bytes: payload_len as u64,
                payload_copy_bytes: 0,
                socket_ns,
                decode_ns,
            },
        ));
    }
    let mut bytes = vec![0; length];
    bytes[0] = tag[0];
    let started = std::time::Instant::now();
    input.read_exact(&mut bytes[1..])?;
    socket_ns = socket_ns.saturating_add(elapsed_ns(started));
    let started = std::time::Instant::now();
    let request = decode_request(&bytes)?;
    let decode_ns = elapsed_ns(started);
    let measurement = RequestReadMeasurement {
        frame_bytes: (bytes.len() as u64).saturating_add(4),
        logical_bytes: request.write_logical_bytes(),
        payload_copy_bytes: request.write_payload_bytes(),
        socket_ns,
        decode_ns,
    };
    Ok((request, measurement))
}

fn decode_request(bytes: &[u8]) -> std::io::Result<Request> {
    let mut input = Input::new(bytes);
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
        26 => Request::MkdirReserved(
            input.node()?,
            input.bytes()?.to_vec(),
            input.u32()?,
            input.node()?,
        ),
        27 => Request::ReaddirPlus(input.node()?),
        _ => return Err(invalid("request tag")),
    };
    input.done()?;
    Ok(request)
}

#[cfg(test)]
pub(crate) fn write_response(output: &mut impl Write, response: &Response) -> std::io::Result<()> {
    write_response_measured(output, response).map(|_| ())
}

pub(crate) fn write_response_measured(
    output: &mut impl Write,
    response: &Response,
) -> std::io::Result<ResponseWriteMeasurement> {
    if let Response::Bytes(value) = response {
        if value.len() > MAX_BYTES {
            return Err(invalid("byte length"));
        }
        let started = std::time::Instant::now();
        let body_len = value
            .len()
            .checked_add(5)
            .filter(|length| *length <= MAX_FRAME)
            .ok_or_else(|| invalid("frame length"))?;
        let mut header = [0_u8; 9];
        header[..4].copy_from_slice(&u32::try_from(body_len).map_err(invalid)?.to_be_bytes());
        header[4] = 1;
        header[5..].copy_from_slice(&u32::try_from(value.len()).map_err(invalid)?.to_be_bytes());
        let encode_ns = elapsed_ns(started);
        let started = std::time::Instant::now();
        output.write_all(&header)?;
        output.write_all(value)?;
        output.flush()?;
        return Ok(ResponseWriteMeasurement {
            frame_count: 1,
            frame_bytes: (header.len() as u64).saturating_add(value.len() as u64),
            logical_bytes: value.len() as u64,
            payload_copy_bytes: 0,
            encode_ns,
            socket_ns: elapsed_ns(started),
        });
    }
    let started = std::time::Instant::now();
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
            return write_directory_response(
                output,
                entries,
                2,
                |entry| 13 + entry.2.len(),
                |bytes, (node, kind, name)| {
                    put_node(bytes, *node);
                    put_kind(bytes, *kind);
                    put_bytes(bytes, name)
                },
            );
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
        Response::EntriesPlus(entries) => {
            return write_directory_response(
                output,
                entries,
                7,
                |entry| 41 + entry.1.len(),
                |bytes, (attr, name)| {
                    put_attr(bytes, *attr);
                    put_bytes(bytes, name)
                },
            );
        }
    }
    let encode_ns = elapsed_ns(started);
    let frame_bytes = (bytes.len() as u64).saturating_add(4);
    let started = std::time::Instant::now();
    write_frame(output, &bytes)?;
    Ok(ResponseWriteMeasurement {
        frame_count: 1,
        frame_bytes,
        logical_bytes: 0,
        payload_copy_bytes: 0,
        encode_ns,
        socket_ns: elapsed_ns(started),
    })
}

// A directory remains one stable port/cache result. Split only its transport:
// tags 8/9 carry indexed Entries/EntriesPlus fragments and an explicit more bit.
// Legacy single-frame responses keep tags 2/7 and their exact representation.
// Both the existing per-frame entry limit and total encoded byte bound remain.
fn write_directory_response<T>(
    output: &mut impl Write,
    entries: &[T],
    final_tag: u8,
    entry_size: impl Fn(&T) -> usize,
    encode: impl Fn(&mut Vec<u8>, &T) -> std::io::Result<()>,
) -> std::io::Result<ResponseWriteMeasurement> {
    let started = std::time::Instant::now();
    let count = entries.len().div_ceil(MAX_ENTRIES).max(1);
    let encoded_bytes = entries.iter().try_fold(
        count
            .checked_mul(if count == 1 { 9 } else { 14 })
            .ok_or_else(|| invalid("directory length"))?,
        |total, entry| {
            total
                .checked_add(entry_size(entry))
                .filter(|total| *total <= MAX_FRAME + 4)
                .ok_or_else(|| invalid("directory length"))
        },
    )?;
    let mut bodies = Vec::with_capacity(encoded_bytes - count * 4);
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let start = bodies.len();
        let continued = index + 1 < count;
        bodies.push(if count == 1 {
            final_tag
        } else if final_tag == 2 {
            8
        } else {
            9
        });
        if count > 1 {
            bodies.extend_from_slice(&(index as u32).to_be_bytes());
            put_bool(&mut bodies, continued);
        }
        let entries = &entries[index * MAX_ENTRIES..entries.len().min((index + 1) * MAX_ENTRIES)];
        bodies.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for entry in entries {
            encode(&mut bodies, entry)?;
        }
        frames.push(start..bodies.len());
    }
    let encode_ns = elapsed_ns(started);
    let started = std::time::Instant::now();
    for frame in frames {
        write_frame(output, &bodies[frame])?;
    }
    Ok(ResponseWriteMeasurement {
        frame_count: count as u64,
        frame_bytes: encoded_bytes as u64,
        logical_bytes: 0,
        payload_copy_bytes: 0,
        encode_ns,
        socket_ns: elapsed_ns(started),
    })
}

#[cfg(test)]
pub(crate) fn read_response(input: &mut impl Read) -> std::io::Result<Response> {
    read_response_measured(input).map(|(response, _)| response)
}

pub(crate) fn read_response_measured(
    input: &mut impl Read,
) -> std::io::Result<(Response, ResponseReadMeasurement)> {
    let (mut response, mut measured, fragment) = read_response_frame(input, MAX_FRAME + 4)?;
    if fragment.is_some_and(|(index, _)| index != 0) {
        return Err(invalid("directory fragment order"));
    }
    let mut continued = fragment.is_some_and(|(_, more)| more);
    while continued {
        let remaining = (MAX_FRAME + 4)
            .checked_sub(measured.frame_bytes as usize)
            .ok_or_else(|| invalid("directory length"))?;
        let (next, part, fragment) = read_response_frame(input, remaining)?;
        let Some((index, more)) = fragment else {
            return Err(invalid("missing directory fragment"));
        };
        if u64::from(index) != measured.frame_count {
            return Err(invalid("directory fragment order"));
        }
        let appended = std::time::Instant::now();
        match (&mut response, next) {
            (Response::Entries(entries), Response::Entries(mut next)) => entries.append(&mut next),
            (Response::EntriesPlus(entries), Response::EntriesPlus(mut next)) => {
                entries.append(&mut next)
            }
            _ => return Err(invalid("directory continuation type")),
        }
        measured.frame_count += part.frame_count;
        measured.frame_bytes += part.frame_bytes;
        measured.socket_ns = measured.socket_ns.saturating_add(part.socket_ns);
        measured.decode_ns = measured
            .decode_ns
            .saturating_add(part.decode_ns)
            .saturating_add(elapsed_ns(appended));
        continued = more;
    }
    Ok((response, measured))
}

fn read_response_frame(
    input: &mut impl Read,
    remaining: usize,
) -> std::io::Result<(Response, ResponseReadMeasurement, Option<(u32, bool)>)> {
    let started = std::time::Instant::now();
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME || length.saturating_add(4) > remaining {
        return Err(invalid("frame length"));
    }
    let mut tag = [0];
    input.read_exact(&mut tag)?;
    let mut socket_ns = elapsed_ns(started);
    let frame_bytes = (length as u64).saturating_add(4);
    if tag == [1] {
        if length < 5 {
            return Err(invalid("frame length"));
        }
        let started = std::time::Instant::now();
        let mut payload_len = [0; 4];
        input.read_exact(&mut payload_len)?;
        socket_ns = socket_ns.saturating_add(elapsed_ns(started));
        let started = std::time::Instant::now();
        let payload_len = u32::from_be_bytes(payload_len) as usize;
        if payload_len > MAX_BYTES || length != payload_len + 5 {
            return Err(invalid("byte length"));
        }
        let mut payload = vec![0; payload_len];
        let decode_ns = elapsed_ns(started);
        let started = std::time::Instant::now();
        input.read_exact(&mut payload)?;
        socket_ns = socket_ns.saturating_add(elapsed_ns(started));
        return Ok((
            Response::Bytes(payload),
            ResponseReadMeasurement {
                frame_count: 1,
                frame_bytes,
                logical_bytes: payload_len as u64,
                payload_copy_bytes: 0,
                socket_ns,
                decode_ns,
            },
            None,
        ));
    }
    let mut bytes = vec![0; length];
    bytes[0] = tag[0];
    let started = std::time::Instant::now();
    input.read_exact(&mut bytes[1..])?;
    socket_ns = socket_ns.saturating_add(elapsed_ns(started));
    let started = std::time::Instant::now();
    let mut input = Input::new(&bytes);
    let tag = input.u8()?;
    let fragment = if matches!(tag, 8 | 9) {
        Some((input.u32()?, input.boolean()?))
    } else {
        None
    };
    let continued = fragment.is_some_and(|(_, more)| more);
    let response = match tag {
        0 => Response::Attr(input.attr()?),
        1 => Response::Bytes(input.bytes()?.to_vec()),
        2 | 8 => {
            let count = input.u32()? as usize;
            if count > MAX_ENTRIES || (continued && count != MAX_ENTRIES) {
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
        7 | 9 => {
            let count = input.u32()? as usize;
            if count > MAX_ENTRIES || (continued && count != MAX_ENTRIES) {
                return Err(invalid("entry count"));
            }
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                entries.push((input.attr()?, input.bytes()?.to_vec()));
            }
            Response::EntriesPlus(entries)
        }
        _ => return Err(invalid("response tag")),
    };
    input.done()?;
    let decode_ns = elapsed_ns(started);
    Ok((
        response,
        ResponseReadMeasurement {
            frame_count: 1,
            frame_bytes,
            logical_bytes: 0,
            payload_copy_bytes: 0,
            socket_ns,
            decode_ns,
        },
        fragment,
    ))
}

fn write_frame(output: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    if bytes.len() > MAX_FRAME {
        return Err(invalid("frame length"));
    }
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(bytes)?;
    output.flush()
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

fn elapsed_ns(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
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
    fn directory_fragments_preserve_complete_entries_and_reject_invalid_streams() {
        // The frozen wide directory has32,000 ordinary children plus dot entries.
        let entries: Vec<_> = (0..32_002)
            .map(|index| {
                (
                    NodeId(index),
                    Kind::File,
                    format!("f{index:05}").into_bytes(),
                )
            })
            .collect();
        let plus: Vec<_> = entries
            .iter()
            .map(|(node, _, name)| {
                (
                    Attr {
                        node: *node,
                        size: node.0,
                        kind: Kind::File,
                        mode: 0o644,
                        links: 1,
                        mtime_seconds: 7,
                        mtime_nanoseconds: 11,
                    },
                    name.clone(),
                )
            })
            .collect();
        let mut ordinary = Vec::new();
        let written =
            write_response_measured(&mut ordinary, &Response::Entries(entries.clone())).unwrap();
        let (decoded, measured) = read_response_measured(&mut ordinary.as_slice()).unwrap();
        let Response::Entries(decoded) = decoded else {
            panic!("entries")
        };
        assert_eq!(decoded, entries);
        assert_eq!(written.frame_count, 2);
        assert_eq!(measured.frame_count, 2);
        assert_eq!(written.frame_bytes, ordinary.len() as u64);
        assert_eq!(measured.frame_bytes, written.frame_bytes);
        let split = u32::from_be_bytes(ordinary[..4].try_into().unwrap()) as usize + 4;
        assert_eq!(ordinary[4], 8);
        assert_eq!(ordinary[9], 1);
        assert_eq!(
            u32::from_be_bytes(ordinary[10..14].try_into().unwrap()) as usize,
            MAX_ENTRIES
        );
        assert_eq!(ordinary[split + 9], 0);

        let mut extended = Vec::new();
        write_response(&mut extended, &Response::EntriesPlus(plus.clone())).unwrap();
        let Response::EntriesPlus(decoded) = read_response(&mut extended.as_slice()).unwrap()
        else {
            panic!("entries plus")
        };
        assert_eq!(decoded, plus);
        let plus_split = u32::from_be_bytes(extended[..4].try_into().unwrap()) as usize + 4;

        // Legacy small directories remain byte-compatible and single-frame.
        let mut small = Vec::new();
        assert_eq!(
            write_response_measured(&mut small, &Response::Entries(entries[..1].to_vec()))
                .unwrap()
                .frame_count,
            1
        );
        assert_eq!(small[4], 2);
        assert_eq!(
            read_response_measured(&mut small.as_slice())
                .unwrap()
                .1
                .frame_count,
            1
        );

        for length in [split, ordinary.len() - 1] {
            assert!(read_response(&mut &ordinary[..length]).is_err());
        }
        let mut reordered = ordinary[split..].to_vec();
        reordered.extend_from_slice(&ordinary[..split]);
        assert!(read_response(&mut reordered.as_slice()).is_err());
        let mut duplicated = ordinary.clone();
        duplicated[split + 5..split + 9].copy_from_slice(&0_u32.to_be_bytes());
        assert!(read_response(&mut duplicated.as_slice()).is_err());
        let mut inconsistent = ordinary[..split].to_vec();
        inconsistent.extend_from_slice(&extended[plus_split..]);
        assert!(read_response(&mut inconsistent.as_slice()).is_err());
        let mut oversized_count = ordinary.clone();
        oversized_count[10..14].copy_from_slice(&((MAX_ENTRIES + 1) as u32).to_be_bytes());
        assert!(read_response(&mut oversized_count.as_slice()).is_err());
        let mut oversized_total = ordinary[..split].to_vec();
        oversized_total.extend_from_slice(&(MAX_FRAME as u32).to_be_bytes());
        assert_eq!(
            read_response(&mut oversized_total.as_slice())
                .err()
                .unwrap()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
        let mut output = Vec::new();
        assert!(
            write_response(
                &mut output,
                &Response::Entries(vec![(NodeId(1), Kind::File, vec![0; MAX_FRAME])])
            )
            .is_err()
        );
        assert!(output.is_empty());
    }

    #[test]
    fn write_and_size_round_trip() {
        let mut bytes = Vec::new();
        let measured =
            write_request_measured(&mut bytes, &Request::Write(NodeId(2), 7, b"bytes".to_vec()))
                .unwrap();
        assert_eq!(measured.frame_bytes, 30);
        assert_eq!(measured.logical_bytes, 5);
        assert_eq!(measured.payload_copy_bytes, 0);
        assert_eq!(&bytes[..4], &26_u32.to_be_bytes());
        assert_eq!(bytes[4], 13);
        assert_eq!(&bytes[25..], b"bytes");
        assert!(read_request_measured(&mut &bytes[..bytes.len() - 1]).is_err());
        let (request, decoded) = read_request_measured(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded.frame_bytes, 30);
        assert_eq!(decoded.logical_bytes, 5);
        assert_eq!(decoded.payload_copy_bytes, 0);
        let Request::Write(node, offset, value) = request else {
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
        let measured =
            write_response_measured(&mut bytes, &Response::Bytes(b"bytes".to_vec())).unwrap();
        assert_eq!(measured.frame_bytes, 14);
        assert_eq!(measured.logical_bytes, 5);
        assert_eq!(measured.payload_copy_bytes, 0);
        assert_eq!(&bytes[..4], &10_u32.to_be_bytes());
        assert_eq!(bytes[4], 1);
        assert_eq!(&bytes[5..9], &5_u32.to_be_bytes());
        assert_eq!(&bytes[9..], b"bytes");
        assert!(read_response_measured(&mut &bytes[..bytes.len() - 1]).is_err());
        let (response, decoded) = read_response_measured(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded.frame_bytes, 14);
        assert_eq!(decoded.logical_bytes, 5);
        assert_eq!(decoded.payload_copy_bytes, 0);
        let Response::Bytes(value) = response else {
            panic!("bytes")
        };
        assert_eq!(value, b"bytes");

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
        write_request(&mut bytes, &Request::ReaddirPlus(NodeId(11))).unwrap();
        let Request::ReaddirPlus(node) = read_request(&mut bytes.as_slice()).unwrap() else {
            panic!("readdir plus")
        };
        assert_eq!(node, NodeId(11));

        let expected = Attr {
            node: NodeId(12),
            size: 2500,
            kind: Kind::File,
            mode: 0o644,
            links: 1,
            mtime_seconds: 7,
            mtime_nanoseconds: 11,
        };
        bytes.clear();
        write_response(
            &mut bytes,
            &Response::EntriesPlus(vec![(expected, b"file".to_vec())]),
        )
        .unwrap();
        let Response::EntriesPlus(entries) = read_response(&mut bytes.as_slice()).unwrap() else {
            panic!("entries plus")
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, expected);
        assert_eq!(entries[0].1, b"file");

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
