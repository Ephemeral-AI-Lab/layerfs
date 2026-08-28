#[derive(Clone)]
struct DirtyRange {
    end: u64,
    spool_offset: u64,
}

struct SpoolRangeLocation {
    node: MountedNodeId,
    start: u64,
    old_offset: u64,
    len: u64,
}

#[derive(Clone)]
enum NodeContent {
    File {
        base: Option<FileStateRoot>,
        base_visible_len: u64,
        logical_len: u64,
        ranges: BTreeMap<u64, DirtyRange>,
        plan: Option<Arc<ReadPlan>>,
    },
    Directory {
        base: Option<DirectoryStateRoot>,
        changes: BTreeMap<CanonicalName, Option<MountedNodeId>>,
    },
    Symlink {
        target: Vec<u8>,
    },
}

struct MountedNode {
    canonical: Option<InodeId>,
    record: Option<InodeRecordV1>,
    kind: MountedFileType,
    mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
    namespace_refs: u64,
    parent: MountedNodeId,
    lookup_refs: u64,
    open_refs: u64,
    deleted: bool,
    dirty_content: bool,
    dirty_metadata: bool,
    dirty_links: bool,
    directory_mtime_before: Option<(i64, u32, bool)>,
    content: NodeContent,
}

impl MountedNode {
    fn pending(&self) -> bool {
        self.canonical.is_none() && !self.deleted
    }

    fn dirty(&self) -> bool {
        self.pending() || self.dirty_content || self.dirty_metadata || self.dirty_links
    }

    fn attr(&self, id: MountedNodeId) -> MountedAttr {
        let size = match &self.content {
            NodeContent::File { logical_len, .. } => *logical_len,
            NodeContent::Directory { .. } => 0,
            NodeContent::Symlink { target } => target.len() as u64,
        };
        MountedAttr {
            node: id,
            kind: self.kind,
            size,
            mode: self.mode,
            mtime_seconds: self.mtime_seconds,
            mtime_nanoseconds: self.mtime_nanoseconds,
            links: u32::try_from(self.namespace_refs).unwrap_or(u32::MAX),
        }
    }
}

#[derive(Clone)]
struct DirectoryCursor {
    node: MountedNodeId,
    scan_after: Option<CanonicalName>,
    base_after: Option<CanonicalName>,
    base: VecDeque<(CanonicalName, InodeId)>,
    base_done: bool,
    cookie: u64,
}

struct DirectoryMutation {
    parent: MountedNodeId,
    name: CanonicalName,
    normalized: Option<Option<MountedNodeId>>,
    change_delta: i8,
    timestamp: (i64, u32),
}

impl DirectoryCursor {
    fn new(node: MountedNodeId) -> Self {
        Self {
            node,
            scan_after: None,
            base_after: None,
            base: VecDeque::new(),
            base_done: false,
            cookie: 0,
        }
    }
}

enum Handle {
    File(MountedNodeId),
    Directory(Box<DirectoryHandle>),
}

struct DirectoryHandle {
    committed: DirectoryCursor,
    pending: Option<(MountedDirEntry, DirectoryCursor)>,
}
