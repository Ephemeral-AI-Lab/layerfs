use crate::{
    BaseId, BranchId, CommitId, LayerHistoryId, LayerId, ResultId, SourceId, StackHistoryId,
    StackId, StorageId,
};
use layerfs_content::ObjectId;

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

pub type InitializedLayer = (LayerHistoryRecord, LayerRecord);
pub type CreatedStack = (StackHistoryRecord, StackRecord);
pub type PulledBranch = (BranchRecord, crate::RefOutcome<CommitId>);

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
fact_value!(stack, Stack, StackRecord);

pub(crate) fn done<T>(input: Decoder<'_>, value: T) -> crate::Result<T> {
    if input.done() {
        Ok(value)
    } else {
        Err(StorageError::Integrity("wire trailing bytes"))
    }
}
