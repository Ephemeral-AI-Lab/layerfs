//! Existing sealed v1 reference roots and reachable sets through the new caller.
#![allow(dead_code)]
#[path = "support/graph_state.rs"]
mod oracle;
mod support;
mod manifest {
    include!("fixtures/filesystem/manifest.rs");
}
use layerfs_content::filesystem::directory::codec::{decode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::codec::{decode_inode_page, InodePage};
use layerfs_content::filesystem::rows::{BindingRows, SliceBindingRows};
use layerfs_content::filesystem::state::{GraphCapacity, GraphStage};
use layerfs_content::filesystem::{
    build_filesystem_binding_rows_with_alias_graph_state,
    build_filesystem_binding_rows_with_graph_state, scope_for_seed,
    update_filesystem_binding_rows_with_alias_graph_state,
    update_filesystem_binding_rows_with_graph_state, DirectoryUpdate, FilesystemInput,
    FilesystemPhases, FilesystemRoot, FilesystemRootId, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{ObjectId, ObjectRole};
use oracle::{scopes, ObservedGraph};
use std::collections::{BTreeMap, BTreeSet};
use support::filesystem::{resources, synthetic, value, with_objects, TreeStore};

fn run(
    case: &manifest::FixtureCase,
    store: &mut TreeStore,
    base: Option<&manifest::FixtureCase>,
    supplied_aliases: bool,
    supplied_facts: bool,
    supplied_counts: bool,
) {
    let mut rows: BTreeMap<u64, Vec<(PathName, Option<u64>)>> = BTreeMap::new();
    for change in case.changes {
        rows.entry(change.parent).or_default().push((
            PathName::new(change.name).unwrap(),
            (change.binding >= 0).then_some(change.binding as u64),
        ));
    }
    for record in case.finals {
        if record.content == "directory" {
            rows.entry(record.serial).or_default();
        }
    }
    let directories: Vec<_> = rows
        .into_iter()
        .map(|(parent, mut changes)| {
            changes.sort_by(|a, b| a.0.cmp(&b.0));
            DirectoryUpdate { parent, changes }
        })
        .collect();
    let values: Vec<_> = case
        .finals
        .iter()
        .map(|record| InodeUpdate {
            serial: record.serial,
            value: value(
                match record.kind {
                    1 => InodeKind::RegularFile,
                    2 => InodeKind::Directory,
                    3 => InodeKind::Symlink,
                    other => panic!("kind{other}"),
                },
                synthetic(record.content),
                synthetic(record.metadata),
            ),
        })
        .collect();
    let fresh: Vec<_> = case
        .finals
        .iter()
        .map(|record| record.serial)
        .filter(|serial| {
            base.is_none_or(|old| !old.finals.iter().any(|record| record.serial == *serial))
        })
        .collect();
    let base = base.map(|old| FilesystemRootId(old.root.parse().unwrap()));
    let operation = FilesystemInput {
        base,
        scope: scope_for_seed([0x5a; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &values,
        new_inodes: &fresh,
        resources: resources(),
    };
    let source = SliceBindingRows::new(&operation).unwrap();
    let selected = scopes(
        source.binding_source_id().unwrap(),
        operation.scope,
        operation.base,
        operation.root_serial,
        GraphCapacity::default(),
    );
    let mut state = ObservedGraph::new(
        &selected,
        directories.len(),
        directories.iter().map(|row| row.changes.len()).sum(),
    );
    let phases = FilesystemPhases::disabled();
    let result = with_objects(store, |objects| {
        if supplied_counts && base.is_none() {
            layerfs_content::filesystem::update::build_filesystem_binding_rows_with_canonical_state(objects,&source,None,&mut state,&selected,&phases)
        } else if supplied_counts {
            layerfs_content::filesystem::update::update_filesystem_binding_rows_with_canonical_state(objects,&source,None,&mut state,&selected,&phases)
        } else if supplied_facts && base.is_none() {
            layerfs_content::filesystem::update::build_filesystem_binding_rows_with_namespace_state(objects, &source, None, &mut state, &selected, &phases)
        } else if supplied_facts {
            layerfs_content::filesystem::update::update_filesystem_binding_rows_with_namespace_state(objects, &source, None, &mut state, &selected, &phases)
        } else if supplied_aliases && base.is_none() {
            build_filesystem_binding_rows_with_alias_graph_state(
                objects, &source, None, &mut state, &selected, &phases,
            )
        } else if supplied_aliases {
            update_filesystem_binding_rows_with_alias_graph_state(
                objects, &source, None, &mut state, &selected, &phases,
            )
        } else if base.is_none() {
            build_filesystem_binding_rows_with_graph_state(
                objects, &source, None, &mut state, &selected, &phases,
            )
        } else {
            update_filesystem_binding_rows_with_graph_state(
                objects, &source, None, &mut state, &selected, &phases,
            )
        }
    })
    .unwrap_or_else(|error| panic!("{}:{error}", case.name));
    assert_eq!(
        result.root.0,
        case.root.parse::<ObjectId>().unwrap(),
        "{} independent reference root",
        case.name
    );
    let expected: BTreeSet<_> = case
        .objects
        .iter()
        .map(|object| object.id.parse::<ObjectId>().unwrap())
        .collect();
    assert_eq!(
        reachable(store, result.root.0),
        expected,
        "{} sealed reachable set",
        case.name
    );
    assert_eq!(state.retirements, 1);
    assert_eq!(state.abandonments, 0);
    assert_eq!(state.stage, GraphStage::Retired);
    assert_eq!(state.sites.retirements, 1);
}
fn reachable(store: &TreeStore, root: ObjectId) -> BTreeSet<ObjectId> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let bytes = store.canonical(id).unwrap();
        match store.role(id).unwrap() {
            ObjectRole::FilesystemRoot => {
                pending.push(FilesystemRoot::decode(bytes).unwrap().inode_table())
            }
            ObjectRole::DirectoryLeaf => {}
            ObjectRole::DirectoryBranch => {
                if let DirectoryPage::Branch { children, .. } =
                    decode_directory_page(bytes).unwrap()
                {
                    pending.extend(children.into_iter().map(|(_, id)| id));
                }
            }
            ObjectRole::InodeLeaf => {
                if let InodePage::Leaf { entries } = decode_inode_page(bytes).unwrap() {
                    pending.extend(
                        entries
                            .into_iter()
                            .filter(|(_, value)| value.kind == InodeKind::Directory)
                            .map(|(_, value)| value.content_root),
                    );
                }
            }
            ObjectRole::InodeBranch => {
                if let InodePage::Branch { children, .. } = decode_inode_page(bytes).unwrap() {
                    pending.extend(children.into_iter().map(|(_, id)| id));
                }
            }
            other => panic!("reachable role{other:?}"),
        }
    }
    seen
}
#[test]
fn new_graph_caller_preserves_every_sealed_v1_construction_root_and_object_set() {
    for case in manifest::CASES.iter().filter(|case| case.base.is_empty()) {
        run(case, &mut TreeStore::new(), None, false, false, false);
    }
}
#[test]
fn new_graph_caller_preserves_every_sealed_v1_update_root_and_object_set() {
    for case in manifest::CASES.iter().filter(|case| !case.base.is_empty()) {
        let base = manifest::CASES
            .iter()
            .find(|candidate| candidate.name == case.base)
            .unwrap();
        let mut store = TreeStore::new();
        run(base, &mut store, None, false, false, false);
        run(case, &mut store, Some(base), false, false, false);
    }
}

#[test]
fn supplied_alias_graph_route_preserves_all_independent_v1_roots_and_object_sets() {
    for case in manifest::CASES {
        let base = manifest::CASES.iter().find(|old| old.name == case.base);
        let mut store = TreeStore::new();
        if let Some(base) = base {
            run(base, &mut store, None, true, false, false);
        }
        run(case, &mut store, base, true, false, false);
    }
}

#[test]
fn supplied_namespace_route_preserves_all_independent_v1_roots_and_object_sets() {
    for case in manifest::CASES {
        let base = manifest::CASES.iter().find(|old| old.name == case.base);
        let mut store = TreeStore::new();
        if let Some(base) = base {
            run(base, &mut store, None, true, true, false);
        }
        run(case, &mut store, base, true, true, false);
    }
}

#[test]
fn supplied_canonical_counts_release_route_preserves_all_independent_v1_roots_and_object_sets() {
    for case in manifest::CASES {
        let base = manifest::CASES.iter().find(|old| old.name == case.base);
        let mut store = TreeStore::new();
        if let Some(base) = base {
            run(base, &mut store, None, true, true, true);
        }
        run(case, &mut store, base, true, true, true);
    }
}
