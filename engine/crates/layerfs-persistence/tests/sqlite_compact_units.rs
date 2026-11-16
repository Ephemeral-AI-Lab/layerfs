//! §7 step 3 and §11 unit checks for the SQLite rebuild, moved out of
//! `backend/sqlite/compact/`: product source holds no tests (the
//! product-boundary guard).

use layerfs_history::HistoryError;
use layerfs_persistence::internals::{io_error, journal_bound_bytes, peak_temp_bytes};

/// §7 step 3 on a bounded filesystem: `ENOSPC`, `EDQUOT` and `EFBIG` are the
/// report's `Capacity` refusal, never a corruption verdict.
#[test]
fn bounded_filesystem_errors_are_capacity() {
    for code in [28, 122, 27, 69] {
        let error = io_error(std::io::Error::from_raw_os_error(code));
        assert!(
            matches!(error, HistoryError::Capacity(_)),
            "errno {code}: {error:?}"
        );
    }
}

/// Every other I/O failure keeps the integrity classification.
#[test]
fn another_io_error_is_integrity() {
    let error = io_error(std::io::Error::from_raw_os_error(5));
    assert!(matches!(error, HistoryError::Integrity(_)), "{error:?}");
}

/// §11 `peakTempBytes` on the 50 GB arm (T-G66): the staged copy holds the
/// source extent, the rewrite holds that copy beside the new Store, and the
/// rollback journal is bounded by the staged page count. The receipt must
/// cover the measured 143.1 GB pre-rebuild peak and the 44.6 GB journal the
/// old `new + visited` formula dropped.
#[test]
fn peak_temp_covers_the_staged_copy_and_the_rollback_journal() {
    let page_size = 4096;
    // 50,862,854,144 B staged copy = the sealed Store's page count.
    let staged_pages = 12_417_689;
    let staged_bytes = staged_pages * page_size;
    let target_bytes = 45_763_203_072;
    let visited_bytes = 20_365_312;
    let journal = journal_bound_bytes(staged_pages, page_size);
    assert_eq!(journal, 50_962_203_848);
    let peak = peak_temp_bytes(staged_bytes, journal, target_bytes, visited_bytes);
    assert_eq!(peak, 147_608_626_376);
    assert!(peak >= target_bytes + journal + visited_bytes);
    assert!(
        peak >= target_bytes + 44_630_503_928 + visited_bytes,
        "the bound must cover the 50 GB arm's observed 44.6 GB journal"
    );
    assert!(
        peak >= 143_100_000_000,
        "the bound must cover the measured 143.1 GB sampled 50 GB peak"
    );
}

/// §11/S7: `deltaBases.references` is capped, and the cap is reported; the
/// bytes and the retained set are unaffected.
#[test]
fn delta_base_references_are_capped_and_the_cap_is_reported() {
    use layerfs_content::ObjectId;
    use layerfs_persistence::internals::{cap_delta_references, DELTA_BASE_REFERENCES_CAP};

    let short: Vec<ObjectId> = (0..3u8)
        .map(|index| ObjectId::from_bytes(&[index; 32]).unwrap())
        .collect();
    let (references, capped) = cap_delta_references(&short);
    assert_eq!(references.len(), 3);
    assert!(!capped);

    let long: Vec<ObjectId> = (0..=DELTA_BASE_REFERENCES_CAP)
        .map(|index| ObjectId::from_bytes(&[(index % 251) as u8; 32]).unwrap())
        .collect();
    let (references, capped) = cap_delta_references(&long);
    assert_eq!(references.len(), DELTA_BASE_REFERENCES_CAP);
    assert!(capped, "a list at the cap is reported as capped");
}
