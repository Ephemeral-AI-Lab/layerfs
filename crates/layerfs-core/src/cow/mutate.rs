use crate::delta::{Delta, DeltaEntry};
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult};

use super::tree::{Metadata, RootHandle, TreeNode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Mutation {
    Add {
        path: CanonicalPath,
        node: TreeNode,
    },
    Remove {
        path: CanonicalPath,
    },
    Replace {
        path: CanonicalPath,
        node: TreeNode,
    },
    SetMetadata {
        path: CanonicalPath,
        metadata: Metadata,
    },
    Rename {
        from: CanonicalPath,
        to: CanonicalPath,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutationResult {
    root: RootHandle,
    delta: Delta,
}

impl MutationResult {
    pub const fn root(&self) -> &RootHandle {
        &self.root
    }

    pub fn into_root(self) -> RootHandle {
        self.root
    }

    pub const fn delta(&self) -> &Delta {
        &self.delta
    }

    pub fn into_parts(self) -> (RootHandle, Delta) {
        (self.root, self.delta)
    }
}

impl RootHandle {
    pub fn apply_mutation(&self, mutation: Mutation) -> CoreResult<MutationResult> {
        let next = match mutation {
            Mutation::Add { path, node } => add_path(self.directory(), &path, node)?,
            Mutation::Remove { path } => remove_path(self.directory(), &path)?.0,
            Mutation::Replace { path, node } => replace_path(self.directory(), &path, node)?,
            Mutation::SetMetadata { path, metadata } => {
                set_metadata(self.directory(), &path, metadata)?
            }
            Mutation::Rename { from, to } => rename_path(self.directory(), &from, &to)?,
        };
        let next = RootHandle::from_directory(next);
        let delta = Delta::between(self, &next)?;
        Ok(MutationResult { root: next, delta })
    }

    pub fn add(&self, path: CanonicalPath, node: TreeNode) -> CoreResult<MutationResult> {
        self.apply_mutation(Mutation::Add { path, node })
    }

    pub fn remove(&self, path: CanonicalPath) -> CoreResult<MutationResult> {
        self.apply_mutation(Mutation::Remove { path })
    }

    pub fn replace(&self, path: CanonicalPath, node: TreeNode) -> CoreResult<MutationResult> {
        self.apply_mutation(Mutation::Replace { path, node })
    }

    pub fn set_metadata(
        &self,
        path: CanonicalPath,
        metadata: Metadata,
    ) -> CoreResult<MutationResult> {
        self.apply_mutation(Mutation::SetMetadata { path, metadata })
    }

    pub fn rename(&self, from: CanonicalPath, to: CanonicalPath) -> CoreResult<MutationResult> {
        self.apply_mutation(Mutation::Rename { from, to })
    }
}

pub(crate) fn apply_delta_entry(root: &RootHandle, entry: &DeltaEntry) -> CoreResult<RootHandle> {
    let next = match entry {
        DeltaEntry::Add { path, node } => {
            if root.lookup(path)?.is_some() {
                return Err(CoreError::DeltaConflict);
            }
            add_path(root.directory(), path, node.clone())?
        }
        DeltaEntry::Remove { path, before } => {
            let current = root.lookup_required(path)?;
            if current.identity() != *before {
                return Err(CoreError::DeltaConflict);
            }
            remove_path(root.directory(), path)?.0
        }
        DeltaEntry::Replace { path, before, node } => {
            let current = root.lookup_required(path)?;
            if current.identity() != *before {
                return Err(CoreError::DeltaConflict);
            }
            replace_path(root.directory(), path, node.clone())?
        }
        DeltaEntry::Metadata {
            path,
            before,
            before_metadata,
            after,
            after_metadata,
        } => {
            let current = root.lookup_required(path)?;
            if current.identity() != *before || current.metadata() != *before_metadata {
                return Err(CoreError::DeltaConflict);
            }
            let updated = current.with_metadata(*after_metadata);
            if updated.identity() != *after {
                return Err(CoreError::DeltaConflict);
            }
            if path.is_root() {
                updated
            } else {
                replace_path(root.directory(), path, updated)?
            }
        }
    };
    Ok(RootHandle::from_directory(next))
}

fn add_path(root: &TreeNode, path: &CanonicalPath, node: TreeNode) -> CoreResult<TreeNode> {
    let components = components(path)?;
    if components.is_empty() {
        return Err(CoreError::RootMutation);
    }
    add_at(root, &components, node)
}

fn add_at(root: &TreeNode, components: &[CanonicalName], node: TreeNode) -> CoreResult<TreeNode> {
    let Some((name, rest)) = components.split_first() else {
        return Err(CoreError::RootMutation);
    };
    if rest.is_empty() {
        return root.add_child(name.clone(), node);
    }
    let child = root
        .entries()
        .ok_or(CoreError::NotDirectory)?
        .get(name)
        .ok_or(CoreError::PathNotFound)?;
    let updated = add_at(child, rest, node)?;
    root.replace_child(name, updated)
}

fn remove_path(root: &TreeNode, path: &CanonicalPath) -> CoreResult<(TreeNode, TreeNode)> {
    let components = components(path)?;
    if components.is_empty() {
        return Err(CoreError::RootMutation);
    }
    remove_at(root, &components)
}

fn remove_at(root: &TreeNode, components: &[CanonicalName]) -> CoreResult<(TreeNode, TreeNode)> {
    let Some((name, rest)) = components.split_first() else {
        return Err(CoreError::RootMutation);
    };
    if rest.is_empty() {
        return root.remove_child(name);
    }
    let child = root
        .entries()
        .ok_or(CoreError::NotDirectory)?
        .get(name)
        .ok_or(CoreError::PathNotFound)?;
    let (updated, removed) = remove_at(child, rest)?;
    Ok((root.replace_child(name, updated)?, removed))
}

fn replace_path(root: &TreeNode, path: &CanonicalPath, node: TreeNode) -> CoreResult<TreeNode> {
    let components = components(path)?;
    if components.is_empty() {
        return Err(CoreError::RootMutation);
    }
    replace_at(root, &components, node)
}

fn replace_at(
    root: &TreeNode,
    components: &[CanonicalName],
    node: TreeNode,
) -> CoreResult<TreeNode> {
    let Some((name, rest)) = components.split_first() else {
        return Err(CoreError::RootMutation);
    };
    if rest.is_empty() {
        return root.replace_child(name, node);
    }
    let child = root
        .entries()
        .ok_or(CoreError::NotDirectory)?
        .get(name)
        .ok_or(CoreError::PathNotFound)?;
    let updated = replace_at(child, rest, node)?;
    root.replace_child(name, updated)
}

fn set_metadata(root: &TreeNode, path: &CanonicalPath, metadata: Metadata) -> CoreResult<TreeNode> {
    let components = components(path)?;
    set_metadata_at(root, &components, metadata)
}

fn set_metadata_at(
    root: &TreeNode,
    components: &[CanonicalName],
    metadata: Metadata,
) -> CoreResult<TreeNode> {
    let Some((name, rest)) = components.split_first() else {
        return Ok(root.with_metadata(metadata));
    };
    let child = root
        .entries()
        .ok_or(CoreError::NotDirectory)?
        .get(name)
        .ok_or(CoreError::PathNotFound)?;
    let updated = set_metadata_at(child, rest, metadata)?;
    root.replace_child(name, updated)
}

fn rename_path(root: &TreeNode, from: &CanonicalPath, to: &CanonicalPath) -> CoreResult<TreeNode> {
    // ponytail: rename is delete+create until first-class rename identity is required.
    let from_components = components(from)?;
    let to_components = components(to)?;
    if from_components.is_empty() || to_components.is_empty() || from == to {
        return Err(CoreError::InvalidRename);
    }
    if to_components.len() > from_components.len() && to_components.starts_with(&from_components) {
        return Err(CoreError::InvalidRename);
    }
    let (without_source, source) = remove_at(root, &from_components)?;
    add_at(&without_source, &to_components, source)
}

fn components(path: &CanonicalPath) -> CoreResult<Vec<CanonicalName>> {
    path.components().map(CanonicalName::from_bytes).collect()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::cas::InMemoryCas;
    use crate::delta::DeltaEntry;
    use crate::limits::MAX_CHILD_REFERENCES;
    use crate::LogicalFile;

    fn path(value: &str) -> CanonicalPath {
        CanonicalPath::new(value).unwrap()
    }

    fn name(value: &str) -> CanonicalName {
        CanonicalName::new(value).unwrap()
    }

    fn logical_file(cas: &mut InMemoryCas, bytes: &[u8]) -> LogicalFile {
        LogicalFile::full_replace(cas, Cursor::new(bytes.to_vec()))
            .unwrap()
            .into_file()
    }

    fn base_tree(cas: &mut InMemoryCas) -> (RootHandle, TreeNode, TreeNode) {
        let file = TreeNode::file(logical_file(cas, b"old file"));
        let sibling = TreeNode::file(logical_file(cas, b"unchanged sibling"));
        let directory = TreeNode::directory(vec![
            (name("file"), file),
            (name("sibling"), sibling.clone()),
        ])
        .unwrap();
        (
            RootHandle::from_entries(vec![
                (name("dir"), directory),
                (name("top-sibling"), sibling.clone()),
            ])
            .unwrap(),
            sibling.clone(),
            TreeNode::file(logical_file(cas, b"replacement")),
        )
    }

    #[test]
    fn mutation_rebuilds_only_the_changed_ancestor_spine() {
        let mut cas = InMemoryCas::new();
        let (parent, sibling, replacement) = base_tree(&mut cas);
        let old_file = parent.lookup_required(&path("dir/file")).unwrap().clone();
        let old_directory = parent.lookup_required(&path("dir")).unwrap().clone();

        let result = parent
            .replace(path("dir/file"), replacement.clone())
            .unwrap();
        let child = result.root();

        assert_ne!(parent.id(), child.id());
        assert_eq!(
            parent.lookup_required(&path("dir/file")).unwrap(),
            &old_file
        );
        assert_eq!(
            child
                .lookup_required(&path("dir/file"))
                .unwrap()
                .file_content()
                .unwrap(),
            replacement.file_content().unwrap()
        );
        assert!(TreeNode::ptr_eq(
            parent.lookup_required(&path("dir/sibling")).unwrap(),
            child.lookup_required(&path("dir/sibling")).unwrap()
        ));
        assert!(TreeNode::ptr_eq(
            parent.lookup_required(&path("top-sibling")).unwrap(),
            child.lookup_required(&path("top-sibling")).unwrap()
        ));
        assert!(TreeNode::ptr_eq(
            parent.lookup_required(&path("top-sibling")).unwrap(),
            &sibling
        ));
        assert!(!TreeNode::ptr_eq(
            &old_directory,
            child.lookup_required(&path("dir")).unwrap()
        ));
    }

    #[test]
    fn add_remove_replace_rename_and_metadata_have_exact_deltas() {
        let mut cas = InMemoryCas::new();
        let (parent, _, replacement) = base_tree(&mut cas);
        let added = TreeNode::file(logical_file(&mut cas, b"added"));

        let add = parent.add(path("dir/added"), added.clone()).unwrap();
        assert_eq!(
            add.delta().entries(),
            &[DeltaEntry::Add {
                path: path("dir/added"),
                node: added.clone(),
            }]
        );

        let remove = parent.remove(path("dir/file")).unwrap();
        assert_eq!(
            remove.delta().entries(),
            &[DeltaEntry::Remove {
                path: path("dir/file"),
                before: parent
                    .lookup_required(&path("dir/file"))
                    .unwrap()
                    .identity(),
            }]
        );

        let replace = parent
            .replace(path("dir/file"), replacement.clone())
            .unwrap();
        assert_eq!(
            replace.delta().entries(),
            &[DeltaEntry::Replace {
                path: path("dir/file"),
                before: parent
                    .lookup_required(&path("dir/file"))
                    .unwrap()
                    .identity(),
                node: replacement,
            }]
        );

        let metadata = parent
            .set_metadata(path("dir/file"), Metadata::new(0o755))
            .unwrap();
        match metadata.delta().entries() {
            [DeltaEntry::Metadata {
                path: delta_path,
                before_metadata,
                after_metadata,
                ..
            }] => {
                assert_eq!(delta_path, &path("dir/file"));
                assert_eq!(before_metadata.mode(), 0);
                assert_eq!(after_metadata.mode(), 0o755);
            }
            entries => panic!("unexpected metadata delta: {entries:?}"),
        }

        let rename = parent
            .rename(path("dir/file"), path("dir/renamed"))
            .unwrap();
        assert_eq!(
            rename.delta().entries(),
            &[
                DeltaEntry::Remove {
                    path: path("dir/file"),
                    before: parent
                        .lookup_required(&path("dir/file"))
                        .unwrap()
                        .identity(),
                },
                DeltaEntry::Add {
                    path: path("dir/renamed"),
                    node: parent.lookup_required(&path("dir/file")).unwrap().clone(),
                },
            ]
        );
    }

    #[test]
    fn delta_application_is_authenticated_and_replay_is_safe() {
        let mut cas = InMemoryCas::new();
        let (parent, _, replacement) = base_tree(&mut cas);
        let mutation = parent.replace(path("dir/file"), replacement).unwrap();
        let applied = mutation.delta().apply(&parent).unwrap();
        assert_eq!(applied, *mutation.root());

        let metadata = parent
            .set_metadata(CanonicalPath::root(), Metadata::new(1))
            .unwrap();
        assert_eq!(metadata.delta().apply(&parent).unwrap(), *metadata.root());

        assert_eq!(
            mutation.delta().apply(mutation.root()),
            Err(CoreError::DeltaParentMismatch {
                expected: parent.id(),
                actual: mutation.root().id(),
            })
        );
        assert_eq!(
            parent
                .lookup_required(&path("dir/file"))
                .unwrap()
                .identity(),
            mutation
                .delta()
                .entries()
                .first()
                .and_then(|entry| match entry {
                    DeltaEntry::Replace { before, .. } => Some(*before),
                    _ => None,
                })
                .unwrap()
        );
    }

    #[test]
    fn failed_mutations_leave_the_parent_unchanged() {
        let mut cas = InMemoryCas::new();
        let (parent, _, replacement) = base_tree(&mut cas);
        let before = parent.clone();
        assert_eq!(
            parent.replace(path("missing"), replacement),
            Err(CoreError::PathNotFound)
        );
        assert_eq!(parent, before);
        assert_eq!(
            parent.rename(path("dir"), path("dir/file/child")),
            Err(CoreError::InvalidRename)
        );
        assert_eq!(parent, before);
    }

    #[test]
    fn one_byte_content_edits_keep_phase_two_reuse_and_range_locality() {
        let mut cas = InMemoryCas::new();
        let data = (0..2 * 1024 * 1024)
            .map(|value| (value as u64).wrapping_mul(31) as u8)
            .collect::<Vec<_>>();
        let old_file = logical_file(&mut cas, &data);
        let parent =
            RootHandle::from_entries(vec![(name("file"), TreeNode::file(old_file.clone()))])
                .unwrap();
        let mut expected = data;
        expected[500_000] ^= 1;
        let edited = old_file
            .replace_range(&mut cas, 500_000..500_001, &[expected[500_000]])
            .unwrap();
        assert!(edited.counters().cdc_bytes_scanned < old_file.length());
        assert!(edited.counters().chunks_reused > 0);
        assert_eq!(
            edited
                .file()
                .read_range(&cas, 500_000..500_001)
                .unwrap()
                .bytes(),
            &expected[500_000..500_001]
        );

        let child = parent
            .replace(path("file"), TreeNode::file(edited.into_file()))
            .unwrap();
        assert_ne!(child.root().id(), parent.id());
        assert_eq!(
            child
                .root()
                .lookup_required(&path("file"))
                .unwrap()
                .metadata(),
            Metadata::default()
        );
    }

    #[test]
    fn directory_entry_limits_are_typed() {
        let node = TreeNode::empty_directory();
        let entries =
            (0..=MAX_CHILD_REFERENCES).map(|index| (name(&format!("entry-{index}")), node.clone()));
        assert_eq!(
            TreeNode::directory(entries),
            Err(CoreError::ObjectLimitExceeded)
        );
    }
}
