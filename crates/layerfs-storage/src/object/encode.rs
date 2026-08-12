//! Canonical `ELSOBJ01` envelope encoding.
//!
//! Payload owners stream their semantic fields, but every object kind uses
//! this single frozen envelope encoder so Create, Update, COW tree building,
//! version construction, admission, and readers cannot drift on magic,
//! schema, kind, profile binding, reserved flags, or payload length.

use crate::format::{
    validate_chunk_refs_per_file, validate_file_mode, validate_logical_length,
    validate_physical_object_len, PhysicalObjectKindV1,
};
use crate::identity::{
    FramedHasherV1, PhysicalChunkIdV1, PhysicalFileIdV1, PhysicalVersionRecordIdV1,
    TAG_PHYSICAL_CHUNK, TAG_PHYSICAL_FILE, TAG_PHYSICAL_SYMLINK, TAG_PHYSICAL_TREE,
    TAG_PHYSICAL_VERSION_RECORD,
};
use crate::object::TypedPhysicalObjectIdV1;
use crate::profile::ProfileSpecV1;
use crate::{CoreError, CoreResult};

use super::model::VersionRecordV1;

pub const OBJECT_HEADER_BYTES: u64 = 52;
pub const VERSION_RECORD_PAYLOAD_BYTES: u64 = 184;
pub(crate) const VERSION_RECORD_OBJECT_BYTES: usize =
    OBJECT_HEADER_BYTES as usize + VERSION_RECORD_PAYLOAD_BYTES as usize;

pub(super) const CANONICAL_OBJECT_MAGIC_V1: &[u8; 8] = b"ELSOBJ01";

pub(crate) fn encode_physical_object_header_v1(
    kind: PhysicalObjectKindV1,
    payload_len: u64,
) -> [u8; OBJECT_HEADER_BYTES as usize] {
    let mut header = [0_u8; OBJECT_HEADER_BYTES as usize];
    header[..8].copy_from_slice(CANONICAL_OBJECT_MAGIC_V1);
    header[8..10].copy_from_slice(&1_u16.to_be_bytes());
    header[10] = match kind {
        PhysicalObjectKindV1::VersionRecord => 0x01,
        PhysicalObjectKindV1::Tree => 0x02,
        PhysicalObjectKindV1::File => 0x03,
        PhysicalObjectKindV1::Symlink => 0x04,
        PhysicalObjectKindV1::Chunk => 0x05,
    };
    header[11] = 0;
    header[12..44].copy_from_slice(ProfileSpecV1::frozen().id().as_bytes());
    header[44..52].copy_from_slice(&payload_len.to_be_bytes());
    header
}

/// The one physical identity transcript used by every object producer and
/// port-backed verifier. Callers provide only already-validated bytes; the
/// domain frame and typed ID remain object-owned.
pub(crate) struct CanonicalPhysicalObjectVerifierV1 {
    kind: PhysicalObjectKindV1,
    hasher: FramedHasherV1,
}

impl CanonicalPhysicalObjectVerifierV1 {
    pub(crate) fn new(kind: PhysicalObjectKindV1, complete_len: u64) -> CoreResult<Self> {
        validate_physical_object_len(complete_len)?;
        Ok(Self {
            kind,
            hasher: FramedHasherV1::new(physical_domain_tag_v1(kind), complete_len),
        })
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> CoreResult<()> {
        self.hasher.write(bytes)
    }

    pub(crate) fn finish(self) -> CoreResult<TypedPhysicalObjectIdV1> {
        Ok(TypedPhysicalObjectIdV1::from_kind_and_digest(
            self.kind,
            self.hasher.finish()?,
        ))
    }
}

/// Streaming object emission backed by the canonical physical identity
/// transcript. Callers provide only already-validated semantic bytes through
/// the emitter; the envelope, exact complete length, and typed ID remain
/// object-owned.
pub(crate) struct CanonicalPhysicalObjectEncoderV1 {
    kind: PhysicalObjectKindV1,
    payload_len: u64,
    complete_len: u64,
    verifier: CanonicalPhysicalObjectVerifierV1,
}

impl CanonicalPhysicalObjectEncoderV1 {
    pub(crate) fn new(kind: PhysicalObjectKindV1, payload_len: u64) -> CoreResult<Self> {
        let complete_len = OBJECT_HEADER_BYTES
            .checked_add(payload_len)
            .ok_or(CoreError::IntegerOverflow)?;
        validate_physical_object_len(complete_len)?;
        Ok(Self {
            kind,
            payload_len,
            complete_len,
            verifier: CanonicalPhysicalObjectVerifierV1::new(kind, complete_len)?,
        })
    }

    pub(crate) const fn complete_len(&self) -> u64 {
        self.complete_len
    }

    pub(crate) fn emit<F>(&mut self, bytes: &[u8], sink: &mut F) -> CoreResult<()>
    where
        F: FnMut(&[u8]) -> CoreResult<()>,
    {
        self.verifier.write(bytes)?;
        sink(bytes)
    }

    pub(crate) fn emit_header<F>(&mut self, sink: &mut F) -> CoreResult<()>
    where
        F: FnMut(&[u8]) -> CoreResult<()>,
    {
        let header = encode_physical_object_header_v1(self.kind, self.payload_len);
        self.emit(&header, sink)
    }

    pub(crate) fn finish(self) -> CoreResult<TypedPhysicalObjectIdV1> {
        self.verifier.finish()
    }
}

/// The canonical full VersionRecord encoder. The pack writer supplies only
/// semantic fields; this owner emits the frozen payload layout and computes
/// the same framed physical identity used by every verifier.
pub(crate) struct EncodedVersionRecordV1 {
    bytes: [u8; VERSION_RECORD_OBJECT_BYTES],
    id: PhysicalVersionRecordIdV1,
}

impl EncodedVersionRecordV1 {
    pub(crate) const fn bytes(&self) -> &[u8; VERSION_RECORD_OBJECT_BYTES] {
        &self.bytes
    }

    pub(crate) const fn id(&self) -> PhysicalVersionRecordIdV1 {
        self.id
    }
}

pub(crate) fn encode_version_record_v1(
    record: VersionRecordV1,
) -> CoreResult<EncodedVersionRecordV1> {
    let mut payload = [0_u8; VERSION_RECORD_PAYLOAD_BYTES as usize];
    payload[..32].copy_from_slice(record.version_id.as_bytes());
    payload[32..64].copy_from_slice(record.chunker_spec_id.as_bytes());
    payload[64..96].copy_from_slice(record.digest_spec_id.as_bytes());
    payload[96..128].copy_from_slice(record.root_tree_id.as_bytes());
    payload[128..136].copy_from_slice(&record.canonical_len.to_be_bytes());
    payload[136..144].copy_from_slice(&record.logical_file_bytes.to_be_bytes());
    payload[144..148].copy_from_slice(&record.entry_count.to_be_bytes());
    payload[148..152].copy_from_slice(&record.tree_count.to_be_bytes());
    payload[152..156].copy_from_slice(&record.file_count.to_be_bytes());
    payload[156..160].copy_from_slice(&record.symlink_count.to_be_bytes());
    payload[160..164].copy_from_slice(&record.chunk_count.to_be_bytes());
    payload[164..168].copy_from_slice(&record.extent_count.to_be_bytes());
    payload[168..172].copy_from_slice(&record.chunk_ref_count.to_be_bytes());
    payload[172..176].copy_from_slice(&record.total_object_count.to_be_bytes());
    payload[176..184].copy_from_slice(&record.physical_chunk_bytes.to_be_bytes());

    let mut bytes = [0_u8; VERSION_RECORD_OBJECT_BYTES];
    let mut written = 0_usize;
    let mut sink = |chunk: &[u8]| {
        let end = written
            .checked_add(chunk.len())
            .ok_or(CoreError::IntegerOverflow)?;
        bytes
            .get_mut(written..end)
            .ok_or(CoreError::PhysicalObjectCap)?
            .copy_from_slice(chunk);
        written = end;
        Ok(())
    };
    let mut encoder = CanonicalPhysicalObjectEncoderV1::new(
        PhysicalObjectKindV1::VersionRecord,
        VERSION_RECORD_PAYLOAD_BYTES,
    )?;
    encoder.emit_header(&mut sink)?;
    encoder.emit(&payload, &mut sink)?;
    let id = match encoder.finish()? {
        TypedPhysicalObjectIdV1::VersionRecord(id) => id,
        _ => return Err(CoreError::TypeDomain),
    };
    if written != bytes.len() {
        return Err(CoreError::Truncated);
    }
    Ok(EncodedVersionRecordV1 { bytes, id })
}

/// Canonical streaming encoder for the frozen two-segment chunk payload.
pub(crate) struct CanonicalChunkObjectEncoderV1 {
    object: CanonicalPhysicalObjectEncoderV1,
}

impl CanonicalChunkObjectEncoderV1 {
    pub(crate) fn new(payload_len: u64) -> CoreResult<Self> {
        Ok(Self {
            object: CanonicalPhysicalObjectEncoderV1::new(
                PhysicalObjectKindV1::Chunk,
                payload_len,
            )?,
        })
    }

    pub(crate) const fn complete_len(&self) -> u64 {
        self.object.complete_len()
    }

    pub(crate) fn emit_header<F>(&mut self, sink: &mut F) -> CoreResult<()>
    where
        F: FnMut(&[u8]) -> CoreResult<()>,
    {
        self.object.emit_header(sink)
    }

    pub(crate) fn emit_segment<F>(&mut self, bytes: &[u8], sink: &mut F) -> CoreResult<()>
    where
        F: FnMut(&[u8]) -> CoreResult<()>,
    {
        self.object.emit(bytes, sink)
    }

    pub(crate) fn finish(self) -> CoreResult<PhysicalChunkIdV1> {
        match self.object.finish()? {
            TypedPhysicalObjectIdV1::Chunk(id) => Ok(id),
            _ => Err(CoreError::TypeDomain),
        }
    }
}

const FILE_FIXED_PAYLOAD_BYTES_V1: u64 = 14;
const DATA_EXTENT_FIXED_BYTES_V1: u64 = 13;
const CHUNK_REFERENCE_BYTES_V1: u64 = 36;

/// Canonical streaming encoder for a regular-file physical object. The
/// caller supplies logical identity work separately, but cannot choose the
/// physical field order, widths, tags, or length formula.
pub(crate) struct CanonicalFileObjectEncoderV1 {
    object: CanonicalPhysicalObjectEncoderV1,
    mode: u16,
    logical_len: u64,
    chunk_count: u64,
    references_written: u64,
    started: bool,
}

impl CanonicalFileObjectEncoderV1 {
    pub(crate) fn new(mode: u16, logical_len: u64, chunk_count: u64) -> CoreResult<Self> {
        validate_file_mode(mode)?;
        validate_logical_length(logical_len)?;
        validate_chunk_refs_per_file(chunk_count)?;
        let references_len = chunk_count
            .checked_mul(CHUNK_REFERENCE_BYTES_V1)
            .ok_or(CoreError::IntegerOverflow)?;
        let payload_len = FILE_FIXED_PAYLOAD_BYTES_V1
            .checked_add(if chunk_count == 0 {
                0
            } else {
                DATA_EXTENT_FIXED_BYTES_V1
                    .checked_add(references_len)
                    .ok_or(CoreError::IntegerOverflow)?
            })
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(Self {
            object: CanonicalPhysicalObjectEncoderV1::new(PhysicalObjectKindV1::File, payload_len)?,
            mode,
            logical_len,
            chunk_count,
            references_written: 0,
            started: false,
        })
    }

    pub(crate) const fn complete_len(&self) -> u64 {
        self.object.complete_len()
    }

    pub(crate) fn begin<F>(&mut self, sink: &mut F) -> CoreResult<()>
    where
        F: FnMut(&[u8]) -> CoreResult<()>,
    {
        if self.started {
            return Err(CoreError::NonCanonicalOrder);
        }
        self.object.emit_header(sink)?;
        self.object.emit(&self.mode.to_be_bytes(), sink)?;
        self.object.emit(&self.logical_len.to_be_bytes(), sink)?;
        self.object
            .emit(&u32::from(self.chunk_count != 0).to_be_bytes(), sink)?;
        if self.chunk_count != 0 {
            self.object.emit(&[0x02], sink)?;
            self.object.emit(&self.logical_len.to_be_bytes(), sink)?;
            let chunk_count =
                u32::try_from(self.chunk_count).map_err(|_| CoreError::IntegerOverflow)?;
            self.object.emit(&chunk_count.to_be_bytes(), sink)?;
        }
        self.started = true;
        Ok(())
    }

    pub(crate) fn emit_chunk_reference<F>(
        &mut self,
        len: u32,
        physical_id: &PhysicalChunkIdV1,
        sink: &mut F,
    ) -> CoreResult<()>
    where
        F: FnMut(&[u8]) -> CoreResult<()>,
    {
        if !self.started {
            return Err(CoreError::Truncated);
        }
        if self.references_written >= self.chunk_count {
            return Err(CoreError::TrailingBytes);
        }
        self.object.emit(&len.to_be_bytes(), sink)?;
        self.object.emit(physical_id.as_bytes(), sink)?;
        self.references_written = self
            .references_written
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn finish(self) -> CoreResult<PhysicalFileIdV1> {
        if !self.started || self.references_written != self.chunk_count {
            return Err(CoreError::Truncated);
        }
        match self.object.finish()? {
            TypedPhysicalObjectIdV1::File(id) => Ok(id),
            _ => Err(CoreError::TypeDomain),
        }
    }
}

/// Seal a caller-owned semantic payload buffer without giving the caller a
/// second physical writer. Tree construction uses this bounded in-place path
/// because its sink admits a complete immutable object slice.
pub(crate) fn seal_physical_object_in_place_v1(
    kind: PhysicalObjectKindV1,
    buffer: &mut [u8],
    payload_len: usize,
) -> CoreResult<(TypedPhysicalObjectIdV1, usize)> {
    let payload_len_u64 = u64::try_from(payload_len).map_err(|_| CoreError::IntegerOverflow)?;
    let complete_len_u64 = OBJECT_HEADER_BYTES
        .checked_add(payload_len_u64)
        .ok_or(CoreError::IntegerOverflow)?;
    let complete_len = usize::try_from(complete_len_u64).map_err(|_| CoreError::IntegerOverflow)?;
    if complete_len > buffer.len() {
        return Err(CoreError::PhysicalObjectCap);
    }
    buffer.copy_within(..payload_len, OBJECT_HEADER_BYTES as usize);
    let mut encoder = CanonicalPhysicalObjectEncoderV1::new(kind, payload_len_u64)?;
    let mut discard = |_bytes: &[u8]| Ok(());
    encoder.emit_header(&mut discard)?;
    encoder.emit(
        &buffer[OBJECT_HEADER_BYTES as usize..complete_len],
        &mut discard,
    )?;
    let id = encoder.finish()?;
    buffer[..OBJECT_HEADER_BYTES as usize]
        .copy_from_slice(&encode_physical_object_header_v1(kind, payload_len_u64));
    Ok((id, complete_len))
}

pub(crate) const fn physical_domain_tag_v1(kind: PhysicalObjectKindV1) -> u8 {
    match kind {
        PhysicalObjectKindV1::VersionRecord => TAG_PHYSICAL_VERSION_RECORD,
        PhysicalObjectKindV1::Tree => TAG_PHYSICAL_TREE,
        PhysicalObjectKindV1::File => TAG_PHYSICAL_FILE,
        PhysicalObjectKindV1::Symlink => TAG_PHYSICAL_SYMLINK,
        PhysicalObjectKindV1::Chunk => TAG_PHYSICAL_CHUNK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{
        derive_physical_version_record_id_v1, ChunkerSpecId, DigestSpecId, PhysicalTreeIdV1,
        VersionIdV1,
    };

    #[test]
    fn canonical_envelope_bytes_are_frozen_for_every_kind() {
        for (kind, tag) in [
            (PhysicalObjectKindV1::VersionRecord, 1),
            (PhysicalObjectKindV1::Tree, 2),
            (PhysicalObjectKindV1::File, 3),
            (PhysicalObjectKindV1::Symlink, 4),
            (PhysicalObjectKindV1::Chunk, 5),
        ] {
            let header = encode_physical_object_header_v1(kind, 0x0102_0304_0506_0708);
            assert_eq!(&header[..8], b"ELSOBJ01");
            assert_eq!(&header[8..10], &1_u16.to_be_bytes());
            assert_eq!(header[10], tag);
            assert_eq!(header[11], 0);
            assert_eq!(&header[12..44], ProfileSpecV1::frozen().id().as_bytes());
            assert_eq!(&header[44..52], &0x0102_0304_0506_0708_u64.to_be_bytes());
        }
    }

    #[test]
    fn canonical_version_record_encoder_matches_frozen_bytes_and_id() {
        let record = VersionRecordV1 {
            version_id: VersionIdV1::from_digest([0x11; 32]),
            chunker_spec_id: ChunkerSpecId::from_digest([0x22; 32]),
            digest_spec_id: DigestSpecId::from_digest([0x33; 32]),
            root_tree_id: PhysicalTreeIdV1::from_digest([0x44; 32]),
            canonical_len: 0x0102_0304_0506_0708,
            logical_file_bytes: 0x1112_1314_1516_1718,
            entry_count: 0x2122_2324,
            tree_count: 0x2526_2728,
            file_count: 0x292a_2b2c,
            symlink_count: 0x2d2e_2f30,
            chunk_count: 0x3132_3334,
            extent_count: 0x3536_3738,
            chunk_ref_count: 0x393a_3b3c,
            total_object_count: 0x3d3e_3f40,
            physical_chunk_bytes: 0x4142_4344_4546_4748,
        };
        let encoded = encode_version_record_v1(record).expect("version record encodes");
        let mut expected = [0_u8; VERSION_RECORD_OBJECT_BYTES];
        expected[..OBJECT_HEADER_BYTES as usize].copy_from_slice(
            &encode_physical_object_header_v1(
                PhysicalObjectKindV1::VersionRecord,
                VERSION_RECORD_PAYLOAD_BYTES,
            ),
        );
        let payload = &mut expected[OBJECT_HEADER_BYTES as usize..];
        payload[..32].copy_from_slice(record.version_id.as_bytes());
        payload[32..64].copy_from_slice(record.chunker_spec_id.as_bytes());
        payload[64..96].copy_from_slice(record.digest_spec_id.as_bytes());
        payload[96..128].copy_from_slice(record.root_tree_id.as_bytes());
        payload[128..136].copy_from_slice(&record.canonical_len.to_be_bytes());
        payload[136..144].copy_from_slice(&record.logical_file_bytes.to_be_bytes());
        payload[144..148].copy_from_slice(&record.entry_count.to_be_bytes());
        payload[148..152].copy_from_slice(&record.tree_count.to_be_bytes());
        payload[152..156].copy_from_slice(&record.file_count.to_be_bytes());
        payload[156..160].copy_from_slice(&record.symlink_count.to_be_bytes());
        payload[160..164].copy_from_slice(&record.chunk_count.to_be_bytes());
        payload[164..168].copy_from_slice(&record.extent_count.to_be_bytes());
        payload[168..172].copy_from_slice(&record.chunk_ref_count.to_be_bytes());
        payload[172..176].copy_from_slice(&record.total_object_count.to_be_bytes());
        payload[176..184].copy_from_slice(&record.physical_chunk_bytes.to_be_bytes());
        assert_eq!(encoded.bytes(), &expected);
        assert_eq!(
            encoded.id().as_bytes(),
            &[
                0x5b, 0xde, 0xdf, 0x3c, 0x17, 0xfe, 0xf7, 0x06, 0x05, 0xf7, 0xd7, 0xe0, 0x57, 0x20,
                0xa1, 0xf3, 0x26, 0x93, 0x4c, 0xeb, 0x3a, 0xd4, 0xc7, 0xc5, 0x50, 0x69, 0x9e, 0x6f,
                0x63, 0x77, 0x2a, 0x5e,
            ]
        );
        assert_eq!(
            encoded.id(),
            derive_physical_version_record_id_v1(&expected).expect("version id derives")
        );
    }
}
