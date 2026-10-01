//! Full mutable Node60 codec and closed checked proposal constructors.
use super::{GraphMode, GraphNodeKey, GraphScope};
use crate::error::{ContentError, ContentResult};
/// key25/value29 plus framing6; all mutable fields are included.
pub const GRAPH_NODE_BYTES: usize = 60;
/// One exact directory vertex and its bounded external DFS/SCC state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphNode {
    key: GraphNodeKey,
    flags: u8,
    discovery: u32,
    lowlink: u32,
    parent: u64,
    after: u64,
    incoming: u32,
}
impl GraphNode {
    /// A mode-valid unexpanded birth, with no solver state.
    pub fn birth(scope: &GraphScope, serial: u64, seed: bool) -> ContentResult<Self> {
        if scope.subject().mode() == GraphMode::Fresh && seed {
            return Err(ContentError::InvalidOrderingRecord("fresh graph seed"));
        }
        let flags = if scope.subject().mode() == GraphMode::Fresh {
            4
        } else {
            u8::from(seed)
        };
        Ok(Self {
            key: GraphNodeKey::new(scope, serial)?,
            flags,
            discovery: 0,
            lowlink: 0,
            parent: 0,
            after: 0,
            incoming: 0,
        })
    }
    /// Checks width/prefix/mode/flags/ranks/pointers before exposing full values.
    pub fn decode(scope: &GraphScope, b: &[u8]) -> ContentResult<Self> {
        if b.len() != 60 || b[..2] != 25u16.to_be_bytes() || b[27..31] != 29u32.to_be_bytes() {
            return Err(ContentError::InvalidOrderingRecord("graph node framing"));
        }
        let node = Self {
            key: GraphNodeKey::decode(scope, &b[2..27])?,
            flags: b[31],
            discovery: u32::from_be_bytes(b[32..36].try_into().unwrap()),
            lowlink: u32::from_be_bytes(b[36..40].try_into().unwrap()),
            parent: u64::from_be_bytes(b[40..48].try_into().unwrap()),
            after: u64::from_be_bytes(b[48..56].try_into().unwrap()),
            incoming: u32::from_be_bytes(b[56..60].try_into().unwrap()),
        };
        node.checked(scope)
    }
    /// Native value BLOB29 checked with its actual selected key.
    pub fn decode_value(scope: &GraphScope, key: GraphNodeKey, b: &[u8]) -> ContentResult<Self> {
        if b.len() != 29 {
            return Err(ContentError::InvalidOrderingRecord(
                "graph node value width",
            ));
        }
        let mut frame = [0; 60];
        frame[..2].copy_from_slice(&25u16.to_be_bytes());
        frame[2..27].copy_from_slice(key.as_bytes());
        frame[27..31].copy_from_slice(&29u32.to_be_bytes());
        frame[31..].copy_from_slice(b);
        Self::decode(scope, &frame)
    }
    fn checked(self, scope: &GraphScope) -> ContentResult<Self> {
        GraphNodeKey::decode(scope, self.key.as_bytes())?;
        if self.flags & 128 != 0
            || self.parent > i64::MAX as u64
            || self.after > i64::MAX as u64
            || u64::from(self.discovery) > scope.capacity().records()
        {
            return Err(ContentError::InvalidOrderingRecord("graph node fields"));
        }
        if scope.subject().mode() == GraphMode::Fresh {
            if !self.root_reached()
                || self.flags & (1 | 8 | 16 | 64) != 0
                || self.discovery != 0
                || self.lowlink != 0
                || self.parent != 0
                || self.after != 0
            {
                return Err(ContentError::InvalidOrderingRecord(
                    "fresh graph node fields",
                ));
            }
        } else if self.root_reached() {
            return Err(ContentError::InvalidOrderingRecord("update graph root bit"));
        }
        if self.discovery == 0 {
            if self.lowlink != 0
                || self.parent != 0
                || self.after != 0
                || self.flags & (8 | 16 | 64) != 0
            {
                return Err(ContentError::InvalidOrderingRecord("white graph fields"));
            }
        } else if !self.expanded()
            || self.lowlink == 0
            || self.lowlink > self.discovery
            || self.on_stack() == self.completed()
            || self.completed() && !self.dfs_finished()
        {
            return Err(ContentError::InvalidOrderingRecord("graph solver fields"));
        }
        Ok(self)
    }
    /// Full frame60.
    pub fn encode(self) -> [u8; 60] {
        let mut b = [0; 60];
        b[..2].copy_from_slice(&25u16.to_be_bytes());
        b[2..27].copy_from_slice(self.key.as_bytes());
        b[27..31].copy_from_slice(&29u32.to_be_bytes());
        b[31..].copy_from_slice(&self.value());
        b
    }
    /// Full native BLOB29; no solver field is omitted from accounting.
    pub fn value(self) -> [u8; 29] {
        let mut b = [0; 29];
        b[0] = self.flags;
        b[1..5].copy_from_slice(&self.discovery.to_be_bytes());
        b[5..9].copy_from_slice(&self.lowlink.to_be_bytes());
        b[9..17].copy_from_slice(&self.parent.to_be_bytes());
        b[17..25].copy_from_slice(&self.after.to_be_bytes());
        b[25..].copy_from_slice(&self.incoming.to_be_bytes());
        b
    }
    /// Selected node key.
    pub const fn key(self) -> GraphNodeKey {
        self.key
    }
    /// Complete closed flags.
    pub const fn flags(self) -> u8 {
        self.flags
    }
    /// SCC discovery ordinal, zero before entry.
    pub const fn discovery(self) -> u32 {
        self.discovery
    }
    /// Active lowlink or Completed component-root discovery.
    pub const fn lowlink(self) -> u32 {
        self.lowlink
    }
    /// Retained exact DFS parent; zero for a DFS root.
    pub const fn parent(self) -> u64 {
        self.parent
    }
    /// Last selected outgoing child; zero before its first edge.
    pub const fn after_child(self) -> u64 {
        self.after
    }
    /// Exact incoming multiplicity accumulated during construction.
    pub const fn incoming(self) -> u32 {
        self.incoming
    }
    /// Selected changed-directory child.
    pub const fn seed(self) -> bool {
        self.flags & 1 != 0
    }
    /// Complete acknowledged adjacency for this node.
    pub const fn expanded(self) -> bool {
        self.flags & 2 != 0
    }
    /// Discovered from the fresh root, distinct from SCC completion.
    pub const fn root_reached(self) -> bool {
        self.flags & 4 != 0
    }
    /// Present in the native SCC discovery stack.
    pub const fn on_stack(self) -> bool {
        self.flags & 8 != 0
    }
    /// Assigned to a completed SCC.
    pub const fn completed(self) -> bool {
        self.flags & 16 != 0
    }
    /// Actual self arc exists.
    pub const fn self_loop(self) -> bool {
        self.flags & 32 != 0
    }
    /// DFS outgoing EOF was acknowledged; may still be OnStack.
    pub const fn dfs_finished(self) -> bool {
        self.flags & 64 != 0
    }
    /// Immutable normalized adjacency projection, never a live solver snapshot.
    pub const fn projection(self) -> Self {
        Self {
            flags: self.flags & 39,
            discovery: 0,
            lowlink: 0,
            parent: 0,
            after: 0,
            ..self
        }
    }
    /// Update seed enrollment, before expansion.
    pub fn seeded(self, scope: &GraphScope) -> ContentResult<Self> {
        if scope.subject().mode() != GraphMode::Update || self.expanded() {
            return Err(ContentError::InvalidOrderingRecord("graph late seed"));
        }
        Self {
            flags: self.flags | 1,
            ..self
        }
        .checked(scope)
    }
    /// Checked construction-only incoming increment.
    pub fn add_incoming(self, scope: &GraphScope, count: u32) -> ContentResult<Self> {
        Self {
            incoming: self
                .incoming
                .checked_add(count)
                .ok_or(ContentError::LengthOverflow)?,
            ..self
        }
        .checked(scope)
    }
    /// Complete adjacency EOF, retaining exact immutable loop facts.
    pub fn finish_expansion(self, scope: &GraphScope, self_loop: bool) -> ContentResult<Self> {
        if self.expanded() || self.discovery != 0 {
            return Err(ContentError::InvalidOrderingRecord(
                "graph already expanded",
            ));
        }
        Self {
            flags: self.flags | 2 | if self_loop { 32 } else { 0 },
            ..self
        }
        .checked(scope)
    }
    /// Exact White entry proposal; native assigns/checks the next rank.
    pub fn enter(self, scope: &GraphScope, discovery: u32, parent: u64) -> ContentResult<Self> {
        if self.discovery != 0 || !self.expanded() || discovery == 0 {
            return Err(ContentError::InvalidOrderingRecord("graph enter"));
        }
        Self {
            flags: self.flags | 8,
            discovery,
            lowlink: discovery,
            parent,
            after: 0,
            ..self
        }
        .checked(scope)
    }
    /// Selected FIRST-edge proposal; native independently forbids skipping.
    pub fn advance(self, scope: &GraphScope, child: u64, lowlink: u32) -> ContentResult<Self> {
        if !self.on_stack()
            || self.dfs_finished()
            || child <= self.after
            || child > i64::MAX as u64
            || lowlink > self.lowlink
        {
            return Err(ContentError::InvalidOrderingRecord("graph advance"));
        }
        Self {
            after: child,
            lowlink,
            ..self
        }
        .checked(scope)
    }
    /// Indexed outgoing EOF proposal.
    pub fn finish(self, scope: &GraphScope) -> ContentResult<Self> {
        if !self.on_stack() || self.dfs_finished() {
            return Err(ContentError::InvalidOrderingRecord("graph finish"));
        }
        Self {
            flags: self.flags | 64,
            ..self
        }
        .checked(scope)
    }
    /// Exact finished tree-child return proposal.
    pub fn returned(self, scope: &GraphScope, child: Self) -> ContentResult<Self> {
        if !self.on_stack()
            || self.dfs_finished()
            || !child.dfs_finished()
            || child.parent() != self.key.serial()
            || self.after != child.key.serial()
        {
            return Err(ContentError::InvalidOrderingRecord("graph return"));
        }
        Self {
            lowlink: self.lowlink.min(child.lowlink),
            ..self
        }
        .checked(scope)
    }
    /// One native-ordered SCC member completion proposal.
    pub fn complete(self, scope: &GraphScope, component: u32) -> ContentResult<Self> {
        if !self.on_stack() || !self.dfs_finished() || component == 0 || component > self.discovery
        {
            return Err(ContentError::InvalidOrderingRecord("graph component"));
        }
        Self {
            flags: (self.flags & !8) | 16,
            lowlink: component,
            ..self
        }
        .checked(scope)
    }
}
