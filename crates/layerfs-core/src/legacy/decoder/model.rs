const MAGIC: &[u8; 8] = b"LFS4MAP\0";
const MAX_ENTRIES: usize = 100_000;
const FILE_FANOUT: usize = 64;
const DESCRIPTOR_BYTES: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappingVersion {
    V1,
    V2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileReferenceV1 {
    pub raw_id: ObjectId,
    pub raw_length: u32,
    pub object_id: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileReferenceV2 {
    pub raw_length: u32,
    pub object_id: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileChild {
    pub cumulative_end: u64,
    pub object_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRoot {
    pub mode: u32,
    pub total_raw: u64,
    pub reference_count: u64,
    pub level: u8,
    pub children: Vec<FileChild>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryPageRef {
    pub count: u32,
    pub first_name: CanonicalName,
    pub object_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyTransition {
    pub parent: Option<ObjectId>,
    pub child: ObjectId,
    pub entry_count: u32,
    pub pages: Vec<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransitionOperation {
    Add {
        path: CanonicalPath,
        after: ObjectId,
    },
    Remove {
        path: CanonicalPath,
        before: ObjectId,
    },
    Replace {
        path: CanonicalPath,
        before: ObjectId,
        after: ObjectId,
    },
    Metadata {
        path: CanonicalPath,
        before: ObjectId,
        before_mode: u32,
        after: ObjectId,
        after_mode: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyMapping {
    FileRoot(MappingVersion, FileRoot),
    FileLeafV1(Vec<FileReferenceV1>),
    FileLeafV2(Vec<FileReferenceV2>),
    FileBranch(MappingVersion, u8, Vec<FileChild>),
    DirectoryIndex(MappingVersion, u32, Vec<DirectoryPageRef>),
    DirectoryMetadata(MappingVersion, u32),
    DeltaIndex(MappingVersion, LegacyTransition),
    DeltaPage(MappingVersion, Vec<TransitionOperation>),
}
