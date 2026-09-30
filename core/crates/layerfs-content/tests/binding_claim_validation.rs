//! Complete C1 caller, semantic precedence and bounded dispatch laws.
#[path = "support/claim_state.rs"]
mod observed;
mod support;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use layerfs_content::filesystem::rows::SliceBindingRows;
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::validate::{check_bindings, check_with_claims, ValidationWork};
use layerfs_content::filesystem::{
    build_filesystem_binding_rows_with_construction_state, scope_for_seed,
    update_filesystem_binding_rows_with_construction_state, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemPhases, FilesystemRead, FilesystemRootId, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use observed::{scopes, ObservedState};
use support::filesystem::{
    resources, synthetic, value, RecordingBacking, Session, TempDir, TreeStore,
};

fn inode(kind: InodeKind) -> InodeValue {
    value(kind, synthetic("claims/content"), synthetic("claims/meta"))
}
fn name(index: usize) -> PathName {
    PathName::new(&format!("n{index:04}")).unwrap()
}
fn fixture(kind: InodeKind, count: usize) -> (Vec<DirectoryUpdate>, Vec<InodeUpdate>, Vec<u64>) {
    let serials: Vec<u64> = (1..=count as u64 + 1).collect();
    let mut directories = vec![DirectoryUpdate {
        parent: 1,
        changes: serials[1..]
            .iter()
            .enumerate()
            .map(|(i, s)| (name(i), Some(*s)))
            .collect(),
    }];
    if kind == InodeKind::Directory {
        directories.extend(serials[1..].iter().map(|s| DirectoryUpdate {
            parent: *s,
            changes: Vec::new(),
        }));
    }
    let mut values = vec![InodeUpdate {
        serial: 1,
        value: inode(InodeKind::Directory),
    }];
    values.extend(serials[1..].iter().map(|s| InodeUpdate {
        serial: *s,
        value: inode(kind),
    }));
    (directories, values, serials)
}
fn input<'a>(
    directories: &'a [DirectoryUpdate],
    inodes: &'a [InodeUpdate],
    fresh: &'a [u64],
) -> FilesystemInput<'a> {
    FilesystemInput {
        base: None,
        scope: scope_for_seed([0x44; 32]),
        root_serial: 1,
        directories,
        inodes,
        new_inodes: fresh,
        resources: resources(),
    }
}
fn check(
    input: &FilesystemInput<'_>,
    state: &mut ObservedState,
    selected: &ConstructionScopes,
) -> ContentResult<()> {
    let source = SliceBindingRows::new(input)?;
    check_with_claims(
        &TreeStore::new(),
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
        state,
        selected.claims(),
    )
    .map(|_| ())
}

#[test]
fn wide_exclusive_bindings_use_three_transactions_and_three_verification_pages() {
    let (d, v, f) = fixture(InodeKind::Symlink, 257);
    let selected = scopes();
    let mut state = ObservedState::new(&selected, d.len(), 257);
    check(&input(&d, &v, &f), &mut state, &selected).unwrap();
    assert_eq!(state.presence, 257);
    assert_eq!(state.batches, vec![128, 128, 1]);
    assert_eq!(state.seals, 1);
    assert_eq!(state.pages, 3);
    assert_eq!(state.maximum_page, 128);
    assert_eq!(state.retirements, 1);
    assert!(state.retired.get());
    assert_eq!(state.abandonments, 0);
    assert_eq!(state.root_capacities.get(), 0);
    eprintln!("C1 dispatch count proof: K257, claim transactions3, verifier pages3, window128; explicit resident provider, no physical/speed qualification");
}

#[test]
fn regular_aliases_emit_no_claims_and_legacy_additions_are_zero() {
    let d = [DirectoryUpdate {
        parent: 1,
        changes: (0..129).map(|i| (name(i), Some(2))).collect(),
    }];
    let v = [
        InodeUpdate {
            serial: 1,
            value: inode(InodeKind::Directory),
        },
        InodeUpdate {
            serial: 2,
            value: inode(InodeKind::RegularFile),
        },
    ];
    let operation = input(&d, &v, &[1, 2]);
    let source = SliceBindingRows::new(&operation).unwrap();
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 129);
    check(&operation, &mut state, &selected).unwrap();
    assert_eq!(state.presence, 0);
    assert!(state.batches.is_empty());
    assert_eq!(state.pages, 1);
    assert_eq!(state.retirements, 1);
    let legacy = check_bindings(
        &TreeStore::new(),
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    assert_eq!(legacy.additions, BTreeMap::from([(2, 0)]));
}

#[test]
fn legacy_result_preserves_one_for_each_directory_symlink_and_zero_for_files() {
    let d = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![
                (name(0), Some(2)),
                (name(1), Some(3)),
                (name(2), Some(4)),
                (name(3), Some(4)),
            ],
        },
        DirectoryUpdate {
            parent: 2,
            changes: Vec::new(),
        },
    ];
    let v = [
        InodeUpdate {
            serial: 1,
            value: inode(InodeKind::Directory),
        },
        InodeUpdate {
            serial: 2,
            value: inode(InodeKind::Directory),
        },
        InodeUpdate {
            serial: 3,
            value: inode(InodeKind::Symlink),
        },
        InodeUpdate {
            serial: 4,
            value: inode(InodeKind::RegularFile),
        },
    ];
    let operation = input(&d, &v, &[1, 2, 3, 4]);
    let source = SliceBindingRows::new(&operation).unwrap();
    let checked = check_bindings(
        &TreeStore::new(),
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    assert_eq!(checked.additions, BTreeMap::from([(2, 1), (3, 1), (4, 0)]));
}

#[test]
fn first_unflushed_duplicate_terminalizes_same_owner_before_any_root_work() {
    let d = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name(0), Some(2)), (name(1), Some(2))],
    }];
    let v = [
        InodeUpdate {
            serial: 1,
            value: inode(InodeKind::Directory),
        },
        InodeUpdate {
            serial: 2,
            value: inode(InodeKind::Symlink),
        },
    ];
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 2);
    assert_eq!(
        check(&input(&d, &v, &[1, 2]), &mut state, &selected),
        Err(ContentError::InvalidRecord("multiple parents"))
    );
    assert_eq!(state.presence, 1);
    assert!(state.batches.is_empty());
    assert_eq!(state.abandonments, 1);
    let valid = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name(0), Some(2))],
    }];
    assert!(check(&input(&valid, &v, &[1, 2]), &mut state, &selected).is_err());
    assert_eq!(state.seals, 0);
    assert_eq!(state.retirements, 0);
    assert_eq!(state.root_capacities.get(), 0);
}

#[test]
fn cross_batch_duplicate_wins_over_a_later_bad_parent_kind_and_cannot_retry() {
    let (mut d, mut v, mut fresh) = fixture(InodeKind::Symlink, 128);
    d[0].changes
        .push((PathName::new("zduplicate").unwrap(), Some(2)));
    d.push(DirectoryUpdate {
        parent: 130,
        changes: Vec::new(),
    });
    v.push(InodeUpdate {
        serial: 130,
        value: inode(InodeKind::RegularFile),
    });
    fresh.push(130);
    let operation = input(&d, &v, &fresh);
    let selected = scopes();
    let mut state = ObservedState::new(&selected, d.len(), 129);
    assert_eq!(
        check(&operation, &mut state, &selected),
        Err(ContentError::InvalidRecord("multiple parents"))
    );
    assert_eq!(state.batches, vec![128]);
    assert_eq!(state.presence, 129);
    assert_eq!(state.abandonments, 1);
    let (good_d, good_v, good_f) = fixture(InodeKind::Symlink, 1);
    assert!(check(&input(&good_d, &good_v, &good_f), &mut state, &selected).is_err());
    let source = SliceBindingRows::new(&operation).unwrap();
    assert!(matches!(
        check_bindings(
            &TreeStore::new(),
            &source,
            &BTreeMap::new(),
            &mut ValidationWork::default()
        ),
        Err(ContentError::InvalidRecord("multiple parents"))
    ));
}

#[test]
fn second_partial_window_duplicates_are_detected_without_a_second_transaction() {
    let (mut d, v, f) = fixture(InodeKind::Symlink, 129);
    d[0].changes
        .push((PathName::new("zduplicate").unwrap(), Some(130)));
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 130);
    assert_eq!(
        check(&input(&d, &v, &f), &mut state, &selected),
        Err(ContentError::InvalidRecord("multiple parents"))
    );
    assert_eq!(state.batches, vec![128]);
    assert_eq!(state.presence, 129);
    assert_eq!(state.abandonments, 1);
}

#[test]
fn provider_acknowledgement_error_is_preserved_and_abandoned_without_seal_or_retirement() {
    let (d, v, f) = fixture(InodeKind::Symlink, 128);
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 128);
    state.refuse_batch = true;
    assert_eq!(
        check(&input(&d, &v, &f), &mut state, &selected),
        Err(ContentError::ProviderFailure {
            what: "claim acknowledgement"
        })
    );
    assert_eq!(state.batches, vec![128]);
    assert_eq!(state.abandonments, 1);
    assert_eq!(state.seals, 0);
    assert_eq!(state.retirements, 0);
}

#[test]
fn source_shape_failure_precedes_duplicate_or_provider_effects() {
    let d = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name(1), Some(2)), (name(0), Some(2))],
    }];
    let v = [
        InodeUpdate {
            serial: 1,
            value: inode(InodeKind::Directory),
        },
        InodeUpdate {
            serial: 2,
            value: inode(InodeKind::Symlink),
        },
    ];
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 2);
    assert!(matches!(
        check(&input(&d, &v, &[1, 2]), &mut state, &selected),
        Err(ContentError::NonCanonicalOrdering)
    ));
    assert_eq!(state.presence, 0);
    assert!(state.batches.is_empty());
    assert_eq!(state.abandonments, 1);
}

struct GateConsumer {
    output: Rc<RefCell<TreeStore>>,
    retired: Rc<Cell<bool>>,
    accepted: u64,
}
impl FinalizedConsumer for GateConsumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        assert!(
            self.retired.get(),
            "canonical emission before known claim retirement"
        );
        self.accepted += 1;
        self.output.borrow_mut().accept(object)
    }
}
struct Overlay<'a> {
    base: &'a TreeStore,
    output: Rc<RefCell<TreeStore>>,
}
impl AuthenticatedObjects for Overlay<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| {
                if self.output.borrow().canonical(*id).is_some() {
                    self.output.borrow().read_canonical(*id)
                } else {
                    self.base.read_canonical(*id)
                }
            })
            .collect()
    }
}

#[test]
fn full_257_directory_build_retires_claims_before_root_admission_and_emission() {
    let (d, v, f) = fixture(InodeKind::Directory, 257);
    let operation = input(&d, &v, &f);
    let source = SliceBindingRows::new(&operation).unwrap();
    let selected = scopes();
    let mut state = ObservedState::new(&selected, d.len(), 257);
    let base = TreeStore::new();
    let output = Rc::new(RefCell::new(TreeStore::new()));
    let reader = Overlay {
        base: &base,
        output: output.clone(),
    };
    let mut consumer = GateConsumer {
        output: output.clone(),
        retired: state.retired.clone(),
        accepted: 0,
    };
    let mut objects = FilesystemObjects::new(&reader, &mut consumer);
    let result = build_filesystem_binding_rows_with_construction_state(
        &mut objects,
        &source,
        None,
        &mut state,
        &selected,
        &FilesystemPhases::disabled(),
    )
    .unwrap();
    assert!(consumer.accepted > 0);
    assert_eq!(state.batches, vec![128, 128, 1]);
    assert_eq!(state.retirements, 1);
    assert_eq!(state.root_capacities.get(), 1);
    let store = output.borrow();
    let mut read = FilesystemRead::new(&*store, result.root).unwrap();
    assert_eq!(read.resolve_inode(1).unwrap().value.namespace_ref_count, 0);
    for index in 0..257 {
        let record = read.resolve_child(1, &name(index)).unwrap();
        assert_eq!(record.serial, index as u64 + 2);
        assert_eq!(record.value.kind, InodeKind::Directory);
        assert_eq!(record.value.namespace_ref_count, 1);
    }
    // Logical identities/kinds/counts are independently specified by fixture.
    // This case does not use its generated root as an expected root pin.
}

#[test]
fn retirement_failure_releases_ordering_once_and_denies_root_allocation_and_bytes() {
    let (d, v, f) = fixture(InodeKind::Symlink, 2);
    let operation = input(&d, &v, &f);
    let source = SliceBindingRows::new(&operation).unwrap();
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 2);
    state.refuse_retire = true;
    let base = TreeStore::new();
    let mut output = TreeStore::new();
    let directory = TempDir::new("claim-retirement");
    let mut backing = RecordingBacking::new(directory.path());
    let mut objects = FilesystemObjects::new(&base, &mut output);
    assert_eq!(
        build_filesystem_binding_rows_with_construction_state(
            &mut objects,
            &source,
            Some(&mut backing),
            &mut state,
            &selected,
            &FilesystemPhases::disabled()
        ),
        Err(ContentError::ProviderFailure {
            what: "claim retirement"
        })
    );
    assert!(output.is_empty());
    assert_eq!(state.retirements, 1);
    assert_eq!(state.abandonments, 1);
    assert_eq!(state.root_capacities.get(), 0);
    assert_eq!(backing.counters().releases, 1);
    assert_eq!(backing.counters().creates, 0);
}

#[test]
fn shape_admission_refuses_before_claim_batch_root_and_canonical_effects() {
    let (d, v, f) = fixture(InodeKind::Symlink, 2);
    let operation = input(&d, &v, &f);
    let source = SliceBindingRows::new(&operation).unwrap();
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 1);
    let base = TreeStore::new();
    let mut output = TreeStore::new();
    let mut objects = FilesystemObjects::new(&base, &mut output);
    assert!(matches!(
        build_filesystem_binding_rows_with_construction_state(
            &mut objects,
            &source,
            None,
            &mut state,
            &selected,
            &FilesystemPhases::disabled()
        ),
        Err(ContentError::BoundedCapacityExceeded {
            what: "binding_claims.records",
            limit: 1,
            actual: 2
        })
    ));
    assert!(output.is_empty());
    assert!(state.batches.is_empty());
    assert_eq!(state.presence, 0);
    assert_eq!(state.root_capacities.get(), 0);
    assert_eq!(state.abandonments, 1);
}

#[test]
fn stored_regular_kind_wins_over_caller_directory_value_and_allows_aliases() {
    let mut session = Session::new(1).unwrap();
    let file = session.allocate();
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name(0), Some(file))],
            }],
            &[InodeUpdate {
                serial: file,
                value: inode(InodeKind::RegularFile),
            }],
            &[file],
        )
        .unwrap();
    let d = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name(1), Some(file)), (name(2), Some(file))],
    }];
    let v = [InodeUpdate {
        serial: file,
        value: inode(InodeKind::Directory),
    }];
    let operation = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &d,
        inodes: &v,
        new_inodes: &[],
        resources: resources(),
    };
    let source = SliceBindingRows::new(&operation).unwrap();
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 2);
    check_with_claims(
        &session.store,
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
        &mut state,
        selected.claims(),
    )
    .unwrap();
    assert_eq!(state.presence, 0);
    assert!(state.batches.is_empty());
    let legacy = check_bindings(
        &session.store,
        &source,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    assert_eq!(legacy.additions, BTreeMap::from([(file, 0)]));
}

#[test]
fn complete_existing_directory_permutation_uses_one_claim_then_keeps_alias_work_linear() {
    let mut session = Session::new(1).unwrap();
    let serials: Vec<_> = (0..129).map(|_| session.allocate()).collect();
    let mut base = vec![DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .enumerate()
            .map(|(i, s)| (name(i), Some(*s)))
            .collect(),
    }];
    base.extend(serials.iter().map(|s| DirectoryUpdate {
        parent: *s,
        changes: Vec::new(),
    }));
    let values: Vec<_> = serials
        .iter()
        .map(|s| InodeUpdate {
            serial: *s,
            value: inode(InodeKind::Directory),
        })
        .collect();
    session.apply(&base, &values, &serials).unwrap();
    let d = [DirectoryUpdate {
        parent: 1,
        changes: (0..129)
            .map(|i| (name(i), Some(serials[(i + 1) % 129])))
            .collect(),
    }];
    let operation = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &d,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let source = SliceBindingRows::new(&operation).unwrap();
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 129);
    let mut work = ValidationWork::default();
    check_with_claims(
        &session.store,
        &source,
        &BTreeMap::new(),
        &mut work,
        &mut state,
        selected.claims(),
    )
    .unwrap();
    assert_eq!(work.entries_examined, 258);
    assert_eq!(state.batches, vec![128, 1]);
    // Execute the actual new update entry with a fresh owner against the same immutable base.
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 129);
    let output = Rc::new(RefCell::new(TreeStore::new()));
    let reader = Overlay {
        base: &session.store,
        output: output.clone(),
    };
    let mut consumer = GateConsumer {
        output: output.clone(),
        retired: state.retired.clone(),
        accepted: 0,
    };
    let mut objects = FilesystemObjects::new(&reader, &mut consumer);
    let result = update_filesystem_binding_rows_with_construction_state(
        &mut objects,
        &source,
        None,
        &mut state,
        &selected,
        &FilesystemPhases::disabled(),
    )
    .unwrap();
    let mut final_store = session.store.clone();
    final_store.absorb(&output.borrow());
    let mut read = FilesystemRead::new(&final_store, result.root).unwrap();
    for i in 0..129 {
        assert_eq!(
            read.resolve_child(1, &name(i)).unwrap().serial,
            serials[(i + 1) % 129]
        );
    }
    assert_eq!(state.retirements, 1);
    assert_eq!(state.root_capacities.get(), 1);
}

#[test]
fn early_build_and_update_base_refusals_terminalize_the_selected_owner() {
    let (d, v, f) = fixture(InodeKind::Symlink, 1);
    for build_with_base in [true, false] {
        let selected = scopes();
        let mut state = ObservedState::new(&selected, 1, 1);
        let mut malformed = input(&d, &v, &f);
        if build_with_base {
            malformed.base = Some(FilesystemRootId(synthetic("early-base")));
        }
        let source = SliceBindingRows::new(&malformed).unwrap();
        let base = TreeStore::new();
        let mut output = TreeStore::new();
        let mut objects = FilesystemObjects::new(&base, &mut output);
        let refused = if build_with_base {
            build_filesystem_binding_rows_with_construction_state(
                &mut objects,
                &source,
                None,
                &mut state,
                &selected,
                &FilesystemPhases::disabled(),
            )
        } else {
            update_filesystem_binding_rows_with_construction_state(
                &mut objects,
                &source,
                None,
                &mut state,
                &selected,
                &FilesystemPhases::disabled(),
            )
        };
        let expected = if build_with_base {
            "initial build base"
        } else {
            "update base root"
        };
        assert_eq!(refused, Err(ContentError::InvalidRecord(expected)));
        assert_eq!(state.abandonments, 1);
        assert_eq!(state.presence, 0);
        assert!(state.batches.is_empty());
        assert_eq!(state.root_capacities.get(), 0);
        assert!(output.is_empty());
        let valid = input(&d, &v, &f);
        let source = SliceBindingRows::new(&valid).unwrap();
        let mut objects = FilesystemObjects::new(&base, &mut output);
        assert!(build_filesystem_binding_rows_with_construction_state(
            &mut objects,
            &source,
            None,
            &mut state,
            &selected,
            &FilesystemPhases::disabled(),
        )
        .is_err());
        assert_eq!(state.seals, 0);
        assert_eq!(state.retirements, 0);
        assert_eq!(state.root_capacities.get(), 0);
        assert!(output.is_empty());
    }
}

#[test]
fn early_resource_refusal_abandons_once_releases_ordering_and_denies_reentry() {
    let (d, v, f) = fixture(InodeKind::Symlink, 1);
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 1);
    let mut malformed = input(&d, &v, &f);
    malformed.resources.scratch_bytes = 1023;
    let source = SliceBindingRows::new(&malformed).unwrap();
    let base = TreeStore::new();
    let mut output = TreeStore::new();
    let directory = TempDir::new("claim-early-resource");
    let mut backing = RecordingBacking::new(directory.path());
    let mut objects = FilesystemObjects::new(&base, &mut output);
    assert_eq!(
        build_filesystem_binding_rows_with_construction_state(
            &mut objects,
            &source,
            Some(&mut backing),
            &mut state,
            &selected,
            &FilesystemPhases::disabled(),
        ),
        Err(ContentError::ResourceUnavailable {
            what: "operation scratch"
        })
    );
    assert_eq!(state.abandonments, 1);
    assert_eq!(backing.counters().releases, 1);
    assert_eq!(backing.counters().creates, 0);
    assert_eq!(state.presence, 0);
    assert_eq!(state.root_capacities.get(), 0);
    assert!(output.is_empty());
    let valid = input(&d, &v, &f);
    let source = SliceBindingRows::new(&valid).unwrap();
    let mut objects = FilesystemObjects::new(&base, &mut output);
    assert!(build_filesystem_binding_rows_with_construction_state(
        &mut objects,
        &source,
        None,
        &mut state,
        &selected,
        &FilesystemPhases::disabled(),
    )
    .is_err());
    assert!(state.batches.is_empty());
    assert_eq!(state.seals, 0);
    assert!(output.is_empty());
}

#[test]
fn early_fallible_binding_source_refusal_abandons_once_and_denies_reentry() {
    let (mut d, v, f) = fixture(InodeKind::Symlink, 2);
    d[0].changes.swap(0, 1);
    let selected = scopes();
    let mut state = ObservedState::new(&selected, 1, 2);
    let malformed = input(&d, &v, &f);
    let source = SliceBindingRows::new(&malformed).unwrap();
    let base = TreeStore::new();
    let mut output = TreeStore::new();
    let directory = TempDir::new("claim-early-source");
    let mut backing = RecordingBacking::new(directory.path());
    let mut objects = FilesystemObjects::new(&base, &mut output);
    assert_eq!(
        build_filesystem_binding_rows_with_construction_state(
            &mut objects,
            &source,
            Some(&mut backing),
            &mut state,
            &selected,
            &FilesystemPhases::disabled(),
        ),
        Err(ContentError::NonCanonicalOrdering)
    );
    assert_eq!(state.abandonments, 1);
    assert_eq!(backing.counters().releases, 1);
    assert_eq!(backing.counters().creates, 0);
    assert_eq!(state.presence, 0);
    assert_eq!(state.root_capacities.get(), 0);
    assert!(output.is_empty());
    let (d, v, f) = fixture(InodeKind::Symlink, 2);
    let valid = input(&d, &v, &f);
    let source = SliceBindingRows::new(&valid).unwrap();
    let mut objects = FilesystemObjects::new(&base, &mut output);
    assert!(build_filesystem_binding_rows_with_construction_state(
        &mut objects,
        &source,
        None,
        &mut state,
        &selected,
        &FilesystemPhases::disabled(),
    )
    .is_err());
    assert!(state.batches.is_empty());
    assert_eq!(state.seals, 0);
    assert_eq!(state.retirements, 0);
    assert!(output.is_empty());
}

#[test]
fn postchecker_root_shape_refusal_terminalizes_even_after_known_claim_retirement() {
    let (d, v, f) = fixture(InodeKind::Directory, 1);
    let operation = input(&d, &v, &f);
    let source = SliceBindingRows::new(&operation).unwrap();
    let selected = scopes();
    // This explicit compatibility owner admitted D1; the actual input has D2.
    // Native Server's pre-admission uses exact D and has a separate provider proof.
    let mut state = ObservedState::new(&selected, 1, 1);
    let base = TreeStore::new();
    let mut output = TreeStore::new();
    let directory = TempDir::new("claim-postchecker-root-shape");
    let mut backing = RecordingBacking::new(directory.path());
    let mut objects = FilesystemObjects::new(&base, &mut output);
    assert!(matches!(
        build_filesystem_binding_rows_with_construction_state(
            &mut objects,
            &source,
            Some(&mut backing),
            &mut state,
            &selected,
            &FilesystemPhases::disabled(),
        ),
        Err(ContentError::BoundedCapacityExceeded {
            what: "indexed_state.records",
            limit: 1,
            actual: 2
        })
    ));
    assert_eq!(state.batches, vec![1]);
    assert_eq!(state.retirements, 1);
    assert!(state.retired.get());
    assert_eq!(state.abandonments, 1);
    assert_eq!(state.root_capacities.get(), 1);
    assert!(output.is_empty());
    assert_eq!(backing.counters().releases, 1);
    assert_eq!(backing.counters().creates, 0);
    assert!(state.capacity(selected.roots()).is_err());
    let record =
        StateRecord::directory_root(selected.roots(), 1, synthetic("denied-root-write")).unwrap();
    assert!(state.append(selected.roots(), &[record]).is_err());
    let (d, v, f) = fixture(InodeKind::Symlink, 0);
    let valid = input(&d, &v, &f);
    let source = SliceBindingRows::new(&valid).unwrap();
    let mut objects = FilesystemObjects::new(&base, &mut output);
    assert!(build_filesystem_binding_rows_with_construction_state(
        &mut objects,
        &source,
        None,
        &mut state,
        &selected,
        &FilesystemPhases::disabled(),
    )
    .is_err());
    assert!(output.is_empty());
    assert_eq!(state.seals, 1);
    assert_eq!(state.retirements, 1);
}
