//! One fallible state protocol; memory retains decoded drafts, backing raw records.
use super::{
    backing::{
        EditRecordApply, EditRecordChange, EditRecordExpected, EditRecordKey, IndexedEditBacking,
    },
    draft_codec,
    references::{EditRef, Node},
};
use crate::file::mapping::{ChildDescriptor, ExtentNode, ExtentSlice};
use crate::{ContentError, ContentResult, FinalizedObject};
use std::collections::BTreeMap;
pub(super) const WINDOW: usize = 65_536;

#[derive(Clone)]
pub(super) enum Draft {
    Node(Node),
    Page(FinalizedObject),
}
impl Draft {
    pub fn charge(&self) -> usize {
        match self {
            Self::Page(object) => object.canonical_len().saturating_add(128),
            Self::Node(node) => std::mem::size_of::<ExtentNode>()
                .saturating_add(match node {
                    Node::Leaf { extents, .. } => extents
                        .len()
                        .saturating_mul(std::mem::size_of::<ExtentSlice>()),
                    Node::Branch { children, .. } => children
                        .len()
                        .saturating_mul(std::mem::size_of::<ChildDescriptor>()),
                })
                .saturating_add(128),
        }
    }
    fn encode(self) -> ContentResult<Vec<u8>> {
        match self {
            Self::Node(node) => draft_codec::encode(&node),
            Self::Page(object) => draft_codec::page(object),
        }
    }
}
#[derive(Default)]
pub(super) struct Memory {
    records: BTreeMap<EditRecordKey, Vec<u8>>,
    drafts: BTreeMap<EditRecordKey, Draft>,
}
pub(super) struct Observed {
    pub draft: Draft,
    pub raw: Option<Vec<u8>>,
}
pub(super) enum DraftChange {
    Put {
        key: EditRecordKey,
        draft: Draft,
    },
    Delete {
        key: EditRecordKey,
        raw: Option<Vec<u8>>,
    },
}
pub(super) enum State<'a> {
    Memory(Memory),
    Backed(&'a mut dyn IndexedEditBacking),
}
impl State<'_> {
    pub fn node(&mut self, id: EditRef, root: bool) -> ContentResult<Node> {
        let convert = |draft: &Draft| match draft {
            Draft::Node(node) => Ok(node.clone()),
            Draft::Page(object) => Ok(Node::from_canonical(
                crate::file::mapping::decode_node_with_context(object.canonical(), root)?,
                true,
            )),
        };
        match self {
            Self::Memory(memory) => convert(
                memory
                    .drafts
                    .get(&super::engine::key(id, 0)?)
                    .ok_or(ContentError::InvalidRecord("missing edit draft"))?,
            ),
            Self::Backed(_) => {
                let observed = self
                    .draft(id)?
                    .ok_or(ContentError::InvalidRecord("missing edit draft"))?;
                convert(&observed.draft)
            }
        }
    }
    pub fn draft(&mut self, id: EditRef) -> ContentResult<Option<Observed>> {
        let key = super::engine::key(id, 0)?;
        match self {
            Self::Memory(memory) => Ok(memory
                .drafts
                .get(&key)
                .cloned()
                .map(|draft| Observed { draft, raw: None })),
            Self::Backed(port) => {
                let Some(raw) = port.get(key)? else {
                    return Ok(None);
                };
                if raw.capacity() > WINDOW {
                    return Err(ContentError::InvalidRecord("edit backing value window"));
                }
                let draft = match id {
                    EditRef::Node(_) => Draft::Node(draft_codec::decode(&raw)?),
                    EditRef::Page(id) => Draft::Page(draft_codec::decode_page(&raw, id)?),
                    EditRef::Stored(_) => return Err(ContentError::InvalidRecord("stored draft")),
                };
                Ok(Some(Observed {
                    draft,
                    raw: Some(raw),
                }))
            }
        }
    }
    pub fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        let value = match self {
            Self::Memory(memory) => memory.records.get(&key).cloned(),
            Self::Backed(port) => port.get(key)?,
        };
        if value
            .as_ref()
            .is_some_and(|value| value.capacity() > WINDOW)
        {
            return Err(ContentError::InvalidRecord("edit backing value window"));
        }
        Ok(value)
    }
    pub fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        match self {
            Self::Memory(memory) => {
                Ok(memory.drafts.contains_key(&key) || memory.records.contains_key(&key))
            }
            Self::Backed(port) => port.contains(key),
        }
    }
    pub fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<()> {
        self.transition(None, changes)
    }
    pub fn transition(
        &mut self,
        draft: Option<DraftChange>,
        mut changes: Vec<EditRecordChange>,
    ) -> ContentResult<()> {
        if matches!(self, Self::Backed(_)) {
            if let Some(draft) = draft {
                changes.push(match draft {
                    DraftChange::Put { key, draft } => change(key, None, Some(draft.encode()?)),
                    DraftChange::Delete { key, raw } => change(
                        key,
                        Some(raw.ok_or(ContentError::InvalidRecord("missing raw draft guard"))?),
                        None,
                    ),
                });
            }
            return Self::backed_changes(self, changes);
        }
        changes.sort_unstable_by_key(|change| change.key);
        let Self::Memory(memory) = self else {
            return Err(ContentError::InvalidRecord("memory state"));
        };
        for pair in changes.windows(2) {
            if pair[0].key >= pair[1].key {
                return Err(ContentError::InvalidRecord("edit backing duplicate target"));
            }
        }
        if let Some(draft) = &draft {
            let valid = match draft {
                DraftChange::Put { key, .. } => !memory.drafts.contains_key(key),
                DraftChange::Delete { key, .. } => memory.drafts.contains_key(key),
            };
            if !valid {
                return Err(ContentError::InvalidRecord("memory draft precondition"));
            }
        }
        for change in &changes {
            let valid = match (&change.expected, memory.records.get(&change.key)) {
                (EditRecordExpected::Missing, None) => true,
                (EditRecordExpected::ExactBytes(a), Some(b)) => a == b,
                _ => false,
            };
            if !valid {
                return Err(ContentError::InvalidRecord("edit backing precondition"));
            }
        }
        for change in changes {
            if let Some(value) = change.value {
                memory.records.insert(change.key, value);
            } else {
                memory.records.remove(&change.key);
            }
        }
        if let Some(draft) = draft {
            match draft {
                DraftChange::Put { key, draft } => {
                    memory.drafts.insert(key, draft);
                }
                DraftChange::Delete { key, .. } => {
                    memory.drafts.remove(&key);
                }
            }
        }
        Ok(())
    }
    fn backed_changes(&mut self, mut changes: Vec<EditRecordChange>) -> ContentResult<()> {
        changes.sort_unstable_by_key(|change| change.key);
        let mut bytes = changes
            .capacity()
            .checked_mul(std::mem::size_of::<EditRecordChange>())
            .ok_or(ContentError::LengthOverflow)?;
        let mut previous = None;
        for change in &changes {
            if previous.is_some_and(|key| key >= change.key) {
                return Err(ContentError::InvalidRecord("edit backing duplicate target"));
            }
            previous = Some(change.key);
            if let EditRecordExpected::ExactBytes(value) = &change.expected {
                bytes = bytes
                    .checked_add(value.capacity())
                    .ok_or(ContentError::LengthOverflow)?;
            }
            bytes = bytes
                .checked_add(change.value.as_ref().map_or(0, Vec::capacity))
                .ok_or(ContentError::LengthOverflow)?;
        }
        if bytes > WINDOW {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "edit.backing_window",
                limit: WINDOW as u64,
                actual: bytes as u64,
            });
        }
        let Self::Backed(port) = self else {
            return Err(ContentError::InvalidRecord("backed state"));
        };
        match port.apply(changes)? {
            EditRecordApply::Applied => Ok(()),
            EditRecordApply::NotApplied { .. } => Err(ContentError::ProviderFailure {
                what: "edit backing precondition",
            }),
        }
    }
    pub fn first(&mut self, kind: u32, excluded: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        let keys = match self {
            Self::Memory(memory) => memory
                .records
                .range(
                    EditRecordKey { kind, key: [0; 32] }..=EditRecordKey {
                        kind,
                        key: [255; 32],
                    },
                )
                .filter_map(|(key, _)| (Some(key.key) != excluded).then_some(key.key))
                .take(64)
                .collect(),
            Self::Backed(port) => port.first_keys(kind, excluded)?,
        };
        if keys.capacity() > 64
            || keys.windows(2).any(|pair| pair[0] >= pair[1])
            || keys.iter().any(|key| Some(*key) == excluded)
        {
            return Err(ContentError::InvalidRecord("edit backing key window"));
        }
        Ok(keys)
    }
}
pub(super) fn change(
    key: EditRecordKey,
    old: Option<Vec<u8>>,
    value: Option<Vec<u8>>,
) -> EditRecordChange {
    EditRecordChange {
        key,
        expected: old.map_or(EditRecordExpected::Missing, EditRecordExpected::ExactBytes),
        value,
    }
}
