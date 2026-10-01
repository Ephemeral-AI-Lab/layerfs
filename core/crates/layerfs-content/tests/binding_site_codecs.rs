//! Independent closed site grammar, domains, counted cursors and monotone facts.
#[path = "support/site_state.rs"]
mod oracle;
use layerfs_content::filesystem::rows::{BindingAuthority, BindingPoint};
use layerfs_content::filesystem::state::*;
use layerfs_content::ContentError;
use oracle::{
    reference_point, reference_record, reference_scope, reference_seal, scopes, ObservedSites,
};

fn selected() -> (BindingAuthority, SiteConstructionScopes) {
    let authority = BindingAuthority::new().unwrap();
    let selected = scopes(authority.source_id());
    (authority, selected)
}
fn birth(
    authority: &BindingAuthority,
    selected: &SiteConstructionScopes,
    serial: u64,
    parent: u64,
    ordinal: u32,
    stored: bool,
) -> SiteRecord {
    let header = authority
        .header(parent, u64::MAX, ordinal + 1, u64::from(ordinal + 1) * 11)
        .unwrap();
    SiteRecord::birth(
        selected.sites(),
        serial,
        BindingPoint::new(&header, ordinal).unwrap(),
        stored,
    )
    .unwrap()
}

#[test]
fn literal_scope89_point28_and_all_closed_frame60_flag_values() {
    let (authority, selected) = selected();
    let row = birth(
        &authority,
        &selected,
        i64::MAX as u64,
        i64::MAX as u64,
        0,
        true,
    );
    assert_eq!(
        selected.sites().as_bytes(),
        reference_scope(selected.sites())
    );
    assert_eq!(
        row.point().encode(),
        reference_point(authority.source_id(), i64::MAX as u64, u64::MAX, 0)
    );
    for observed in [row, row.observe(false).unwrap(), row.observe(true).unwrap()] {
        let expected = reference_record(
            selected.sites(),
            observed.key().serial(),
            observed.point(),
            observed.flags(),
        );
        assert_eq!(observed.encode(), expected);
        assert_eq!(
            SiteRecord::decode(selected.sites(), &expected).unwrap(),
            observed
        );
    }
    let fresh = birth(&authority, &selected, 2, 1, 1, false);
    assert_eq!(
        fresh.encode(),
        reference_record(selected.sites(), 2, fresh.point(), 0)
    );
    assert!(fresh.observe(false).is_err());
    assert_eq!(
        row.observe(true).unwrap().observe(false).unwrap().flags(),
        7
    );
    assert_eq!(row.observe(false).unwrap().birth_projection(), row);
}

#[test]
fn independent_empty_and_non_key_sorted_birth_seals_and_key_sorted_final_seals() {
    let (authority, selected) = selected();
    let rows = [
        birth(&authority, &selected, 9, 1, 0, true),
        birth(&authority, &selected, 2, 1, 1, false),
    ];
    for expected in [Vec::new(), rows.to_vec()] {
        let mut birth = SiteBirthLedger::new(selected.sites().clone()).unwrap();
        birth.acknowledge(&expected).unwrap();
        assert_eq!(
            birth.seal().encode(),
            reference_seal(selected.sites(), &expected, true)
        );
        assert_eq!(birth.maximum(), expected.iter().map(|row| row.key()).max());
        let mut ordered = expected;
        ordered.sort_by_key(|row| row.key());
        let mut final_ledger = SiteLedger::new(selected.sites().clone()).unwrap();
        final_ledger.acknowledge(&ordered).unwrap();
        assert_eq!(
            final_ledger.seal().encode(),
            reference_seal(selected.sites(), &ordered, false)
        );
        assert_ne!(final_ledger.seal().digest(), birth.seal().digest());
    }
    let mut final_ledger = SiteLedger::new(selected.sites().clone()).unwrap();
    assert!(final_ledger.acknowledge(&rows).is_err());
    let mut birth_ledger = SiteBirthLedger::new(selected.sites().clone()).unwrap();
    assert!(birth_ledger.acknowledge(&[rows[1], rows[0]]).is_err());
    assert_eq!(birth_ledger.records(), 0);
}

#[test]
fn exact_headers96_138_164_171_189_and_valid_some_zero_ordinal() {
    let (authority, selected) = selected();
    let row = birth(&authority, &selected, 2, 1, 0, true);
    let mut ledger = SiteBirthLedger::new(selected.sites().clone()).unwrap();
    let mut expected = [0; 96];
    expected[0] = 1;
    expected[1..90].copy_from_slice(&reference_scope(selected.sites()));
    expected[90..92].copy_from_slice(&1u16.to_be_bytes());
    expected[92..96].copy_from_slice(&60u32.to_be_bytes());
    assert_eq!(ledger.append_header(&[row]).unwrap(), expected);
    ledger.acknowledge(&[row]).unwrap();
    let members = SiteMembership::new(ledger.seal(), Some(row.key())).unwrap();
    let mut expected_members = [0; 164];
    expected_members[..138].copy_from_slice(&reference_seal(selected.sites(), &[row], true));
    expected_members[138] = 1;
    expected_members[139..164].copy_from_slice(row.key().as_bytes());
    assert_eq!(members.encode(), expected_members);
    let parent = SiteParentPage::new(members.clone(), 1, Some(0), vec![row], true).unwrap();
    let mut expected_parent = [0; 189];
    expected_parent[..164].copy_from_slice(&expected_members);
    expected_parent[164..172].copy_from_slice(&1u64.to_be_bytes());
    expected_parent[172] = 1; // actual last Some(0), not absent
    expected_parent[177] = 1; // actual maximum Some(0), not absent
    expected_parent[182..184].copy_from_slice(&1u16.to_be_bytes());
    expected_parent[184..188].copy_from_slice(&60u32.to_be_bytes());
    expected_parent[188] = 1;
    assert_eq!(parent.encode_header(), expected_parent);
    let eof = SiteParentPage::after(members, 1, Some(0), Some(0), Vec::new(), true).unwrap();
    assert_eq!((eof.last(), eof.maximum()), (Some(0), Some(0)));
    let mut final_ledger = SiteLedger::new(selected.sites().clone()).unwrap();
    final_ledger.acknowledge(&[row]).unwrap();
    let page = SitePage::new(final_ledger.seal(), vec![row], true).unwrap();
    let mut expected_page = [0; 171];
    expected_page[..138].copy_from_slice(&reference_seal(selected.sites(), &[row], false));
    expected_page[138] = 1;
    expected_page[139..164].copy_from_slice(row.key().as_bytes());
    expected_page[164..166].copy_from_slice(&1u16.to_be_bytes());
    expected_page[166..170].copy_from_slice(&60u32.to_be_bytes());
    expected_page[170] = 1;
    assert_eq!(page.encode_header(), expected_page);
}

#[test]
fn frame_flags_lengths_foreign_source_scope_and_table_fences() {
    let (authority, selected) = selected();
    let row = birth(&authority, &selected, 2, 1, 0, true);
    let original = reference_record(selected.sites(), 2, row.point(), 1);
    for flag in [2, 4, 5, 6, 8, 255] {
        let mut bad = original;
        bad[31] = flag;
        assert!(SiteRecord::decode(selected.sites(), &bad).is_err());
    }
    for index in [0, 18, 27, 32] {
        let mut bad = original;
        bad[index] ^= 1;
        assert!(SiteRecord::decode(selected.sites(), &bad).is_err());
    }
    assert!(SiteRecord::decode(selected.sites(), &original[..59]).is_err());
    assert!(SiteKey::new(selected.sites(), 0).is_err());
    assert!(SiteKey::new(selected.sites(), i64::MAX as u64 + 1).is_err());
    assert!(StateKey::directory_root(selected.sites().state(), 2).is_err());
    assert!(ClaimKey::new(selected.sites().state(), 2).is_err());
    assert!(SiteScope::new(selected.roots().clone(), authority.source_id()).is_err());
    let other = scopes(BindingAuthority::new().unwrap().source_id());
    assert!(SiteRecord::decode(other.sites(), &original).is_err());
    let mut observation = SiteObservation::new(row.key(), true).encode();
    observation[25] = 2;
    assert!(SiteObservation::decode(selected.sites(), &observation).is_err());
    let mut seal = reference_seal(selected.sites(), &[row], true);
    seal[105] ^= 1;
    assert!(SiteBirthSeal::decode(selected.sites(), &seal).is_err());
}

#[test]
fn bounded_windows_include_headers_actual_capacity_and_refuse129th_record() {
    let (authority, selected) = selected();
    let rows: Vec<_> = (0..129)
        .map(|i| birth(&authority, &selected, i as u64 + 2, 1, i, true))
        .collect();
    let ledger = SiteBirthLedger::new(selected.sites().clone()).unwrap();
    assert_eq!(
        ledger.append_header(&rows[..128]).unwrap().len() + 128 * 60,
        7776
    );
    assert!(ledger.validate_append(&rows).is_err());
    assert_eq!(
        SitePageLimit::new(128, 7851).unwrap().fitting_records(),
        128
    );
    assert_eq!(
        SiteParentPageLimit::new(128, 7869)
            .unwrap()
            .fitting_records(),
        128
    );
    assert!(SitePageLimit::new(129, 65536).is_err());
    assert!(SiteParentPageLimit::new(128, 188).is_err());
    let final_seal = SiteLedger::new(selected.sites().clone()).unwrap().seal();
    assert!(SitePage::new(final_seal, Vec::with_capacity(129), true).is_err());
    assert!(SiteCapacity::new(65536, 4128768)
        .unwrap()
        .check_requested(65537)
        .is_err());
    assert!(SiteCapacity::new(65536, 4128768)
        .unwrap()
        .check_requested(65536)
        .is_ok());
}

#[test]
fn header_only_empty_eof_and_sticky_corrupt_digest_never_resend_or_retire() {
    let (authority, selected) = selected();
    let mut state = ObservedSites::new(&selected, 1, 2);
    let empty = SiteBirthLedger::new(selected.sites().clone())
        .unwrap()
        .seal();
    let members = state.site_close_membership(&empty).unwrap();
    let mut parent = SiteParentCursor::new(&mut state, members.clone(), 1).unwrap();
    let page = parent
        .next_page(SiteParentPageLimit::new(128, 189).unwrap())
        .unwrap()
        .unwrap();
    assert!(page.eof());
    assert_eq!(page.encoded_len(), 189);
    assert!(parent
        .next_page(SiteParentPageLimit::default())
        .unwrap()
        .is_none());
    drop(parent);
    let seal = state.site_final_seal(&members).unwrap();
    let mut cursor = SiteCursor::new(&mut state, seal.clone()).unwrap();
    let page = cursor
        .next_page(SitePageLimit::new(128, 171).unwrap())
        .unwrap()
        .unwrap();
    assert!(page.eof());
    assert_eq!(page.encoded_len(), 171);
    assert!(cursor
        .next_page(SitePageLimit::default())
        .unwrap()
        .is_none());
    drop(cursor);
    state.site_retire(&seal).unwrap();
    let mut state = ObservedSites::new(&selected, 1, 1);
    let row = birth(&authority, &selected, 2, 1, 0, true);
    state.site_insert_batch(selected.sites(), &[row]).unwrap();
    let mut birth = SiteBirthLedger::new(selected.sites().clone()).unwrap();
    birth.acknowledge(&[row]).unwrap();
    let members = state.site_close_membership(&birth.seal()).unwrap();
    let seal = state.site_final_seal(&members).unwrap();
    state.corrupt_page = true;
    let mut cursor = SiteCursor::new(&mut state, seal).unwrap();
    let first = cursor.next_page(SitePageLimit::default()).unwrap_err();
    assert_eq!(
        first,
        ContentError::InvalidOrderingRecord("site page digest")
    );
    assert_eq!(
        cursor.next_page(SitePageLimit::default()).unwrap_err(),
        first
    );
    drop(cursor);
    assert_eq!(state.final_pages, 1);
    assert_eq!(state.retirements, 0);
}

#[test]
fn parent_projection_is_birth_only_and_ordered_with_exact_maximum() {
    let (authority, selected) = selected();
    let first = birth(&authority, &selected, 9, 1, 0, true);
    let second = birth(&authority, &selected, 2, 1, 1, true);
    let mut state = ObservedSites::new(&selected, 1, 2);
    state
        .site_insert_batch(selected.sites(), &[first, second])
        .unwrap();
    let mut ledger = SiteBirthLedger::new(selected.sites().clone()).unwrap();
    ledger.acknowledge(&[first, second]).unwrap();
    let members = state.site_close_membership(&ledger.seal()).unwrap();
    state
        .site_observe_base_batch(&members, &[SiteObservation::new(first.key(), true)])
        .unwrap();
    let mut cursor = SiteParentCursor::new(&mut state, members.clone(), 1).unwrap();
    let limit = SiteParentPageLimit::new(1, 249).unwrap();
    let page = cursor.next_page(limit).unwrap().unwrap();
    assert_eq!(page.records(), &[first]);
    assert!(!page.eof());
    let page = cursor.next_page(limit).unwrap().unwrap();
    assert_eq!(page.records(), &[second]);
    assert!(page.eof());
    assert!(cursor.next_page(limit).unwrap().is_none());
    assert!(SiteParentPage::new(
        members.clone(),
        1,
        Some(0),
        vec![first.observe(true).unwrap()],
        true
    )
    .is_err());
    assert!(SiteParentPage::new(members.clone(), 1, Some(1), vec![second, first], true).is_err());
    assert!(SiteParentPage::after(members, 1, Some(2), Some(1), Vec::new(), true).is_err());
}

#[test]
fn changed_immutable_birth_refuses_final_seal_and_never_enables_roots() {
    let (authority, selected) = selected();
    let mut state = ObservedSites::new(&selected, 1, 1);
    let row = birth(&authority, &selected, 2, 1, 0, true);
    state.site_insert_batch(selected.sites(), &[row]).unwrap();
    let mut ledger = SiteBirthLedger::new(selected.sites().clone()).unwrap();
    ledger.acknowledge(&[row]).unwrap();
    let members = state.site_close_membership(&ledger.seal()).unwrap();
    let replacement = birth(&authority, &selected, 2, 1, 1, true);
    state.rows.insert(row.key(), replacement);
    assert_eq!(
        state.site_final_seal(&members),
        Err(ContentError::InvalidOrderingRecord(
            "oracle immutable birth"
        ))
    );
    assert!(state.capacity(selected.roots()).is_err());
    assert_eq!(state.retirements, 0);
}
