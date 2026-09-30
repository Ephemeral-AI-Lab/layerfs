//! Real LFCS SQLite/native-provider proofs, separate from speed or RSS admission.

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_oracle.rs"]
mod oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;

    use layerfs_content::filesystem::state::{
        IndexedState, PageLimit, StateKey, StateRecord, StateScope, StateTable,
    };
    use layerfs_content::{ContentError, ObjectId};
    use layerfs_storage::construction_state::{
        NativeIdentity, ScratchAuthority, ScratchDisposition, ScratchOwnerStatus, ScratchSession,
    };
    use layerfs_storage::StorageError;
    use rusqlite::{Connection, OpenFlags};

    use super::{oracle, support::TempDir};

    const CLASS: u64 = 16 * 1024 * 1024;
    const ROWS: u64 = 65_536;

    fn selected(session: &ScratchSession) -> StateScope {
        StateScope::new(session.selection().clone(), 1, StateTable::DirectoryRoots).unwrap()
    }

    fn input_scope(scope: &StateScope) -> [u8; 81] {
        oracle::scope(
            *scope.selection().selector(),
            scope.selection().token(),
            *scope.selection().owner_binding().unwrap(),
            scope.phase(),
        )
    }

    fn record(scope: &StateScope, serial: u64) -> StateRecord {
        StateRecord::directory_root(
            scope,
            serial,
            ObjectId::from_bytes(&oracle::root(serial)).unwrap(),
        )
        .unwrap()
    }

    fn status(authority: &ScratchAuthority, token: u64) -> ScratchOwnerStatus {
        authority
            .status()
            .unwrap()
            .into_iter()
            .find(|status| status.token == token)
            .unwrap()
    }

    fn external(path: &Path, writable: bool) -> Connection {
        let access = if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let connection =
            Connection::open_with_flags(path, access | OpenFlags::SQLITE_OPEN_NOFOLLOW).unwrap();
        connection.busy_timeout(std::time::Duration::ZERO).unwrap();
        if writable {
            connection
                .execute_batch("PRAGMA synchronous=OFF; PRAGMA journal_mode=MEMORY;")
                .unwrap();
        }
        connection
    }

    fn rows(path: &Path) -> i64 {
        external(path, false)
            .query_row("SELECT COUNT(*) FROM directory_roots", [], |row| row.get(0))
            .unwrap()
    }

    fn identity(metadata: &std::fs::Metadata) -> NativeIdentity {
        NativeIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            uid: metadata.uid(),
            mode: metadata.mode(),
        }
    }

    fn native_bytes(identity: NativeIdentity) -> [u8; 24] {
        let mut bytes = [0; 24];
        bytes[..8].copy_from_slice(&identity.device.to_be_bytes());
        bytes[8..16].copy_from_slice(&identity.inode.to_be_bytes());
        bytes[16..20].copy_from_slice(&identity.uid.to_be_bytes());
        bytes[20..24].copy_from_slice(&identity.mode.to_be_bytes());
        bytes
    }

    fn check_association(base: &Path, status: &ScratchOwnerStatus, scope: &StateScope) {
        let connection = external(&status.path, false);
        let header: Vec<u8> = connection
            .query_row("SELECT header FROM session_owner WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(header.len(), 192);
        assert_eq!(&header[..8], b"LFCSOWN1");
        assert_eq!(&header[8..10], &1u16.to_be_bytes());
        assert_eq!(&header[10..16], &[0; 6]);
        assert_eq!(&header[16..24], &status.token.to_be_bytes());
        assert_eq!(&header[24..56], &status.selector);
        assert_ne!(&header[88..120], &[0; 32]);
        let parent = identity(&std::fs::symlink_metadata(base).unwrap());
        let directory =
            identity(&std::fs::symlink_metadata(status.path.parent().unwrap()).unwrap());
        let file = identity(&std::fs::symlink_metadata(&status.path).unwrap());
        assert_eq!(status.parent, parent);
        assert_eq!(status.directory, Some(directory));
        assert_eq!(status.file, Some(file));
        assert_eq!(directory.mode & 0o777, 0o700);
        assert_eq!(file.mode & 0o777, 0o600);
        assert_eq!(&header[120..144], &native_bytes(parent));
        assert_eq!(&header[144..168], &native_bytes(directory));
        assert_eq!(&header[168..192], &native_bytes(file));
        let mut digest = blake3::Hasher::new();
        digest.update(b"layerfs/construction-state/native/v1\0");
        digest.update(&header[88..120]);
        digest.update(&native_bytes(parent));
        digest.update(&native_bytes(directory));
        digest.update(&native_bytes(file));
        digest.update(&status.selector);
        digest.update(&status.token.to_be_bytes());
        assert_eq!(&header[56..88], digest.finalize().as_bytes());
        assert_eq!(&header[56..88], scope.selection().owner_binding().unwrap());
    }

    #[test]
    fn directory_roots_65536_stream_seal_pages_and_native_release() {
        let directory = TempDir::new("construction_state_full");
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let foreign = directory.join("preserved-owner-file");
        std::fs::write(&foreign, b"unrelated").unwrap();
        let authority = ScratchAuthority::new(directory.path(), 2).unwrap();
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            1,
            "no scratch creation during constructor"
        );
        let mut session = authority.begin([0x19; 32], ROWS).unwrap();
        let scope = selected(&session);
        let expected_scope = input_scope(&scope);
        assert_eq!(scope.as_bytes(), expected_scope);
        let initial = status(&authority, session.selection().token());
        let private_directory = initial.path.parent().unwrap().to_path_buf();
        assert_eq!(initial.disposition, ScratchDisposition::Open);
        assert_eq!(initial.reserved_bytes, CLASS);
        assert_eq!(initial.allocated_bytes, Some(CLASS));
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        check_association(directory.path(), &initial, &scope);
        let profile = session.profile().unwrap();
        assert_eq!(
            (profile.application_id, profile.version, profile.page_size),
            (0x4c46_4353, 1, 4096)
        );
        assert_eq!(
            (
                profile.cache_size,
                profile.mmap_size,
                profile.max_page_count
            ),
            (-512, 0, 4096)
        );
        assert_eq!(
            (
                profile.synchronous,
                profile.temp_store,
                profile.foreign_keys,
                profile.busy_timeout
            ),
            (0, 2, 1, 0)
        );
        let capacity = session.capacity(&scope).unwrap();
        assert_eq!(
            (capacity.records(), capacity.encoded_bytes()),
            (ROWS, ROWS * 63)
        );
        assert!(matches!(
            capacity.check_requested(ROWS as usize + 1),
            Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.records",
                limit: ROWS,
                actual: 65_537
            })
        ));
        assert_eq!(rows(&initial.path), 0, "shape refusal has no row effects");

        let mut expected = oracle::Transcript::new(expected_scope);
        let mut batch = Vec::with_capacity(128);
        for serial in 1..=ROWS {
            let value = record(&scope, serial);
            assert_eq!(value.encode(), oracle::record(&expected_scope, serial));
            batch.push(value);
            expected.append(serial);
            if batch.len() == 128 {
                session.append(&scope, &batch).unwrap();
                batch.clear();
            }
        }
        assert!(batch.is_empty());
        drop(batch); // Producer fixture window ends before page consumers start.
        let expected_seal = expected.seal();
        let seal = session.seal(&scope).unwrap();
        assert_eq!(
            seal.encode(),
            expected_seal,
            "independent framing/transcript oracle"
        );
        assert_eq!(seal.records(), ROWS);
        assert_eq!(seal.encoded_bytes(), 4_128_768);
        let connection = external(&initial.path, false);
        let page_count: i64 = connection
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap();
        assert!(page_count <= 4096);
        assert_eq!(
            connection
                .query_row::<String, _, _>("PRAGMA quick_check", [], |row| row.get(0))
                .unwrap(),
            "ok"
        );
        drop(connection);
        let metadata = initial.path.metadata().unwrap();
        assert_eq!(metadata.len(), page_count as u64 * 4096);
        assert_eq!(metadata.blocks() * 512, CLASS);
        eprintln!("LFCS correctness/resource diagnostic: rows={ROWS}, record_bytes={}, SQLite_pages={page_count}, apparent_bytes={}, allocated_bytes={}, class={CLASS}; no speed/heap/RSS claim", ROWS * 63, metadata.len(), metadata.blocks() * 512);

        let mut after = None;
        let mut next = 1;
        let mut pages = 0;
        loop {
            let page = session.page(&seal, after, PageLimit::default()).unwrap();
            assert_eq!(page.records().len(), 128);
            assert_eq!(page.encoded_len(), 8227);
            for value in page.records() {
                assert_eq!(value.encode(), oracle::record(&expected_scope, next));
                next += 1;
            }
            let eof = next == ROWS + 1;
            assert_eq!(page.eof(), eof);
            assert_eq!(
                page.encode_header(),
                oracle::page_header(
                    &expected_seal,
                    Some(oracle::key(&expected_scope, next - 1)),
                    128,
                    eof
                )
            );
            after = page.last();
            let values = page.into_records();
            assert!(values.capacity() <= 128);
            drop(values);
            pages += 1;
            if eof {
                break;
            }
        }
        assert_eq!(pages, 512);
        let final_page = session.page(&seal, after, PageLimit::default()).unwrap();
        assert!(final_page.records().is_empty() && final_page.eof());
        assert_eq!(final_page.last(), after);
        assert_eq!(
            final_page.encode_header(),
            oracle::page_header(
                &expected_seal,
                Some(oracle::key(&expected_scope, ROWS)),
                0,
                true
            )
        );
        drop(final_page);
        for serial in [1, ROWS] {
            assert_eq!(
                session
                    .get(&seal, StateKey::directory_root(&scope, serial).unwrap())
                    .unwrap(),
                Some(record(&scope, serial))
            );
        }
        assert_eq!(
            session
                .get(&seal, StateKey::directory_root(&scope, ROWS + 1).unwrap())
                .unwrap(),
            None
        );

        session.adapter().release(&scope).unwrap();
        assert_eq!(
            authority.reserved_bytes().unwrap(),
            CLASS,
            "logical completion refunds no native credit"
        );
        assert!(initial.path.exists());
        assert!(session
            .get(&seal, StateKey::directory_root(&scope, 1).unwrap())
            .is_err());
        session.release().unwrap();
        session.release().unwrap(); // Known already released, no second native I/O.
        assert!(!initial.path.exists());
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
        assert!(authority.status().unwrap().is_empty());
        drop(session);
        drop(authority);
        assert!(
            !private_directory.exists(),
            "observed known-empty directory retirement"
        );
        assert_eq!(std::fs::read(&foreign).unwrap(), b"unrelated");
        assert_eq!(directory.path().metadata().unwrap().mode() & 0o777, 0o755);
    }

    #[test]
    fn owner_admission_and_native_path_authority_refuse_before_effects() {
        let directory = TempDir::new("construction_state_slots");
        for owners in [0, 65] {
            assert!(matches!(
                ScratchAuthority::new(directory.path(), owners),
                Err(StorageError::CapacityExceeded {
                    what: "construction scratch owners",
                    ..
                })
            ));
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        let authority = ScratchAuthority::new(directory.path(), 2).unwrap();
        for declared in [65_537, u64::MAX] {
            assert!(
                matches!(authority.begin([0; 32], declared), Err(StorageError::CapacityExceeded { what: "construction scratch declared rows", limit: ROWS, actual }) if actual == declared)
            );
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
            assert!(authority.status().unwrap().is_empty());
            assert_eq!(
                std::fs::read_dir(directory.path()).unwrap().count(),
                0,
                "declared shape refuses before token/native/SQL effects"
            );
        }
        let mut first = authority.begin([0x25; 32], 0).unwrap();
        let mut second = authority.begin([0x25; 32], 0).unwrap();
        assert_ne!(first.selection(), second.selection());
        assert_ne!(first.selection().token(), second.selection().token());
        assert_ne!(
            first.selection().owner_binding().unwrap(),
            second.selection().owner_binding().unwrap()
        );
        let private = status(&authority, first.selection().token())
            .path
            .parent()
            .unwrap()
            .to_path_buf();
        let before: std::collections::BTreeSet<_> = std::fs::read_dir(&private)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(before.len(), 2);
        assert!(matches!(
            authority.begin([0x26; 32], 0),
            Err(StorageError::OwnershipUnavailable)
        ));
        let after: std::collections::BTreeSet<_> = std::fs::read_dir(&private)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(before, after);
        assert_eq!(authority.reserved_bytes().unwrap(), 2 * CLASS);
        first.release().unwrap();
        second.release().unwrap();
        drop(first);
        drop(second);
        drop(authority);
        let alias = directory.join("alias");
        std::os::unix::fs::symlink(directory.path(), &alias).unwrap();
        assert!(matches!(
            ScratchAuthority::new(&alias, 1),
            Err(StorageError::Integrity("construction scratch parent type"))
        ));
        std::fs::remove_file(alias).unwrap();
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(matches!(
            ScratchAuthority::new(directory.path(), 1),
            Err(StorageError::Integrity(
                "construction scratch parent ownership"
            ))
        ));
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn append_count_and_order_refuse_before_batch_rows_and_keep_typed_failure() {
        let directory = TempDir::new("construction_state_append_refusal");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        for excessive in [true, false] {
            let mut session = authority.begin([0x34; 32], 129).unwrap();
            let scope = selected(&session);
            let current = status(&authority, session.selection().token());
            if excessive {
                let batch: Vec<_> = (1..=129).map(|serial| record(&scope, serial)).collect();
                assert!(matches!(
                    session.append(&scope, &batch),
                    Err(StorageError::Content(
                        ContentError::BoundedCapacityExceeded {
                            what: "indexed_state.append_records",
                            limit: 128,
                            actual: 129
                        }
                    ))
                ));
                assert_eq!(rows(&current.path), 0);
            } else {
                session
                    .append(&scope, &[record(&scope, 1), record(&scope, 2)])
                    .unwrap();
                let error = session
                    .adapter()
                    .append(&scope, &[record(&scope, 2), record(&scope, 3)])
                    .unwrap_err();
                assert!(matches!(
                    error,
                    ContentError::ProviderFailure {
                        what: "construction scratch state"
                    }
                ));
                assert_eq!(rows(&current.path), 2);
                assert!(matches!(
                    session.take_failure(),
                    Some(StorageError::Content(ContentError::InvalidOrderingRecord(
                        "state append order"
                    )))
                ));
                assert!(session.take_failure().is_none());
            }
            assert!(
                session.seal(&scope).is_err(),
                "failed producer phase never seals"
            );
            drop(session);
            let retained = status(&authority, current.token);
            assert!(retained.retained && !retained.quarantined && retained.failure.is_some());
            assert_eq!(retained.disposition, ScratchDisposition::Failed);
            assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
            assert!(matches!(
                authority.begin([0x35; 32], 0),
                Err(StorageError::OwnershipUnavailable)
            ));
            authority.release_retained(current.token).unwrap();
            assert!(!current.path.exists());
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        }
    }

    #[test]
    fn byte_limit_empty_and_small_pages_have_exact_eof_and_selected_seal() {
        let directory = TempDir::new("construction_state_page_bounds");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        for count in [0, 3] {
            let mut session = authority.begin([0x41; 32], count).unwrap();
            let scope = selected(&session);
            let bytes = input_scope(&scope);
            let mut expected = oracle::Transcript::new(bytes);
            for serial in 1..=count {
                session.append(&scope, &[record(&scope, serial)]).unwrap();
                expected.append(serial);
            }
            let seal = session.seal(&scope).unwrap();
            assert_eq!(seal.encode(), expected.seal());
            let mut after = None;
            for serial in 1..=count {
                let page = session
                    .page(&seal, after, PageLimit::new(128, 226).unwrap())
                    .unwrap();
                assert_eq!(page.records(), &[record(&scope, serial)]);
                assert_eq!(
                    page.encode_header(),
                    oracle::page_header(
                        &expected.seal(),
                        Some(oracle::key(&bytes, serial)),
                        1,
                        serial == count
                    )
                );
                after = page.last();
            }
            let empty = session
                .page(&seal, after, PageLimit::new(1, 163).unwrap())
                .unwrap();
            assert!(empty.records().is_empty() && empty.eof());
            assert_eq!(empty.last(), after);
            let foreign_scope =
                StateScope::new(session.selection().clone(), 2, StateTable::DirectoryRoots)
                    .unwrap();
            let foreign_key = StateKey::directory_root(&foreign_scope, 1).unwrap();
            assert!(session.get(&seal, foreign_key).is_err());
            session.release().unwrap();
        }
        let mut session = authority.begin([0x42; 32], 1).unwrap();
        let scope = selected(&session);
        session.append(&scope, &[record(&scope, 1)]).unwrap();
        let seal = session.seal(&scope).unwrap();
        assert!(matches!(
            session.page(&seal, None, PageLimit::new(128, 225).unwrap()),
            Err(StorageError::Content(
                ContentError::BoundedCapacityExceeded {
                    what: "indexed_state.page_bytes",
                    limit: 225,
                    actual: 226
                }
            ))
        ));
        session.release().unwrap();
    }

    #[test]
    fn indexed_queries_and_real_sql_ownership_preserve_original_error() {
        let directory = TempDir::new("construction_state_sql");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority.begin([0x51; 32], 1).unwrap();
        let scope = selected(&session);
        let current = status(&authority, session.selection().token());
        let connection = external(&current.path, true);
        for sql in [
            "EXPLAIN QUERY PLAN SELECT root FROM directory_roots WHERE key=?1",
            "EXPLAIN QUERY PLAN SELECT ordinal FROM directory_roots WHERE key<=?1 ORDER BY key DESC LIMIT 1",
            "EXPLAIN QUERY PLAN SELECT key,ordinal,root FROM directory_roots WHERE key>?1 AND key<=?2 ORDER BY key LIMIT ?3",
        ] {
            let mut statement = connection.prepare(sql).unwrap();
            let plan: Vec<String> = if sql.contains("?3") {
                statement.query_map(rusqlite::params![record(&scope, 1).key().as_bytes().as_slice(), record(&scope, 2).key().as_bytes().as_slice(), 128], |row| row.get(3)).unwrap().map(Result::unwrap).collect()
            } else {
                statement.query_map([record(&scope, 1).key().as_bytes().as_slice()], |row| row.get(3)).unwrap().map(Result::unwrap).collect()
            };
            assert!(plan.iter().any(|detail| detail.contains("SEARCH directory_roots USING PRIMARY KEY")), "{plan:?}");
            assert!(plan.iter().all(|detail| !detail.contains("TEMP B-TREE") && !detail.starts_with("SCAN")), "{plan:?}");
            eprintln!("LFCS indexed-query diagnostic: {plan:?}");
        }
        connection.execute_batch("BEGIN IMMEDIATE").unwrap(); // Acknowledged provider ownership establishes contention.
        assert!(matches!(
            session.adapter().append(&scope, &[record(&scope, 1)]),
            Err(ContentError::ProviderFailure {
                what: "construction scratch state"
            })
        ));
        assert!(matches!(
            session.take_failure(),
            Some(StorageError::OwnershipUnavailable)
        ));
        assert!(session.take_failure().is_none());
        assert_eq!(
            connection
                .query_row::<i64, _, _>("SELECT COUNT(*) FROM directory_roots", [], |row| row
                    .get(0))
                .unwrap(),
            0
        );
        connection.execute_batch("ROLLBACK").unwrap();
        drop(connection);
        session.release().unwrap();
    }

    #[test]
    fn changed_phase_and_borrowed_malformed_rows_fail_closed() {
        let directory = TempDir::new("construction_state_damaged");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        for damage in 0..3 {
            let mut session = authority.begin([0x62; 32], 3).unwrap();
            let scope = selected(&session);
            session
                .append(&scope, &[record(&scope, 1), record(&scope, 2)])
                .unwrap();
            let current = status(&authority, session.selection().token());
            let connection = external(&current.path, true);
            if damage == 0 {
                let mut foreign = input_scope(&scope);
                foreign[79] = 2;
                connection
                    .execute(
                        "UPDATE session_owner SET scope=?1 WHERE id=1",
                        [foreign.as_slice()],
                    )
                    .unwrap();
                drop(connection);
                assert!(matches!(
                    session.append(&scope, &[record(&scope, 3)]),
                    Err(StorageError::Integrity(
                        "construction scratch writable owner facts"
                    ))
                ));
                assert_eq!(
                    rows(&current.path),
                    2,
                    "bad phase refused before row append"
                );
            } else {
                drop(connection);
                let seal = session.seal(&scope).unwrap();
                let connection = external(&current.path, true);
                if damage == 1 {
                    connection
                        .execute_batch("PRAGMA ignore_check_constraints=ON;")
                        .unwrap();
                    connection
                        .execute(
                            "UPDATE directory_roots SET root=zeroblob(8193) WHERE ordinal=1",
                            [],
                        )
                        .unwrap();
                } else {
                    connection
                        .execute("DELETE FROM directory_roots WHERE ordinal=2", [])
                        .unwrap();
                }
                drop(connection);
                let error = session.page(&seal, None, PageLimit::default()).unwrap_err();
                if damage == 1 {
                    assert!(matches!(
                        error,
                        StorageError::Integrity("construction scratch BLOB width/type")
                    ));
                } else {
                    assert!(matches!(
                        error,
                        StorageError::Integrity("construction scratch sealed page EOF")
                    ));
                }
            }
            session.release().unwrap();
        }
    }

    #[test]
    fn explicit_identity_failure_and_restoration_never_authorize_retry() {
        // Failed owner teardown is isolated in a real child. The child reports
        // custody before exit; parent fixture cleanup follows owned process exit.
        // Kernel teardown is not product cleanup or credit refund evidence.
        let directory = TempDir::new("construction_state_release_child");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "unix::release_identity_child", "--nocapture"])
            .env("LAYERFS_SCRATCH_RELEASE_PROOF", directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failure: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("retained release proof complete"),
            "{stdout}"
        );
        let private: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(private.len(), 1);
        assert_eq!(
            std::fs::read_dir(&private[0]).unwrap().count(),
            1,
            "owner loss never guessed/unlinked failed state"
        );
    }

    #[test]
    fn release_identity_child() {
        let Some(base) = std::env::var_os("LAYERFS_SCRATCH_RELEASE_PROOF") else {
            return;
        };
        let authority = ScratchAuthority::new(Path::new(&base), 1).unwrap();
        let mut session = authority.begin([0x73; 32], 1).unwrap();
        let scope = selected(&session);
        session.append(&scope, &[record(&scope, 1)]).unwrap();
        let current = status(&authority, session.selection().token());
        let displaced = current.path.with_extension("displaced");
        std::fs::rename(&current.path, &displaced).unwrap();
        assert!(session.release().is_err());
        let failed = status(&authority, current.token);
        assert!(failed.release_attempted && !failed.quarantined);
        assert_eq!(failed.disposition, ScratchDisposition::ReleaseFailed);
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        std::fs::rename(&displaced, &current.path).unwrap();
        assert!(
            session.release().is_err(),
            "restored pathname cannot restart explicit release"
        );
        drop(session);
        let retained = status(&authority, current.token);
        assert!(retained.retained && retained.release_attempted);
        assert!(authority.release_retained(current.token).is_err());
        assert!(current.path.exists());
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        drop(authority); // No hidden Connection close, rollback, file close or unlink.
        assert!(current.path.exists());
        println!("retained release proof complete");
    }

    #[test]
    fn real_failed_commit_keeps_unknown_owner_credit_after_error_handoff() {
        let directory = TempDir::new("construction_state_unknown_child");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "unix::unknown_commit_child", "--nocapture"])
            .env("LAYERFS_SCRATCH_UNKNOWN_PROOF", directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failure: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("retained Unknown proof complete"),
            "{stdout}"
        );
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        let private: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(private.len(), 1);
        assert_eq!(std::fs::read_dir(&private[0]).unwrap().count(), 1);
    }

    struct ReadCoordinator {
        child: Option<std::process::Child>,
        control: Option<std::os::unix::net::UnixStream>,
        _directory: TempDir,
    }

    impl ReadCoordinator {
        fn held(path: &Path) -> Self {
            use std::io::Read;
            use std::os::unix::net::UnixListener;
            use std::process::Stdio;
            use std::time::{Duration, Instant};

            let directory = TempDir::new("lfcs");
            let socket = directory.join("r");
            let listener = UnixListener::bind(&socket).unwrap();
            listener.set_nonblocking(true).unwrap();
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "unix::unknown_shared_reader_child",
                    "--nocapture",
                ])
                .env("LAYERFS_SCRATCH_READ_DB", path)
                .env("LAYERFS_SCRATCH_READ_SOCKET", &socket)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            // Own the spawned process immediately, including every failed accept
            // or acknowledgement path. No unguarded launched reader can survive.
            let mut owner = Self {
                child: Some(child),
                control: None,
                _directory: directory,
            };
            let started = Instant::now();
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            owner.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                            "reader exited before IPC connection"
                        );
                        assert!(
                            started.elapsed() < Duration::from_secs(5),
                            "reader IPC connection bound"
                        );
                        std::thread::yield_now();
                    }
                    Err(error) => panic!("reader IPC accept: {error}"),
                }
            };
            owner.control = Some(stream);
            let control = owner.control.as_mut().unwrap();
            // Darwin accepts can inherit the listener's O_NONBLOCK. Timeouts do
            // not clear it; explicitly select blocking mode before the ACK read.
            control.set_nonblocking(false).unwrap();
            control
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            control
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut acknowledgement = [0];
            control.read_exact(&mut acknowledgement).unwrap();
            assert_eq!(
                acknowledgement,
                [1],
                "acknowledged BEGIN+SELECT establishes SHARED overlap"
            );
            owner
        }

        fn release_and_reap(&mut self) -> std::process::Output {
            use std::io::{Read, Write};
            let control = self.control.as_mut().unwrap();
            control.write_all(&[2]).unwrap();
            let mut acknowledgement = [0];
            control.read_exact(&mut acknowledgement).unwrap();
            assert_eq!(acknowledgement, [3], "known reader transaction release");
            // Keep the child inside the guard until a known reaped exit, even if
            // wait itself fails. Pipe reads happen only after all child writers exit.
            let status = self.child.as_mut().unwrap().wait().unwrap();
            let mut child = self.child.take().unwrap();
            // Child caches the acknowledged exit. This is a status read on the
            // moved binding, not a second blocking wait, new bound or retry.
            assert_eq!(child.wait().unwrap(), status);
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            child
                .stdout
                .take()
                .unwrap()
                .read_to_end(&mut stdout)
                .unwrap();
            child
                .stderr
                .take()
                .unwrap()
                .read_to_end(&mut stderr)
                .unwrap();
            std::process::Output {
                status,
                stdout,
                stderr,
            }
        }
    }

    impl Drop for ReadCoordinator {
        fn drop(&mut self) {
            if self.child.is_none() {
                return; // Explicit completion already reaped the known exit.
            }
            if let Some(control) = self.control.take() {
                if let Err(error) = control.shutdown(std::net::Shutdown::Both) {
                    eprintln!("LFCS reader control shutdown observation: {error}");
                }
            }
            let Some(mut child) = self.child.take() else {
                return;
            };
            let exited = match child.try_wait() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(error) => {
                    eprintln!("LFCS reader exit inspection failed: {error}");
                    false
                }
            };
            if !exited {
                if let Err(error) = child.kill() {
                    eprintln!("LFCS reader owned kill failed: {error}");
                }
                if let Err(error) = child.wait() {
                    eprintln!("LFCS reader owned reap failed: {error}");
                }
            }
        }
    }

    #[test]
    fn unknown_shared_reader_child() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;
        use std::time::Duration;

        let Some(path) = std::env::var_os("LAYERFS_SCRATCH_READ_DB") else {
            return;
        };
        let socket = std::env::var_os("LAYERFS_SCRATCH_READ_SOCKET").unwrap();
        let mut control = UnixStream::connect(Path::new(&socket)).unwrap();
        control
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        control
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let reader = external(Path::new(&path), false);
        let source: String = reader
            .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
            .unwrap();
        let binary = std::env::current_exe().unwrap();
        #[cfg(target_os = "macos")]
        {
            let links = std::process::Command::new("/usr/bin/otool")
                .arg("-L")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(links.status.success());
            let links = String::from_utf8(links.stdout).unwrap();
            let sqlite: Vec<_> = links
                .lines()
                .filter(|line| line.contains("libsqlite3."))
                .collect();
            assert_eq!(sqlite.len(), 1, "one selected SQLite library: {links}");
            eprintln!("LFCS reader selected library: {}", sqlite[0].trim());
        }
        eprintln!("LFCS reader actual runtime: binary={} version={} source={source}; same current executable/provider, READ_ONLY|NOFOLLOW, busy0", binary.display(), rusqlite::version());
        reader.execute_batch("BEGIN").unwrap();
        assert_eq!(
            reader
                .query_row::<i64, _, _>("SELECT records FROM session_owner WHERE id=1", [], |row| {
                    row.get(0)
                })
                .unwrap(),
            0
        );
        eprintln!("LFCS reader barrier: BEGIN and SELECT acknowledged, SHARED retained until control RELEASE");
        control.write_all(&[1]).unwrap();
        let mut release = [0];
        control.read_exact(&mut release).unwrap();
        assert_eq!(release, [2]);
        reader.execute_batch("ROLLBACK").unwrap();
        drop(reader);
        eprintln!("LFCS reader completion: ROLLBACK acknowledged and selected connection closed before release acknowledgement");
        control.write_all(&[3]).unwrap();
    }

    #[test]
    fn unknown_commit_child() {
        let Some(base) = std::env::var_os("LAYERFS_SCRATCH_UNKNOWN_PROOF") else {
            return;
        };
        let authority = ScratchAuthority::new(Path::new(&base), 1).unwrap();
        let mut session = authority.begin([0x84; 32], 1).unwrap();
        let scope = selected(&session);
        let current = status(&authority, session.selection().token());
        eprintln!("LFCS writer actual runtime: binary={} version={}; supplied ScratchAuthority/ScratchSession, no alternate provider", std::env::current_exe().unwrap().display(), rusqlite::version());
        let mut reader = ReadCoordinator::held(&current.path);
        // The same-process read-only Darwin topology fails BEGIN with IOERR_LOCK
        // /EBADF (preserved diagnostic). An independently acknowledged read-only
        // process holds SHARED without that mixed-access descriptor interaction.
        // BEGIN IMMEDIATE and INSERT run; real COMMIT cannot acquire EXCLUSIVE.
        assert!(matches!(
            session.adapter().append(&scope, &[record(&scope, 1)]),
            Err(ContentError::ProviderFailure {
                what: "construction scratch state"
            })
        ));
        let failure = session.take_failure().expect("original typed C2 Unknown");
        let StorageError::UnknownOutcome { original } = failure else {
            panic!("expected COMMIT Unknown, got {failure:?}");
        };
        let StorageError::Engine(rusqlite::Error::SqliteFailure(code, _)) = *original else {
            panic!("expected real SQLite COMMIT failure, got {original:?}");
        };
        assert_eq!(code.code, rusqlite::ErrorCode::DatabaseBusy);
        assert!(session.take_failure().is_none());
        let reader_output = reader.release_and_reap();
        assert!(
            reader_output.status.success(),
            "reader failure: stdout={} stderr={}",
            String::from_utf8_lossy(&reader_output.stdout),
            String::from_utf8_lossy(&reader_output.stderr)
        );
        eprintln!("{}", String::from_utf8_lossy(&reader_output.stderr));
        let failed = status(&authority, current.token);
        assert_eq!(failed.disposition, ScratchDisposition::Unknown);
        assert!(failed.quarantined && !failed.release_attempted);
        assert_eq!(failed.allocated_bytes, Some(CLASS));
        assert!(
            session.release().is_err(),
            "handing off error and ending reader never authorizes Unknown cleanup"
        );
        drop(session);
        let retained = status(&authority, current.token);
        assert!(retained.retained && retained.quarantined && !retained.release_attempted);
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        assert!(authority.release_retained(current.token).is_err());
        assert!(matches!(
            authority.begin([0x85; 32], 1),
            Err(StorageError::OwnershipUnavailable)
        ));
        let writer = Connection::open_with_flags(
            &current.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        writer.busy_timeout(std::time::Duration::ZERO).unwrap();
        drop(authority);
        // One external lock check on a pre-opened selected connection proves
        // final field destruction did not silently roll back/close the owned
        // Unknown transaction. No adoption, cleanup decision or product retry.
        let error = writer.execute_batch("BEGIN IMMEDIATE").unwrap_err();
        assert!(
            matches!(error, rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::DatabaseBusy)
        );
        drop(writer);
        assert!(current.path.exists());
        println!("retained Unknown proof complete");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[test]
fn unsupported_native_provider_refuses_before_scratch_effects() {
    let result =
        layerfs_storage::construction_state::ScratchAuthority::new(std::path::Path::new("."), 1);
    assert!(matches!(
        result,
        Err(layerfs_storage::StorageError::UnsupportedPolicy {
            field: "construction scratch native platform"
        })
    ));
}
