//! Real profile6 native ownership, exact v1 canonical edit and failure custody.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_draft_process.rs"]
mod draft_process;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "../../layerfs-content/tests/support/edits.rs"]
#[allow(dead_code)] // This proof deliberately uses only the shared Edits/Parts subset.
mod edits;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "../../layerfs-content/tests/support/cache_oracle.rs"]
mod oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use super::{
        draft_process::HeldDraftReader,
        edits::{Edits, Parts},
        oracle, support,
    };
    use layerfs_content::file::edit::{
        apply_edits_with_state, DraftCapacity, DraftRecord, DraftState,
    };
    use layerfs_content::file::mapping::{ChildDescriptor, ExtentNode, ExtentSlice};
    use layerfs_content::{
        ContentError, ContentResult, Edit, EditRequest, FinalizedConsumer, FinalizedObject,
        ObjectId, ObjectRole,
    };
    use layerfs_storage::construction_state::{DraftAdapter, ScratchAuthority, ScratchDisposition};
    use layerfs_storage::{SaveHandoff, Store, StoreProvider};
    use rusqlite::{Connection, OpenFlags};
    use std::path::PathBuf;
    fn id(value: u8) -> ObjectId {
        ObjectId::from_bytes(&[value; 32]).unwrap()
    }
    fn leaf() -> DraftRecord {
        DraftRecord::Node(ExtentNode::Leaf {
            subtree_logical_bytes: 64,
            extents: (0..64)
                .map(|ordinal| ExtentSlice::new(id(90), ordinal * 2, 1).unwrap())
                .collect(),
        })
    }
    fn parent(child: ObjectId) -> DraftRecord {
        DraftRecord::Node(ExtentNode::Branch {
            level: 1,
            subtree_logical_bytes: 4096,
            subtree_extent_count: 4096,
            children: (1..=64)
                .map(|ordinal| ChildDescriptor {
                    cumulative_logical_end: ordinal * 64,
                    cumulative_extent_end: ordinal * 64,
                    child_object_id: child,
                })
                .collect(),
        })
    }
    fn readonly(path: &std::path::Path) -> Connection {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        connection.busy_timeout(std::time::Duration::ZERO).unwrap();
        connection
    }
    fn digest(bytes: &[u8]) -> ObjectId {
        let mut hash = blake3::Hasher::new();
        hash.update(b"layerfs/object/v2\0");
        hash.update(bytes);
        ObjectId::from_bytes(hash.finalize().as_bytes()).unwrap()
    }
    fn envelope(value: &[u8]) -> Vec<u8> {
        let mut bytes = b"LFSO\x01".to_vec();
        bytes.extend_from_slice(&((value.len() + 4) as u32).to_be_bytes());
        bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
        bytes.extend_from_slice(value);
        bytes
    }
    fn save_fixture(store: &Store, fixture: &oracle::Fixture) {
        support::disabled(|scope| {
            let mut save = store.begin_save(scope.child("fixture"))?;
            // Independent fixture IDs remain unchanged; the real producer must
            // offer children before their parent rather than sort by digest.
            let order = std::iter::once(fixture.payload_id)
                .chain(fixture.leaves.iter().copied())
                .chain(fixture.dummy.iter().copied())
                .chain([fixture.state.mapping_root, fixture.file_root]);
            for id in order {
                let bytes = fixture.canonical(id);
                let role = fixture.role(id);
                let references = match role {
                    ObjectRole::Chunk => Vec::new(),
                    ObjectRole::ExtentLeaf => vec![fixture.payload_id; 128],
                    ObjectRole::ExtentBranch => fixture.leaves.clone(),
                    ObjectRole::FileState => vec![fixture.state.mapping_root],
                    _ => unreachable!(),
                };
                save.accept(
                    FinalizedObject::new(role, bytes.to_vec())?.with_references(references),
                )?;
            }
            save.finish(scope.child("fixture.finish"))
        })
        .unwrap();
    }
    struct Audit<'a, 'b> {
        handoff: &'a mut SaveHandoff<'b>,
        scratch: PathBuf,
        accepted: Vec<(ObjectId, ObjectRole)>,
    }
    impl FinalizedConsumer for Audit<'_, '_> {
        fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
            if object.role() == ObjectRole::FileState {
                let connection = readonly(&self.scratch);
                let row: (i64, i64, i64) = connection
                    .query_row(
                        "SELECT stage,records,bytes FROM draft_owner WHERE id=1",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .unwrap();
                assert_eq!(
                    row,
                    (1, 0, 0),
                    "real native metadata EOF precedes file-state acceptance"
                );
            }
            self.accepted.push((object.id(), object.role()));
            self.handoff.accept(object)
        }
    }
    #[test]
    fn actual_apply_edits_uses_real_drafts_and_reproduces_independent_v1_root() {
        let fixture = oracle::wide();
        let replacement: Vec<u8> = (0..32).map(|i| (i * 7 + 11) as u8).collect();
        // Freeze exact v1 bytes/IDs before any candidate output. This boundary edit
        // retains65 leaves and128 extents per leaf, replacing only its first slice.
        let mut chunk = b"LFS4CHK\0".to_vec();
        chunk.extend_from_slice(&replacement);
        let payload = digest(&envelope(&chunk));
        let mut expected_leaf = fixture.canonical(fixture.leaves[0]).to_vec();
        expected_leaf[44..76].copy_from_slice(payload.as_bytes());
        let new_leaf = digest(&expected_leaf);
        let mut expected_mapping = fixture.canonical(fixture.state.mapping_root).to_vec();
        expected_mapping[60..92].copy_from_slice(new_leaf.as_bytes());
        let new_mapping = digest(&expected_mapping);
        let mut expected_state = fixture.canonical(fixture.file_root).to_vec();
        expected_state[74..106].copy_from_slice(new_mapping.as_bytes());
        let expected_root = digest(&expected_state);
        let mut expected = fixture.expected.clone();
        expected[..32].copy_from_slice(&replacement);
        let directory = support::TempDir::new("draft-real-edit");
        let store = support::create_store(&directory.store_path("objects"));
        save_fixture(&store, &fixture);
        let scratch = support::TempDir::new("draft-real-authority");
        let authority = ScratchAuthority::new(scratch.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([71; 32], DraftCapacity::default())
            .unwrap();
        let path = authority.status().unwrap()[0].path.clone();
        let mut parts = Parts::new();
        parts.push(replacement);
        let sequence = Edits::new(fixture.state.logical_len, vec![Edit::overwrite(0, 32)]).unwrap();
        let provider = StoreProvider::new(&store);
        support::disabled(|scope| {
            let mut save = store.begin_save(scope.child("save"))?;
            let mut handoff = SaveHandoff::new(&mut save);
            let mut audit = Audit {
                handoff: &mut handoff,
                scratch: path.clone(),
                accepted: Vec::new(),
            };
            let mut state = DraftAdapter::new(&mut session)?;
            let built = apply_edits_with_state(
                store.policy().construction(),
                &store.policy().construction().capacities(),
                &provider,
                EditRequest {
                    root: fixture.file_root,
                    edits: &sequence,
                    source: &parts,
                },
                &mut audit,
                &mut state,
                scope.child("edit"),
            )?;
            assert_eq!(built.root, expected_root);
            assert_eq!(state.stats().bytes, 0);
            assert!(state.stats().retired > 0);
            assert!(state.stats().links_retired > 0);
            let order = &audit.accepted;
            let leaf_pos = order.iter().position(|(id, _)| *id == new_leaf).unwrap();
            let branch_pos = order.iter().position(|(id, _)| *id == new_mapping).unwrap();
            let file_pos = order
                .iter()
                .position(|(id, _)| *id == expected_root)
                .unwrap();
            assert!(leaf_pos < branch_pos && branch_pos < file_pos);
            drop(state);
            drop(audit);
            drop(handoff);
            save.finish(scope.child("save.finish"))?;
            Ok::<_, layerfs_storage::StorageError>(())
        })
        .unwrap();
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
        support::disabled(|scope| {
            let mut output = Vec::new();
            layerfs_content::read_all(&provider, expected_root, &mut output, scope.child("read"))?;
            assert_eq!(output, expected);
            let mut old = Vec::new();
            layerfs_content::read_all(&provider, fixture.file_root, &mut old, scope.child("old"))?;
            assert_eq!(old, fixture.expected);
            Ok::<_, ContentError>(())
        })
        .unwrap();
    }
    #[test]
    fn actual_shared_counts_and_revival_have_exact_cleanup() {
        let directory = support::TempDir::new("draft-links");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([72; 32], DraftCapacity::default())
            .unwrap();
        let path = authority.status().unwrap()[0].path.clone();
        let mut state = DraftAdapter::new(&mut session).unwrap();
        state.hold(id(1), leaf()).unwrap();
        state.hold(id(2), parent(id(1))).unwrap();
        state.select_root(None, id(2)).unwrap();
        let first = state.next_job().unwrap().unwrap();
        assert_eq!(first.links, 64);
        state.retire_job(first).unwrap();
        let second = state.next_job().unwrap().unwrap();
        assert_eq!(second.links, 1);
        state.retire_job(second).unwrap();
        state.hold(id(3), leaf()).unwrap();
        state.select_root(Some(id(2)), id(3)).unwrap();
        while let Some(job) = state.next_job().unwrap() {
            state.retire_job(job).unwrap();
        }
        assert!(state.get(id(1)).unwrap().is_none());
        assert!(state.get(id(2)).unwrap().is_none());
        assert_eq!(state.stats().links_retired, 64);
        let finished = state.finish();
        drop(state);
        assert!(
            finished.is_ok(),
            "native finish original: {:?}",
            session.take_failure()
        );
        assert_eq!(
            readonly(&path)
                .query_row("SELECT records FROM draft_owner WHERE id=1", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
    #[test]
    fn real_commit_unknown_retains_exact_attempt_and_denies_replay() {
        let directory = support::TempDir::new("draft-unknown");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([73; 32], DraftCapacity::default())
            .unwrap();
        let token = session.selection().token();
        let path = authority.status().unwrap()[0].path.clone();
        let mut blocker = HeldDraftReader::new(&path, "CreatingStart");
        {
            let mut state = DraftAdapter::new(&mut session).unwrap();
            assert!(state.hold(id(1), leaf()).is_err());
            assert!(state.hold(id(1), leaf()).is_err());
        }
        assert!(
            session.is_quarantined(),
            "CreatingStart original: {:?}",
            session.take_failure()
        );
        assert!(matches!(
            session.take_failure(),
            Some(layerfs_storage::StorageError::UnknownOutcome { .. })
        ));
        drop(session);
        let status = authority
            .status()
            .unwrap()
            .into_iter()
            .find(|status| status.token == token)
            .unwrap();
        assert_eq!(status.disposition, ScratchDisposition::Unknown);
        assert!(status.retained);
        assert!(status.quarantined);
        assert!(status.failure.unwrap().contains("CreatingStart"));
        assert_eq!(authority.reserved_bytes().unwrap(), 16 * 1024 * 1024);
        assert!(authority.release_retained(token).is_err());
        let output = blocker.release_and_reap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(path.exists());
        std::mem::forget(directory); // Retained Unknown native owner is never fixture-unlinked.
    }
    #[test]
    fn draft_reader_child() {
        let Some(path) = std::env::var_os("LAYERFS_DRAFT_READER_DB") else {
            return;
        };
        use std::io::{Read, Write};
        let socket = std::env::var_os("LAYERFS_DRAFT_READER_SOCKET").unwrap();
        let phase = std::env::var("LAYERFS_DRAFT_READER_PHASE").unwrap();
        let connection = readonly(std::path::Path::new(&path));
        connection.execute_batch("BEGIN").unwrap();
        let (stage, records, pending): (i64, i64, i64) = connection.query_row(
            "SELECT stage,records,(SELECT COUNT(*) FROM draft_emissions WHERE accepted=0) FROM draft_owner WHERE id=1", [],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).unwrap();
        assert_eq!(stage, 0);
        match phase.as_str() {
            "CreatingStart" => assert_eq!((records, pending), (0, 0)),
            "EmissionAcceptedResolved" => {
                assert!(records > 0);
                assert_eq!(pending, 1);
            }
            _ => panic!("unexpected selected barrier phase"),
        }
        let mut control = std::os::unix::net::UnixStream::connect(socket).unwrap();
        control
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        control
            .set_write_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        control.write_all(&[1]).unwrap();
        let mut release = [0];
        control.read_exact(&mut release).unwrap();
        assert_eq!(release, [2]);
        // The immutable reader snapshot is unchanged by the writer's unacknowledged transaction.
        let actual:(i64,i64,i64)=connection.query_row(
            "SELECT stage,records,(SELECT COUNT(*) FROM draft_emissions WHERE accepted=0) FROM draft_owner WHERE id=1",[],
            |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).unwrap();
        assert_eq!(actual, (stage, records, pending));
        connection.execute_batch("ROLLBACK").unwrap();
        drop(connection);
        control.write_all(&[3]).unwrap();
        println!("DIAGNOSTIC actual separate-process selected provider: {phase} BEGIN+SELECT SHARED snapshot {stage}/{records}/{pending}; ROLLBACK+close acknowledged");
    }
    #[test]
    fn corrupt_private_body_is_refused_and_known_native_cleanup_is_explicit() {
        let directory = support::TempDir::new("draft-corrupt-body");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([74; 32], DraftCapacity::default())
            .unwrap();
        let path = authority.status().unwrap()[0].path.clone();
        {
            let mut state = DraftAdapter::new(&mut session).unwrap();
            state.hold(id(1), leaf()).unwrap();
        }
        let external = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        external
            .execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF;")
            .unwrap();
        let mut body: Vec<u8> = external
            .query_row(
                "SELECT value FROM draft_bodies WHERE id=?1",
                [id(1).as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        body[20] ^= 1;
        external
            .execute(
                "UPDATE draft_bodies SET value=?1 WHERE id=?2",
                rusqlite::params![body, id(1).as_bytes().as_slice()],
            )
            .unwrap();
        drop(external);
        {
            let mut state = DraftAdapter::new(&mut session).unwrap();
            assert!(state.get(id(1)).is_err());
        }
        assert!(!session.is_quarantined());
        assert!(matches!(
            session.take_failure(),
            Some(layerfs_storage::StorageError::Integrity(
                "draft body checksum/totals"
            ))
        ));
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }

    struct HoldAtMapping<'a, 'b> {
        handoff: &'a mut SaveHandoff<'b>,
        path: PathBuf,
        held: Option<HeldDraftReader>,
        mapping_accepts: usize,
        file_accepts: usize,
    }
    impl FinalizedConsumer for HoldAtMapping<'_, '_> {
        fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
            let role = object.role();
            self.handoff.accept(object)?;
            if matches!(role, ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch) {
                self.mapping_accepts += 1;
                assert!(
                    self.held.is_none(),
                    "one known acceptance before native Unknown"
                );
                self.held = Some(HeldDraftReader::new(&self.path, "EmissionAcceptedResolved"));
            }
            if role == ObjectRole::FileState {
                self.file_accepts += 1;
            }
            Ok(())
        }
    }

    #[test]
    fn known_mapping_acceptance_then_real_ack_commit_unknown_never_emits_file_root() {
        let fixture = oracle::wide();
        let directory = support::TempDir::new("draft-accepted-unknown-store");
        let store = support::create_store(&directory.store_path("objects"));
        save_fixture(&store, &fixture);
        let scratch = support::TempDir::new("draft-accepted-unknown-authority");
        let authority = ScratchAuthority::new(scratch.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([75; 32], DraftCapacity::default())
            .unwrap();
        let token = session.selection().token();
        let path = authority.status().unwrap()[0].path.clone();
        let sequence = Edits::new(fixture.state.logical_len, vec![Edit::overwrite(0, 32)]).unwrap();
        let mut parts = Parts::new();
        parts.push(vec![77; 32]);
        let provider = StoreProvider::new(&store);
        support::disabled(|scope| {
            let mut save = store.begin_save(scope.child("save"))?;
            let mut handoff = SaveHandoff::new(&mut save);
            let mut held = HoldAtMapping {
                handoff: &mut handoff,
                path: path.clone(),
                held: None,
                mapping_accepts: 0,
                file_accepts: 0,
            };
            let mut state = DraftAdapter::new(&mut session)?;
            let result = apply_edits_with_state(
                store.policy().construction(),
                &store.policy().construction().capacities(),
                &provider,
                EditRequest {
                    root: fixture.file_root,
                    edits: &sequence,
                    source: &parts,
                },
                &mut held,
                &mut state,
                scope.child("edit"),
            );
            assert!(result.is_err());
            assert_eq!(held.mapping_accepts, 1);
            assert_eq!(held.file_accepts, 0);
            assert!(
                state.begin_emission(id(1), id(2)).is_err(),
                "Unknown denies replay"
            );
            drop(state);
            let output = held.held.take().unwrap().release_and_reap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            drop(held);
            drop(handoff);
            save.abort(scope.child("save.abort"))?;
            Ok::<_, layerfs_storage::StorageError>(())
        })
        .unwrap();
        assert!(
            session.is_quarantined(),
            "accepted-resolution original: {:?}",
            session.take_failure()
        );
        assert!(matches!(
            session.take_failure(),
            Some(layerfs_storage::StorageError::UnknownOutcome { .. })
        ));
        drop(session);
        let status = authority
            .status()
            .unwrap()
            .into_iter()
            .find(|status| status.token == token)
            .unwrap();
        assert!(status.retained && status.quarantined);
        assert!(status.failure.unwrap().contains("EmissionAcceptedResolved"));
        assert!(authority.release_retained(token).is_err());
        assert!(path.exists());
        std::mem::forget(scratch); // This exact retained owner is not guessed-cleaned by the fixture.
    }
    #[test]
    fn oversized_incoming_owner_is_refused_before_first_native_reservation() {
        let fixture = oracle::wide();
        let directory = support::TempDir::new("draft-owner-refusal");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([76; 32], DraftCapacity::default())
            .unwrap();
        let before = authority.status().unwrap()[0].clone();
        let mut oversized = Vec::with_capacity(8193);
        oversized.extend_from_slice(fixture.canonical(fixture.leaves[0]));
        let object = FinalizedObject::new(ObjectRole::ExtentLeaf, oversized)
            .unwrap()
            .with_references(vec![fixture.payload_id; 128]);
        {
            let mut state = DraftAdapter::new(&mut session).unwrap();
            assert!(state.hold(object.id(), DraftRecord::Page(object)).is_err());
        }
        let after = authority.status().unwrap()[0].clone();
        assert_eq!(after.allocated_bytes, before.allocated_bytes);
        assert_eq!(
            readonly(&after.path)
                .query_row("SELECT records FROM draft_owner WHERE id=1", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        assert!(!session.is_quarantined());
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
    #[test]
    fn mixed_sixteen_and_forty_eight_classes_keep_fixed_metadata_and_captured_owners() {
        let directory = support::TempDir::new("draft-mixed-S");
        let authority = ScratchAuthority::new(directory.path(), 2).unwrap();
        let small = DraftCapacity::new(16 * 1024 * 1024).unwrap();
        let wide = DraftCapacity::new(48 * 1024 * 1024).unwrap();
        assert_eq!(small.records(), wide.records());
        assert_eq!(small.encoded_bytes(), wide.encoded_bytes());
        let mut first = authority.begin_drafts([77; 32], small).unwrap();
        let mut second = authority.begin_drafts([78; 32], wide).unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 64 * 1024 * 1024);
        let first_scope = first.draft_scope().unwrap();
        let second_scope = second.draft_scope().unwrap();
        assert_eq!(
            u64::from_be_bytes(first_scope.as_bytes()[97..105].try_into().unwrap()),
            16 * 1024 * 1024
        );
        assert_eq!(
            u64::from_be_bytes(second_scope.as_bytes()[97..105].try_into().unwrap()),
            48 * 1024 * 1024
        );
        assert_eq!(first.profile().unwrap().max_page_count, 4096);
        assert_eq!(second.profile().unwrap().max_page_count, 12288);
        let finished = {
            let mut owner = DraftAdapter::new(&mut first).unwrap();
            owner.hold(id(1), leaf()).unwrap();
            owner.finish()
        };
        assert!(
            finished.is_ok(),
            "16MiB finish original: {:?}",
            first.take_failure()
        );
        let finished = {
            let mut owner = DraftAdapter::new(&mut second).unwrap();
            owner.hold(id(1), leaf()).unwrap();
            owner.finish()
        };
        assert!(
            finished.is_ok(),
            "48MiB finish original: {:?}",
            second.take_failure()
        );
        let statuses = authority.status().unwrap();
        assert_eq!(
            statuses
                .iter()
                .map(|status| status.reserved_bytes)
                .sum::<u64>(),
            64 * 1024 * 1024
        );
        assert_ne!(statuses[0].file, statuses[1].file);
        first.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 48 * 1024 * 1024);
        second.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
        assert!(DraftCapacity::new(16 * 1024 * 1024 - 4096).is_err());
        assert!(DraftCapacity::new(48 * 1024 * 1024 + 1).is_err());
    }
    #[test]
    fn checked_repeated_link_overflow_refuses_before_parent_creating_effects() {
        let directory = support::TempDir::new("draft-link-overflow");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([79; 32], DraftCapacity::default())
            .unwrap();
        let path = authority.status().unwrap()[0].path.clone();
        {
            let mut owner = DraftAdapter::new(&mut session).unwrap();
            owner.hold(id(1), leaf()).unwrap();
        }
        let external = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        external
            .execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF;")
            .unwrap();
        external
            .execute(
                "UPDATE draft_counts SET links=?1 WHERE id=?2",
                rusqlite::params![
                    u64::MAX.to_be_bytes().as_slice(),
                    id(1).as_bytes().as_slice()
                ],
            )
            .unwrap();
        {
            let mut owner = DraftAdapter::new(&mut session).unwrap();
            assert!(owner.hold(id(2), parent(id(1))).is_err());
        }
        assert_eq!(
            external
                .query_row(
                    "SELECT COUNT(*) FROM draft_headers WHERE id=?1",
                    [id(2).as_bytes().as_slice()],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        assert!(matches!(
            session.take_failure(),
            Some(layerfs_storage::StorageError::Integrity(
                "draft creation link overflow"
            ))
        ));
        drop(external);
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
}
