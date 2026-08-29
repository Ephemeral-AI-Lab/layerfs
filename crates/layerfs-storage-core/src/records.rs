use crate::{
    BaseId, BranchId, CommitId, LayerHistoryId, LayerId, ResultId, SourceId, StackHistoryId,
    StackId, StorageId,
};
use layerfs_core::ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerHistoryRecord {
    pub id: LayerHistoryId,
    pub head_layer_id: LayerId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerRecord {
    pub id: LayerId,
    pub history_id: LayerHistoryId,
    pub parent_id: Option<LayerId>,
    pub root_id: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StackHistoryRecord {
    pub id: StackHistoryId,
    pub base_layer_id: LayerId,
    pub head_stack_id: StackId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StackRecord {
    pub id: StackId,
    pub history_id: StackHistoryId,
    pub parent_id: Option<StackId>,
    pub root_id: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchRecord {
    pub id: BranchId,
    pub head_commit_id: CommitId,
    pub base_id: BaseId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitRecord {
    pub id: CommitId,
    pub root_id: ObjectId,
    pub parent_id: Option<CommitId>,
    pub merge_parent_id: Option<CommitId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddResultRecord {
    pub source_id: SourceId,
    pub result_id: ResultId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddResult<T> {
    pub result_id: T,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fact {
    Commit(CommitRecord),
    Branch(BranchRecord),
    LayerHistory(LayerHistoryRecord),
    Layer(LayerRecord),
    StackHistory(StackHistoryRecord),
    Stack(StackRecord),
    AddResult(AddResultRecord),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FactKind {
    Commit,
    Branch,
    LayerHistory,
    Layer,
    StackHistory,
    Stack,
    AddResult,
}

impl Fact {
    pub const fn kind(self) -> FactKind {
        match self {
            Self::Commit(_) => FactKind::Commit,
            Self::Branch(_) => FactKind::Branch,
            Self::LayerHistory(_) => FactKind::LayerHistory,
            Self::Layer(_) => FactKind::Layer,
            Self::StackHistory(_) => FactKind::StackHistory,
            Self::Stack(_) => FactKind::Stack,
            Self::AddResult(_) => FactKind::AddResult,
        }
    }

    pub fn id(self) -> Vec<u8> {
        match self {
            Self::Commit(value) => value.id.as_slice().to_vec(),
            Self::Branch(value) => value.id.as_slice().to_vec(),
            Self::LayerHistory(value) => value.id.as_slice().to_vec(),
            Self::Layer(value) => value.id.as_slice().to_vec(),
            Self::StackHistory(value) => value.id.as_slice().to_vec(),
            Self::Stack(value) => value.id.as_slice().to_vec(),
            Self::AddResult(value) => value.source_id.as_slice().to_vec(),
        }
    }

    pub const fn encoded_size(self) -> usize {
        match self {
            Self::Commit(_) | Self::Layer(_) | Self::Stack(_) => 132,
            Self::Branch(_) | Self::StackHistory(_) => 99,
            Self::LayerHistory(_) | Self::AddResult(_) => 66,
        }
    }

    pub fn signing_bytes(self) -> Vec<u8> {
        let mut bytes = vec![self.kind() as u8];
        match self {
            Self::Commit(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.root_id.as_bytes());
                optional(
                    &mut bytes,
                    value.parent_id.as_ref().map(StorageId::as_slice),
                );
                optional(
                    &mut bytes,
                    value.merge_parent_id.as_ref().map(StorageId::as_slice),
                );
            }
            Self::Branch(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.head_commit_id.as_slice());
                bytes.extend_from_slice(value.base_id.as_slice());
            }
            Self::LayerHistory(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.head_layer_id.as_slice());
            }
            Self::Layer(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.history_id.as_slice());
                optional(
                    &mut bytes,
                    value.parent_id.as_ref().map(StorageId::as_slice),
                );
                bytes.extend_from_slice(value.root_id.as_bytes());
            }
            Self::StackHistory(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.base_layer_id.as_slice());
                bytes.extend_from_slice(value.head_stack_id.as_slice());
            }
            Self::Stack(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.history_id.as_slice());
                optional(
                    &mut bytes,
                    value.parent_id.as_ref().map(StorageId::as_slice),
                );
                bytes.extend_from_slice(value.root_id.as_bytes());
            }
            Self::AddResult(value) => {
                bytes.extend_from_slice(value.source_id.as_slice());
                bytes.extend_from_slice(value.result_id.as_slice());
            }
        }
        bytes
    }
}

fn optional(output: &mut Vec<u8>, value: Option<&[u8]>) {
    output.push(u8::from(value.is_some()));
    if let Some(value) = value {
        output.extend_from_slice(value);
    }
}

use crate::ids::{Decoder, Encoder};
use crate::{StackPush, StorageError};

pub(crate) fn put_facts(out: &mut Encoder, facts: &[Fact]) {
    out.u32(facts.len());
    for fact in facts {
        put_fact(out, *fact);
    }
}
pub(crate) fn get_facts(input: &mut Decoder<'_>) -> crate::Result<Vec<Fact>> {
    let mut values = Vec::new();
    for _ in 0..input.u32()? {
        values.push(get_fact(input)?);
    }
    Ok(values)
}
pub(crate) fn put_fact(out: &mut Encoder, fact: Fact) {
    out.bytes(&fact.signing_bytes());
}
pub(crate) fn get_fact(input: &mut Decoder<'_>) -> crate::Result<Fact> {
    decode_fact(input.bytes()?)
}

#[doc(hidden)]
pub fn decode_fact(bytes: &[u8]) -> crate::Result<Fact> {
    let mut input = Decoder::new(bytes);
    let fact = match fact_kind(input.byte()?)? {
        FactKind::Commit => Fact::Commit(CommitRecord {
            id: input.id(33)?,
            root_id: object(&mut input)?,
            parent_id: input.optional_id(33)?,
            merge_parent_id: input.optional_id(33)?,
        }),
        FactKind::Branch => Fact::Branch(BranchRecord {
            id: input.id(17)?,
            head_commit_id: input.id(33)?,
            base_id: get_base(&mut input)?,
        }),
        FactKind::LayerHistory => Fact::LayerHistory(LayerHistoryRecord {
            id: input.id(17)?,
            head_layer_id: input.id(33)?,
        }),
        FactKind::Layer => Fact::Layer(LayerRecord {
            id: input.id(33)?,
            history_id: input.id(17)?,
            parent_id: input.optional_id(33)?,
            root_id: object(&mut input)?,
        }),
        FactKind::StackHistory => Fact::StackHistory(StackHistoryRecord {
            id: input.id(49)?,
            base_layer_id: input.id(33)?,
            head_stack_id: input.id(33)?,
        }),
        FactKind::Stack => Fact::Stack(StackRecord {
            id: input.id(33)?,
            history_id: input.id(49)?,
            parent_id: input.optional_id(33)?,
            root_id: object(&mut input)?,
        }),
        FactKind::AddResult => Fact::AddResult(AddResultRecord {
            source_id: get_source(&mut input)?,
            result_id: ResultId::from_slice(input.raw(33)?)?,
        }),
    };
    done(input, fact)
}

pub(crate) fn put_push(out: &mut Encoder, value: &StackPush) {
    out.id(&value.history_id);
    out.id(&value.base_layer_id);
    out.optional_id(value.expected_head.as_ref());
    out.id(&value.incoming_head);
    out.u64(value.fact_count);
    out.u64(value.root_count);
    out.raw(&value.provenance_digest);
    out.u64(value.publication_count);
    out.raw(&value.publication_digest);
    out.raw(&value.public_key);
    out.raw(&value.signature);
}
pub(crate) fn get_push(input: &mut Decoder<'_>) -> crate::Result<StackPush> {
    Ok(StackPush {
        history_id: input.id(49)?,
        base_layer_id: input.id(33)?,
        expected_head: input.optional_id(33)?,
        incoming_head: input.id(33)?,
        fact_count: input.u64()?,
        root_count: input.u64()?,
        provenance_digest: input.raw(32)?.try_into().unwrap(),
        publication_count: input.u64()?,
        publication_digest: input.raw(32)?.try_into().unwrap(),
        public_key: input.raw(32)?.try_into().unwrap(),
        signature: input.raw(64)?.try_into().unwrap(),
    })
}
pub(crate) fn put_objects(out: &mut Encoder, ids: &[ObjectId]) {
    out.u32(ids.len());
    for id in ids {
        out.raw(id.as_bytes());
    }
}
pub(crate) fn get_objects(input: &mut Decoder<'_>) -> crate::Result<Vec<ObjectId>> {
    let mut ids = Vec::new();
    for _ in 0..input.u32()? {
        ids.push(object(input)?);
    }
    Ok(ids)
}
pub(crate) fn object(input: &mut Decoder<'_>) -> crate::Result<ObjectId> {
    Ok(ObjectId::from_bytes(input.raw(32)?)?)
}
pub(crate) fn put_base(out: &mut Encoder, id: BaseId) {
    out.raw(id.as_slice());
}
pub(crate) fn get_base(input: &mut Decoder<'_>) -> crate::Result<BaseId> {
    BaseId::from_slice(input.raw(33)?)
}
fn get_source(input: &mut Decoder<'_>) -> crate::Result<SourceId> {
    let tag = input.byte()?;
    let len = match tag {
        0x11 => 17,
        0x22 => 33,
        _ => return Err(StorageError::Integrity("wire source")),
    };
    let mut bytes = vec![tag];
    bytes.extend_from_slice(input.raw(len - 1)?);
    SourceId::from_slice(&bytes)
}
pub(crate) fn fact_kind(value: u8) -> crate::Result<FactKind> {
    match value {
        0 => Ok(FactKind::Commit),
        1 => Ok(FactKind::Branch),
        2 => Ok(FactKind::LayerHistory),
        3 => Ok(FactKind::Layer),
        4 => Ok(FactKind::StackHistory),
        5 => Ok(FactKind::Stack),
        6 => Ok(FactKind::AddResult),
        _ => Err(StorageError::Integrity("wire fact kind")),
    }
}
macro_rules! fact_value {
    ($name:ident, $variant:ident, $type:ty) => {
        pub(crate) fn $name(fact: Fact) -> crate::Result<$type> {
            if let Fact::$variant(value) = fact {
                Ok(value)
            } else {
                Err(StorageError::Integrity(concat!(
                    "wire ",
                    stringify!($variant)
                )))
            }
        }
    };
}
fact_value!(branch, Branch, BranchRecord);
fact_value!(layer_history, LayerHistory, LayerHistoryRecord);
fact_value!(stack_history, StackHistory, StackHistoryRecord);

pub(crate) fn done<T>(input: Decoder<'_>, value: T) -> crate::Result<T> {
    if input.done() {
        Ok(value)
    } else {
        Err(StorageError::Integrity("wire trailing bytes"))
    }
}

use crate::wire::WireValue;
use crate::{
    AddLayerSource, BaseSnapshot, Conflict, EndpointReply, EndpointRequest, EndpointResponse,
    MissingBitmap, ReadOnlyHistory, RefOutcome, Result, TransferExchange, TransferIntent,
    TransferOutcome, WrongHistory,
};

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
        admission.inserted_ids,
        admission.inserted_bytes,
        admission.raced_existing_ids,
        admission.raced_existing_bytes,
        admission.transactions,
    ] {
        out.u64(value);
    }
    out.raw(objects.as_bytes());
    out.raw(facts.as_bytes());
}

fn get_exchange(input: &mut Decoder<'_>) -> Result<TransferExchange> {
    Ok(TransferExchange::new(
        crate::AdmissionStats {
            inserted_ids: input.u64()?,
            inserted_bytes: input.u64()?,
            raced_existing_ids: input.u64()?,
            raced_existing_bytes: input.u64()?,
            transactions: input.u64()?,
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
                    AddLayerSource::BranchSource {
                        branch_id,
                        commit_id,
                    } => {
                        out.byte(0);
                        out.id(branch_id);
                        out.id(commit_id);
                    }
                    AddLayerSource::StackSource(stack_id) => {
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
                    0 => AddLayerSource::BranchSource {
                        branch_id: input.id(17)?,
                        commit_id: input.id(33)?,
                    },
                    1 => AddLayerSource::StackSource(input.id(33)?),
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
            put_exchange(out, *value);
        }
        EndpointResponse::TransferDone { exchange, outcome } => {
            out.byte(2);
            put_exchange(out, *exchange);
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

fn put_conflict(out: &mut Encoder, value: &Conflict) {
    out.bytes(value.path.as_bytes());
    put_object_option(out, value.base);
    put_object_option(out, value.current);
    put_object_option(out, value.candidate);
}

fn get_conflict(input: &mut Decoder<'_>) -> Result<Conflict> {
    Ok(Conflict {
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
