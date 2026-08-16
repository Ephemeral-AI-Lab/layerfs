use std::collections::BTreeSet;

use crate::cow::{Metadata, RootHandle, RootId, TreeNode};
use crate::identity::ObjectId;
use crate::limits::MAX_CHILD_REFERENCES;
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeltaEntry {
    Add {
        path: CanonicalPath,
        node: TreeNode,
    },
    Remove {
        path: CanonicalPath,
        before: ObjectId,
    },
    Replace {
        path: CanonicalPath,
        before: ObjectId,
        node: TreeNode,
    },
    Metadata {
        path: CanonicalPath,
        before: ObjectId,
        before_metadata: Metadata,
        after: ObjectId,
        after_metadata: Metadata,
    },
}

impl DeltaEntry {
    pub fn path(&self) -> &CanonicalPath {
        match self {
            Self::Add { path, .. }
            | Self::Remove { path, .. }
            | Self::Replace { path, .. }
            | Self::Metadata { path, .. } => path,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delta {
    parent: RootId,
    child: RootId,
    entries: Vec<DeltaEntry>,
}

impl Delta {
    pub fn new(parent: RootId, child: RootId, entries: Vec<DeltaEntry>) -> CoreResult<Self> {
        if entries.len() > MAX_CHILD_REFERENCES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        Ok(Self {
            parent,
            child,
            entries,
        })
    }

    pub fn between(parent: &RootHandle, child: &RootHandle) -> CoreResult<Self> {
        let mut entries = Vec::new();
        let mut path = Vec::new();
        diff_nodes(parent.node(), child.node(), &mut path, &mut entries)?;
        Self::new(parent.id(), child.id(), entries)
    }

    pub const fn parent(&self) -> RootId {
        self.parent
    }

    pub const fn child(&self) -> RootId {
        self.child
    }

    pub fn entries(&self) -> &[DeltaEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn apply(&self, parent: &RootHandle) -> CoreResult<RootHandle> {
        if parent.id() != self.parent {
            return Err(CoreError::DeltaParentMismatch {
                expected: self.parent,
                actual: parent.id(),
            });
        }

        let mut current = parent.clone();
        for entry in &self.entries {
            current = crate::cow::apply_delta_entry(&current, entry)?;
        }
        if current.id() != self.child {
            return Err(CoreError::DeltaChildMismatch {
                expected: self.child,
                actual: current.id(),
            });
        }
        Ok(current)
    }
}

fn diff_nodes(
    old: &TreeNode,
    new: &TreeNode,
    path: &mut Vec<CanonicalName>,
    entries: &mut Vec<DeltaEntry>,
) -> CoreResult<()> {
    if old.identity() == new.identity() {
        return Ok(());
    }
    if old.kind() != new.kind() {
        return push_entry(
            entries,
            DeltaEntry::Replace {
                path: make_path(path)?,
                before: old.identity(),
                node: new.clone(),
            },
        );
    }

    match (old.file_content(), new.file_content()) {
        (Some(old_content), Some(new_content)) => {
            if old_content != new_content {
                return push_entry(
                    entries,
                    DeltaEntry::Replace {
                        path: make_path(path)?,
                        before: old.identity(),
                        node: new.clone(),
                    },
                );
            }
            if old.metadata() != new.metadata() {
                return push_entry(entries, metadata_entry(old, new, path)?);
            }
            Ok(())
        }
        (None, None) => {
            if old.metadata() != new.metadata() {
                push_entry(entries, metadata_entry(old, new, path)?)?;
            }
            let old_entries = old.entries().ok_or(CoreError::NotDirectory)?;
            let new_entries = new.entries().ok_or(CoreError::NotDirectory)?;
            let names = old_entries
                .keys()
                .chain(new_entries.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            for name in names {
                path.push(name.clone());
                match (old_entries.get(&name), new_entries.get(&name)) {
                    (None, Some(node)) => push_entry(
                        entries,
                        DeltaEntry::Add {
                            path: make_path(path)?,
                            node: node.clone(),
                        },
                    )?,
                    (Some(node), None) => push_entry(
                        entries,
                        DeltaEntry::Remove {
                            path: make_path(path)?,
                            before: node.identity(),
                        },
                    )?,
                    (Some(old_node), Some(new_node)) => {
                        diff_nodes(old_node, new_node, path, entries)?;
                    }
                    (None, None) => {}
                }
                path.pop();
            }
            Ok(())
        }
        _ => Err(CoreError::DeltaConflict),
    }
}

fn metadata_entry(
    old: &TreeNode,
    new: &TreeNode,
    path: &[CanonicalName],
) -> CoreResult<DeltaEntry> {
    let after = old.with_metadata(new.metadata());
    Ok(DeltaEntry::Metadata {
        path: make_path(path)?,
        before: old.identity(),
        before_metadata: old.metadata(),
        after: after.identity(),
        after_metadata: new.metadata(),
    })
}

fn push_entry(entries: &mut Vec<DeltaEntry>, entry: DeltaEntry) -> CoreResult<()> {
    if entries.len() >= MAX_CHILD_REFERENCES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    entries.push(entry);
    Ok(())
}

fn make_path(components: &[CanonicalName]) -> CoreResult<CanonicalPath> {
    let mut bytes = Vec::new();
    for (index, component) in components.iter().enumerate() {
        if index != 0 {
            bytes.push(b'/');
        }
        bytes.extend_from_slice(component.as_bytes());
    }
    CanonicalPath::from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::TreeNode;

    #[test]
    fn empty_delta_is_deterministic_and_idempotent() {
        let root = RootHandle::empty();
        let delta = Delta::between(&root, &root).unwrap();
        assert!(delta.is_empty());
        assert_eq!(delta.apply(&root).unwrap(), root);
    }

    #[test]
    fn wrong_parent_and_wrong_target_fail_without_mutating_the_parent() {
        let parent = RootHandle::empty();
        let other = RootHandle::new(
            TreeNode::directory_with_metadata(std::iter::empty(), Metadata::new(1)).unwrap(),
        )
        .unwrap();
        let delta = Delta::new(parent.id(), other.id(), Vec::new()).unwrap();
        assert_eq!(
            delta.apply(&other),
            Err(CoreError::DeltaParentMismatch {
                expected: parent.id(),
                actual: other.id(),
            })
        );
        assert_eq!(
            delta.apply(&parent),
            Err(CoreError::DeltaChildMismatch {
                expected: other.id(),
                actual: parent.id(),
            })
        );
        assert_eq!(parent, RootHandle::empty());
    }
}
