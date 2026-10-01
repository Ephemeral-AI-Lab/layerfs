//! Complete source-bound caller and independent logical alias/count/custody proof.
#[path = "support/site_state.rs"]
mod oracle;
mod support;
use layerfs_content::filesystem::rows::*;
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::validate::{check_bindings, check_with_sites, ValidationWork};
use layerfs_content::filesystem::{
    build_filesystem_binding_rows_with_site_state, scope_for_seed,
    update_filesystem_binding_rows_with_site_state, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemPhases, FilesystemRootId, InodeScope, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId, ObjectRole};
use oracle::{scopes, ObservedSites};
use std::cell::Cell;
use std::collections::BTreeMap;
use support::filesystem::{resources, synthetic, value, Session, TreeStore};

fn name(index: usize) -> PathName {
    PathName::new(&format!("n{index:04}")).unwrap()
}
fn inode(kind: InodeKind) -> InodeValue {
    value(kind, synthetic("site/content"), synthetic("site/meta"))
}
fn fixture(kind: InodeKind, count: usize) -> (Vec<DirectoryUpdate>, Vec<InodeUpdate>, Vec<u64>) {
    let serials: Vec<_> = (1..=count as u64 + 1).collect();
    let mut rows = vec![DirectoryUpdate {
        parent: 1,
        changes: serials[1..]
            .iter()
            .enumerate()
            .map(|(i, serial)| (name(i), Some(*serial)))
            .collect(),
    }];
    if kind == InodeKind::Directory {
        rows.extend(serials[1..].iter().map(|parent| DirectoryUpdate {
            parent: *parent,
            changes: Vec::new(),
        }));
    }
    let mut values = vec![InodeUpdate {
        serial: 1,
        value: inode(InodeKind::Directory),
    }];
    values.extend(serials[1..].iter().map(|serial| InodeUpdate {
        serial: *serial,
        value: inode(kind),
    }));
    (rows, values, serials)
}
fn input<'a>(
    rows: &'a [DirectoryUpdate],
    values: &'a [InodeUpdate],
    fresh: &'a [u64],
) -> FilesystemInput<'a> {
    FilesystemInput {
        base: None,
        scope: scope_for_seed([0x45; 32]),
        root_serial: 1,
        directories: rows,
        inodes: values,
        new_inodes: fresh,
        resources: resources(),
    }
}
fn checked(
    reader: &dyn AuthenticatedObjects,
    source: &dyn PreparedBindingRows,
    state: &mut ObservedSites,
    selected: &SiteConstructionScopes,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    check_with_sites(reader, source, unreachable, work, state, selected.sites()).map(|_| ())
}

#[test]
fn wide_births_acknowledge_three128_windows_then_verify_and_retire_before_roots() {
    let (d, v, fresh) = fixture(InodeKind::Symlink, 257);
    let operation = input(&d, &v, &fresh);
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, d.len(), 257);
    checked(
        &TreeStore::new(),
        &source,
        &mut state,
        &selected,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    assert_eq!(state.batches, vec![128, 128, 1]);
    assert_eq!(state.final_pages, 3);
    assert_eq!(state.peak_page, 128);
    assert_eq!(state.retirements, 1);
    assert_eq!(state.abandonments, 0);
    assert_eq!(state.parent_queries, 0);
    assert_eq!(state.root_capacities.get(), 0);
    assert_eq!(source.whole.get(), 0);
    eprintln!("C1 site dispatch: K257,3 birth batches,3 final pages,window128; finite external semantic oracle, no physical/speed proof");
}

#[test]
fn regular_aliases_create_zero_sites_and_preserve_the_old_zero_additions_result() {
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
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, 1, 129);
    checked(
        &TreeStore::new(),
        &source,
        &mut state,
        &selected,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    assert!(state.batches.is_empty());
    assert_eq!(state.final_pages, 1);
    assert_eq!(state.parent_queries, 0);
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
fn first_window_and_cross128_duplicates_precede_a_later_missing_kind_and_terminalize_once() {
    for count in [2, 130] {
        let (mut d, v, fresh) = fixture(InodeKind::Symlink, count);
        d[0].changes[count - 1].1 = Some(2);
        d[0].changes.push((name(count), Some(9_999)));
        let operation = input(&d, &v, &fresh);
        let source = PointSource::new(&operation);
        let selected = scopes(source.binding_source_id().unwrap());
        let mut state = ObservedSites::new(&selected, 1, count + 1);
        assert_eq!(
            checked(
                &TreeStore::new(),
                &source,
                &mut state,
                &selected,
                &BTreeMap::new(),
                &mut ValidationWork::default()
            ),
            Err(ContentError::InvalidRecord("multiple parents"))
        );
        assert_eq!(state.abandonments, 1);
        assert_eq!(
            state.batches,
            if count == 2 { Vec::new() } else { vec![128] }
        );
        assert!(state.capacity(selected.roots()).is_err());
        assert!(checked(
            &TreeStore::new(),
            &source,
            &mut state,
            &selected,
            &BTreeMap::new(),
            &mut ValidationWork::default()
        )
        .is_err());
        assert_eq!(state.retirements, 0);
    }
}

#[test]
fn original_provider_failures_and_wrong_membership_maximum_keep_roots_disabled() {
    for failure in [
        "site presence",
        "site append",
        "site close",
        "site final",
        "site page",
        "site retire",
    ] {
        let (d, v, fresh) = fixture(InodeKind::Symlink, 2);
        let operation = input(&d, &v, &fresh);
        let source = PointSource::new(&operation);
        let selected = scopes(source.binding_source_id().unwrap());
        let mut state = ObservedSites::new(&selected, 1, 2);
        state.fail_at = Some(failure);
        assert_eq!(
            checked(
                &TreeStore::new(),
                &source,
                &mut state,
                &selected,
                &BTreeMap::new(),
                &mut ValidationWork::default()
            ),
            Err(ContentError::ProviderFailure { what: failure })
        );
        assert_eq!(state.abandonments, 1);
        assert_eq!(state.retirements, 0);
        assert!(state.capacity(selected.roots()).is_err());
    }
    let (d, v, fresh) = fixture(InodeKind::Symlink, 2);
    let operation = input(&d, &v, &fresh);
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, 1, 2);
    state.corrupt_maximum = true;
    assert_eq!(
        checked(
            &TreeStore::new(),
            &source,
            &mut state,
            &selected,
            &BTreeMap::new(),
            &mut ValidationWork::default()
        ),
        Err(ContentError::InvalidOrderingRecord(
            "site acknowledged membership"
        ))
    );
    assert_eq!(state.abandonments, 1);
    assert_eq!(state.parent_queries, 0);
}

fn directory_session(count: usize) -> (Session, Vec<u64>) {
    directory_session_named(count, name)
}
fn directory_session_named(count: usize, name: fn(usize) -> PathName) -> (Session, Vec<u64>) {
    let mut session = Session::new(1).unwrap();
    let serials: Vec<_> = (0..count).map(|_| session.allocate()).collect();
    let mut d = vec![DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .enumerate()
            .map(|(i, serial)| (name(i), Some(*serial)))
            .collect(),
    }];
    d.extend(serials.iter().map(|parent| DirectoryUpdate {
        parent: *parent,
        changes: Vec::new(),
    }));
    let v: Vec<_> = serials
        .iter()
        .map(|serial| InodeUpdate {
            serial: *serial,
            value: inode(InodeKind::Directory),
        })
        .collect();
    session.apply(&d, &v, &serials).unwrap();
    (session, serials)
}

#[test]
fn full_name255_star_uses_the_same_point_and_visit_law_without_retained_names() {
    fn full_name(i: usize) -> PathName {
        PathName::new(&format!("{}n{i:06}", "a".repeat(248))).unwrap()
    }
    let (session, serials) = directory_session_named(129, full_name);
    let d = [DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .enumerate()
            .map(|(i, _)| (full_name(i), Some(serials[(i + 1) % serials.len()])))
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
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, 1, 129);
    let mut work = ValidationWork::default();
    checked(
        &session.store,
        &source,
        &mut state,
        &selected,
        &BTreeMap::new(),
        &mut work,
    )
    .unwrap();
    assert_eq!(work.entries_examined, 258);
    assert_eq!(state.parent_pages, 2);
    assert_eq!(source.points.get(), 387);
    assert_eq!(source.whole.get(), 0);
    assert!(state.observed.is_empty());
}

#[test]
fn star_permutation_follows_each_changed_site_once_and_skips_old_serial_edges() {
    let (session, serials) = directory_session(129);
    let d = [DirectoryUpdate {
        parent: 1,
        changes: serials
            .iter()
            .enumerate()
            .map(|(i, _)| (name(i), Some(serials[(i + 1) % serials.len()])))
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
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, 1, 129);
    let mut work = ValidationWork::default();
    checked(
        &session.store,
        &source,
        &mut state,
        &selected,
        &BTreeMap::new(),
        &mut work,
    )
    .unwrap();
    assert_eq!(work.entries_examined, 258);
    assert_eq!(state.parent_pages, 2);
    assert!(state.observed.is_empty());
    assert_eq!(source.points.get(), 3 * serials.len() as u64);
    assert_eq!(state.batches, vec![128, 1]);
    assert_eq!(state.final_pages, 2);
    assert_eq!(source.whole.get(), 0);
    eprintln!("C1 permutation count proof:N129,258 entry visits,2 parent pages,387 point resolutions,0 historical binding rows");
}

#[test]
fn independent_uncertified_base_preserves_or_between_removed_and_retained_old_aliases() {
    let (store, base, scope) = uncertified_base();
    let d = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(PathName::new("target").unwrap(), Some(4))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: vec![(PathName::new("x").unwrap(), None)],
        },
    ];
    let operation = FilesystemInput {
        base: Some(base),
        scope,
        root_serial: 1,
        directories: &d,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, 2, 2);
    checked(
        &store,
        &source,
        &mut state,
        &selected,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    let mut facts: Vec<_> = state.observed.iter().map(|fact| fact.legal()).collect();
    facts.sort_unstable();
    assert_eq!(facts, vec![false, true]);
    assert_eq!(state.retirements, 1);
    // The second old alias survives; this acceptance is the existing v1 OR
    // policy, not a claim that authentication certified single-parent topology.
    let retained = [DirectoryUpdate {
        parent: 1,
        changes: vec![(PathName::new("target").unwrap(), Some(4))],
    }];
    let operation = FilesystemInput {
        directories: &retained,
        ..operation
    };
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, 1, 1);
    assert_eq!(
        checked(
            &store,
            &source,
            &mut state,
            &selected,
            &BTreeMap::new(),
            &mut ValidationWork::default()
        ),
        Err(ContentError::InvalidRecord("multiple parents"))
    );
    assert_eq!(state.abandonments, 1);
    assert_eq!(state.retirements, 0);
}

#[test]
fn a_new_name_is_followed_and_removing_an_old_back_edge_allows_the_move() {
    let mut session = Session::new(1).unwrap();
    let d = session.allocate();
    let e = session.allocate();
    let rows = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(PathName::new("d").unwrap(), Some(d))],
        },
        DirectoryUpdate {
            parent: d,
            changes: vec![(PathName::new("e").unwrap(), Some(e))],
        },
        DirectoryUpdate {
            parent: e,
            changes: Vec::new(),
        },
    ];
    let values = [
        InodeUpdate {
            serial: d,
            value: inode(InodeKind::Directory),
        },
        InodeUpdate {
            serial: e,
            value: inode(InodeKind::Directory),
        },
    ];
    session.apply(&rows, &values, &[d, e]).unwrap();
    let rows = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![
                (PathName::new("d").unwrap(), None),
                (PathName::new("e").unwrap(), Some(e)),
            ],
        },
        DirectoryUpdate {
            parent: d,
            changes: vec![(PathName::new("e").unwrap(), None)],
        },
        DirectoryUpdate {
            parent: e,
            changes: vec![(PathName::new("new").unwrap(), Some(d))],
        },
    ];
    let operation = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &rows,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, rows.len(), 4);
    checked(
        &session.store,
        &source,
        &mut state,
        &selected,
        &BTreeMap::new(),
        &mut ValidationWork::default(),
    )
    .unwrap();
    assert!(state.parent_pages >= 2);
    assert_eq!(state.observed.iter().filter(|fact| fact.legal()).count(), 2);
    // With the old back edge retained and E rebound as well, the original
    // alias verdict wins over the effective cycle in the same candidate.
    let conflicted = [rows[0].clone(), rows[2].clone()];
    let conflict_operation = FilesystemInput {
        directories: &conflicted,
        ..operation
    };
    let conflict_source = PointSource::new(&conflict_operation);
    let conflict_scopes = scopes(conflict_source.binding_source_id().unwrap());
    let mut conflict_state = ObservedSites::new(&conflict_scopes, 2, 3);
    let mut conflict_work = ValidationWork::default();
    assert_eq!(
        checked(
            &session.store,
            &conflict_source,
            &mut conflict_state,
            &conflict_scopes,
            &BTreeMap::new(),
            &mut conflict_work
        ),
        Err(ContentError::InvalidRecord("multiple parents"))
    );
    assert_eq!(conflict_work.inode_pages_by_site.cycles, 0);
    assert_eq!(conflict_state.retirements, 0);
    assert_eq!(conflict_state.abandonments, 1);
    let bad_rows = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(PathName::new("d").unwrap(), None)],
        },
        DirectoryUpdate {
            parent: e,
            changes: rows[2].changes.clone(),
        },
    ];
    let operation = FilesystemInput {
        directories: &bad_rows,
        ..operation
    };
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, bad_rows.len(), 3);
    assert_eq!(
        checked(
            &session.store,
            &source,
            &mut state,
            &selected,
            &BTreeMap::new(),
            &mut ValidationWork::default()
        ),
        Err(ContentError::InvalidRecord("effective tree cycle"))
    );
    assert_eq!(state.retirements, 1);
    assert_eq!(state.abandonments, 1);
    assert!(state.capacity(selected.roots()).is_err());
}

#[test]
fn zero_active_stored_sites_do_not_add_a_base_walk_or_walk_limit_refusal() {
    let (session, _) = directory_session(129);
    let d = [DirectoryUpdate {
        parent: 999,
        changes: vec![(PathName::new("s").unwrap(), Some(4))],
    }];
    let v = [InodeUpdate {
        serial: 999,
        value: inode(InodeKind::Directory),
    }];
    let operation = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &d,
        inodes: &v,
        new_inodes: &[999],
        resources: resources(),
    };
    let source = PointSource::new(&operation);
    let selected = scopes(source.binding_source_id().unwrap());
    let mut state = ObservedSites::new(&selected, 1, 1);
    let mut work = ValidationWork::default();
    checked(
        &session.store,
        &source,
        &mut state,
        &selected,
        &BTreeMap::from([(999, ())]),
        &mut work,
    )
    .unwrap();
    assert_eq!(state.parent_queries, 0);
    assert_eq!(state.parent_pages, 0);
    assert_eq!(work.inode_pages_by_site.aliases, 0);
    assert!(state.observed.is_empty());
}

#[test]
fn foreign_and_unavailable_source_identity_leave_the_rightful_owner_unselected_in_all_entries() {
    let (d, v, fresh) = fixture(InodeKind::Symlink, 1);
    let operation = input(&d, &v, &fresh);
    for foreign in [true, false] {
        for route in 0..3 {
            let mut source = PointSource::new(&operation);
            let actual = source.binding_source_id().unwrap();
            let selected = if foreign {
                scopes(BindingAuthority::new().unwrap().source_id())
            } else {
                scopes(actual)
            };
            source.source_error = !foreign;
            let mut state = ObservedSites::new(&selected, 1, 1);
            let result = if route == 0 {
                checked(
                    &TreeStore::new(),
                    &source,
                    &mut state,
                    &selected,
                    &BTreeMap::new(),
                    &mut ValidationWork::default(),
                )
            } else {
                let fixture = TreeStore::new();
                let mut output = TreeStore::new();
                let mut objects = FilesystemObjects::new(&fixture, &mut output);
                let phases = FilesystemPhases::disabled();
                if route == 1 {
                    build_filesystem_binding_rows_with_site_state(
                        &mut objects,
                        &source,
                        None,
                        &mut state,
                        &selected,
                        &phases,
                    )
                    .map(|_| ())
                } else {
                    update_filesystem_binding_rows_with_site_state(
                        &mut objects,
                        &source,
                        None,
                        &mut state,
                        &selected,
                        &phases,
                    )
                    .map(|_| ())
                }
            };
            assert_eq!(
                result,
                Err(if foreign {
                    ContentError::InvalidOrderingRecord("site input source")
                } else {
                    ContentError::ProviderFailure {
                        what: "source identity",
                    }
                })
            );
            assert_eq!(state.stage, 0);
            assert_eq!(state.abandonments, 0);
            assert!(state.batches.is_empty());
            assert_eq!(state.root_capacities.get(), 0);
            assert_eq!(source.headers.get(), 0);
        }
    }
}

#[test]
fn matching_source_cursor_and_early_entry_and_post_retirement_capacity_failures_abandon_once() {
    let (d, v, fresh) = fixture(InodeKind::Directory, 1);
    let mut operation = input(&d, &v, &fresh);
    for cause in 0..4 {
        operation.base = None;
        operation.resources = resources();
        let mut source = PointSource::new(&operation);
        source.refuse_bindings = cause == 0;
        let selected = scopes(source.binding_source_id().unwrap());
        let mut state = ObservedSites::new(&selected, if cause == 3 { 1 } else { 2 }, 1);
        let fixture = TreeStore::new();
        let mut output = TreeStore::new();
        let mut objects = FilesystemObjects::new(&fixture, &mut output);
        let phases = FilesystemPhases::disabled();
        let result = if cause == 1 {
            update_filesystem_binding_rows_with_site_state(
                &mut objects,
                &source,
                None,
                &mut state,
                &selected,
                &phases,
            )
        } else {
            if cause == 2 {
                operation.resources.ordering_bytes = 0;
                let source = PointSource::new(&operation);
                let selected = scopes(source.binding_source_id().unwrap());
                let mut state = ObservedSites::new(&selected, 2, 1);
                let mut objects = FilesystemObjects::new(&fixture, &mut output);
                assert!(build_filesystem_binding_rows_with_site_state(
                    &mut objects,
                    &source,
                    None,
                    &mut state,
                    &selected,
                    &phases
                )
                .is_err());
                assert_eq!(state.abandonments, 1);
                assert!(state.batches.is_empty());
                continue;
            }
            build_filesystem_binding_rows_with_site_state(
                &mut objects,
                &source,
                None,
                &mut state,
                &selected,
                &phases,
            )
        };
        assert!(result.is_err());
        assert_eq!(state.abandonments, 1);
        assert!(state.capacity(selected.roots()).is_err());
        assert_eq!(state.retirements, u64::from(cause == 3));
        assert!(output.is_empty());
    }
}

// Independently encoded finite base with two old aliases for symlink4. Local
// canonical authentication does not imply certified namespace-parent topology.
fn envelope(value: &[u8]) -> Vec<u8> {
    let mut bytes = b"LFSO\x01".to_vec();
    bytes.extend_from_slice(&((value.len() + 4) as u32).to_be_bytes());
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
    bytes
}
fn leaf(magic: &[u8; 8], role: u8, count: usize, rows: &[u8]) -> Vec<u8> {
    let mut value = magic.to_vec();
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(&[role, 0, 0]);
    value.extend_from_slice(&(count as u16).to_be_bytes());
    value.extend_from_slice(&(count as u64).to_be_bytes());
    value.extend_from_slice(&(rows.len() as u64).to_be_bytes());
    value.extend_from_slice(rows);
    envelope(&value)
}
fn uncertified_base() -> (TreeStore, FilesystemRootId, InodeScope) {
    let scope = scope_for_seed([0x79; 32]);
    let mut store = TreeStore::new();
    let mut roots = Vec::new();
    for bindings in [vec![("a", 2u64), ("b", 3)], vec![("x", 4)], vec![("y", 4)]] {
        let mut bytes = Vec::new();
        for (name, serial) in &bindings {
            bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(&serial.to_be_bytes());
        }
        roots.push(store.insert(
            ObjectRole::DirectoryLeaf,
            leaf(b"LFS6NSP\0", 1, bindings.len(), &bytes),
        ));
    }
    let mut rows = Vec::new();
    for serial in 1u64..=4 {
        rows.extend_from_slice(&serial.to_be_bytes());
        rows.push(if serial == 4 { 3 } else { 2 });
        rows.extend_from_slice(&u64::from(serial != 1).to_be_bytes());
        rows.extend_from_slice(if serial == 4 {
            &[0x91; 32]
        } else {
            roots[serial as usize - 1].as_bytes()
        });
        rows.extend_from_slice(&[0x92; 32]);
    }
    let table = store.insert(ObjectRole::InodeLeaf, leaf(b"LFS6INT\0", 7, 4, &rows));
    let profile = ObjectId::for_bytes(b"layerfs/namespace-profile/scoped-inline/v1\0scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5");
    let mut root = b"LFS6FSR\0".to_vec();
    root.extend_from_slice(&1u16.to_be_bytes());
    root.extend_from_slice(&[6, 0]);
    root.extend_from_slice(profile.as_bytes());
    root.extend_from_slice(scope.object().as_bytes());
    root.extend_from_slice(&1u64.to_be_bytes());
    root.extend_from_slice(table.as_bytes());
    let base = store.insert(ObjectRole::FilesystemRoot, envelope(&root));
    (store, FilesystemRootId(base), scope)
}

struct PointSource<'a> {
    inner: SliceBindingRows<'a>,
    source_error: bool,
    refuse_bindings: bool,
    points: Cell<u64>,
    whole: Cell<u64>,
    headers: Cell<u64>,
}
impl<'a> PointSource<'a> {
    fn new(input: &'a FilesystemInput<'a>) -> Self {
        Self {
            inner: SliceBindingRows::new(input).unwrap(),
            source_error: false,
            refuse_bindings: false,
            points: Cell::new(0),
            whole: Cell::new(0),
            headers: Cell::new(0),
        }
    }
}
impl RowSource for PointSource<'_> {
    fn directory_rows(&self) -> usize {
        self.inner.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.inner.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.inner.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        self.whole.set(self.whole.get() + 1);
        Err(ContentError::UnsupportedProfile {
            what: "whole source",
        })
    }
    fn directory_for(&self, _: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.whole.set(self.whole.get() + 1);
        Err(ContentError::UnsupportedProfile {
            what: "whole source",
        })
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.inner.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.inner.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.inner.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.inner.new_position(serial)
    }
}
impl PreparedRows for PointSource<'_> {
    fn base(&self) -> Option<FilesystemRootId> {
        self.inner.base()
    }
    fn scope(&self) -> InodeScope {
        self.inner.scope()
    }
    fn root_serial(&self) -> u64 {
        self.inner.root_serial()
    }
    fn resources(&self) -> layerfs_content::filesystem::FilesystemResources {
        self.inner.resources()
    }
}
impl BindingRows for PointSource<'_> {
    fn binding_source_id(&self) -> ContentResult<BindingSourceId> {
        if self.source_error {
            Err(ContentError::ProviderFailure {
                what: "source identity",
            })
        } else {
            self.inner.binding_source_id()
        }
    }
    fn binding_at(&self, point: &BindingPoint) -> ContentResult<(PathName, Option<u64>)> {
        self.points.set(self.points.get() + 1);
        self.inner.binding_at(point)
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        self.headers.set(self.headers.get() + 1);
        self.inner.directory_headers()
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        self.inner.directory_header(parent)
    }
    fn bindings(&self, header: &DirectoryHeader) -> ContentResult<Box<dyn BindingRowSource + '_>> {
        if self.refuse_bindings {
            Err(ContentError::ProviderFailure {
                what: "selected cursor",
            })
        } else {
            self.inner.bindings(header)
        }
    }
    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        self.inner.binding_for(parent, name)
    }
}
