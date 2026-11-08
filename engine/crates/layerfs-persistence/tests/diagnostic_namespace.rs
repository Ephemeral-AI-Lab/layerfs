//! C08/C09 valid-CID namespace damage and all historical contexts.
mod support;
use layerfs_content::{
    encode_whole_file_payload,
    filesystem::{
        attributes::PortableMetadata,
        directory::{encode_directory_page, DirectoryPage},
        root::profile_id,
        scope_for_seed, FilesystemRoot, PathName,
    },
    inode_leaf::{
        encode_inode_value, InodeKind, InodeLeaf, InodeLeafRow, InodeMetadata, InodeValue,
        LEAF_ROW_BYTES,
    },
    ObjectId, ObjectRole,
};
use layerfs_history::{
    AddLayerRequest, CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest, ForkSource,
    HistoryCatalog, StackInitialization,
};
use support::doctor::{clean, failure, Fixture};
type Node<'a> = (u64, InodeKind, u64, &'a [(&'a str, u64)]);
fn tree(f: &Fixture, nodes: &[Node<'_>]) -> ObjectId {
    let content = f
        .put(
            ObjectRole::WholeFile,
            encode_whole_file_payload(b"valid namespace file").unwrap(),
        )
        .id();
    let mut values = Vec::new();
    let mut rows = Vec::new();
    for (serial, kind, count, entries) in nodes {
        let content = if *kind == InodeKind::Directory {
            f.put(
                ObjectRole::DirectoryLeaf,
                encode_directory_page(&DirectoryPage::Leaf {
                    entries: entries
                        .iter()
                        .map(|(name, serial)| (PathName::new(name).unwrap(), *serial))
                        .collect(),
                })
                .unwrap(),
            )
            .id()
        } else {
            content
        };
        let value = encode_inode_value(InodeValue {
            kind: *kind,
            namespace_ref_count: *count,
            content_root: content,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode: 0o755,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            }),
        });
        values.push(value);
        rows.push(InodeLeafRow {
            serial: *serial,
            value,
        });
    }
    let table = f.inode(
        InodeLeaf {
            subtree_bytes: LEAF_ROW_BYTES as u64 * rows.len() as u64,
            rows,
        }
        .encode()
        .unwrap(),
        values,
    );
    f.put(
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(profile_id(), scope_for_seed([11; 32]), 1, table)
            .unwrap()
            .encode()
            .unwrap(),
    )
    .id()
}
fn initialize(f: &Fixture, root: ObjectId, tag: u8) -> layerfs_history::LayerStackRecord {
    f.handles
        .history
        .initialize_layerstack(&StackInitialization {
            stack: support::stack(tag),
            name: support::name(&format!("stack{tag}")),
            scope: scope_for_seed([11; 32]).object(),
            profile: profile_id(),
            genesis_root: root,
            logical: 0,
        })
        .unwrap()
}
const DIR: InodeKind = InodeKind::Directory;
const FILE: InodeKind = InodeKind::RegularFile;
#[test]
fn c08_missing_inode_alias_island_count_and_cycle_fail_in_real_root_context() {
    let cases: Vec<Vec<Node<'_>>> = vec![
        vec![(1, DIR, 0, &[("file", 2)]), (2, FILE, 1, &[])],
        vec![(1, DIR, 0, &[("file", 2)])],
        vec![(1, DIR, 0, &[("a", 2), ("b", 2)]), (2, DIR, 1, &[])],
        vec![(1, DIR, 0, &[]), (2, FILE, 1, &[])],
        vec![(1, DIR, 0, &[("file", 2)]), (2, FILE, 2, &[])],
        vec![
            (1, DIR, 0, &[("d", 2)]),
            (2, DIR, 1, &[("down", 3)]),
            (3, DIR, 1, &[("back", 2)]),
        ],
    ];
    for (index, nodes) in cases.iter().enumerate() {
        let f = Fixture::new();
        let root = tree(&f, nodes);
        initialize(&f, root, 1);
        let out = f.audit(false);
        if index == 0 {
            clean(&out);
        } else {
            failure(&out, "store.namespace");
            assert!(
                out.iter().any(|o| o.message.contains(&root.to_string())),
                "root identity must remain attributable: {out:?}"
            );
        }
    }
}
#[test]
fn c09_old_corrupt_namespace_is_checked_after_current_head_becomes_valid() {
    let f = Fixture::new();
    let old = tree(&f, &[(1, DIR, 0, &[]), (2, FILE, 1, &[])]);
    let stack = initialize(&f, old, 1);
    let current = tree(&f, &[(1, DIR, 0, &[("file", 2)]), (2, FILE, 1, &[])]);
    let branch = support::branch_id(1);
    f.handles
        .history
        .fork(&ForkRequest {
            branch,
            stack: stack.id,
            name: support::name("main"),
            source: ForkSource::Layer(stack.head_layer),
        })
        .unwrap();
    let snapshot = f.handles.history.branch_snapshot(branch).unwrap().unwrap();
    let CommitOutcome::Committed { record, .. } = f
        .handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: None,
            expected_base: snapshot.branch.base_layer,
            expected_root: snapshot.effective_root,
            candidate_root: current,
            profile: snapshot.profile,
            scope: snapshot.scope,
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap()
    else {
        panic!("Commit");
    };
    f.handles
        .history
        .add_layer(&AddLayerRequest {
            stack: stack.id,
            branch,
            commit: record.id,
            expected_stack_head: stack.head_layer,
            expected_branch_base: record.base_layer,
        })
        .unwrap();
    let full = f.audit(false);
    failure(&full, "store.namespace");
    assert!(full
        .iter()
        .any(|o| o.id == "store.namespace" && o.message.contains(&old.to_string())));
    assert!(full
        .iter()
        .any(|o| o.id == "store.namespace.summary" && o.message.starts_with("2 historical")));
    let quick = f.audit(true);
    clean(&quick);
    assert!(quick.iter().any(|o| o.id == "store.namespace"
        && o.outcome == layerfs_persistence::diagnostic::Outcome::Skipped));
}
#[test]
fn c08_same_canonical_root_under_wrong_scope_and_profile_is_not_deduplicated() {
    for wrong_profile in [false, true] {
        let f = Fixture::new();
        let root = tree(&f, &[(1, DIR, 0, &[("file", 2)]), (2, FILE, 1, &[])]);
        initialize(&f, root, 1);
        clean(&f.audit(false));
        f.handles
            .history
            .initialize_layerstack(&StackInitialization {
                stack: support::stack(2),
                name: support::name("wrong"),
                scope: if wrong_profile {
                    scope_for_seed([11; 32]).object()
                } else {
                    support::root(99)
                },
                profile: if wrong_profile {
                    support::root(98)
                } else {
                    profile_id()
                },
                genesis_root: root,
                logical: 0,
            })
            .unwrap();
        let out = f.audit(false);
        failure(&out, "store.namespace");
        assert!(out
            .iter()
            .any(|o| o.id == "store.namespace.summary" && o.message.starts_with("2 historical")));
    }
}
