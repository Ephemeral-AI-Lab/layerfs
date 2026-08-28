const STREAM_FLUSH_AT: usize = MAX_ENTRIES + 64;

pub use crate::object::access::{ObjectRead, ObjectStore};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RopeCounters {
    pub payload_bytes_read: u64,
    pub payload_bytes_written: u64,
    pub cdc_bytes_scanned: u64,
    pub chunks_created: u64,
    pub nodes_read: u64,
    pub nodes_created: u64,
    pub tree_level_before: Option<u8>,
    pub logical_len_before: Option<u64>,
    pub logical_len_after: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileStateRoot(pub ObjectId);

#[derive(Clone, Debug)]
pub struct ReadPlan {
    state: FileStateV3,
    mapping: ExtentNodeV3,
}

impl ReadPlan {
    pub fn logical_len(&self) -> u64 {
        self.state.logical_len
    }
}

#[derive(Clone, Copy)]
struct Summary {
    id: ObjectId,
    bytes: u64,
    extents: u64,
    level: u8,
}

enum Pending {
    Extents(Vec<ExtentSliceV3>),
    Children(Vec<Summary>),
}

struct ReplacementScan {
    levels: Vec<Pending>,
    counters: RopeCounters,
    bytes_scanned: u64,
    pending: BTreeMap<ObjectId, Vec<u8>>,
    persisted_nodes: u64,
}
