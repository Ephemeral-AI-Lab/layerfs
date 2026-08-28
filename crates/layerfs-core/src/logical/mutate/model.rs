#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRoot {
    parent_root: ObjectId,
    root: ObjectId,
    counters: LogicalCounters,
}

impl CandidateRoot {
    pub(super) const fn new(
        parent_root: ObjectId,
        root: ObjectId,
        counters: LogicalCounters,
    ) -> Self {
        Self {
            parent_root,
            root,
            counters,
        }
    }

    pub const fn parent_root(&self) -> ObjectId {
        self.parent_root
    }

    pub const fn root(&self) -> ObjectId {
        self.root
    }

    pub const fn counters(&self) -> LogicalCounters {
        self.counters
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InodeMutation {
    Upsert {
        inode: InodeId,
        record: InodeRecordV1,
    },
    Remove {
        inode: InodeId,
    },
}
