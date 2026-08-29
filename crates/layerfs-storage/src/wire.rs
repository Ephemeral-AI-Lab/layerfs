use crate::{Result, StorageError};
use layerfs_content::ObjectId;
use std::io::{Read, Write};

const MAGIC: &[u8; 8] = b"LFSWIRE1";
const HEADER_BYTES: usize = 45;
pub const MAX_FRAME_BYTES: usize = layerfs_content::limits::MAX_OBJECT_BYTES + 128 * 1024;
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
        layerfs_content::decode_object(&bytes)?;
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

mod values {
    use super::WireValue;
    use crate::ids::{Decoder, Encoder};
    use crate::records::*;
    use crate::*;
    use layerfs_content::filesystem::ContentConflict;
    use layerfs_content::ObjectId;

    fn put_descriptors(out: &mut Encoder, objects: &[(ObjectId, u64)]) {
        out.u32(objects.len());
        for (id, len) in objects {
            out.raw(id.as_bytes());
            out.u64(*len);
        }
    }

    fn get_descriptors(input: &mut Decoder<'_>) -> Result<Vec<(ObjectId, u64)>> {
        let mut objects = Vec::new();
        for _ in 0..input.u32()? {
            objects.push((object(input)?, input.u64()?));
        }
        Ok(objects)
    }

    fn put_intent(out: &mut Encoder, intent: &TransferIntent) {
        match intent {
            TransferIntent::None => out.byte(0),
            TransferIntent::Branch { branch, expected } => {
                out.byte(1);
                put_fact(out, Fact::Branch(*branch));
                out.optional_id(expected.as_ref());
            }
            TransferIntent::Stack(push) => {
                out.byte(2);
                put_push(out, push);
            }
            TransferIntent::ObserveLayer(history) => {
                out.byte(3);
                put_fact(out, Fact::LayerHistory(*history));
            }
            TransferIntent::ObserveStack { history, expected } => {
                out.byte(4);
                put_fact(out, Fact::StackHistory(*history));
                out.optional_id(expected.as_ref());
            }
        }
    }

    fn get_intent(input: &mut Decoder<'_>) -> Result<TransferIntent> {
        match input.byte()? {
            0 => Ok(TransferIntent::None),
            1 => Ok(TransferIntent::Branch {
                branch: branch(get_fact(input)?)?,
                expected: input.optional_id(33)?,
            }),
            2 => Ok(TransferIntent::Stack(get_push(input)?)),
            3 => Ok(TransferIntent::ObserveLayer(layer_history(get_fact(
                input,
            )?)?)),
            4 => Ok(TransferIntent::ObserveStack {
                history: stack_history(get_fact(input)?)?,
                expected: input.optional_id(33)?,
            }),
            _ => Err(StorageError::Integrity("wire transfer intent")),
        }
    }

    fn put_exchange(out: &mut Encoder, exchange: TransferExchange) {
        let (admission, objects, facts) = exchange.into_parts();
        for value in [
            admission.objects.inserted_ids,
            admission.objects.inserted_bytes,
            admission.objects.raced_existing_ids,
            admission.objects.raced_existing_bytes,
        ] {
            out.u64(value);
        }
        for kind in crate::contract::FACT_KINDS {
            let receipt = admission.facts.get(&kind).copied().unwrap_or_default();
            for value in [
                receipt.inserted_ids,
                receipt.inserted_bytes,
                receipt.raced_existing_ids,
                receipt.raced_existing_bytes,
            ] {
                out.u64(value);
            }
        }
        for value in [
            admission.database.write_transactions,
            admission.database.rollback_transactions,
            admission.database.object_admission_transactions,
            admission.database.fact_admission_transactions,
            admission.database.visibility_transactions,
            admission.database.commit_sync_elapsed_ns,
        ] {
            out.u64(value);
        }
        out.raw(objects.as_bytes());
        out.raw(facts.as_bytes());
    }

    fn get_exchange(input: &mut Decoder<'_>) -> Result<TransferExchange> {
        let objects = crate::AdmissionSetReceipt {
            inserted_ids: input.u64()?,
            inserted_bytes: input.u64()?,
            raced_existing_ids: input.u64()?,
            raced_existing_bytes: input.u64()?,
        };
        let mut facts = std::collections::BTreeMap::new();
        for kind in crate::contract::FACT_KINDS {
            let receipt = crate::AdmissionSetReceipt {
                inserted_ids: input.u64()?,
                inserted_bytes: input.u64()?,
                raced_existing_ids: input.u64()?,
                raced_existing_bytes: input.u64()?,
            };
            if receipt != crate::AdmissionSetReceipt::default() {
                facts.insert(kind, receipt);
            }
        }
        Ok(TransferExchange::new(
            crate::AdmissionStats {
                objects,
                facts,
                database: crate::DatabaseReceipt {
                    write_transactions: input.u64()?,
                    rollback_transactions: input.u64()?,
                    object_admission_transactions: input.u64()?,
                    fact_admission_transactions: input.u64()?,
                    visibility_transactions: input.u64()?,
                    commit_sync_elapsed_ns: input.u64()?,
                },
            },
            MissingBitmap::from_bytes(input.raw(64)?.try_into().unwrap()),
            MissingBitmap::from_bytes(input.raw(64)?.try_into().unwrap()),
        ))
    }

    fn put_outcome(out: &mut Encoder, outcome: TransferOutcome) {
        match outcome {
            TransferOutcome::Unit => out.byte(0),
            TransferOutcome::Commit(value) => {
                out.byte(1);
                put_ref(out, &value);
            }
            TransferOutcome::Stack(value) => {
                out.byte(2);
                put_ref(out, &value);
            }
            TransferOutcome::Layer(value) => {
                out.byte(3);
                put_ref(out, &value);
            }
        }
    }

    fn get_outcome(input: &mut Decoder<'_>) -> Result<TransferOutcome> {
        match input.byte()? {
            0 => Ok(TransferOutcome::Unit),
            1 => Ok(TransferOutcome::Commit(get_ref(input, 33)?)),
            2 => Ok(TransferOutcome::Stack(get_ref(input, 33)?)),
            3 => Ok(TransferOutcome::Layer(get_ref(input, 33)?)),
            _ => Err(StorageError::Integrity("wire transfer outcome")),
        }
    }

    impl WireValue for EndpointRequest {
        fn encode(&self) -> Vec<u8> {
            let mut out = Encoder::new();
            match self {
                Self::ReadObjects(ids) => {
                    out.byte(0);
                    put_objects(&mut out, ids);
                }
                Self::Transfer {
                    objects,
                    facts,
                    object_ids,
                    fact_kind,
                    fact_ids,
                } => {
                    out.byte(1);
                    out.u32(objects.len());
                    for value in objects {
                        out.raw(value.0.as_bytes());
                        out.u64(value.1);
                    }
                    put_facts(&mut out, facts);
                    put_objects(&mut out, object_ids);
                    match fact_kind {
                        Some(kind) => {
                            out.byte(1);
                            out.byte(*kind as u8);
                        }
                        None => out.byte(0),
                    }
                    out.u32(fact_ids.len());
                    for id in fact_ids {
                        out.bytes(id);
                    }
                }
                Self::TransferEnd {
                    objects,
                    facts,
                    intent,
                } => {
                    out.byte(2);
                    put_descriptors(&mut out, objects);
                    put_facts(&mut out, facts);
                    put_intent(&mut out, intent);
                }
                Self::TransferAbort => out.byte(3),
                Self::BaseSnapshot(id) => {
                    out.byte(5);
                    put_base(&mut out, *id);
                }
                Self::CommonBase { left, right } => {
                    out.byte(6);
                    put_base(&mut out, *left);
                    put_base(&mut out, *right);
                }
                Self::CommitPages(id) => {
                    out.byte(7);
                    out.id(id);
                }
                Self::LayerHistoryPrefix {
                    history_id,
                    through,
                } => {
                    out.byte(8);
                    out.id(history_id);
                    out.id(through);
                }
                Self::StackHistoryPrefix {
                    history_id,
                    through,
                } => {
                    out.byte(9);
                    out.id(history_id);
                    out.id(through);
                }
                Self::StackHistoryHead(id) => {
                    out.byte(10);
                    out.id(id);
                }
                Self::AddStack {
                    stack_history_id,
                    branch_id,
                    commit_id,
                } => {
                    out.byte(14);
                    out.id(stack_history_id);
                    out.id(branch_id);
                    out.id(commit_id);
                }
                Self::AddLayer {
                    layer_history_id,
                    source,
                } => {
                    out.byte(15);
                    out.id(layer_history_id);
                    match source {
                        LayerSource::BranchCommit(source) => {
                            out.byte(0);
                            out.id(&source.branch_id);
                            out.id(&source.commit_id);
                        }
                        LayerSource::Stack(stack_id) => {
                            out.byte(1);
                            out.id(stack_id);
                        }
                    }
                }
                Self::BranchRecord(id) => {
                    out.byte(16);
                    out.id(id);
                }
                Self::LayerHistoryRecord(id) => {
                    out.byte(17);
                    out.id(id);
                }
                Self::TransferBeginBranch { branch, root } => {
                    out.byte(18);
                    put_fact(&mut out, Fact::Branch(*branch));
                    out.raw(root.as_bytes());
                }
                Self::TransferBeginStack {
                    history_id,
                    base_layer_id,
                    incoming,
                    root,
                } => {
                    out.byte(19);
                    out.id(history_id);
                    out.id(base_layer_id);
                    out.id(incoming);
                    out.raw(root.as_bytes());
                }
                Self::HistoryMissing(bitmap) => {
                    out.byte(20);
                    out.raw(bitmap.as_bytes());
                }
                Self::PushStack(id) => {
                    out.byte(21);
                    out.id(id);
                }
                Self::StackRecord(id) => {
                    out.byte(22);
                    out.id(id);
                }
                Self::StoreIdentity => out.byte(23),
            }
            out.0
        }

        fn decode(bytes: &[u8]) -> Result<Self> {
            let mut input = Decoder::new(bytes);
            let value = match input.byte()? {
                0 => Self::ReadObjects(get_objects(&mut input)?),
                1 => {
                    let mut objects = Vec::new();
                    for _ in 0..input.u32()? {
                        objects.push((object(&mut input)?, input.u64()?));
                    }
                    let facts = get_facts(&mut input)?;
                    let object_ids = get_objects(&mut input)?;
                    let fact_kind = match input.byte()? {
                        0 => None,
                        1 => Some(fact_kind(input.byte()?)?),
                        _ => return Err(StorageError::Integrity("wire fact announcement")),
                    };
                    let mut fact_ids = Vec::new();
                    for _ in 0..input.u32()? {
                        fact_ids.push(input.bytes()?.to_vec());
                    }
                    Self::Transfer {
                        objects,
                        facts,
                        object_ids,
                        fact_kind,
                        fact_ids,
                    }
                }
                2 => Self::TransferEnd {
                    objects: get_descriptors(&mut input)?,
                    facts: get_facts(&mut input)?,
                    intent: Box::new(get_intent(&mut input)?),
                },
                3 => Self::TransferAbort,
                5 => Self::BaseSnapshot(get_base(&mut input)?),
                6 => Self::CommonBase {
                    left: get_base(&mut input)?,
                    right: get_base(&mut input)?,
                },
                7 => Self::CommitPages(input.id(17)?),
                8 => Self::LayerHistoryPrefix {
                    history_id: input.id(17)?,
                    through: input.id(33)?,
                },
                9 => Self::StackHistoryPrefix {
                    history_id: input.id(49)?,
                    through: input.id(33)?,
                },
                10 => Self::StackHistoryHead(input.id(49)?),
                14 => Self::AddStack {
                    stack_history_id: input.id(49)?,
                    branch_id: input.id(17)?,
                    commit_id: input.id(33)?,
                },
                15 => Self::AddLayer {
                    layer_history_id: input.id(17)?,
                    source: match input.byte()? {
                        0 => LayerSource::BranchCommit(crate::BranchCommit {
                            branch_id: input.id(17)?,
                            commit_id: input.id(33)?,
                        }),
                        1 => LayerSource::Stack(input.id(33)?),
                        _ => return Err(StorageError::Integrity("wire Layer source")),
                    },
                },
                16 => Self::BranchRecord(input.id(17)?),
                17 => Self::LayerHistoryRecord(input.id(17)?),
                18 => Self::TransferBeginBranch {
                    branch: branch(get_fact(&mut input)?)?,
                    root: object(&mut input)?,
                },
                19 => Self::TransferBeginStack {
                    history_id: input.id(49)?,
                    base_layer_id: input.id(33)?,
                    incoming: input.id(33)?,
                    root: object(&mut input)?,
                },
                20 => Self::HistoryMissing(MissingBitmap::from_bytes(
                    input.raw(64)?.try_into().unwrap(),
                )),
                21 => Self::PushStack(input.id(33)?),
                22 => Self::StackRecord(input.id(33)?),
                23 => Self::StoreIdentity,
                _ => return Err(StorageError::Integrity("wire request")),
            };
            done(input, value)
        }
    }

    impl WireValue for EndpointReply {
        fn encode(&self) -> Vec<u8> {
            let mut out = Encoder::new();
            match self {
                Ok(value) => {
                    out.byte(0);
                    put_response(&mut out, value);
                }
                Err(error) => {
                    out.byte(1);
                    put_error(&mut out, error);
                }
            }
            out.0
        }

        fn decode(bytes: &[u8]) -> Result<Self> {
            let mut input = Decoder::new(bytes);
            let value = match input.byte()? {
                0 => Ok(get_response(&mut input)?),
                1 => Err(get_error(&mut input)?),
                _ => return Err(StorageError::Integrity("wire reply")),
            };
            done(input, value)
        }
    }

    fn put_response(out: &mut Encoder, value: &EndpointResponse) {
        match value {
            EndpointResponse::Objects(values) => {
                out.byte(0);
                out.u32(values.len());
                for value in values {
                    out.raw(value.0.as_bytes());
                    out.u64(value.1);
                }
            }
            EndpointResponse::Exchange(value) => {
                out.byte(1);
                put_exchange(out, value.clone());
            }
            EndpointResponse::TransferDone { exchange, outcome } => {
                out.byte(2);
                put_exchange(out, exchange.clone());
                put_outcome(out, *outcome);
            }
            EndpointResponse::BaseSnapshot(value) => {
                out.byte(3);
                put_base(out, value.base_id);
                out.id(&value.layer_history_id);
                out.raw(value.root_id.as_bytes());
            }
            EndpointResponse::BranchRecord(value) => {
                out.byte(4);
                put_fact(out, Fact::Branch(*value));
            }
            EndpointResponse::LayerHistoryRecord(value) => {
                out.byte(5);
                put_fact(out, Fact::LayerHistory(*value));
            }
            EndpointResponse::StackHistoryRecord(value) => {
                out.byte(6);
                put_fact(out, Fact::StackHistory(*value));
            }
            EndpointResponse::Facts(value) => {
                out.byte(7);
                put_facts(out, value);
            }
            EndpointResponse::CommitRef(value) => {
                out.byte(8);
                put_ref(out, value);
            }
            EndpointResponse::StackRef(value) => {
                out.byte(9);
                put_ref(out, value);
            }
            EndpointResponse::Unit => out.byte(10),
            EndpointResponse::StackAdd(value) => {
                out.byte(11);
                out.id(&value.result_id);
            }
            EndpointResponse::LayerAdd(value) => {
                out.byte(12);
                out.id(&value.result_id);
            }
            EndpointResponse::FactIds { kind, ids } => {
                out.byte(13);
                out.byte(match kind {
                    FactKind::Commit => 0,
                    FactKind::Branch => 1,
                    FactKind::LayerHistory => 2,
                    FactKind::Layer => 3,
                    FactKind::StackHistory => 4,
                    FactKind::Stack => 5,
                    FactKind::AddResult => 6,
                });
                out.u32(ids.len());
                for id in ids {
                    out.bytes(id);
                }
            }
            EndpointResponse::StackRecord(value) => {
                out.byte(14);
                put_fact(out, Fact::Stack(*value));
            }
            EndpointResponse::StoreIdentity(value) => {
                out.byte(15);
                out.raw(value);
            }
        }
    }

    fn get_response(input: &mut Decoder<'_>) -> Result<EndpointResponse> {
        Ok(match input.byte()? {
            0 => {
                let mut values = Vec::new();
                for _ in 0..input.u32()? {
                    values.push((object(input)?, input.u64()?));
                }
                EndpointResponse::Objects(values)
            }
            1 => EndpointResponse::Exchange(get_exchange(input)?),
            2 => EndpointResponse::TransferDone {
                exchange: get_exchange(input)?,
                outcome: get_outcome(input)?,
            },
            3 => EndpointResponse::BaseSnapshot(BaseSnapshot {
                base_id: get_base(input)?,
                layer_history_id: input.id(17)?,
                root_id: object(input)?,
            }),
            4 => EndpointResponse::BranchRecord(branch(get_fact(input)?)?),
            5 => EndpointResponse::LayerHistoryRecord(layer_history(get_fact(input)?)?),
            6 => EndpointResponse::StackHistoryRecord(stack_history(get_fact(input)?)?),
            7 => EndpointResponse::Facts(get_facts(input)?),
            8 => EndpointResponse::CommitRef(get_ref(input, 33)?),
            9 => EndpointResponse::StackRef(get_ref(input, 33)?),
            10 => EndpointResponse::Unit,
            11 => EndpointResponse::StackAdd(AddResult {
                result_id: input.id(33)?,
            }),
            12 => EndpointResponse::LayerAdd(AddResult {
                result_id: input.id(33)?,
            }),
            13 => {
                let kind = fact_kind(input.byte()?)?;
                let mut ids = Vec::new();
                for _ in 0..input.u32()? {
                    ids.push(input.bytes()?.to_vec());
                }
                EndpointResponse::FactIds { kind, ids }
            }
            14 => EndpointResponse::StackRecord(stack(get_fact(input)?)?),
            15 => {
                EndpointResponse::StoreIdentity(input.raw(32)?.try_into().expect("fixed identity"))
            }
            _ => return Err(StorageError::Integrity("wire response")),
        })
    }

    fn put_ref<I: StorageId>(out: &mut Encoder, value: &RefOutcome<I>) {
        match value {
            RefOutcome::Created(id) => {
                out.byte(0);
                out.id(id);
            }
            RefOutcome::FastForwarded(id) => {
                out.byte(1);
                out.id(id);
            }
            RefOutcome::UpToDate(id) => {
                out.byte(2);
                out.id(id);
            }
        }
    }

    fn get_ref<I: StorageId>(input: &mut Decoder<'_>, len: usize) -> Result<RefOutcome<I>> {
        let tag = input.byte()?;
        let id = input.id(len)?;
        match tag {
            0 => Ok(RefOutcome::Created(id)),
            1 => Ok(RefOutcome::FastForwarded(id)),
            2 => Ok(RefOutcome::UpToDate(id)),
            _ => Err(StorageError::Integrity("wire ref")),
        }
    }

    fn put_error(out: &mut Encoder, error: &StorageError) {
        out.byte(match error {
            StorageError::CommitHeadMoved(_) => 0,
            StorageError::StackHeadMoved(_) => 1,
            StorageError::LayerHeadMoved(_) => 2,
            StorageError::WrongStackHistory(_) => 3,
            StorageError::WrongLayerHistory(_) => 4,
            StorageError::ReadOnlyStackHistory(_) => 5,
            StorageError::WrongSourceRoute => 6,
            StorageError::NoCommonBase => 7,
            StorageError::AmbiguousMergeBase => 8,
            StorageError::MissingBaseData => 9,
            StorageError::Conflict(_) => 10,
            StorageError::StoreBusy => 11,
            StorageError::NotFound(_) => 13,
            _ => 12,
        });
        match error {
            StorageError::CommitHeadMoved(value) => put_head(out, value),
            StorageError::StackHeadMoved(value) => put_head(out, value),
            StorageError::LayerHeadMoved(value) => put_head(out, value),
            StorageError::WrongStackHistory(value) => {
                out.id(&value.expected);
                out.id(&value.actual);
            }
            StorageError::WrongLayerHistory(value) => {
                out.id(&value.expected);
                out.id(&value.actual);
            }
            StorageError::ReadOnlyStackHistory(value) => out.id(&value.history_id),
            StorageError::Conflict(value) => put_conflict(out, value),
            StorageError::WrongSourceRoute
            | StorageError::NoCommonBase
            | StorageError::AmbiguousMergeBase
            | StorageError::MissingBaseData
            | StorageError::StoreBusy
            | StorageError::NotFound(_) => {}
            other => out.bytes(other.to_string().as_bytes()),
        }
    }

    fn get_error(input: &mut Decoder<'_>) -> Result<StorageError> {
        Ok(match input.byte()? {
            0 => StorageError::CommitHeadMoved(get_head(input, 33)?),
            1 => StorageError::StackHeadMoved(get_head(input, 33)?),
            2 => StorageError::LayerHeadMoved(get_head(input, 33)?),
            3 => StorageError::WrongStackHistory(WrongHistory {
                expected: input.id(49)?,
                actual: input.id(49)?,
            }),
            4 => StorageError::WrongLayerHistory(WrongHistory {
                expected: input.id(17)?,
                actual: input.id(17)?,
            }),
            5 => StorageError::ReadOnlyStackHistory(ReadOnlyHistory {
                history_id: input.id(49)?,
            }),
            6 => StorageError::WrongSourceRoute,
            7 => StorageError::NoCommonBase,
            8 => StorageError::AmbiguousMergeBase,
            9 => StorageError::MissingBaseData,
            10 => StorageError::Conflict(Box::new(get_conflict(input)?)),
            11 => StorageError::StoreBusy,
            12 => StorageError::Database(format!(
                "remote: {}",
                String::from_utf8(input.bytes()?.to_vec())
                    .map_err(|_| StorageError::Integrity("wire error"))?
            )),
            13 => StorageError::NotFound("remote"),
            _ => return Err(StorageError::Integrity("wire error")),
        })
    }

    fn put_head<I: StorageId>(out: &mut Encoder, value: &crate::HeadMoved<I>) {
        out.optional_id(value.expected.as_ref());
        out.optional_id(value.actual.as_ref());
    }

    fn get_head<I: StorageId>(input: &mut Decoder<'_>, len: usize) -> Result<crate::HeadMoved<I>> {
        Ok(crate::HeadMoved {
            expected: input.optional_id(len)?,
            actual: input.optional_id(len)?,
        })
    }

    fn put_conflict(out: &mut Encoder, value: &ContentConflict) {
        out.bytes(value.path.as_bytes());
        put_object_option(out, value.base);
        put_object_option(out, value.current);
        put_object_option(out, value.candidate);
    }

    fn get_conflict(input: &mut Decoder<'_>) -> Result<ContentConflict> {
        Ok(ContentConflict {
            path: String::from_utf8(input.bytes()?.to_vec())
                .map_err(|_| StorageError::Integrity("wire path"))?,
            base: get_object_option(input)?,
            current: get_object_option(input)?,
            candidate: get_object_option(input)?,
        })
    }

    fn put_object_option(out: &mut Encoder, value: Option<ObjectId>) {
        out.byte(u8::from(value.is_some()));
        if let Some(value) = value {
            out.raw(value.as_bytes());
        }
    }

    fn get_object_option(input: &mut Decoder<'_>) -> Result<Option<ObjectId>> {
        match input.byte()? {
            0 => Ok(None),
            1 => Ok(Some(object(input)?)),
            _ => Err(StorageError::Integrity("wire option")),
        }
    }
}
