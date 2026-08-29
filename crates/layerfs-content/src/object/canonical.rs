use crate::error::{CoreError, CoreResult};
use crate::object::{ObjectHashWriter, ObjectId};
use crate::tree::CanonicalName;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum ObjectKind {
    Bytes = 0x01,
    Directory = 0x02,
}

impl TryFrom<u8> for ObjectKind {
    type Error = CoreError;

    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            0x01 => Ok(Self::Bytes),
            0x02 => Ok(Self::Directory),
            tag => Err(CoreError::InvalidObjectKind { tag }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectReference {
    kind: ObjectKind,
    id: ObjectId,
}

impl ObjectReference {
    pub const fn new(kind: ObjectKind, id: ObjectId) -> Self {
        Self { kind, id }
    }

    pub const fn kind(self) -> ObjectKind {
        self.kind
    }

    pub const fn id(self) -> ObjectId {
        self.id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntry {
    name: CanonicalName,
    reference: ObjectReference,
}

impl DirectoryEntry {
    pub const fn new(name: CanonicalName, reference: ObjectReference) -> Self {
        Self { name, reference }
    }

    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    pub const fn reference(&self) -> ObjectReference {
        self.reference
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Object {
    Bytes(Vec<u8>),
    Directory(Vec<DirectoryEntry>),
}

impl Object {
    pub fn bytes(value: Vec<u8>) -> CoreResult<Self> {
        if value.len() > crate::limits::MAX_OBJECT_FIELD_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        Ok(Self::Bytes(value))
    }

    pub fn directory(entries: Vec<DirectoryEntry>) -> CoreResult<Self> {
        if entries.len() > crate::limits::MAX_CHILD_REFERENCES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        Ok(Self::Directory(entries))
    }

    pub fn kind(&self) -> ObjectKind {
        match self {
            Self::Bytes(_) => ObjectKind::Bytes,
            Self::Directory(_) => ObjectKind::Directory,
        }
    }

    pub fn id(&self) -> CoreResult<ObjectId> {
        let mut writer = ObjectHashWriter::new();
        crate::object::encode_object_to(self, &mut writer)?;
        Ok(ObjectId::from_digest(writer.finish()))
    }
}
