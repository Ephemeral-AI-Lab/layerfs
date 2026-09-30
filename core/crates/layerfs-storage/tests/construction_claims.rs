//! Phased LFCS real-provider bytes/custody/count proofs; no performance admission.

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_claim_oracle.rs"]
mod oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_claim_process.rs"]
mod process;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/construction_oracle.rs"]
mod roots_oracle;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix {
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    use layerfs_content::filesystem::state::{
        BindingClaimState, ClaimAdmission, ClaimCursor, ClaimKey, ClaimPageLimit, ClaimSeal,
        ConstructionScopes, PageLimit, StateRecord, StateScope, StateSelection,
    };
    use layerfs_content::{ContentError, ObjectId};
    use layerfs_storage::construction_state::{
        ScratchAuthority, ScratchDisposition, ScratchOwnerStatus, ScratchSession,
    };
    use layerfs_storage::StorageError;
    use rusqlite::{Connection, OpenFlags};

    use super::{oracle, process::HeldReader, roots_oracle, support::TempDir};

    const CLASS: u64 = 16 * 1024 * 1024;
    const ROWS: u64 = 65_536;

    fn scopes(session: &ScratchSession) -> ConstructionScopes {
        ConstructionScopes::new(session.selection().clone()).unwrap()
    }

    fn status(authority: &ScratchAuthority, token: u64) -> ScratchOwnerStatus {
        authority
            .status()
            .unwrap()
            .into_iter()
            .find(|status| status.token == token)
            .unwrap()
    }

    fn external(path: &Path) -> Connection {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        connection.busy_timeout(std::time::Duration::ZERO).unwrap();
        connection
            .execute_batch("PRAGMA synchronous=OFF; PRAGMA journal_mode=MEMORY;")
            .unwrap();
        connection
    }

    fn rows(path: &Path, table: &str) -> u64 {
        let query = match table {
            "claims" => "SELECT COUNT(*) FROM exclusive_claims",
            "roots" => "SELECT COUNT(*) FROM directory_roots",
            _ => unreachable!(),
        };
        nonnegative_scalar(&external(path), query)
    }

    fn nonnegative_scalar(connection: &Connection, query: &str) -> u64 {
        let value: i64 = connection.query_row(query, [], |row| row.get(0)).unwrap();
        u64::try_from(value).expect("nonnegative SQLite count")
    }

    fn declared_scope(scope: &StateScope) -> [u8; 81] {
        oracle::scope(
            *scope.selection().selector(),
            scope.selection().token(),
            *scope.selection().owner_binding().unwrap(),
            scope.phase(),
            scope.table().code(),
        )
    }

    fn key(scope: &StateScope, serial: u64) -> ClaimKey {
        ClaimKey::new(scope, serial).unwrap()
    }

    fn root(scope: &StateScope, serial: u64) -> StateRecord {
        StateRecord::directory_root(
            scope,
            serial,
            ObjectId::from_bytes(&roots_oracle::root(serial)).unwrap(),
        )
        .unwrap()
    }

    fn native_identity(metadata: &std::fs::Metadata) -> [u8; 24] {
        let mut bytes = [0; 24];
        bytes[..8].copy_from_slice(&metadata.dev().to_be_bytes());
        bytes[8..16].copy_from_slice(&metadata.ino().to_be_bytes());
        bytes[16..20].copy_from_slice(&metadata.uid().to_be_bytes());
        bytes[20..].copy_from_slice(&metadata.mode().to_be_bytes());
        bytes
    }

    fn assert_header(base: &Path, owner: &ScratchOwnerStatus, scope: &StateScope) {
        let header: Vec<u8> = external(&owner.path)
            .query_row("SELECT header FROM session_owner WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(header.len(), 192);
        assert_eq!(&header[..8], b"LFCSOWN2");
        assert_eq!(&header[8..10], &2u16.to_be_bytes());
        assert_eq!(&header[10..16], &[0; 6]);
        assert_eq!(&header[16..24], &owner.token.to_be_bytes());
        assert_eq!(&header[24..56], &owner.selector);
        assert_eq!(&header[56..88], scope.selection().owner_binding().unwrap());
        let parent = native_identity(&std::fs::metadata(base).unwrap());
        let directory = native_identity(&std::fs::metadata(owner.path.parent().unwrap()).unwrap());
        let file = native_identity(&std::fs::metadata(&owner.path).unwrap());
        assert_eq!(&header[120..144], &parent);
        assert_eq!(&header[144..168], &directory);
        assert_eq!(&header[168..192], &file);
        let mut digest = blake3::Hasher::new();
        digest.update(b"layerfs/construction-state/native/v1\0");
        digest.update(&header[88..120]);
        digest.update(&parent);
        digest.update(&directory);
        digest.update(&file);
        digest.update(&owner.selector);
        digest.update(&owner.token.to_be_bytes());
        assert_eq!(&header[56..88], digest.finalize().as_bytes());
    }

    #[test]
    fn maximum_claims_retire_then_maximum_roots_share_one_native_class() {
        let temp = TempDir::new("claims_maximum");
        let authority = ScratchAuthority::new(temp.path(), 2).unwrap();
        let mut session = authority.begin_phased([0x27; 32], ROWS, ROWS).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        assert_header(temp.path(), &owner, selected.claims());
        assert_eq!(session.profile().unwrap().version, 2);
        let capacity = session.claim_capacity(selected.claims()).unwrap();
        assert_eq!(
            (capacity.records(), capacity.encoded_bytes()),
            (ROWS, ROWS * 32)
        );
        let mut batch = Vec::with_capacity(128);
        for ordinal in 0..ROWS {
            // Odd multiplication is a bijection modulo2^16; input batches are
            // globally unordered while the independent final oracle is1..N.
            batch.push(key(selected.claims(), (ordinal * 32_771 % ROWS) + 1));
            if batch.len() == 128 {
                assert_eq!(
                    session.claim_batch(selected.claims(), &batch).unwrap(),
                    ClaimAdmission::Fresh
                );
                batch.clear();
            }
        }
        assert!(batch.is_empty());
        assert_eq!(rows(&owner.path, "claims"), ROWS);
        assert_eq!(rows(&owner.path, "roots"), 0);
        let seal = session.claim_seal(selected.claims()).unwrap();
        let expected_scope = declared_scope(selected.claims());
        let mut expected = oracle::Transcript::new(expected_scope);
        for serial in 1..=ROWS {
            expected.append(serial);
        }
        assert_eq!(seal.encode(), expected.seal());
        let mut after = None;
        let mut serial = 1;
        for _ in 0..ROWS / 128 {
            let page = session
                .claim_page(&seal, after, ClaimPageLimit::default())
                .unwrap();
            assert_eq!(page.records().len(), 128);
            for record in page.records() {
                assert_eq!(record.encode(), oracle::record(&expected_scope, serial));
                serial += 1;
            }
            assert_eq!(
                page.encode_header(),
                oracle::page_header(
                    &expected.seal(),
                    Some(oracle::key(&expected_scope, serial - 1)),
                    128,
                    serial == ROWS + 1
                )
            );
            after = page.last();
        }
        assert_eq!(serial, ROWS + 1);
        let terminal = session
            .claim_page(&seal, after, ClaimPageLimit::new(128, 163).unwrap())
            .unwrap();
        assert!(terminal.eof() && terminal.records().is_empty());
        let claim_pages = nonnegative_scalar(&external(&owner.path), "PRAGMA page_count");
        session.claim_retire(&seal).unwrap();
        assert_eq!(rows(&owner.path, "claims"), 0);
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        let connection = external(&owner.path);
        let retirement: (i64, i64, i64) = connection
            .query_row(
                "SELECT claim_state,claim_records,claim_remaining FROM session_owner",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(retirement, (3, ROWS as i64, 0));
        let free_pages = nonnegative_scalar(&connection, "PRAGMA freelist_count");
        assert!(
            free_pages > 0,
            "retired pages remain owned and available for reuse"
        );
        drop(connection);
        let capacity = session.capacity(selected.roots()).unwrap();
        assert_eq!(
            (capacity.records(), capacity.encoded_bytes()),
            (ROWS, ROWS * 63)
        );
        let root_scope = roots_oracle::scope(
            *session.selection().selector(),
            session.selection().token(),
            *session.selection().owner_binding().unwrap(),
            2,
        );
        let mut roots_expected = roots_oracle::Transcript::new(root_scope);
        let mut root_batch = Vec::with_capacity(128);
        for serial in 1..=ROWS {
            roots_expected.append(serial);
            root_batch.push(root(selected.roots(), serial));
            if root_batch.len() == 128 {
                session.append(selected.roots(), &root_batch).unwrap();
                root_batch.clear();
            }
        }
        let root_seal = session.seal(selected.roots()).unwrap();
        assert_eq!(root_seal.encode(), roots_expected.seal());
        let page = session
            .page(&root_seal, None, PageLimit::default())
            .unwrap();
        assert_eq!(
            page.encode_header(),
            roots_oracle::page_header(
                &roots_expected.seal(),
                Some(roots_oracle::key(&root_scope, 128)),
                128,
                false
            )
        );
        assert_eq!(page.records()[0].root().as_bytes(), &roots_oracle::root(1));
        assert_eq!(rows(&owner.path, "roots"), ROWS);
        let final_owner = status(&authority, owner.token);
        assert_eq!(final_owner.file, owner.file);
        assert_eq!(final_owner.allocated_bytes, Some(CLASS));
        let final_pages = nonnegative_scalar(&external(&owner.path), "PRAGMA page_count");
        assert!(final_pages <= 4096 && claim_pages <= 4096);
        eprintln!("LFCS2 correctness/resource diagnostic: claims={ROWS}/{}frameB, roots={ROWS}/{}frameB, claim_pages={claim_pages}, retired_free_pages={free_pages}, final_pages={final_pages}, same_file={:?}, allocated={:?}, retained_class={CLASS}; claim native_element={}, wave128={}B; no speed/global heap/RSS/protected-progress claim", ROWS * 32, ROWS * 63, owner.file, final_owner.allocated_bytes, std::mem::size_of::<ClaimKey>(), 128 * std::mem::size_of::<ClaimKey>());
        session.complete_phase(selected.roots()).unwrap();
        session.release().unwrap();
        session.release().unwrap();
        assert!(!owner.path.exists());
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }

    #[test]
    fn empty_and_header_only_sealed_pages_complete_without_allocating_records() {
        let temp = TempDir::new("claims_empty");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin_phased([0x28; 32], 0, 0).unwrap();
        let selected = scopes(&session);
        let seal = session.claim_seal(selected.claims()).unwrap();
        assert_eq!(
            seal.encode(),
            oracle::Transcript::new(declared_scope(selected.claims())).seal()
        );
        let page = session
            .claim_page(&seal, None, ClaimPageLimit::new(128, 163).unwrap())
            .unwrap();
        assert!(page.eof() && page.records().is_empty());
        assert_eq!(page.into_records().capacity(), 0);
        session.claim_retire(&seal).unwrap();
        assert_eq!(session.capacity(selected.roots()).unwrap().records(), 0);
        session.seal(selected.roots()).unwrap();
        session.release().unwrap();
    }

    #[test]
    fn same_and_cross_window_duplicates_acknowledge_no_current_rows_and_terminalize() {
        for same_window in [true, false] {
            let temp = TempDir::new("claims_duplicate");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let mut session = authority.begin_phased([0x29; 32], 1, 256).unwrap();
            let selected = scopes(&session);
            let owner = status(&authority, session.selection().token());
            let accepted: Vec<_> = (1..=128)
                .map(|serial| key(selected.claims(), serial))
                .collect();
            session.claim_batch(selected.claims(), &accepted).unwrap();
            assert!(session
                .claim_present(selected.claims(), accepted[0])
                .unwrap());
            assert!(!session
                .claim_present(selected.claims(), key(selected.claims(), 129))
                .unwrap());
            let refused = if same_window {
                vec![key(selected.claims(), 129), key(selected.claims(), 129)]
            } else {
                vec![key(selected.claims(), 129), accepted[0]]
            };
            assert_eq!(
                session.claim_batch(selected.claims(), &refused).unwrap(),
                ClaimAdmission::Duplicate
            );
            assert_eq!(rows(&owner.path, "claims"), 128);
            assert_eq!(rows(&owner.path, "roots"), 0);
            assert_eq!(
                status(&authority, owner.token).disposition,
                ScratchDisposition::Failed
            );
            assert!(session.claim_seal(selected.claims()).is_err());
            assert!(session
                .claim_present(selected.claims(), accepted[0])
                .is_err());
            assert!(session.capacity(selected.roots()).is_err());
            session.release().unwrap();
            assert_eq!(authority.reserved_bytes().unwrap(), 0);
        }
    }

    #[test]
    fn exact_abandon_is_sticky_metadata_and_foreign_abandon_changes_no_owner() {
        let temp = TempDir::new("claims_abandon");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin_phased([0x2a; 32], 1, 2).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        let mut foreign = StateSelection::issue([0x2a; 32]).unwrap();
        foreign
            .bind_owner(*session.selection().owner_binding().unwrap())
            .unwrap();
        let foreign = ConstructionScopes::new(foreign).unwrap();
        assert!(matches!(
            session.adapter().claim_abandon(foreign.claims()),
            Err(ContentError::InvalidOrderingRecord("claim abandon scope"))
        ));
        assert!(session.take_failure().is_none());
        session
            .claim_batch(selected.claims(), &[key(selected.claims(), 1)])
            .unwrap();
        session.claim_abandon(selected.claims()).unwrap();
        session.claim_abandon(selected.claims()).unwrap();
        assert_eq!(rows(&owner.path, "claims"), 1);
        assert_eq!(
            status(&authority, owner.token).disposition,
            ScratchDisposition::Failed
        );
        assert!(session
            .claim_batch(selected.claims(), &[key(selected.claims(), 2)])
            .is_err());
        assert!(session.capacity(selected.roots()).is_err());
        session.release().unwrap();
    }

    #[test]
    fn existing_duplicate_does_not_mask_a_later_selected_corrupt_class() {
        let temp = TempDir::new("claims_duplicate_corrupt_tail");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin_phased([0x37; 32], 1, 3).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        let first = key(selected.claims(), 1);
        let later = key(selected.claims(), 2);
        session
            .claim_batch(selected.claims(), &[first, later])
            .unwrap();
        let connection = external(&owner.path);
        connection
            .execute_batch("PRAGMA ignore_check_constraints=ON")
            .unwrap();
        connection
            .execute(
                "UPDATE exclusive_claims SET class=x'02' WHERE key=?1",
                [later.as_bytes().as_slice()],
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            session.adapter().claim_batch(
                selected.claims(),
                &[first, later, key(selected.claims(), 3)]
            ),
            Err(ContentError::ProviderFailure {
                what: "construction scratch state"
            })
        ));
        assert!(matches!(
            session.take_failure(),
            Some(StorageError::Integrity(
                "construction scratch exclusive class"
            ))
        ));
        assert_eq!(rows(&owner.path, "claims"), 2);
        assert_eq!(rows(&owner.path, "roots"), 0);
        assert!(
            !session.is_quarantined(),
            "known rollback retains precise corruption failure"
        );
        assert!(session.capacity(selected.roots()).is_err());
        session.release().unwrap();
        assert!(!owner.path.exists());
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }

    #[test]
    fn phased_admission_and_per_batch_declaration_refuse_before_effects() {
        let temp = TempDir::new("claims_admission");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        for (directories, bindings) in [(ROWS + 1, 0), (0, ROWS + 1), (u64::MAX, u64::MAX)] {
            assert!(matches!(
                authority.begin_phased([0x2b; 32], directories, bindings),
                Err(StorageError::CapacityExceeded { .. })
            ));
            assert!(authority.status().unwrap().is_empty());
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        }
        let mut session = authority.begin_phased([0x2b; 32], 1, 1).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        assert!(matches!(
            session.claim_batch(
                selected.claims(),
                &[key(selected.claims(), 1), key(selected.claims(), 2)]
            ),
            Err(StorageError::CapacityExceeded {
                what: "construction scratch claim rows",
                limit: 1,
                actual: 2
            })
        ));
        assert_eq!(rows(&owner.path, "claims"), 0);
        assert_eq!(rows(&owner.path, "roots"), 0);
        session.release().unwrap();
        let mut session = authority.begin_phased([0x2c; 32], 0, 129).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        let keys: Vec<_> = (1..=129)
            .map(|serial| key(selected.claims(), serial))
            .collect();
        assert!(matches!(
            session.claim_batch(selected.claims(), &keys),
            Err(StorageError::CapacityExceeded {
                limit: 128,
                actual: 129,
                ..
            })
        ));
        assert_eq!(rows(&owner.path, "claims"), 0);
        session.release().unwrap();
    }

    #[test]
    fn root_capacity_and_effects_are_denied_until_exact_claim_retirement() {
        for sealed in [false, true] {
            let temp = TempDir::new("claims_root_fence");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let mut session = authority.begin_phased([0x2d; 32], 1, 1).unwrap();
            let selected = scopes(&session);
            let owner = status(&authority, session.selection().token());
            session
                .claim_batch(selected.claims(), &[key(selected.claims(), 1)])
                .unwrap();
            if sealed {
                session.claim_seal(selected.claims()).unwrap();
            }
            assert!(session
                .append(selected.roots(), &[root(selected.roots(), 1)])
                .is_err());
            assert_eq!(rows(&owner.path, "roots"), 0);
            assert!(session.capacity(selected.roots()).is_err());
            session.release().unwrap();
        }
    }

    #[test]
    fn claimed_pages_middle_maximum_and_small_byte_bounds_use_exact_primary_keys() {
        let temp = TempDir::new("claims_pages");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin_phased([0x2e; 32], 0, 4).unwrap();
        let selected = scopes(&session);
        session
            .claim_batch(
                selected.claims(),
                &[
                    key(selected.claims(), 90),
                    key(selected.claims(), 7),
                    key(selected.claims(), 51),
                    key(selected.claims(), 23),
                ],
            )
            .unwrap();
        let seal = session.claim_seal(selected.claims()).unwrap();
        let first = session
            .claim_page(&seal, None, ClaimPageLimit::new(128, 195).unwrap())
            .unwrap();
        assert_eq!(first.records()[0].key().serial(), 7);
        assert!(!first.eof());
        let middle = session
            .claim_page(
                &seal,
                Some(key(selected.claims(), 8)),
                ClaimPageLimit::default(),
            )
            .unwrap();
        assert_eq!(
            middle
                .records()
                .iter()
                .map(|record| record.key().serial())
                .collect::<Vec<_>>(),
            vec![23, 51, 90]
        );
        assert!(middle.eof());
        let empty = session
            .claim_page(
                &seal,
                Some(key(selected.claims(), 90)),
                ClaimPageLimit::new(1, 163).unwrap(),
            )
            .unwrap();
        assert!(empty.eof() && empty.records().is_empty());
        let mut port = session.adapter();
        let mut cursor = ClaimCursor::new(&mut port, seal.clone()).unwrap();
        let mut count = 0;
        while let Some(page) = cursor
            .next_page(ClaimPageLimit::new(2, 227).unwrap())
            .unwrap()
        {
            count += page.records().len();
        }
        assert_eq!(count, 4);
        drop(cursor);
        session.claim_retire(&seal).unwrap();
        session.release().unwrap();
    }

    #[test]
    fn short_header_only_nonempty_refuses_without_a_page_or_mutation() {
        let temp = TempDir::new("claims_short_page");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin_phased([0x2f; 32], 0, 1).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        session
            .claim_batch(selected.claims(), &[key(selected.claims(), 1)])
            .unwrap();
        let seal = session.claim_seal(selected.claims()).unwrap();
        assert!(matches!(
            session.claim_page(&seal, None, ClaimPageLimit::new(1, 163).unwrap()),
            Err(StorageError::Content(
                ContentError::BoundedCapacityExceeded { .. }
            ))
        ));
        assert_eq!(rows(&owner.path, "claims"), 1);
        session.release().unwrap();
    }

    #[test]
    fn foreign_keys_wrong_owner_and_forged_seals_never_change_rows() {
        for forged in [false, true] {
            let temp = TempDir::new("claims_foreign");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let mut session = authority.begin_phased([0x30; 32], 1, 1).unwrap();
            let selected = scopes(&session);
            let owner = status(&authority, session.selection().token());
            if forged {
                session
                    .claim_batch(selected.claims(), &[key(selected.claims(), 1)])
                    .unwrap();
                let seal = session.claim_seal(selected.claims()).unwrap();
                let mut bytes = seal.encode();
                bytes[129] ^= 1;
                let altered = ClaimSeal::decode(selected.claims(), &bytes).unwrap();
                assert!(session.claim_retire(&altered).is_err());
                assert_eq!(rows(&owner.path, "claims"), 1);
            } else {
                let mut foreign = StateSelection::issue([0x30; 32]).unwrap();
                foreign.bind_owner([0x44; 32]).unwrap();
                let foreign = ConstructionScopes::new(foreign).unwrap();
                assert!(session
                    .claim_batch(selected.claims(), &[key(foreign.claims(), 1)])
                    .is_err());
                assert_eq!(rows(&owner.path, "claims"), 0);
            }
            assert_eq!(rows(&owner.path, "roots"), 0);
            session.release().unwrap();
        }
    }

    #[test]
    fn real_malformed_class_and_key_refuse_seal_with_known_cleanup() {
        for malformed_class in [true, false] {
            let temp = TempDir::new("claims_corrupt");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let mut session = authority.begin_phased([0x31; 32], 1, 1).unwrap();
            let selected = scopes(&session);
            let owner = status(&authority, session.selection().token());
            session
                .claim_batch(selected.claims(), &[key(selected.claims(), 1)])
                .unwrap();
            let connection = external(&owner.path);
            connection
                .execute_batch("PRAGMA ignore_check_constraints=ON")
                .unwrap();
            connection
                .execute_batch(if malformed_class {
                    "UPDATE exclusive_claims SET class=x'02'"
                } else {
                    "UPDATE exclusive_claims SET key=x'00'"
                })
                .unwrap();
            drop(connection);
            assert!(session.claim_seal(selected.claims()).is_err());
            assert!(!session.is_quarantined());
            assert!(session.capacity(selected.roots()).is_err());
            assert_eq!(rows(&owner.path, "roots"), 0);
            session.release().unwrap();
            assert!(!owner.path.exists());
        }
    }

    #[test]
    fn known_partial_retirement_keeps_remaining_rows_and_blocks_roots() {
        let temp = TempDir::new("claims_partial_retire");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin_phased([0x32; 32], 1, 256).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        for start in [1, 129] {
            let keys: Vec<_> = (start..start + 128)
                .map(|serial| key(selected.claims(), serial))
                .collect();
            session.claim_batch(selected.claims(), &keys).unwrap();
        }
        let seal = session.claim_seal(selected.claims()).unwrap();
        {
            let mut port = session.adapter();
            let mut cursor = ClaimCursor::new(&mut port, seal.clone()).unwrap();
            while cursor
                .next_page(ClaimPageLimit::default())
                .unwrap()
                .is_some()
            {}
        }
        let connection = external(&owner.path);
        connection
            .execute_batch("PRAGMA ignore_check_constraints=ON")
            .unwrap();
        connection
            .execute(
                "UPDATE exclusive_claims SET class=x'02' WHERE key=?1",
                [key(selected.claims(), 151).as_bytes().as_slice()],
            )
            .unwrap();
        drop(connection);
        assert!(session.claim_retire(&seal).is_err());
        assert!(!session.is_quarantined());
        let connection = external(&owner.path);
        let state: (i64, i64) = connection
            .query_row(
                "SELECT claim_state,claim_remaining FROM session_owner",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, (2, 128));
        assert_eq!(
            connection
                .query_row::<i64, _, _>("SELECT COUNT(*) FROM exclusive_claims", [], |row| row
                    .get(0))
                .unwrap(),
            128
        );
        drop(connection);
        assert!(session.capacity(selected.roots()).is_err());
        assert!(session
            .append(selected.roots(), &[root(selected.roots(), 1)])
            .is_err());
        assert_eq!(rows(&owner.path, "roots"), 0);
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        session.release().unwrap();
    }

    #[test]
    fn sealed_surplus_and_missing_rows_refuse_without_unlocking_roots() {
        for surplus in [true, false] {
            let temp = TempDir::new("claims_sealed_population");
            let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
            let mut session = authority.begin_phased([0x35; 32], 1, 3).unwrap();
            let selected = scopes(&session);
            let owner = status(&authority, session.selection().token());
            session
                .claim_batch(
                    selected.claims(),
                    &[key(selected.claims(), 1), key(selected.claims(), 2)],
                )
                .unwrap();
            let seal = session.claim_seal(selected.claims()).unwrap();
            let connection = external(&owner.path);
            if surplus {
                connection
                    .execute(
                        "INSERT INTO exclusive_claims VALUES(?1,x'01')",
                        [key(selected.claims(), 3).as_bytes().as_slice()],
                    )
                    .unwrap();
            } else {
                connection
                    .execute(
                        "DELETE FROM exclusive_claims WHERE key=?1",
                        [key(selected.claims(), 1).as_bytes().as_slice()],
                    )
                    .unwrap();
            }
            drop(connection);
            if surplus {
                assert!(session
                    .claim_page(&seal, None, ClaimPageLimit::default())
                    .is_err());
            } else {
                let mut port = session.adapter();
                let mut cursor = ClaimCursor::new(&mut port, seal.clone()).unwrap();
                assert!(
                    cursor.next_page(ClaimPageLimit::default()).is_err(),
                    "whole cursor refuses forged terminal count"
                );
                drop(cursor);
                assert!(session.claim_retire(&seal).is_err());
            }
            assert!(session.capacity(selected.roots()).is_err());
            assert_eq!(rows(&owner.path, "roots"), 0);
            session.release().unwrap();
        }
    }

    #[test]
    fn roots_keep_the_exact_declared_limit_after_retirement() {
        let temp = TempDir::new("claims_root_limit");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin_phased([0x36; 32], 1, 0).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        let seal = session.claim_seal(selected.claims()).unwrap();
        session.claim_retire(&seal).unwrap();
        assert_eq!(session.capacity(selected.roots()).unwrap().records(), 1);
        assert!(matches!(
            session.append(
                selected.roots(),
                &[root(selected.roots(), 1), root(selected.roots(), 2)]
            ),
            Err(StorageError::CapacityExceeded {
                what: "construction scratch rows",
                limit: 1,
                actual: 2
            })
        ));
        assert_eq!(rows(&owner.path, "roots"), 0);
        session.release().unwrap();
    }

    #[test]
    fn closed_primary_key_queries_have_no_temp_sort_or_prefix_count() {
        let temp = TempDir::new("claims_plans");
        let authority = ScratchAuthority::new(temp.path(), 1).unwrap();
        let mut session = authority.begin_phased([0x33; 32], 0, 2).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        session
            .claim_batch(
                selected.claims(),
                &[key(selected.claims(), 2), key(selected.claims(), 1)],
            )
            .unwrap();
        let connection = external(&owner.path);
        let mut point = connection
            .prepare("EXPLAIN QUERY PLAN SELECT class FROM exclusive_claims WHERE key=?1")
            .unwrap();
        let plan: Vec<String> = point
            .query_map([key(selected.claims(), 1).as_bytes().as_slice()], |row| {
                row.get(3)
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            plan.iter()
                .any(|line| line.contains("SEARCH") && line.contains("PRIMARY KEY")),
            "{plan:?}"
        );
        drop(point);
        let mut page = connection.prepare("EXPLAIN QUERY PLAN SELECT key,class FROM exclusive_claims WHERE key>?1 ORDER BY key LIMIT ?2").unwrap();
        let plan: Vec<String> = page
            .query_map(
                rusqlite::params![key(selected.claims(), 1).as_bytes().as_slice(), 128],
                |row| row.get(3),
            )
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            plan.iter()
                .any(|line| line.contains("SEARCH") && line.contains("PRIMARY KEY")),
            "{plan:?}"
        );
        assert!(
            !plan.iter().any(|line| line.contains("TEMP B-TREE")),
            "{plan:?}"
        );
        drop(page);
        let mut maximum = connection
            .prepare(
                "EXPLAIN QUERY PLAN SELECT key FROM exclusive_claims ORDER BY key DESC LIMIT 1",
            )
            .unwrap();
        let plan: Vec<String> = maximum
            .query_map([], |row| row.get(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            !plan.iter().any(|line| line.contains("TEMP B-TREE")),
            "{plan:?}"
        );
        drop(maximum);
        drop(connection);
        session.release().unwrap();
    }

    #[test]
    fn real_shared_reader_commit_unknown_retains_exact_capsule_and_class() {
        for phase in ["batch", "seal", "retire"] {
            let temp = TempDir::new("claims_unknown_process");
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "unix::unknown_writer_child", "--nocapture"])
                .env("LAYERFS_CLAIMS_UNKNOWN_BASE", temp.path())
                .env("LAYERFS_CLAIMS_UNKNOWN_PHASE", phase)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "phase={phase} stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout)
                .contains(&format!("LFCS2 exact Unknown retained: {phase}")));
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            let private: Vec<_> = std::fs::read_dir(temp.path())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            assert_eq!(private.len(), 1);
            assert_eq!(
                std::fs::read_dir(&private[0]).unwrap().count(),
                1,
                "{phase} Unknown source was never unlinked"
            );
        }
    }

    #[test]
    fn unknown_writer_child() {
        let Some(base) = std::env::var_os("LAYERFS_CLAIMS_UNKNOWN_BASE") else {
            return;
        };
        let authority = ScratchAuthority::new(Path::new(&base), 1).unwrap();
        let mut session = authority.begin_phased([0x34; 32], 1, 2).unwrap();
        let selected = scopes(&session);
        let owner = status(&authority, session.selection().token());
        let phase = std::env::var("LAYERFS_CLAIMS_UNKNOWN_PHASE").unwrap();
        let keys = [key(selected.claims(), 2), key(selected.claims(), 1)];
        let fixture_records = match phase.as_str() {
            "batch" => 0,
            "seal" | "retire" => {
                assert_eq!(
                    session.claim_batch(selected.claims(), &keys).unwrap(),
                    ClaimAdmission::Fresh
                );
                2
            }
            _ => panic!("unregistered owning phase {phase}"),
        };
        let known_seal = if phase == "retire" {
            Some(session.claim_seal(selected.claims()).unwrap())
        } else {
            None
        };
        assert_eq!(rows(&owner.path, "claims"), fixture_records);
        assert_eq!(rows(&owner.path, "roots"), 0);
        let mut reader = HeldReader::new(&owner.path, fixture_records);
        let outcome = match phase.as_str() {
            "batch" => session
                .adapter()
                .claim_batch(selected.claims(), &keys)
                .map(|_| ()),
            "seal" => session.adapter().claim_seal(selected.claims()).map(|_| ()),
            "retire" => session.adapter().claim_retire(known_seal.as_ref().unwrap()),
            _ => unreachable!(),
        };
        assert!(matches!(
            outcome,
            Err(ContentError::ProviderFailure {
                what: "construction scratch state"
            })
        ));
        let original = session.take_failure().expect("original typed C2 failure");
        let StorageError::UnknownOutcome { original } = original else {
            panic!("expected COMMIT Unknown: {original:?}")
        };
        let StorageError::Engine(rusqlite::Error::SqliteFailure(code, _)) = *original else {
            panic!("real SQLite failure expected: {original:?}")
        };
        assert_eq!(code.code, rusqlite::ErrorCode::DatabaseBusy);
        assert!(session.is_quarantined());
        let before = status(&authority, owner.token);
        let before_metadata = std::fs::metadata(&owner.path).unwrap();
        session.claim_abandon(selected.claims()).unwrap();
        session.claim_abandon(selected.claims()).unwrap();
        assert!(
            session.take_failure().is_none(),
            "abandon leaves original handoff intact"
        );
        let quarantined = status(&authority, owner.token);
        assert_eq!(quarantined.disposition, ScratchDisposition::Unknown);
        let failure = quarantined.failure.as_ref().unwrap();
        let attempt = match phase.as_str() {
            "batch" => "claim attempt Batch: before=0, proposed=2, serials=[2, 1]",
            "seal" => "claim attempt Seal: before=2, proposed=2, serials=[]",
            "retire" => "claim attempt Retire: before=2, proposed=0, serials=[1, 2]",
            _ => unreachable!(),
        };
        assert!(
            failure.contains(attempt),
            "{phase} exact capsule: {failure}"
        );
        if phase == "seal" {
            let mut independent = oracle::Transcript::new(declared_scope(selected.claims()));
            independent.append(1);
            independent.append(2);
            let expected = independent.seal();
            assert!(failure.contains("proposed claim seal: records=2, bytes=64"));
            assert!(failure.contains(&format!("digest={:?}", &expected[98..130])));
            assert!(failure.contains("maximum=Some(2)"));
        }
        assert_eq!(quarantined.failure, before.failure);
        assert_eq!(quarantined.file, owner.file);
        assert_eq!(quarantined.path, owner.path);
        assert_eq!(quarantined.allocated_bytes, Some(CLASS));
        assert!(!quarantined.release_attempted);
        let after_metadata = std::fs::metadata(&owner.path).unwrap();
        assert_eq!(
            native_identity(&before_metadata),
            native_identity(&after_metadata)
        );
        assert_eq!(before_metadata.len(), after_metadata.len());
        assert_eq!(before_metadata.blocks(), after_metadata.blocks());
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        let output = reader.release_and_reap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(session.capacity(selected.roots()).is_err());
        assert!(session
            .append(selected.roots(), &[root(selected.roots(), 1)])
            .is_err());
        assert!(session.release().is_err());
        drop(session);
        let retained = status(&authority, owner.token);
        assert_eq!(retained.disposition, ScratchDisposition::Unknown);
        assert!(retained.retained && retained.quarantined);
        assert_eq!(authority.reserved_bytes().unwrap(), CLASS);
        assert!(authority.release_retained(owner.token).is_err());
        assert!(owner.path.exists());
        assert_eq!(retained.file, owner.file);
        assert_eq!(retained.allocated_bytes, Some(CLASS));
        assert!(retained.failure.as_ref().unwrap().contains(attempt));
        println!("LFCS2 exact Unknown retained: {phase}");
    }

    #[test]
    fn shared_reader_child() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;
        use std::time::Duration;
        let Some(path) = std::env::var_os("LAYERFS_CLAIMS_READ_DB") else {
            return;
        };
        let socket = std::env::var_os("LAYERFS_CLAIMS_READ_SOCKET").unwrap();
        let expected_records: i64 = std::env::var("LAYERFS_CLAIMS_EXPECTED_RECORDS")
            .unwrap()
            .parse()
            .unwrap();
        let mut control = UnixStream::connect(Path::new(&socket)).unwrap();
        control
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        control
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let reader = Connection::open_with_flags(
            Path::new(&path),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        reader.busy_timeout(Duration::ZERO).unwrap();
        let source: String = reader
            .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
            .unwrap();
        eprintln!("LFCS2 actual reader provider: version={} source={source}; current executable={}, READ_ONLY|NOFOLLOW, busy0", rusqlite::version(), std::env::current_exe().unwrap().display());
        reader.execute_batch("BEGIN").unwrap();
        assert_eq!(
            reader
                .query_row::<i64, _, _>("SELECT claim_records FROM session_owner", [], |row| row
                    .get(0))
                .unwrap(),
            expected_records
        );
        eprintln!("LFCS2 SHARED barrier acknowledged by actual BEGIN+SELECT records={expected_records} before writer");
        control.write_all(&[1]).unwrap();
        let mut command = [0];
        control.read_exact(&mut command).unwrap();
        assert_eq!(command, [2]);
        reader.execute_batch("ROLLBACK").unwrap();
        drop(reader);
        control.write_all(&[3]).unwrap();
    }
}
