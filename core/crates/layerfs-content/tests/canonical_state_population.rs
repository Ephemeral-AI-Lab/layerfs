//! Real supplied caller population/depth cases; external finite providers are unqualified.
#![allow(dead_code)]
#[path = "support/graph_state.rs"]
mod oracle;
mod support;
use layerfs_content::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use layerfs_content::filesystem::rows::{BindingRows, SliceBindingRows};
use layerfs_content::filesystem::state::GraphCapacity;
use layerfs_content::filesystem::{
    scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemPhases, FilesystemResult,
    InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::InodeKind;
use oracle::{scopes, ObservedGraph};
use support::filesystem::{resources, synthetic, value, with_objects, TreeStore};
fn run(store: &mut TreeStore, input: &FilesystemInput<'_>) -> (FilesystemResult, ObservedGraph) {
    let source = SliceBindingRows::new(input).unwrap();
    let selected = scopes(
        source.binding_source_id().unwrap(),
        input.scope,
        input.base,
        1,
        GraphCapacity::default(),
    );
    let mut state = ObservedGraph::new(
        &selected,
        input.directories.len(),
        input.directories.iter().map(|d| d.changes.len()).sum(),
    );
    let phases = FilesystemPhases::disabled();
    let result=with_objects(store,|objects|if input.base.is_none(){layerfs_content::filesystem::update::build_filesystem_binding_rows_with_canonical_state(objects,&source,None,&mut state,&selected,&phases)}else{layerfs_content::filesystem::update::update_filesystem_binding_rows_with_canonical_state(objects,&source,None,&mut state,&selected,&phases)}).unwrap();
    (result, state)
}
#[test]
fn fresh_count_membership_and_final_output_cross128_without_a_full_c1_result_owner() {
    let files = 385u64;
    let directories = vec![DirectoryUpdate {
        parent: 1,
        changes: (2..=files + 1)
            .map(|s| (PathName::new(&format!("f{s:04}")).unwrap(), Some(s)))
            .collect(),
    }];
    let values: Vec<_> = (1..=files + 1)
        .map(|s| InodeUpdate {
            serial: s,
            value: value(
                if s == 1 {
                    InodeKind::Directory
                } else {
                    InodeKind::RegularFile
                },
                synthetic("payload"),
                synthetic("metadata"),
            ),
        })
        .collect();
    let new: Vec<_> = (1..=files + 1).collect();
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([0x2c; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &values,
        new_inodes: &new,
        resources: resources(),
    };
    let mut store = TreeStore::new();
    let (result, state) = run(&mut store, &input);
    let found = lookup_many(
        &store,
        InodeTable {
            root: result.value.inode_table(),
            root_serial: 1,
        },
        &new,
        &mut InodeReadWork::default(),
    )
    .unwrap();
    assert_eq!(found[0].unwrap().namespace_ref_count, 0);
    for file in &found[1..] {
        assert_eq!(file.unwrap().namespace_ref_count, 1);
    }
    assert_eq!(result.counters.references.final_values, 386);
    assert_eq!(state.canonical.stage, 5);
    assert!(state.canonical.counts.is_empty());
    assert!(state.facts.bases.is_empty());
}
#[test]
fn descendant_release_depth180_uses_external_frames_and_preserves_a_surviving_file_alias() {
    let depth = 180u64;
    let leaf = depth + 2;
    let mut directories = Vec::new();
    // The leaf regular file is also named by the root; that outside alias survives.
    directories.push(DirectoryUpdate {
        parent: 1,
        changes: vec![
            (PathName::new("chain").unwrap(), Some(2)),
            (PathName::new("keep").unwrap(), Some(leaf)),
        ],
    });
    for parent in 2..=depth + 1 {
        directories.push(DirectoryUpdate {
            parent,
            changes: vec![(PathName::new("next").unwrap(), Some(parent + 1))],
        });
    }
    let values: Vec<_> = (1..=leaf)
        .map(|serial| InodeUpdate {
            serial,
            value: value(
                if serial == leaf {
                    InodeKind::RegularFile
                } else {
                    InodeKind::Directory
                },
                synthetic("payload"),
                synthetic("metadata"),
            ),
        })
        .collect();
    let new: Vec<_> = (1..=leaf).collect();
    let ns = scope_for_seed([0x3c; 32]);
    let mut store = TreeStore::new();
    let initial = FilesystemInput {
        base: None,
        scope: ns,
        root_serial: 1,
        directories: &directories,
        inodes: &values,
        new_inodes: &new,
        resources: resources(),
    };
    let (base, _) = run(&mut store, &initial);
    let remove = [DirectoryUpdate {
        parent: 1,
        changes: vec![(PathName::new("chain").unwrap(), None)],
    }];
    let update = FilesystemInput {
        base: Some(base.root),
        scope: ns,
        root_serial: 1,
        directories: &remove,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let (result, state) = run(&mut store, &update);
    let found = lookup_many(
        &store,
        InodeTable {
            root: result.value.inode_table(),
            root_serial: 1,
        },
        &new,
        &mut InodeReadWork::default(),
    )
    .unwrap();
    assert!(found[0].is_some());
    for row in &found[1..leaf as usize - 1] {
        assert_eq!(*row, None);
    }
    assert_eq!(found[leaf as usize - 1].unwrap().namespace_ref_count, 1);
    assert_eq!(result.counters.release.peak_depth, 180);
    assert_eq!(state.canonical.maximum_depth, 180);
    assert!(state.canonical.retired);
    assert!(state.canonical.jobs.is_empty() && state.canonical.frames.is_empty());
    assert!(state.canonical.counts.is_empty() && state.facts.bases.is_empty());
}
