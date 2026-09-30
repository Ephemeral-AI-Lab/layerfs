//! Independent byte grammar/transcript and closed claim-provider contracts.
use std::cell::Cell;

use layerfs_content::filesystem::state::*;
use layerfs_content::ContentResult;

fn scopes() -> ConstructionScopes {
    let mut selection = StateSelection::issue([0x31; 32]).unwrap();
    selection.bind_owner([0x52; 32]).unwrap();
    ConstructionScopes::new(selection).unwrap()
}

// Independently spelled fixed grammar; no candidate record/ledger encoder used.
fn reference_record(scope: &StateScope, serial: u64) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[..2].copy_from_slice(&25u16.to_be_bytes());
    bytes[2..10].copy_from_slice(&scope.selection().token().to_be_bytes());
    bytes[10..18].copy_from_slice(&1u64.to_be_bytes());
    bytes[18] = 2;
    bytes[19..27].copy_from_slice(&serial.to_be_bytes());
    bytes[27..31].copy_from_slice(&1u32.to_be_bytes());
    bytes[31] = 1;
    bytes
}
fn reference_seal(scope: &StateScope, serials: &[u64]) -> [u8; 130] {
    let mut context = [0; 81];
    context[..32].copy_from_slice(&[0x31; 32]);
    context[32..40].copy_from_slice(&scope.selection().token().to_be_bytes());
    context[40..72].copy_from_slice(&[0x52; 32]);
    context[72..80].copy_from_slice(&1u64.to_be_bytes());
    context[80] = 2;
    let count = serials.len() as u64;
    let bytes = count * 32;
    let mut hash = blake3::Hasher::new();
    hash.update(b"layerfs/binding-claims/v1\0");
    hash.update(&context);
    for &serial in serials {
        hash.update(&reference_record(scope, serial));
    }
    hash.update(&count.to_be_bytes());
    hash.update(&bytes.to_be_bytes());
    let mut seal = [0; 130];
    seal[0] = 1;
    seal[1..82].copy_from_slice(&context);
    seal[82..90].copy_from_slice(&count.to_be_bytes());
    seal[90..98].copy_from_slice(&bytes.to_be_bytes());
    seal[98..130].copy_from_slice(hash.finalize().as_bytes());
    seal
}

#[test]
fn independent_empty_small_and_maximum_serial_transcripts() {
    let selected = scopes();
    for serials in [Vec::new(), vec![3], vec![2, 9, i64::MAX as u64]] {
        let mut ledger = ClaimLedger::new(selected.claims().clone()).unwrap();
        for &serial in &serials {
            let record = ClaimRecord::exclusive(selected.claims(), serial).unwrap();
            assert_eq!(record.encode(), reference_record(selected.claims(), serial));
            assert_eq!(
                ClaimRecord::decode(selected.claims(), &record.encode()).unwrap(),
                record
            );
            ledger.acknowledge(&[record]).unwrap();
        }
        assert_eq!(
            ledger.seal().encode(),
            reference_seal(selected.claims(), &serials)
        );
        assert_eq!(
            ClaimSeal::decode(selected.claims(), &ledger.seal().encode()).unwrap(),
            ledger.seal()
        );
    }
}

#[test]
fn closed_tables_and_scopes_refuse_before_resident_allocation_or_append() {
    let selected = scopes();
    assert!(StateKey::directory_root(selected.claims(), 2).is_err());
    assert!(ResidentState::new(selected.claims().clone(), 1).is_err());
    assert!(StateLedger::new(selected.claims().clone())
        .validate_append(&[])
        .is_err());
    assert!(ClaimKey::new(selected.roots(), 2).is_err());
    assert!(ClaimLedger::new(selected.roots().clone()).is_err());
    assert!(ResidentClaims::new(selected.roots().clone(), 1).is_err());
    let key = ClaimKey::new(selected.claims(), 2).unwrap();
    assert!(StateKey::decode(selected.claims(), key.as_bytes()).is_err());
    assert!(ClaimKey::decode(scopes().claims(), key.as_bytes()).is_err());
    assert!(ClaimKey::new(selected.claims(), 0).is_err());
    assert!(ClaimKey::new(selected.claims(), i64::MAX as u64 + 1).is_err());
}

#[test]
fn malformed_fixed_frame_and_terminal_arithmetic_are_refused() {
    let selected = scopes();
    let original = reference_record(selected.claims(), 7);
    for index in [0, 18, 27, 31] {
        let mut bad = original;
        bad[index] ^= 1;
        assert!(ClaimRecord::decode(selected.claims(), &bad).is_err());
    }
    assert!(ClaimRecord::decode(selected.claims(), &original[..31]).is_err());
    let mut seal = reference_seal(selected.claims(), &[7]);
    seal[97] ^= 1;
    assert!(ClaimSeal::decode(selected.claims(), &seal).is_err());
}

#[test]
fn known_duplicate_terminalizes_and_sealed_retired_access_is_denied() {
    let selected = scopes();
    let mut state = ResidentClaims::new(selected.claims().clone(), 130).unwrap();
    let key = |serial| ClaimKey::new(selected.claims(), serial).unwrap();
    assert_eq!(
        state
            .claim_batch(selected.claims(), &[key(2), key(3)])
            .unwrap(),
        ClaimAdmission::Fresh
    );
    assert_eq!(
        state
            .claim_batch(selected.claims(), &[key(4), key(2)])
            .unwrap(),
        ClaimAdmission::Duplicate
    );
    assert!(state.claim_present(selected.claims(), key(4)).is_err());
    assert!(state.claim_seal(selected.claims()).is_err());
    assert!(state.claim_batch(selected.claims(), &[]).is_err());
    state.claim_abandon(selected.claims()).unwrap();
    let mut state = ResidentClaims::new(selected.claims().clone(), 130).unwrap();
    assert_eq!(
        state
            .claim_batch(selected.claims(), &[key(5), key(5)])
            .unwrap(),
        ClaimAdmission::Duplicate
    );
    assert!(state.claim_capacity(selected.claims()).is_err());
    let mut state = ResidentClaims::new(selected.claims().clone(), 130).unwrap();
    state
        .claim_batch(selected.claims(), &[key(2), key(3)])
        .unwrap();
    let seal = state.claim_seal(selected.claims()).unwrap();
    assert_eq!(seal.records(), 2);
    assert!(state.claim_present(selected.claims(), key(2)).is_err());
    assert!(state.claim_batch(selected.claims(), &[]).is_err());
    assert!(state
        .claim_retire(&ClaimLedger::new(selected.claims().clone()).unwrap().seal())
        .is_err());
    state.claim_retire(&seal).unwrap();
    assert!(state
        .claim_page(&seal, None, ClaimPageLimit::default())
        .is_err());
    assert!(state.claim_retire(&seal).is_err());
}

#[test]
fn page_bounds_use_32_byte_claim_records_and_exact_empty_eof_continuation() {
    let selected = scopes();
    let mut state = ResidentClaims::new(selected.claims().clone(), 129).unwrap();
    let keys: Vec<_> = (1..=129)
        .map(|s| ClaimKey::new(selected.claims(), s).unwrap())
        .collect();
    state.claim_batch(selected.claims(), &keys[..128]).unwrap();
    state.claim_batch(selected.claims(), &keys[128..]).unwrap();
    let seal = state.claim_seal(selected.claims()).unwrap();
    let first = state
        .claim_page(&seal, None, ClaimPageLimit::default())
        .unwrap();
    assert_eq!(first.records().len(), 128);
    assert_eq!(first.encoded_len(), 4259);
    assert!(!first.eof());
    let header = first.encode_header();
    assert_eq!(header.len(), 163);
    assert_eq!(&header[156..158], &128u16.to_be_bytes());
    assert_eq!(&header[158..162], &4096u32.to_be_bytes());
    let tail = state
        .claim_page(&seal, first.last(), ClaimPageLimit::new(1, 195).unwrap())
        .unwrap();
    assert_eq!(tail.records().len(), 1);
    assert!(tail.eof());
    let empty = state
        .claim_page(&seal, tail.last(), ClaimPageLimit::default())
        .unwrap();
    assert!(empty.records().is_empty() && empty.eof());
    assert_eq!(empty.last(), tail.last());
    assert_eq!(empty.encoded_len(), 163);
    let mut cursor = ClaimCursor::new(&mut state, seal.clone()).unwrap();
    let mut count = 0;
    while let Some(page) = cursor
        .next_page(ClaimPageLimit::new(7, 387).unwrap())
        .unwrap()
    {
        count += page.records().len();
    }
    assert_eq!(count, 129);
}

#[test]
fn width_capacity_and_129th_record_refusal_are_distinct() {
    let selected = scopes();
    let ledger = ClaimLedger::new(selected.claims().clone()).unwrap();
    let records: Vec<_> = (1..=129)
        .map(|s| ClaimRecord::exclusive(selected.claims(), s).unwrap())
        .collect();
    assert!(ledger.validate_append(&records).is_err());
    assert_eq!(
        ledger.append_header(&records[..128]).unwrap()[84..],
        4096u32.to_be_bytes()
    );
    assert!(ClaimCapacity::new(1, 31)
        .unwrap()
        .check_requested(1)
        .is_err());
    assert_eq!(ClaimPageLimit::new(128, 195).unwrap().fitting_records(), 1);
    assert!(ClaimPageLimit::new(128, 194)
        .unwrap()
        .check_records(1)
        .is_err());
    let mut excess = Vec::with_capacity(129);
    excess.push(records[0]);
    assert!(ClaimPage::new(ledger.seal(), excess, true).is_err());
}

// An independently supplied malformed protocol provider, exclusively external.
struct BadPage {
    seal: ClaimSeal,
    record: ClaimRecord,
    mode: u8,
    calls: Cell<u64>,
}
impl BindingClaimState for BadPage {
    fn claim_capacity(&self, _: &StateScope) -> ContentResult<ClaimCapacity> {
        unreachable!()
    }
    fn claim_present(&mut self, _: &StateScope, _: ClaimKey) -> ContentResult<bool> {
        unreachable!()
    }
    fn claim_batch(&mut self, _: &StateScope, _: &[ClaimKey]) -> ContentResult<ClaimAdmission> {
        unreachable!()
    }
    fn claim_seal(&mut self, _: &StateScope) -> ContentResult<ClaimSeal> {
        unreachable!()
    }
    fn claim_abandon(&mut self, _: &StateScope) -> ContentResult<()> {
        unreachable!()
    }
    fn claim_retire(&mut self, _: &ClaimSeal) -> ContentResult<()> {
        unreachable!()
    }
    fn claim_page(
        &mut self,
        _: &ClaimSeal,
        after: Option<ClaimKey>,
        _: ClaimPageLimit,
    ) -> ContentResult<ClaimPage> {
        self.calls.set(self.calls.get() + 1);
        match self.mode {
            0 => ClaimPage::after(self.seal.clone(), after, Vec::new(), true),
            1 => ClaimPage::after(self.seal.clone(), after, vec![self.record], false),
            _ => ClaimPage::after(self.seal.clone(), after, vec![self.record], true),
        }
    }
}
#[test]
fn cursor_refuses_missing_premature_wrong_digest_and_wrong_seal_without_resend() {
    let selected = scopes();
    let record = ClaimRecord::exclusive(selected.claims(), 7).unwrap();
    let expected =
        ClaimSeal::decode(selected.claims(), &reference_seal(selected.claims(), &[7])).unwrap();
    for mode in 0..4 {
        let mut selected_bytes = expected.encode();
        let mut returned_bytes = expected.encode();
        if mode == 2 {
            selected_bytes[129] ^= 1;
            returned_bytes = selected_bytes;
        }
        if mode == 3 {
            returned_bytes[129] ^= 1;
        }
        let requested = ClaimSeal::decode(selected.claims(), &selected_bytes).unwrap();
        let returned = ClaimSeal::decode(selected.claims(), &returned_bytes).unwrap();
        let mut state = BadPage {
            seal: returned,
            record,
            mode,
            calls: Cell::new(0),
        };
        let mut cursor = ClaimCursor::new(&mut state, requested).unwrap();
        let first = cursor.next_page(ClaimPageLimit::default()).unwrap_err();
        assert_eq!(
            cursor.next_page(ClaimPageLimit::default()).unwrap_err(),
            first
        );
        drop(cursor);
        assert_eq!(state.calls.get(), 1);
    }
}

#[test]
fn header_only_empty_terminal_page_is_supported_and_abandon_is_metadata_terminal() {
    let selected = scopes();
    let mut state = ResidentClaims::new(selected.claims().clone(), 0).unwrap();
    let seal = state.claim_seal(selected.claims()).unwrap();
    let limit = ClaimPageLimit::new(128, 163).unwrap();
    let page = state.claim_page(&seal, None, limit).unwrap();
    assert!(page.eof() && page.records().is_empty());
    assert_eq!(page.last(), None);
    assert_eq!(page.encoded_len(), 163);
    let mut cursor = ClaimCursor::new(&mut state, seal.clone()).unwrap();
    assert!(cursor.next_page(limit).unwrap().unwrap().eof());
    assert!(cursor.next_page(limit).unwrap().is_none());
    drop(cursor);
    state.claim_abandon(selected.claims()).unwrap();
    state.claim_abandon(selected.claims()).unwrap();
    assert!(state.claim_page(&seal, None, limit).is_err());
    assert!(state.claim_retire(&seal).is_err());
}
