//! Public typed state framing, exact seals/cursors and pre-effect refusal.
//! This target uses resident logical authority; real native/SQL proof is in C2.
use layerfs_content::{
    filesystem::{
        build_filesystem_with_state,
        state::{
            IndexedState, PageLimit, ResidentState, StateCapacity, StateCursor, StateKey,
            StateLedger, StatePage, StateRecord, StateScope, StateSeal, StateSelection, StateTable,
            DIRECTORY_ROOT_RECORD_BYTES, STATE_APPEND_HEADER_BYTES, STATE_PAGE_HEADER_BYTES,
            STATE_SEAL_BYTES,
        },
        DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources, InodeUpdate,
    },
    inode_leaf::{InodeKind, InodeValue},
    ContentError, ContentResult, ObjectId, ObjectRole,
};

#[path = "support/filesystem.rs"]
mod support;

fn new_scope(selector: u8) -> StateScope {
    let mut selection = StateSelection::issue([selector; 32]).unwrap();
    let mut binding = blake3::Hasher::new();
    binding.update(b"layerfs/tests/indexed-state-owner/v1\0");
    binding.update(&selection.token().to_be_bytes());
    selection
        .bind_owner(*binding.finalize().as_bytes())
        .unwrap();
    StateScope::new(selection, 7, StateTable::DirectoryRoots).unwrap()
}

fn records(scope: &StateScope, first: u64, count: usize) -> Vec<StateRecord> {
    (first..first + count as u64)
        .map(|serial| {
            StateRecord::directory_root(scope, serial, ObjectId::for_bytes(&serial.to_be_bytes()))
                .unwrap()
        })
        .collect()
}

/// Independent transcription of all original input fields, before provider output.
fn digest(scope: &StateScope, first: u64, count: usize) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(b"layerfs/indexed-state/v1\0");
    hash.update(scope.selection().selector());
    hash.update(&scope.selection().token().to_be_bytes());
    hash.update(scope.selection().owner_binding().unwrap());
    hash.update(&scope.phase().to_be_bytes());
    hash.update(&[1]);
    for serial in first..first + count as u64 {
        hash.update(&25u16.to_be_bytes());
        hash.update(&scope.selection().token().to_be_bytes());
        hash.update(&scope.phase().to_be_bytes());
        hash.update(&[1]);
        hash.update(&serial.to_be_bytes());
        hash.update(&32u32.to_be_bytes());
        hash.update(ObjectId::for_bytes(&serial.to_be_bytes()).as_bytes());
    }
    hash.update(&(count as u64).to_be_bytes());
    hash.update(&(count as u64 * 63).to_be_bytes());
    *hash.finalize().as_bytes()
}

#[test]
fn issued_identity_full_selector_and_one_time_binding_are_exact() {
    assert!(StateSelection::issue([0; 32]).is_err());
    let unbound = StateSelection::issue([3; 32]).unwrap();
    assert!(StateScope::new(unbound, 1, StateTable::DirectoryRoots).is_err());
    let mut selection = StateSelection::issue([3; 32]).unwrap();
    selection.bind_owner([4; 32]).unwrap();
    assert!(selection.bind_owner([5; 32]).is_err());
    let first = StateScope::new(selection.clone(), 1, StateTable::DirectoryRoots).unwrap();
    assert_eq!(first.selection(), &selection);
    assert_eq!(&first.as_bytes()[..32], &[3; 32]);
    assert_eq!(&first.as_bytes()[40..72], &[4; 32]);
    assert_eq!(first.as_bytes()[80], 1);
    assert!(StateScope::new(selection.clone(), 0, StateTable::DirectoryRoots).is_err());
    let other = new_scope(3);
    assert_ne!(first.selection(), other.selection());
    assert!(other.selection().token() > first.selection().token());
    let mut shared = StateSelection::issue([9; 32]).unwrap();
    let held = shared.clone();
    assert!(shared.bind_owner([10; 32]).is_err());
    drop(held);
    assert!(shared.bind_owner([10; 32]).is_err());
}

#[test]
fn fixed_records_headers_and_seal_digest_match_independent_input_transcription() {
    assert_eq!(std::mem::size_of::<StateRecord>(), 57);
    let scope = new_scope(11);
    let rows = records(&scope, 8, 3);
    let mut ledger = StateLedger::new(scope.clone());
    let empty = ledger.seal();
    assert_eq!(*empty.digest(), digest(&scope, 0, 0));
    let append = ledger.append_header(&rows).unwrap();
    assert_eq!(append.len(), STATE_APPEND_HEADER_BYTES);
    assert_eq!(append[0], 1);
    assert_eq!(&append[1..82], &scope.as_bytes());
    assert_eq!(&append[82..84], &3u16.to_be_bytes());
    assert_eq!(&append[84..], &189u32.to_be_bytes());
    assert_eq!(
        ledger.records(),
        0,
        "validation/header encoding does not acknowledge"
    );
    for (index, row) in rows.iter().enumerate() {
        let bytes = row.encode();
        assert_eq!(bytes.len(), DIRECTORY_ROOT_RECORD_BYTES);
        assert_eq!(&bytes[..2], &25u16.to_be_bytes());
        assert_eq!(&bytes[2..10], &scope.selection().token().to_be_bytes());
        assert_eq!(&bytes[10..18], &7u64.to_be_bytes());
        assert_eq!(bytes[18], 1);
        let serial = 8 + index as u64;
        assert_eq!(&bytes[19..27], &serial.to_be_bytes());
        assert_eq!(
            &bytes[31..],
            ObjectId::for_bytes(&serial.to_be_bytes()).as_bytes()
        );
        assert_eq!(StateRecord::decode(&scope, &bytes).unwrap(), *row);
    }
    ledger.acknowledge(&rows).unwrap();
    let seal = ledger.seal();
    assert_eq!(seal.records(), 3);
    assert_eq!(seal.encoded_bytes(), 189);
    assert_eq!(*seal.digest(), digest(&scope, 8, 3));
    assert_eq!(
        StateSeal::decode(scope.clone(), &seal.encode()).unwrap(),
        seal
    );
    assert_eq!(seal.encode().len(), STATE_SEAL_BYTES);
    let page = StatePage::new(seal.clone(), rows.clone(), true).unwrap();
    let header = page.encode_header();
    assert_eq!(header.len(), STATE_PAGE_HEADER_BYTES);
    assert_eq!(&header[..130], &seal.encode());
    assert_eq!(header[130], 1);
    assert_eq!(&header[131..156], rows.last().unwrap().key().as_bytes());
    assert_eq!(&header[156..158], &3u16.to_be_bytes());
    assert_eq!(&header[158..162], &189u32.to_be_bytes());
    assert_eq!(header[162], 1);
    let empty_page = StatePage::after(seal, page.last(), Vec::new(), true).unwrap();
    assert_eq!(empty_page.last(), page.last());
    assert_eq!(empty_page.encode_header()[130], 1);
    let mut invalid = rows[0].encode();
    invalid[18] = 2;
    assert!(StateRecord::decode(&scope, &invalid).is_err());
    assert!(StateRecord::decode(&scope, &invalid[..62]).is_err());
    assert!(StateKey::decode(&scope, &[1; 289]).is_err());
    let mut invalid = rows[0].encode();
    invalid[30] = 31;
    assert!(StateRecord::decode(&scope, &invalid).is_err());
    let mut invalid_seal = ledger.seal().encode();
    invalid_seal[97] = invalid_seal[97].wrapping_add(1);
    assert!(StateSeal::decode(scope.clone(), &invalid_seal).is_err());
    assert!(StateKey::directory_root(&scope, 0).is_err());
    assert!(StateKey::directory_root(&scope, i64::MAX as u64 + 1).is_err());
    assert!(StatePage::new(ledger.seal(), Vec::new(), false).is_err());
    let mut over_capacity = Vec::with_capacity(129);
    over_capacity.push(rows[0]);
    assert_eq!(
        StatePage::new(ledger.seal(), over_capacity, true).unwrap_err(),
        ContentError::BoundedCapacityExceeded {
            what: "indexed_state.page_capacity",
            limit: 128,
            actual: 129,
        }
    );
    assert!(PageLimit::new(129, 65536).is_err());
    assert!(PageLimit::new(128, 65537).is_err());
}

#[test]
fn empty_selected_table_has_one_exact_terminal_page_and_independent_byte_admission() {
    let scope = new_scope(16);
    let mut state = ResidentState::new(scope.clone(), 0).unwrap();
    let seal = state.seal(&scope).unwrap();
    assert_eq!(*seal.digest(), digest(&scope, 0, 0));
    let mut cursor = StateCursor::new(&mut state, seal);
    let page = cursor.next_page(PageLimit::default()).unwrap().unwrap();
    assert!(page.records().is_empty() && page.eof());
    assert_eq!(page.last(), None);
    assert_eq!(page.encode_header()[130..156], [0; 26]);
    assert!(cursor.next_page(PageLimit::default()).unwrap().is_none());
    assert_eq!(
        StateCapacity::new(1, 62)
            .unwrap()
            .check_requested(1)
            .unwrap_err(),
        ContentError::BoundedCapacityExceeded {
            what: "indexed_state.record_bytes",
            limit: 62,
            actual: 63,
        }
    );
}

#[test]
fn ordered_batches_refuse_before_mutation_and_sealed_pages_obey_count_and_byte_limits() {
    let scope = new_scope(21);
    let rows = records(&scope, 1, 300);
    let expected_digest = digest(&scope, 1, 300);
    let mut state = ResidentState::new(scope.clone(), 300).unwrap();
    assert!(state.append(&scope, &rows[..129]).is_err());
    for batch in rows.chunks(128) {
        state.append(&scope, batch).unwrap();
    }
    assert!(state.append(&scope, &rows[299..]).is_err());
    let seal = state.seal(&scope).unwrap();
    assert_eq!(seal.records(), 300);
    assert_eq!(*seal.digest(), expected_digest);
    assert!(state.append(&scope, &[]).is_err());
    assert!(state.seal(&scope).is_err());
    for serial in [1, 128, 129, 300] {
        assert_eq!(
            state
                .get(&seal, StateKey::directory_root(&scope, serial).unwrap())
                .unwrap(),
            Some(rows[serial as usize - 1])
        );
    }
    assert!(state
        .get(&seal, StateKey::directory_root(&scope, 301).unwrap())
        .unwrap()
        .is_none());
    let too_small = PageLimit::new(128, STATE_PAGE_HEADER_BYTES + 62).unwrap();
    assert_eq!(
        state.page(&seal, None, too_small).unwrap_err(),
        ContentError::BoundedCapacityExceeded {
            what: "indexed_state.page_bytes",
            limit: (STATE_PAGE_HEADER_BYTES + 62) as u64,
            actual: (STATE_PAGE_HEADER_BYTES + 63) as u64,
        }
    );
    for limit in [
        PageLimit::default(),
        PageLimit::new(128, STATE_PAGE_HEADER_BYTES + 3 * 63).unwrap(),
    ] {
        let mut cursor = StateCursor::new(&mut state, seal.clone());
        let mut observed = Vec::new();
        while let Some(page) = cursor.next_page(limit).unwrap() {
            assert!(page.records().len() <= limit.records());
            assert!(page.encoded_len() <= limit.bytes());
            observed.extend_from_slice(page.records());
        }
        assert_eq!(observed, rows);
    }
    let last = rows.last().unwrap().key();
    let eof = state.page(&seal, Some(last), PageLimit::default()).unwrap();
    assert!(eof.records().is_empty() && eof.eof());
    assert_eq!(eof.last(), Some(last));
    let other = new_scope(22);
    assert!(state.capacity(&other).is_err());
    assert!(state.release(&other).is_err());
    state.release(&scope).unwrap();
    state.release(&scope).unwrap();
    assert!(state.get(&seal, last).is_err());
}

struct ChangedPage {
    inner: ResidentState,
    calls: usize,
    early_eof: bool,
}
impl IndexedState for ChangedPage {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.inner.capacity(scope)
    }
    fn append(&mut self, scope: &StateScope, rows: &[StateRecord]) -> ContentResult<()> {
        self.inner.append(scope, rows)
    }
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.inner.seal(scope)
    }
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.inner.get(seal, key)
    }
    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage> {
        self.calls += 1;
        let page = self.inner.page(seal, after, limit)?;
        // Consume the bounded public view while the original page retains its
        // data and any associated last-owner credit. This compatibility fixture
        // owns a separate bounded copy to simulate a corrupt provider response.
        let mut rows = page.records().to_vec();
        if self.early_eof {
            rows.truncate(1);
        } else {
            rows[0] = StateRecord::new(rows[0].key(), ObjectId::for_bytes(b"changed-after-seal"));
        }
        let response = StatePage::after(seal.clone(), after, rows, true);
        drop(page);
        response
    }
    fn release(&mut self, scope: &StateScope) -> ContentResult<()> {
        self.inner.release(scope)
    }
}

#[test]
fn cursor_rejects_early_eof_and_changed_sealed_bytes_and_keeps_failure_terminal() {
    for early_eof in [false, true] {
        let scope = new_scope(31);
        let rows = records(&scope, 1, 3);
        let mut inner = ResidentState::new(scope.clone(), 3).unwrap();
        inner.append(&scope, &rows).unwrap();
        let seal = inner.seal(&scope).unwrap();
        let mut state = ChangedPage {
            inner,
            calls: 0,
            early_eof,
        };
        let mut cursor = StateCursor::new(&mut state, seal);
        let error = cursor.next_page(PageLimit::default()).unwrap_err();
        assert_eq!(
            error,
            ContentError::InvalidOrderingRecord(if early_eof {
                "state page EOF"
            } else {
                "state page digest"
            })
        );
        assert_eq!(cursor.next_page(PageLimit::default()).unwrap_err(), error);
        drop(cursor);
        assert_eq!(
            state.calls, 1,
            "a failed cursor performs no second provider request"
        );
    }
}

#[test]
fn larger_resident_compatibility_is_admitted_explicitly_and_canonical_refusal_precedes_output() {
    let scope = new_scope(41);
    let rows = records(&scope, 1, 1200);
    let mut state = ResidentState::new(scope.clone(), 1200).unwrap();
    assert!(state.capacity(&scope).unwrap().encoded_bytes() > 65536);
    for batch in rows.chunks(128) {
        state.append(&scope, batch).unwrap();
    }
    assert_eq!(state.seal(&scope).unwrap().records(), 1200);
    let scope = new_scope(42);
    let mut state = ResidentState::new(scope.clone(), 0).unwrap();
    let directory = [DirectoryUpdate {
        parent: 1,
        changes: Vec::new(),
    }];
    let inodes = [InodeUpdate {
        serial: 1,
        value: InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: ObjectId::for_bytes(b"old-directory"),
            metadata_root: ObjectId::for_bytes(b"metadata"),
        },
    }];
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([5; 32]),
        root_serial: 1,
        directories: &directory,
        inodes: &inodes,
        new_inodes: &[1],
        resources: FilesystemResources::default(),
    };
    let reader = support::TreeStore::new();
    let mut output = support::TreeStore::new();
    {
        let mut objects = FilesystemObjects::new(&reader, &mut output);
        let error = build_filesystem_with_state(&mut objects, &input, None, &mut state, &scope)
            .unwrap_err();
        assert_eq!(
            error,
            ContentError::BoundedCapacityExceeded {
                what: "indexed_state.records",
                limit: 0,
                actual: 1
            }
        );
        assert_eq!(objects.work().objects_emitted, 0);
    }
    assert!(output.is_empty());
}

struct RefusingCompletion {
    inner: ResidentState,
    attempts: usize,
}

impl IndexedState for RefusingCompletion {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.inner.capacity(scope)
    }
    fn append(&mut self, scope: &StateScope, rows: &[StateRecord]) -> ContentResult<()> {
        self.inner.append(scope, rows)
    }
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.inner.seal(scope)
    }
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.inner.get(seal, key)
    }
    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage> {
        self.inner.page(seal, after, limit)
    }
    fn release(&mut self, _scope: &StateScope) -> ContentResult<()> {
        self.attempts += 1;
        Err(ContentError::ProviderFailure {
            what: "external logical completion",
        })
    }
}

#[test]
fn logical_completion_runs_once_before_root_and_preserves_an_earlier_failure() {
    for invalid_value in [false, true] {
        let scope = new_scope(51);
        let mut state = RefusingCompletion {
            inner: ResidentState::new(scope.clone(), 1).unwrap(),
            attempts: 0,
        };
        let directories = [DirectoryUpdate {
            parent: 1,
            changes: Vec::new(),
        }];
        let inodes = [InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: if invalid_value {
                    InodeKind::RegularFile
                } else {
                    InodeKind::Directory
                },
                namespace_ref_count: 0,
                content_root: ObjectId::for_bytes(b"unused initial directory"),
                metadata_root: ObjectId::for_bytes(b"state cleanup metadata"),
            },
        }];
        let input = FilesystemInput {
            base: None,
            scope: layerfs_content::filesystem::scope_for_seed([5; 32]),
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &[1],
            resources: FilesystemResources::default(),
        };
        let reader = support::TreeStore::new();
        let mut output = support::TreeStore::new();
        {
            let mut objects = FilesystemObjects::new(&reader, &mut output);
            let error = build_filesystem_with_state(&mut objects, &input, None, &mut state, &scope)
                .unwrap_err();
            if invalid_value {
                assert_eq!(error, ContentError::InvalidRecord("root inode kind"));
            } else {
                assert_eq!(
                    error,
                    ContentError::ProviderFailure {
                        what: "external logical completion"
                    }
                );
                assert!(objects.work().objects_emitted > 0);
            }
        }
        assert!(output
            .order()
            .iter()
            .all(|(_, role)| *role != ObjectRole::FilesystemRoot));
        assert_eq!(
            state.attempts, 1,
            "completion failure does not trigger a second release"
        );
    }
}
