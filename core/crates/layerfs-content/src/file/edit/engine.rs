//! Shared tagged split/join state, exact reference transitions and publication.
use super::{
    backing::EditRecordKey,
    objects::{EditCounters, EDIT_DEFERRED_LIMIT},
    references::{EditRef, Node, Summary},
    state::{change, Draft, DraftChange, State},
};
use crate::file::mapping::{decode_node_with_context, encode_node, NodeSummary, PageCache};
use crate::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use std::collections::BTreeMap;

pub(super) struct Engine<'a> {
    pub reader: &'a dyn AuthenticatedObjects,
    pub consumer: &'a mut dyn FinalizedConsumer,
    pub pages: &'a mut PageCache,
    pub state: State<'a>,
    pub charged: usize,
    pub counters: EditCounters,
    next: usize,
    pub committing: bool,
    pub file_state_attempted: bool,
    started: bool,
}
pub(super) fn key(id: EditRef, offset: u32) -> ContentResult<EditRecordKey> {
    Ok(EditRecordKey {
        kind: id
            .draft_kind()
            .ok_or(ContentError::InvalidRecord("stored scratch reference"))?
            + offset,
        key: id.key(),
    })
}
fn count(value: Option<&[u8]>) -> ContentResult<usize> {
    match value {
        None => Ok(0),
        Some(value) if value.len() == 8 => {
            let number = u64::from_be_bytes(
                value
                    .try_into()
                    .map_err(|_| ContentError::InvalidRecord("parent count"))?,
            );
            if number == 0 {
                return Err(ContentError::InvalidRecord("zero parent count"));
            }
            usize::try_from(number).map_err(|_| ContentError::LengthOverflow)
        }
        _ => Err(ContentError::InvalidRecord("parent count")),
    }
}
impl<'a> Engine<'a> {
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        consumer: &'a mut dyn FinalizedConsumer,
        pages: &'a mut PageCache,
        state: State<'a>,
    ) -> Self {
        Self {
            reader,
            consumer,
            pages,
            state,
            charged: 0,
            counters: EditCounters::default(),
            next: 0,
            committing: false,
            file_state_attempted: false,
            started: false,
        }
    }
    pub fn load_node(&mut self, summary: Summary, root: bool) -> ContentResult<Node> {
        let node = match summary.id {
            EditRef::Stored(id) => {
                if let Some(canonical) = self.pages.get(id, root) {
                    Node::from_canonical(decode_node_with_context(canonical, root)?, false)
                } else {
                    self.counters.nodes_read = self.counters.nodes_read.saturating_add(1);
                    let canonical = self.reader.read_canonical(id)?;
                    let node =
                        Node::from_canonical(decode_node_with_context(&canonical, root)?, false);
                    Self::check_summary(&node, summary)?;
                    self.pages.make_room_for(1);
                    self.pages.insert(id, root, canonical);
                    node
                }
            }
            id => {
                self.counters.nodes_read = self.counters.nodes_read.saturating_add(1);
                self.state.node(id, root)?
            }
        };
        Self::check_summary(&node, summary)?;
        Ok(node)
    }
    fn check_summary(node: &Node, summary: Summary) -> ContentResult<()> {
        if node.level() != summary.level
            || node.logical_len() != summary.bytes
            || node.extent_count() != summary.extents
        {
            return Err(ContentError::InvalidRecord("extent summary"));
        }
        Ok(())
    }
    fn draft_node(draft: &Draft, root: bool) -> ContentResult<Node> {
        match draft {
            Draft::Node(node) => Ok(node.clone()),
            Draft::Page(object) => Ok(Node::from_canonical(
                decode_node_with_context(object.canonical(), root)?,
                true,
            )),
        }
    }
    fn begin(&mut self) -> ContentResult<()> {
        if !self.started {
            self.state.apply(vec![change(
                EditRecordKey {
                    kind: 10,
                    key: [0; 32],
                },
                None,
                Some(vec![0]),
            )])?;
            self.started = true;
        }
        Ok(())
    }
    fn plan_children(
        &mut self,
        node: &Node,
        retain: bool,
        changes: &mut Vec<super::backing::EditRecordChange>,
    ) -> ContentResult<()> {
        let mut multiplicity = BTreeMap::<EditRef, usize>::new();
        if let Node::Branch { children, .. } = node {
            for child in children {
                if child.child_object_id.draft_kind().is_some() {
                    let number = multiplicity.entry(child.child_object_id).or_default();
                    *number = number.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                }
            }
        }
        for (child, amount) in multiplicity {
            let refs = key(child, 2)?;
            let old = self.state.get(refs)?;
            let before = count(old.as_deref())?;
            let after = if retain {
                before.checked_add(amount)
            } else {
                before.checked_sub(amount)
            }
            .ok_or(ContentError::InvalidRecord("parent reference transition"))?;
            let detached = key(child, 4)?;
            let old_detached = self.state.get(detached)?;
            if old_detached.as_ref().is_some_and(|value| !value.is_empty())
                || (before != 0 && old_detached.is_some())
            {
                return Err(ContentError::InvalidRecord("detached reference state"));
            }
            let draft = self.state.contains(key(child, 0)?)?;
            if !draft && !self.state.contains(key(child, 6)?)? {
                return Err(ContentError::InvalidRecord("missing draft child"));
            }
            if before == 0 && draft && old_detached.is_none() {
                return Err(ContentError::InvalidRecord("missing detached child"));
            }
            let new_detached = (after == 0 && draft).then(Vec::new);
            changes.push(change(
                refs,
                old,
                if after == 0 {
                    None
                } else {
                    Some(
                        u64::try_from(after)
                            .map_err(|_| ContentError::LengthOverflow)?
                            .to_be_bytes()
                            .to_vec(),
                    )
                },
            ));
            if old_detached.is_some() || new_detached.is_some() {
                changes.push(change(detached, old_detached, new_detached));
            }
        }
        Ok(())
    }
    fn add_charge(&self, draft: &Draft) -> ContentResult<usize> {
        let next = self
            .charged
            .checked_add(draft.charge())
            .ok_or(ContentError::LengthOverflow)?;
        if matches!(self.state, State::Memory(_)) && next > EDIT_DEFERRED_LIMIT {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "edit.deferred_nodes",
                limit: EDIT_DEFERRED_LIMIT as u64,
                actual: next as u64,
            });
        }
        Ok(next)
    }
    pub fn hold_node(&mut self, node: &Node) -> ContentResult<Summary> {
        if self.committing {
            return Err(ContentError::InvalidRecord("edit already committing"));
        }
        self.begin()?;
        let sequence = self.next;
        u64::try_from(sequence).map_err(|_| ContentError::LengthOverflow)?;
        self.next = sequence
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        let id = EditRef::Node(sequence);
        let draft = Draft::Node(node.clone());
        let charged = self.add_charge(&draft)?;
        let mut changes = Vec::with_capacity(2 * 128 + 5);
        changes.push(change(key(id, 4)?, None, Some(Vec::new())));
        self.plan_children(node, true, &mut changes)?;
        self.state.transition(
            Some(DraftChange::Put {
                key: key(id, 0)?,
                draft,
            }),
            changes,
        )?;
        self.charged = charged;
        self.counters.peak_deferred_bytes = self.counters.peak_deferred_bytes.max(charged);
        Ok(Summary {
            id,
            bytes: node.logical_len(),
            extents: node.extent_count(),
            level: node.level(),
        })
    }
    pub fn hold_page(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if self.committing {
            return Err(ContentError::InvalidRecord("edit already committing"));
        }
        self.begin()?;
        let id = EditRef::Page(object.id());
        if self.state.contains(key(id, 0)?)? {
            return Ok(());
        }
        let draft = Draft::Page(object);
        let node = if matches!(&draft, Draft::Page(object) if object.role() == ObjectRole::ExtentBranch)
        {
            Some(Self::draft_node(&draft, true)?)
        } else {
            None
        };
        let charged = self.add_charge(&draft)?;
        let mut changes = Vec::with_capacity(2 * 128 + 5);
        changes.push(change(key(id, 4)?, None, Some(Vec::new())));
        if let Some(node) = node {
            self.plan_children(&node, true, &mut changes)?;
        }
        self.state.transition(
            Some(DraftChange::Put {
                key: key(id, 0)?,
                draft,
            }),
            changes,
        )?;
        self.charged = charged;
        self.counters.peak_deferred_bytes = self.counters.peak_deferred_bytes.max(charged);
        Ok(())
    }
    pub fn release(&mut self, id: EditRef) -> ContentResult<()> {
        if id.draft_kind().is_none() {
            return Ok(());
        }
        if self.committing {
            return Err(ContentError::InvalidRecord("release after commit"));
        }
        let Some(observed) = self.state.draft(id)? else {
            return if self.state.contains(key(id, 6)?)? {
                Ok(())
            } else {
                Err(ContentError::InvalidRecord("missing released draft"))
            };
        };
        if count(self.state.get(key(id, 2)?)?.as_deref())? != 0 {
            return Ok(());
        }
        let node = Self::draft_node(&observed.draft, true)?;
        let detached = self.state.get(key(id, 4)?)?;
        if detached.as_deref() != Some(&[]) {
            return Err(ContentError::InvalidRecord("released detached marker"));
        }
        let charge = observed.draft.charge();
        let mut changes = Vec::with_capacity(2 * 128 + 5);
        changes.push(change(key(id, 4)?, detached, None));
        self.plan_children(&node, false, &mut changes)?;
        self.state.transition(
            Some(DraftChange::Delete {
                key: key(id, 0)?,
                raw: observed.raw,
            }),
            changes,
        )?;
        self.charged = self
            .charged
            .checked_sub(charge)
            .ok_or(ContentError::InvalidRecord("deferred charge"))?;
        Ok(())
    }
    pub fn settle(&mut self, live: Summary) -> ContentResult<()> {
        loop {
            let mut progressed = false;
            for domain in [1, 2] {
                let excluded = (live.id.draft_kind() == Some(domain)).then(|| live.id.key());
                let pending = self.state.first(domain + 4, excluded)?;
                for candidate in pending {
                    let id = EditRef::from_key(domain, candidate)?;
                    if count(self.state.get(key(id, 2)?)?.as_deref())? != 0 {
                        return Err(ContentError::InvalidRecord("referenced detached candidate"));
                    }
                    progressed = true;
                    self.release(id)?;
                }
            }
            if !progressed {
                return Ok(());
            }
        }
    }
    pub fn publish_payload(&mut self, object: FinalizedObject) -> ContentResult<ObjectId> {
        self.begin()?;
        let id = object.id();
        let length = object.canonical_len();
        self.consumer.accept(object)?;
        self.counters.payloads_created = self.counters.payloads_created.saturating_add(1);
        self.counters.payload_bytes = self.counters.payload_bytes.saturating_add(length as u64);
        Ok(id)
    }
    pub fn commit(&mut self, summary: Summary) -> ContentResult<ObjectId> {
        self.committing = true;
        self.commit_node(summary, true)
    }
    fn commit_node(&mut self, summary: Summary, root: bool) -> ContentResult<ObjectId> {
        if let EditRef::Stored(id) = summary.id {
            return Ok(id);
        }
        if let Some(resolved) = self.state.get(key(summary.id, 6)?)? {
            let id = super::resolution::check(&resolved, summary, root)?;
            if self
                .state
                .get(EditRecordKey {
                    kind: 9,
                    key: id.to_bytes(),
                })?
                .as_deref()
                != Some(&[1])
            {
                return Err(ContentError::InvalidRecord("unacknowledged resolution"));
            }
            if matches!(summary.id, EditRef::Page(expected) if expected != id) {
                return Err(ContentError::IdentityMismatch);
            }
            return Ok(id);
        }
        let observed = self
            .state
            .draft(summary.id)?
            .ok_or(ContentError::InvalidRecord("missing committed draft"))?;
        let mut node = Self::draft_node(&observed.draft, root)?;
        if node.level() != summary.level
            || node.logical_len() != summary.bytes
            || node.extent_count() != summary.extents
        {
            return Err(ContentError::InvalidRecord("commit summary"));
        }
        let original = node.clone();
        if let Node::Branch {
            level, children, ..
        } = &mut node
        {
            let mut before_bytes = 0;
            let mut before_extents = 0;
            for child in children {
                let id = self.commit_node(
                    Summary {
                        id: child.child_object_id,
                        bytes: child
                            .cumulative_logical_end
                            .checked_sub(before_bytes)
                            .ok_or(ContentError::NonCanonicalOrdering)?,
                        extents: child
                            .cumulative_extent_end
                            .checked_sub(before_extents)
                            .ok_or(ContentError::NonCanonicalOrdering)?,
                        level: level.checked_sub(1).ok_or(ContentError::WrongLogicalRole)?,
                    },
                    false,
                )?;
                if matches!(child.child_object_id, EditRef::Page(expected) if expected != id) {
                    return Err(ContentError::IdentityMismatch);
                }
                before_bytes = child.cumulative_logical_end;
                before_extents = child.cumulative_extent_end;
                child.child_object_id = EditRef::Stored(id);
            }
        }
        let charge = observed.draft.charge();
        let object = match observed.draft {
            Draft::Page(object) => object,
            Draft::Node(_) => {
                let canonical_node = node.canonical()?;
                let role = if canonical_node.level() == 0 {
                    ObjectRole::ExtentLeaf
                } else {
                    ObjectRole::ExtentBranch
                };
                FinalizedObject::new(role, encode_node(&canonical_node, root)?)?
                    .with_references(canonical_node.references())
            }
        };
        let id = object.id();
        let emitted = EditRecordKey {
            kind: 9,
            key: id.to_bytes(),
        };
        let prior = self.state.get(emitted)?;
        let accepted = match prior.as_deref() {
            None => false,
            Some([1]) => true,
            _ => return Err(ContentError::InvalidRecord("unacknowledged emission")),
        };
        if !accepted {
            self.state
                .apply(vec![change(emitted, None, Some(vec![0]))])?;
            self.consumer.accept(object)?;
            self.counters.nodes_created = self.counters.nodes_created.saturating_add(1);
        }
        let detached = self.state.get(key(summary.id, 4)?)?;
        if detached.as_ref().is_some_and(|v| !v.is_empty()) {
            return Err(ContentError::InvalidRecord("commit detached marker"));
        }
        let mut changes = Vec::with_capacity(2 * 128 + 5);
        changes.push(change(
            key(summary.id, 6)?,
            None,
            Some(super::resolution::encode(
                id,
                summary,
                original.entry_count() >= crate::file::mapping::MIN_ENTRIES,
            )),
        ));
        if detached.is_some() {
            changes.push(change(key(summary.id, 4)?, detached, None));
        }
        if !accepted {
            changes.push(change(emitted, Some(vec![0]), Some(vec![1])));
        }
        self.plan_children(&original, false, &mut changes)?;
        self.state.transition(
            Some(DraftChange::Delete {
                key: key(summary.id, 0)?,
                raw: observed.raw,
            }),
            changes,
        )?;
        self.charged = self
            .charged
            .checked_sub(charge)
            .ok_or(ContentError::InvalidRecord("deferred charge"))?;
        Ok(id)
    }
    pub fn finish(&mut self, mapping: Summary) -> ContentResult<ObjectId> {
        if self.file_state_attempted {
            return Err(ContentError::InvalidRecord("file state already attempted"));
        }
        self.begin()?;
        let id = self.commit(mapping)?;
        let scope = EditRecordKey {
            kind: 10,
            key: [0; 32],
        };
        self.state
            .apply(vec![change(scope, Some(vec![0]), Some(vec![2]))])?;
        self.file_state_attempted = true;
        let root = crate::file::mapping::emit_file_state(
            self.consumer,
            NodeSummary {
                id,
                bytes: mapping.bytes,
                extents: mapping.extents,
                level: mapping.level,
            },
        )?;
        self.state
            .apply(vec![change(scope, Some(vec![2]), Some(vec![1]))])?;
        Ok(root)
    }
}
pub(super) struct DeferredSink<'a, 'b> {
    objects: &'b mut Engine<'a>,
}
impl<'a, 'b> DeferredSink<'a, 'b> {
    pub fn new(objects: &'b mut Engine<'a>) -> Self {
        Self { objects }
    }
}
impl FinalizedConsumer for DeferredSink<'_, '_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        match object.role() {
            ObjectRole::Chunk => self.objects.publish_payload(object).map(|_| ()),
            ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch => self.objects.hold_page(object),
            _ => Err(ContentError::WrongLogicalRole),
        }
    }
}
