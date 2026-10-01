//! Independent legacy-stack priority and exact continuation/profile vectors.
use layerfs_content::filesystem::rows::BindingAuthority;
use layerfs_content::filesystem::state::*;
use layerfs_content::filesystem::PathName;
use std::collections::{BTreeMap, BTreeSet};

fn membership() -> SiteMembership {
    let authority = BindingAuthority::new().unwrap();
    let mut selection = StateSelection::issue([0x59; 32]).unwrap();
    selection.bind_owner([0x64; 32]).unwrap();
    let scope = SiteScope::new(
        StateScope::new(selection, 1, StateTable::BindingSites).unwrap(),
        authority.source_id(),
    )
    .unwrap();
    SiteMembership::new(SiteBirthLedger::new(scope).unwrap().seal(), None).unwrap()
}
fn finish_current(state: &mut dyn AliasFrontier, members: &SiteMembership, current: AliasCurrent) {
    let start = AliasProgress::initial();
    let sites = AliasProgress::base(None);
    let end = AliasProgress::complete();
    state
        .alias_advance(members, current, &start, &sites)
        .unwrap();
    state.alias_advance(members, current, &sites, &end).unwrap();
    state.alias_complete(members, current).unwrap();
}
#[test]
fn repeated_discoveries_preserve_independent_legacy_stack_priority_and_follow_once() {
    let adjacency = BTreeMap::from([
        (1, vec![2, 3, 2, 4]),
        (4, vec![6, 3]),
        (3, vec![5, 6]),
        (2, vec![7, 5]),
        (6, vec![6, 1]),
    ]);
    let mut stack = vec![1];
    let mut seen = BTreeSet::new();
    let mut expected = vec![];
    while let Some(serial) = stack.pop() {
        if seen.insert(serial) {
            expected.push(serial);
            stack.extend(adjacency.get(&serial).into_iter().flatten());
        }
    }
    assert_eq!(expected, vec![1, 4, 3, 6, 5, 2, 7]);
    let members = membership();
    let mut actual = ResidentAliasFrontier::new(AliasCapacity::new(128, 64 * 1024, 0).unwrap());
    actual.alias_begin(&members, Some(1)).unwrap();
    let mut found = vec![];
    while let Some(current) = actual.alias_take(&members).unwrap() {
        found.push(current.serial);
        actual
            .alias_enqueue(
                &members,
                adjacency.get(&current.serial).map_or(&[], |a| a.as_slice()),
            )
            .unwrap();
        finish_current(&mut actual, &members, current);
    }
    assert_eq!(found, expected);
    let seal = actual.alias_finish(&members).unwrap();
    assert_eq!(
        (seal.records, seal.sequence, seal.maximum),
        (7, 10, Some(7))
    );
    actual.alias_retire(&seal).unwrap();
    assert!(actual.alias_take(&members).is_err());
}
#[test]
fn wide_and_deep_discovery_and_retirement_are_independent_of_resident_stack_depth() {
    let members = membership();
    let mut state = ResidentAliasFrontier::new(AliasCapacity::new(1200, 128 * 1024, 0).unwrap());
    state.alias_begin(&members, Some(1)).unwrap();
    let root = state.alias_take(&members).unwrap().unwrap();
    for chunk in (2..=385).collect::<Vec<_>>().chunks(128) {
        state.alias_enqueue(&members, chunk).unwrap();
    }
    finish_current(&mut state, &members, root);
    let mut order = vec![];
    while let Some(current) = state.alias_take(&members).unwrap() {
        order.push(current.serial);
        if current.serial >= 385 {
            state
                .alias_enqueue(&members, &[current.serial + 1])
                .unwrap();
        }
        if current.serial == 1024 {
            state.alias_enqueue(&members, &[1, current.serial]).unwrap();
        }
        finish_current(&mut state, &members, current);
        if current.serial == 1024 {
            break;
        }
    }
    assert_eq!(order, (385..=1024).collect::<Vec<_>>());
    while let Some(current) = state.alias_take(&members).unwrap() {
        finish_current(&mut state, &members, current);
    }
    let seal = state.alias_finish(&members).unwrap();
    assert_eq!(seal.records, 1025);
    state.alias_retire(&seal).unwrap();
}
#[test]
fn combined_sites_frontier_capacity_refuses_before_new_discovery_and_foreign_scope_does_not_consume_owner(
) {
    let members = membership();
    let foreign = membership();
    let mut state = ResidentAliasFrontier::new(AliasCapacity::new(128, 784, 2).unwrap());
    state.alias_begin(&members, Some(1)).unwrap();
    assert!(state.alias_take(&foreign).is_err());
    let current = state.alias_take(&members).unwrap().unwrap();
    assert_eq!(current.serial, 1);
    assert!(state.alias_enqueue(&members, &[2]).is_err());
    assert!(state.alias_take(&members).is_err());
    assert!(AliasCapacity::new(1, 119, 2).is_err());
}
#[test]
fn literal_name255_and_ordinal_zero_progress_oracle_and_closed_phase_refusals() {
    let name = "a".repeat(255);
    let p = AliasProgress::base(Some(PathName::new(&name).unwrap()));
    let mut expected = [0u8; 264];
    expected[1] = 1;
    expected[2..4].copy_from_slice(&255u16.to_be_bytes());
    expected[4..259].fill(b'a');
    assert_eq!(p.encode(), expected);
    assert_eq!(AliasProgress::decode(&expected).unwrap(), p);
    let mut ordinal = [0u8; 264];
    ordinal[0] = 1;
    ordinal[259] = 1;
    assert_eq!(AliasProgress::sites(Some(0), false).encode(), ordinal);
    assert!(p.advances_to(&p).is_err());
    assert!(AliasProgress::initial()
        .advances_to(&AliasProgress::complete())
        .is_err());
    assert!(AliasProgress::sites(Some(3), false)
        .advances_to(&AliasProgress::sites(Some(2), false))
        .is_err());
    ordinal[0] = 2;
    assert!(AliasProgress::decode(&ordinal).is_err());
    expected[3] = 254;
    assert!(AliasProgress::decode(&expected).is_err());
}
