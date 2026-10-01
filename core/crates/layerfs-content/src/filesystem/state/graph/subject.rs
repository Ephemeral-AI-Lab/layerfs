//! Static graph subject captured before native ownership issuance.
use super::GraphCapacity;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::identity::InodeScope;
use crate::filesystem::root::FilesystemRootId;
use crate::filesystem::rows::BindingSourceId;
use crate::object::ObjectId;
/// SID8/namespace32/mode1/base-present1/base32/root8/S8/R8/L8.
pub const GRAPH_SUBJECT_BYTES: usize = 106;
/// The actual base relation, not an independently chosen solver mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphMode {
    /// Root-only fresh reachability.
    Fresh = 1,
    /// Selective SCC of an immutable update's seed closure.
    Update = 2,
}
impl GraphMode {
    /// Exact private mode byte.
    pub const fn code(self) -> u8 {
        self as u8
    }
}
/// Immutable namespace/source/budget subject of one selected operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphSubject {
    source: BindingSourceId,
    namespace: InodeScope,
    base: Option<FilesystemRootId>,
    root: u64,
    capacity: GraphCapacity,
}
impl GraphSubject {
    /// Checks the supplied live source and positive root; mode follows Base.
    pub fn new(
        source: BindingSourceId,
        namespace: InodeScope,
        base: Option<FilesystemRootId>,
        root_serial: u64,
        capacity: GraphCapacity,
    ) -> ContentResult<Self> {
        if root_serial == 0 || root_serial > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("graph root serial"));
        }
        Ok(Self {
            source,
            namespace,
            base,
            root: root_serial,
            capacity,
        })
    }
    /// Actual issued source, never decoded from bare issuer integers.
    pub const fn source_id(&self) -> BindingSourceId {
        self.source
    }
    /// Selected namespace allocation scope.
    pub const fn namespace(&self) -> InodeScope {
        self.namespace
    }
    /// Exact immutable base root, or absent for fresh.
    pub const fn base(&self) -> Option<FilesystemRootId> {
        self.base
    }
    /// Exact root directory serial.
    pub const fn root_serial(&self) -> u64 {
        self.root
    }
    /// Captured selected disk budget/derivation.
    pub const fn capacity(&self) -> GraphCapacity {
        self.capacity
    }
    /// Mode follows the actual Base relation.
    pub const fn mode(&self) -> GraphMode {
        if self.base.is_some() {
            GraphMode::Update
        } else {
            GraphMode::Fresh
        }
    }
    /// Exact106-byte private native subject.
    pub fn encode(&self) -> [u8; GRAPH_SUBJECT_BYTES] {
        let mut b = [0; GRAPH_SUBJECT_BYTES];
        b[..8].copy_from_slice(&self.source.as_bytes());
        b[8..40].copy_from_slice(self.namespace.object().as_bytes());
        b[40] = self.mode().code();
        if let Some(root) = self.base {
            b[41] = 1;
            b[42..74].copy_from_slice(root.0.as_bytes());
        }
        b[74..82].copy_from_slice(&self.root.to_be_bytes());
        b[82..90].copy_from_slice(&self.capacity.scratch_bytes().to_be_bytes());
        b[90..98].copy_from_slice(&self.capacity.records().to_be_bytes());
        b[98..106].copy_from_slice(&self.capacity.encoded_bytes().to_be_bytes());
        b
    }
    /// Recomputes caps and verifies actual source/mode/absent placeholders.
    pub fn decode_for(source: BindingSourceId, b: &[u8]) -> ContentResult<Self> {
        if b.len() != GRAPH_SUBJECT_BYTES || b[..8] != source.as_bytes() || b[41] > 1 {
            return Err(ContentError::InvalidOrderingRecord("graph subject framing"));
        }
        let base = if b[41] == 1 {
            Some(FilesystemRootId(ObjectId::from_bytes(&b[42..74])?))
        } else {
            if b[42..74] != [0; 32] {
                return Err(ContentError::InvalidOrderingRecord("graph absent base"));
            }
            None
        };
        let cap = GraphCapacity::new(u64::from_be_bytes(b[82..90].try_into().unwrap()))?;
        if u64::from_be_bytes(b[90..98].try_into().unwrap()) != cap.records()
            || u64::from_be_bytes(b[98..106].try_into().unwrap()) != cap.encoded_bytes()
        {
            return Err(ContentError::InvalidOrderingRecord(
                "graph subject derivation",
            ));
        }
        let subject = Self::new(
            source,
            InodeScope::from_object(ObjectId::from_bytes(&b[8..40])?),
            base,
            u64::from_be_bytes(b[74..82].try_into().unwrap()),
            cap,
        )?;
        if b[40] != subject.mode().code() {
            return Err(ContentError::InvalidOrderingRecord("graph subject mode"));
        }
        Ok(subject)
    }
}
