use crate::{Result, StorageError};
use layerfs_core::ObjectId;
use std::io::{Read, Write};

const MAGIC: &[u8; 8] = b"LFSWIRE1";
const HEADER_BYTES: usize = 45;
pub const MAX_FRAME_BYTES: usize = layerfs_core::limits::MAX_OBJECT_BYTES + 128 * 1024;
pub const ID_BATCH_COUNT: usize = 512;
pub const OBJECT_BATCH_COUNT: usize = 128;
pub const OBJECT_BATCH_BYTES: usize = 4 * 1024 * 1024;
pub const FACT_BATCH_COUNT: usize = 128;
pub const FACT_BATCH_BYTES: usize = 64 * 1024;

pub fn read_object_frames(
    input: &mut impl Read,
    descriptors: Vec<(ObjectId, u64)>,
) -> Result<Vec<CanonicalObject>> {
    descriptors
        .into_iter()
        .map(|(id, len)| {
            Ok(CanonicalObject {
                id,
                bytes: read_payload_bytes(input, len)?,
            })
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalObject {
    pub id: ObjectId,
    pub bytes: Vec<u8>,
}

impl CanonicalObject {
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        let id = ObjectId::for_bytes(&bytes);
        layerfs_core::decode_object(&bytes)?;
        Ok(Self { id, bytes })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissingBitmap([u8; 64]);

impl MissingBitmap {
    pub const fn empty() -> Self {
        Self([0; 64])
    }

    pub fn from_missing(len: usize, missing: impl Fn(usize) -> bool) -> Result<Self> {
        if len > ID_BATCH_COUNT {
            return Err(StorageError::InvalidInput("membership page"));
        }
        let mut bytes = [0; 64];
        for index in 0..len {
            if missing(index) {
                bytes[index / 8] |= 1 << (index % 8);
            }
        }
        Ok(Self(bytes))
    }

    pub fn is_missing(self, index: usize) -> Result<bool> {
        if index >= ID_BATCH_COUNT {
            return Err(StorageError::InvalidInput("bitmap index"));
        }
        Ok(self.0[index / 8] & (1 << (index % 8)) != 0)
    }

    pub fn validate_tail(self, len: usize) -> Result<()> {
        if len > ID_BATCH_COUNT {
            return Err(StorageError::Integrity("bitmap length"));
        }
        for index in len..ID_BATCH_COUNT {
            if self.is_missing(index)? {
                return Err(StorageError::Integrity("bitmap tail"));
            }
        }
        Ok(())
    }

    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FrameKind {
    Command = 1,
    Announcement = 2,
    Payload = 3,
    Reply = 4,
    Final = 5,
}

impl TryFrom<u8> for FrameKind {
    type Error = StorageError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Command),
            2 => Ok(Self::Announcement),
            3 => Ok(Self::Payload),
            4 => Ok(Self::Reply),
            5 => Ok(Self::Final),
            _ => Err(StorageError::Integrity("wire frame kind")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub kind: FrameKind,
    pub bytes: Vec<u8>,
}

pub fn write_frame(writer: &mut impl Write, frame: &Frame) -> Result<()> {
    write_frame_bytes(writer, frame.kind, &frame.bytes)
}

pub fn write_frame_bytes(writer: &mut impl Write, kind: FrameKind, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(StorageError::InvalidInput("wire frame size"));
    }
    let len =
        u32::try_from(bytes.len()).map_err(|_| StorageError::InvalidInput("wire frame size"))?;
    writer.write_all(MAGIC)?;
    writer.write_all(&[kind as u8])?;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(&checksum(kind, bytes))?;
    writer.write_all(bytes)?;
    writer.flush()?;
    Ok(())
}

pub trait WireValue: Sized {
    fn encode(&self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> Result<Self>;
}

pub fn write_value<T: WireValue>(
    writer: &mut impl Write,
    kind: FrameKind,
    value: &T,
) -> Result<()> {
    let bytes = value.encode();
    write_frame_bytes(writer, kind, &bytes)
}

pub fn read_value<T: WireValue>(reader: &mut impl Read, kind: FrameKind) -> Result<T> {
    let frame = read_frame(reader)?;
    if frame.kind != kind {
        return Err(StorageError::Integrity("wire frame sequence"));
    }
    T::decode(&frame.bytes)
}

pub fn read_frame(reader: &mut impl Read) -> Result<Frame> {
    let mut header = [0; HEADER_BYTES];
    reader
        .read_exact(&mut header)
        .map_err(|_| StorageError::Integrity("incomplete wire frame"))?;
    if &header[..8] != MAGIC {
        return Err(StorageError::Integrity("wire magic"));
    }
    let kind = FrameKind::try_from(header[8])?;
    let len = u32::from_be_bytes(header[9..13].try_into().unwrap()) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(StorageError::Integrity("wire frame size"));
    }
    let mut bytes = vec![0; len];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| StorageError::Integrity("incomplete wire frame"))?;
    if header[13..45] != checksum(kind, &bytes) {
        return Err(StorageError::Integrity("wire checksum"));
    }
    Ok(Frame { kind, bytes })
}

pub fn read_payload_bytes(reader: &mut impl Read, expected_len: u64) -> Result<Vec<u8>> {
    let frame = read_frame(reader)?;
    if frame.kind != FrameKind::Payload || frame.bytes.len() as u64 != expected_len {
        Err(StorageError::Integrity("wire object payload"))
    } else {
        Ok(frame.bytes)
    }
}

fn checksum(kind: FrameKind, bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/wire-frame/v1\0");
    hasher.update(&[kind as u8]);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

pub(crate) struct ByteInput<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> ByteInput<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(StorageError::Integrity("field length"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(StorageError::Integrity("field eof"))?;
        self.position = end;
        Ok(value)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub(crate) fn done(&self) -> bool {
        self.position == self.bytes.len()
    }
}
