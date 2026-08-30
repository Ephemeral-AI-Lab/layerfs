use crate::{
    BranchFact, BranchId, CanonicalObject, CommitId, CommitRecord, EntityName, Fact, LayerId,
    LayerRecord, LayerStackFact, LayerStackId, Result, StorageError, StorageId,
    TRANSFER_BUFFER_BYTES,
};
use layerfs_content::ObjectId;
use std::io::{Read, Write};

pub fn write_frame(output: &mut impl Write, bytes: &[u8]) -> Result<()> {
    if bytes.len() >= TRANSFER_BUFFER_BYTES {
        return Err(StorageError::InvalidInput("wire frame"));
    }
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(bytes)?;
    output.flush()?;
    Ok(())
}

pub fn read_frame(input: &mut impl Read) -> Result<Vec<u8>> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length >= TRANSFER_BUFFER_BYTES {
        return Err(StorageError::Integrity("wire frame"));
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub fn encode_fact(fact: &Fact) -> Vec<u8> {
    fact.signing_bytes()
}

pub fn decode_fact(bytes: &[u8]) -> Result<Fact> {
    let mut input = Decoder::new(bytes);
    let fact = match input.byte()? {
        0 => Fact::Commit(CommitRecord {
            id: input.id(33)?,
            root_id: input.object()?,
            parent_commit_id: input.optional_id(33)?,
            base_layer_id: input.id(33)?,
        }),
        1 => Fact::Branch(BranchFact {
            id: input.id(17)?,
            layer_stack_id: input.id(17)?,
            name: input.name()?,
            forked_from_layer_id: input.optional_id(33)?,
            forked_from_branch_id: input.optional_id(17)?,
            forked_from_commit_id: input.optional_id(33)?,
        }),
        2 => Fact::LayerStack(LayerStackFact {
            id: input.id(17)?,
            name: input.name()?,
        }),
        3 => Fact::Layer(LayerRecord {
            id: input.id(33)?,
            layer_stack_id: input.id(17)?,
            parent_layer_id: input.optional_id(33)?,
            root_id: input.object()?,
            source_branch_id: input.optional_id(17)?,
            source_commit_id: input.optional_id(33)?,
        }),
        _ => return Err(StorageError::Integrity("wire fact")),
    };
    if !input.done() {
        return Err(StorageError::Integrity("wire trailing bytes"));
    }
    Ok(fact)
}

pub fn encode_objects(objects: &[CanonicalObject]) -> Result<Vec<u8>> {
    let mut out = Encoder::new();
    out.u32(objects.len())?;
    for object in objects {
        out.raw(object.id.as_bytes());
        out.bytes(&object.bytes)?;
    }
    Ok(out.finish())
}

pub fn decode_objects(bytes: &[u8]) -> Result<Vec<CanonicalObject>> {
    let mut input = Decoder::new(bytes);
    let count = input.u32()?;
    if count > crate::OBJECT_BATCH_COUNT {
        return Err(StorageError::Integrity("wire object count"));
    }
    let mut objects = Vec::with_capacity(count);
    for _ in 0..count {
        let id = input.object()?;
        let bytes = input.bytes()?.to_vec();
        layerfs_content::authenticate_identity(&bytes, id)?;
        objects.push(CanonicalObject { id, bytes });
    }
    if !input.done() {
        return Err(StorageError::Integrity("wire trailing bytes"));
    }
    Ok(objects)
}

pub fn encode_diff_entry(entry: &crate::DiffEntry) -> Result<Vec<u8>> {
    let mut out = Encoder::new();
    match entry {
        crate::DiffEntry::Add { path, after } => {
            out.raw(&[0]);
            out.bytes(path.as_bytes())?;
            put_summary(&mut out, *after);
        }
        crate::DiffEntry::Remove { path, before } => {
            out.raw(&[1]);
            out.bytes(path.as_bytes())?;
            put_summary(&mut out, *before);
        }
        crate::DiffEntry::Modify {
            path,
            before,
            after,
            aspects,
        } => {
            out.raw(&[2]);
            out.bytes(path.as_bytes())?;
            put_summary(&mut out, *before);
            put_summary(&mut out, *after);
            out.raw(&[u8::from(aspects.node_type)
                | u8::from(aspects.content) << 1
                | u8::from(aspects.metadata) << 2
                | u8::from(aspects.directory_membership) << 3
                | u8::from(aspects.hard_links) << 4]);
        }
    }
    Ok(out.finish())
}

pub fn decode_diff_entry(bytes: &[u8]) -> Result<crate::DiffEntry> {
    let mut input = Decoder::new(bytes);
    let tag = input.byte()?;
    let path = layerfs_content::CanonicalPath::from_bytes(input.bytes()?)?;
    let entry = match tag {
        0 => crate::DiffEntry::Add {
            path,
            after: get_summary(&mut input)?,
        },
        1 => crate::DiffEntry::Remove {
            path,
            before: get_summary(&mut input)?,
        },
        2 => {
            let before = get_summary(&mut input)?;
            let after = get_summary(&mut input)?;
            let aspects = input.byte()?;
            crate::DiffEntry::Modify {
                path,
                before,
                after,
                aspects: crate::DiffAspects {
                    node_type: aspects & 1 != 0,
                    content: aspects & 2 != 0,
                    metadata: aspects & 4 != 0,
                    directory_membership: aspects & 8 != 0,
                    hard_links: aspects & 16 != 0,
                },
            }
        }
        _ => return Err(StorageError::Integrity("Diff entry tag")),
    };
    if !input.done() {
        return Err(StorageError::Integrity("wire trailing bytes"));
    }
    Ok(entry)
}

fn put_summary(out: &mut Encoder, summary: crate::NodeSummary) {
    out.raw(&[summary.kind as u8]);
    out.raw(summary.content_root.as_bytes());
    out.raw(summary.metadata_root.as_bytes());
    out.raw(&summary.namespace_ref_count.to_be_bytes());
}

fn get_summary(input: &mut Decoder<'_>) -> Result<crate::NodeSummary> {
    let kind = match input.byte()? {
        1 => layerfs_content::tree::inode::InodeKind::RegularFile,
        2 => layerfs_content::tree::inode::InodeKind::Directory,
        3 => layerfs_content::tree::inode::InodeKind::Symlink,
        _ => return Err(StorageError::Integrity("Diff node kind")),
    };
    let content_root = input.object()?;
    let metadata_root = input.object()?;
    let namespace_ref_count = u64::from_be_bytes(input.take(8)?.try_into().unwrap());
    Ok(crate::NodeSummary {
        kind,
        content_root,
        metadata_root,
        namespace_ref_count,
    })
}

struct Encoder(Vec<u8>);

impl Encoder {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn u32(&mut self, value: usize) -> Result<()> {
        let value: u32 = value
            .try_into()
            .map_err(|_| StorageError::InvalidInput("wire length"))?;
        self.0.extend_from_slice(&value.to_be_bytes());
        Ok(())
    }

    fn raw(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }

    fn bytes(&mut self, value: &[u8]) -> Result<()> {
        self.u32(value.len())?;
        self.raw(value);
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

struct Decoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<usize> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }

    fn bytes(&mut self) -> Result<&'a [u8]> {
        let length = self.u32()?;
        self.take(length)
    }

    fn object(&mut self) -> Result<ObjectId> {
        Ok(ObjectId::from_bytes(self.take(32)?)?)
    }

    fn id<T: StorageId>(&mut self, length: usize) -> Result<T> {
        T::from_slice(self.take(length)?)
    }

    fn optional_id<T: StorageId>(&mut self, length: usize) -> Result<Option<T>> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.id(length)?)),
            _ => Err(StorageError::Integrity("wire option")),
        }
    }

    fn name(&mut self) -> Result<EntityName> {
        let length = usize::from(self.byte()?);
        let value = std::str::from_utf8(self.take(length)?)
            .map_err(|_| StorageError::Integrity("wire entity name"))?;
        EntityName::new(value).map_err(|_| StorageError::Integrity("wire entity name"))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(StorageError::Integrity("wire length"))?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or(StorageError::Integrity("wire truncated"))?;
        self.cursor = end;
        Ok(value)
    }

    fn done(&self) -> bool {
        self.cursor == self.bytes.len()
    }
}

#[allow(dead_code)]
fn identity_type_checks() {
    let _: Option<BranchId> = None;
    let _: Option<CommitId> = None;
    let _: Option<LayerId> = None;
    let _: Option<LayerStackId> = None;
}
