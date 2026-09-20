//! Observe actual provider work when zero-count evaluation includes new rows.

mod support;

use layerfs_content::filesystem::{
    update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead,
    FilesystemResources, FilesystemRootId, InodeUpdate,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{ContentError, ObjectRole};
use support::filesystem::{name_of, synthetic, value, CountingProvider, Session, TreeStore};

fn regular(serial: u64, edited: bool) -> InodeUpdate {
    InodeUpdate {
        serial,
        value: value(
            InodeKind::RegularFile,
            synthetic(&format!("content/{serial}")),
            synthetic(if edited { "edited" } else { "original" }),
        ),
    }
}

#[test]
fn mixed_new_and_existing_counts_preserve_roots_and_expose_provider_work() {
    for interior in [false, true] {
        let mut session = Session::new(1).unwrap();
        // Reserve odd serials without binding them. They are legal absent IDs
        // within the allocator scope, not declarations that overwrite old rows.
        let allocated = (2..=400).map(|_| session.allocate()).collect::<Vec<_>>();
        let existing = allocated
            .iter()
            .copied()
            .filter(|id| id % 2 == 0)
            .collect::<Vec<_>>();
        let base_directories = [DirectoryUpdate {
            parent: 1,
            changes: existing
                .iter()
                .map(|id| (name_of(&format!("f{id:05}")), Some(*id)))
                .collect(),
        }];
        let base_inodes = existing
            .iter()
            .map(|id| regular(*id, false))
            .collect::<Vec<_>>();
        session
            .apply(&base_directories, &base_inodes, &existing)
            .unwrap();
        assert_eq!(
            session.store.role(session.value.inode_table()),
            Some(ObjectRole::InodeBranch)
        );

        let new = if interior {
            allocated
                .iter()
                .copied()
                .filter(|id| id % 2 != 0)
                .take(129)
                .collect::<Vec<_>>()
        } else {
            (0..129).map(|_| session.allocate()).collect::<Vec<_>>()
        };
        let mut changes = new
            .iter()
            .map(|id| (name_of(&format!("f{id:05}")), Some(*id)))
            .collect::<Vec<_>>();
        changes.push((name_of("f00400"), None));
        changes.sort_by(|a, b| a.0.cmp(&b.0));
        let directories = [DirectoryUpdate { parent: 1, changes }];
        let mut inodes = new.iter().map(|id| regular(*id, false)).collect::<Vec<_>>();
        inodes.extend([regular(2, true), regular(4, true)]);
        inodes.sort_by_key(|inode| inode.serial);

        let mut expected_root = None;
        for batch in [1, 32] {
            let input = FilesystemInput {
                base: Some(FilesystemRootId(session.root)),
                scope: session.scope,
                root_serial: 1,
                directories: &directories,
                inodes: &inodes,
                new_inodes: &new,
                resources: FilesystemResources {
                    base_read_batch: batch,
                    ..FilesystemResources::default()
                },
            };
            let provider = CountingProvider::new(&session.store);
            let mut sink = TreeStore::new();
            let result = update_filesystem(
                &mut FilesystemObjects::new(&provider, &mut sink),
                &input,
                None,
            )
            .expect("mixed update");
            // Baseline demand count; actual pages and waves are reported below.
            assert_eq!(result.counters.base_records_read, 133);
            if let Some(root) = expected_root {
                assert_eq!(
                    result.root, root,
                    "batch size cannot alter canonical output"
                );
            }
            expected_root = Some(result.root);
            println!(
                "zero-count interior={interior} batch={batch} root={:?} provider_pages={} provider_waves={} zero_count_base_records={} ids={:?}",
                result.root,
                provider.demands(),
                provider.waves().len(),
                result.counters.base_records_read,
                provider.demanded(),
            );
            assert!(
                provider.peak_wave() <= layerfs_content::filesystem::objects::MAXIMUM_READ_DEMANDS
            );
            let mut store = session.store.clone();
            store.absorb(&sink);
            let mut reader = FilesystemRead::new(&store, result.root).unwrap();
            let records = reader.lookup_inodes(&new).unwrap();
            for (serial, record) in new.iter().zip(records) {
                assert_eq!(record.unwrap().namespace_ref_count, 1, "new {serial}");
            }
            let records = reader.lookup_inodes(&[2, 4, 400]).unwrap();
            for record in &records[..2] {
                let record = record.unwrap();
                assert_eq!(record.namespace_ref_count, 1);
                assert_eq!(record.metadata_root, synthetic("edited"));
            }
            assert_eq!(records[2], None, "existing zero-count inode is released");

            let mut invalid_new = new.clone();
            invalid_new.push(2);
            invalid_new.sort_unstable();
            let invalid = FilesystemInput {
                new_inodes: &invalid_new,
                ..input
            };
            let error = update_filesystem(
                &mut FilesystemObjects::new(&session.store, &mut TreeStore::new()),
                &invalid,
                None,
            )
            .unwrap_err();
            assert!(matches!(
                error,
                ContentError::InvalidRecord("reused inode serial")
            ));
        }
    }
}
