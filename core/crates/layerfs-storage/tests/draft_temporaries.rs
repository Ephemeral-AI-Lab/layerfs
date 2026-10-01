//! Actual native temporary ownership, distinct selectors and uncertain COMMIT custody.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_draft_process.rs"]
mod draft_process;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use super::{draft_process::HeldDraftReader, support};
    use layerfs_content::{
        file::{
            edit::{DraftCapacity, DraftRecord, DraftState},
            mapping::{ChildDescriptor, ExtentNode, ExtentSlice},
        },
        ObjectId,
    };
    use layerfs_storage::{
        construction_state::{DraftAdapter, ScratchAuthority, ScratchDisposition},
        StorageError,
    };
    use rusqlite::{Connection, OpenFlags};
    fn id(value: u8) -> ObjectId {
        ObjectId::from_bytes(&[value; 32]).unwrap()
    }
    fn leaf() -> DraftRecord {
        DraftRecord::Node(ExtentNode::Leaf {
            subtree_logical_bytes: 64,
            extents: (0..64)
                .map(|i| ExtentSlice::new(id(90), i * 2, 1).unwrap())
                .collect(),
        })
    }
    fn parent(child: ObjectId) -> DraftRecord {
        DraftRecord::Node(ExtentNode::Branch {
            level: 1,
            subtree_logical_bytes: 128,
            subtree_extent_count: 128,
            children: (1..=2)
                .map(|i| ChildDescriptor {
                    cumulative_logical_end: i * 64,
                    cumulative_extent_end: i * 64,
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
    #[test]
    fn native_target_supersession_preserves_shared_temporaries_and_unrelated_first_zero() {
        let directory = support::TempDir::new("draft-pinned-shared");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([81; 32], DraftCapacity::default())
            .unwrap();
        {
            let mut state = DraftAdapter::new(&mut session).unwrap();
            state.hold(id(1), leaf()).unwrap();
            state.hold(id(2), leaf()).unwrap();
            state.hold(id(3), parent(id(2))).unwrap();
            state.retain_temporaries(&[id(3)]).unwrap();
            state.select_root(None, id(3)).unwrap();
            state.retain_temporaries(&[id(2), id(2)]).unwrap();
            state.supersede(id(3), Some(id(3))).unwrap();
            assert!(state.get(id(3)).unwrap().is_none());
            assert_eq!(state.next_job().unwrap().unwrap().id, id(1));
            state.supersede(id(2), None).unwrap();
            assert!(state.get(id(2)).unwrap().is_some());
            state.supersede(id(2), None).unwrap();
            assert!(state.get(id(2)).unwrap().is_none());
            assert!(state.get(id(1)).unwrap().is_some());
            assert_eq!(state.stats().links_retired, 2);
            assert_eq!(
                state.stats().deferred_body_bytes,
                std::mem::size_of::<ExtentNode>() + 64 * std::mem::size_of::<ExtentSlice>() + 128
            );
            state.finish().unwrap();
            assert_eq!(
                (state.stats().bytes, state.stats().deferred_body_bytes),
                (0, 0)
            );
        }
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
    #[test]
    fn another_issued_owners_selected_root_refuses_before_mutation_and_cannot_poison_that_owner() {
        let directory = support::TempDir::new("draft-pin-foreign-selected");
        let authority = ScratchAuthority::new(directory.path(), 2).unwrap();
        let mut first = authority
            .begin_drafts([82; 32], DraftCapacity::default())
            .unwrap();
        let mut second = authority
            .begin_drafts([83; 32], DraftCapacity::new(48 * 1024 * 1024).unwrap())
            .unwrap();
        assert_ne!(first.selection().token(), second.selection().token());
        for (session, target) in [(&mut first, id(1)), (&mut second, id(2))] {
            let mut state = DraftAdapter::new(session).unwrap();
            state.hold(target, leaf()).unwrap();
            state.retain_temporaries(&[target]).unwrap();
            state.select_root(None, target).unwrap();
        }
        {
            let mut state = DraftAdapter::new(&mut first).unwrap();
            let before = state.stats();
            assert!(state.supersede(id(2), Some(id(2))).is_err());
            assert_eq!(state.stats(), before);
        }
        assert!(!first.is_quarantined());
        assert!(matches!(
            first.take_failure(),
            Some(StorageError::Integrity("draft selected root expected"))
        ));
        {
            let mut state = DraftAdapter::new(&mut second).unwrap();
            assert!(state.get(id(2)).unwrap().is_some());
            state.supersede(id(2), Some(id(2))).unwrap();
            state.finish().unwrap();
        }
        first.release().unwrap();
        second.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
    #[test]
    fn real_supersede_commit_unknown_retains_exact_selected_and_temporary_attempt_without_replay() {
        let directory = support::TempDir::new("draft-pin-unknown");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([84; 32], DraftCapacity::default())
            .unwrap();
        let token = session.selection().token();
        let path = authority.status().unwrap()[0].path.clone();
        {
            let mut state = DraftAdapter::new(&mut session).unwrap();
            state.hold(id(1), leaf()).unwrap();
            state.retain_temporaries(&[id(1)]).unwrap();
            state.select_root(None, id(1)).unwrap();
        }
        let mut blocker = HeldDraftReader::new(&path, "TemporarySupersede");
        {
            let mut state = DraftAdapter::new(&mut session).unwrap();
            assert!(state.supersede(id(1), Some(id(1))).is_err());
            assert!(state.supersede(id(1), Some(id(1))).is_err());
        }
        assert!(
            session.is_quarantined(),
            "original failure: {:?}",
            session.take_failure()
        );
        assert!(matches!(
            session.take_failure(),
            Some(StorageError::UnknownOutcome { .. })
        ));
        drop(session);
        let status = authority
            .status()
            .unwrap()
            .into_iter()
            .find(|row| row.token == token)
            .unwrap();
        assert_eq!(status.disposition, ScratchDisposition::Unknown);
        assert!(status.retained && status.quarantined);
        assert!(status.failure.unwrap().contains("TemporarySupersede"));
        assert_eq!(authority.reserved_bytes().unwrap(), 16 * 1024 * 1024);
        assert!(authority.release_retained(token).is_err());
        let output = blocker.release_and_reap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(path.exists());
        std::mem::forget(directory);
    }
    #[test]
    fn draft_reader_child() {
        let Some(path) = std::env::var_os("LAYERFS_DRAFT_READER_DB") else {
            return;
        };
        use std::io::{Read, Write};
        assert_eq!(
            std::env::var("LAYERFS_DRAFT_READER_PHASE").unwrap(),
            "TemporarySupersede"
        );
        let connection = readonly(std::path::Path::new(&path));
        connection.execute_batch("BEGIN").unwrap();
        let before: (Vec<u8>, Vec<u8>, i64) = connection.query_row(
            "SELECT selected,(SELECT links FROM draft_counts WHERE id=?1),(SELECT COUNT(*) FROM draft_bodies) FROM draft_owner WHERE id=1",
            [id(1).as_bytes().as_slice()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(
            before,
            (id(1).as_bytes().to_vec(), 2u64.to_be_bytes().to_vec(), 1)
        );
        let mut control = std::os::unix::net::UnixStream::connect(
            std::env::var_os("LAYERFS_DRAFT_READER_SOCKET").unwrap(),
        )
        .unwrap();
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
        let after: (Vec<u8>, Vec<u8>, i64) = connection.query_row(
            "SELECT selected,(SELECT links FROM draft_counts WHERE id=?1),(SELECT COUNT(*) FROM draft_bodies) FROM draft_owner WHERE id=1",
            [id(1).as_bytes().as_slice()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(after, before);
        connection.execute_batch("ROLLBACK").unwrap();
        drop(connection);
        control.write_all(&[3]).unwrap();
    }
}
