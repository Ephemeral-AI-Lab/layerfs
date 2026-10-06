//! Public memory-profile compatibility over the shared fallible tagged engine.
use super::{
    engine::Engine,
    references::{EditRef, Node, Summary},
    state::{Memory, State},
};
use crate::file::mapping::{ExtentNode, NodeSummary, PageCache};
use crate::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};

/// Existing memory profile's unfinished-record charge ceiling. Backed editing
/// uses explicit bounded jobs rather than a total-state ceiling.
pub const EDIT_DEFERRED_LIMIT: usize = 8 * 1024 * 1024 - 1;
/// Original edit work. Deferred charge preserves decoded-node/entry sizes or canonical page lengths
/// plus fixed overhead, not reference indexes or whole-system residency.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EditCounters {
    /// Stored or owned nodes read.
    pub nodes_read: u64,
    /// Distinct mapping objects accepted by the consumer.
    pub nodes_created: u64,
    /// Payload objects accepted by the consumer.
    pub payloads_created: u64,
    /// Accepted canonical payload bytes.
    pub payload_bytes: u64,
    /// Peak unfinished-record charge; backed records are not heap.
    pub peak_deferred_bytes: usize,
}
// Legacy public NodeSummary has no provenance bit. No canonical-digest
// disjointness is claimed for these aliases. Ordinary editing uses EditRef.
const LEGACY_TAG: [u8; 24] = *b"layerfs-edit-draft-key\0\0";
/// Existing memory object API over one shared tagged editing engine.
pub struct EditObjects<'a> {
    core: Engine<'a>,
    failed: bool,
    pending: Option<ContentError>,
}
impl<'a> EditObjects<'a> {
    /// Empty memory-profile state over the caller's actual providers.
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        consumer: &'a mut dyn FinalizedConsumer,
        pages: &'a mut PageCache,
    ) -> Self {
        Self {
            core: Engine::new(reader, consumer, pages, State::Memory(Memory::default())),
            failed: false,
            pending: None,
        }
    }
    /// Original work so far; does not consume a pending failure.
    pub const fn counters(&self) -> EditCounters {
        self.core.counters
    }
    /// Current unfinished-record charge in this profile.
    pub const fn charged_bytes(&self) -> usize {
        self.core.charged
    }
    fn active(&mut self) -> ContentResult<()> {
        if let Some(error) = self.pending.take() {
            return Err(error);
        }
        if self.failed {
            return Err(ContentError::ProviderFailure {
                what: "terminated memory edit",
            });
        }
        Ok(())
    }
    fn result<T>(&mut self, result: ContentResult<T>) -> ContentResult<T> {
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn alias(id: EditRef) -> ContentResult<ObjectId> {
        match id {
            EditRef::Stored(id) | EditRef::Page(id) => Ok(id),
            EditRef::Node(number) => {
                let mut bytes = [0; 32];
                bytes[..24].copy_from_slice(&LEGACY_TAG);
                bytes[24..].copy_from_slice(
                    &u64::try_from(number)
                        .map_err(|_| ContentError::LengthOverflow)?
                        .to_be_bytes(),
                );
                ObjectId::from_bytes(&bytes)
            }
        }
    }
    fn legacy_ref(&mut self, id: ObjectId) -> ContentResult<EditRef> {
        let bytes = id.to_bytes();
        if bytes[..24] == LEGACY_TAG {
            let number = usize::try_from(u64::from_be_bytes(
                bytes[24..]
                    .try_into()
                    .map_err(|_| ContentError::InvalidRecord("legacy draft alias"))?,
            ))
            .map_err(|_| ContentError::LengthOverflow)?;
            let draft = EditRef::Node(number);
            if self.core.state.contains(super::engine::key(draft, 0)?)?
                || self.core.state.contains(super::engine::key(draft, 6)?)?
            {
                return Ok(draft);
            }
        }
        let page = EditRef::Page(id);
        if self.core.state.contains(super::engine::key(page, 0)?)?
            || self.core.state.contains(super::engine::key(page, 6)?)?
        {
            return Ok(page);
        }
        Ok(EditRef::Stored(id))
    }
    fn summary(&mut self, value: NodeSummary) -> ContentResult<Summary> {
        Ok(Summary {
            id: self.legacy_ref(value.id)?,
            bytes: value.bytes,
            extents: value.extents,
            level: value.level,
        })
    }
    /// Reads a node under the original public compatibility summary.
    pub fn load_node(&mut self, summary: NodeSummary, root: bool) -> ContentResult<ExtentNode> {
        self.active()?;
        let result = (|| {
            let summary = self.summary(summary)?;
            let mut node = self.core.load_node(summary, root)?;
            if let Node::Branch { children, .. } = &mut node {
                for child in children {
                    child.child_object_id = EditRef::Stored(Self::alias(child.child_object_id)?);
                }
            }
            node.canonical()
        })();
        self.result(result)
    }
    /// Holds one unfinished node, translating legacy aliases only at this boundary.
    pub fn hold_node(&mut self, node: &ExtentNode) -> ContentResult<NodeSummary> {
        self.active()?;
        let result = (|| {
            let mut node = Node::from_canonical(node.clone(), false);
            if let Node::Branch { children, .. } = &mut node {
                for child in children {
                    let EditRef::Stored(id) = child.child_object_id else {
                        return Err(ContentError::InvalidRecord("legacy child"));
                    };
                    child.child_object_id = self.legacy_ref(id)?;
                }
            }
            let value = self.core.hold_node(&node)?;
            Ok(NodeSummary {
                id: Self::alias(value.id)?,
                bytes: value.bytes,
                extents: value.extents,
                level: value.level,
            })
        })();
        self.result(result)
    }
    /// Compatibility void method. A first error is retained for the next fallible
    /// call; later work refuses. Ordinary drivers use fallible settle directly.
    pub fn settle(&mut self, live: NodeSummary) {
        if self.failed {
            return;
        }
        let result = self.summary(live).and_then(|live| self.core.settle(live));
        if let Err(error) = result {
            self.failed = true;
            self.pending = Some(error);
        }
    }
    /// Publishes the original payload once.
    pub fn publish_payload(&mut self, object: FinalizedObject) -> ContentResult<ObjectId> {
        self.active()?;
        let result = self.core.publish_payload(object);
        self.result(result)
    }
    /// Accepts reached mappings child before parent, then their file state.
    pub fn finish(&mut self, mapping: NodeSummary) -> ContentResult<ObjectId> {
        self.active()?;
        let result = self
            .summary(mapping)
            .and_then(|mapping| self.core.finish(mapping));
        self.result(result)
    }
    /// Accepts reached mappings and returns their canonical root.
    pub fn commit(&mut self, root: NodeSummary) -> ContentResult<ObjectId> {
        self.active()?;
        let result = self.summary(root).and_then(|root| self.core.commit(root));
        self.result(result)
    }
}
