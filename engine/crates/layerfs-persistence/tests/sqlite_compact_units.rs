//! §7 steps 3–5 and §11 unit checks for the fresh-copy SQLite rebuild, moved out of
//! `backend/sqlite/compact/`: product source holds no tests (the
//! product-boundary guard).

use layerfs_history::HistoryError;
use layerfs_persistence::internals::{io_error, peak_temp_bytes, receipt};
use layerfs_persistence::{CompactKeptWaste, CompactPath, CompactResidue};

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

/// §11 `peakTempBytes` for the fresh-copy rebuild: the fresh Store plus the
/// `visited` scratch, and nothing else. The path stages no whole-Store copy,
/// writes no prune journal and opens no rewrite temp database, so the old
/// `staged + journal + target + visited` bound no longer describes it.
#[test]
fn peak_temp_is_the_fresh_store_plus_the_visited_scratch() {
    let target_bytes = 45_763_203_072;
    let visited_bytes = 20_365_312;
    assert_eq!(
        peak_temp_bytes(target_bytes, visited_bytes),
        target_bytes + visited_bytes
    );
    assert_eq!(peak_temp_bytes(0, visited_bytes), visited_bytes);
    assert_eq!(peak_temp_bytes(target_bytes, 0), target_bytes);
    assert_eq!(peak_temp_bytes(0, 0), 0);
    // The bound saturates instead of wrapping.
    assert_eq!(peak_temp_bytes(u64::MAX, 1), u64::MAX);
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

/// A rebuild whose source Store had no free pages names that absence; it
/// does not claim there was no waste (there were 100 dead bytes).
#[test]
fn a_rebuild_without_free_pages_explains_no_free_pages() {
    let receipt = receipt(
        CompactPath::Rebuild,
        1,
        2,
        100,
        10,
        0,
        64,
        CompactKeptWaste::default(),
        64,
        CompactResidue::default(),
        1_000,
        2,
    );
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "backendReusable" && zero.reason == "no-free-pages"));
    assert!(
        receipt
            .zero_reasons
            .iter()
            .all(|zero| zero.field != "physicalShrink"),
        "a nonzero shrink carries no zero reason"
    );
}

/// A rebuild can collect dead bytes and still measure no file delta; that
/// is `no-shrink`, never the misleading `no-waste`.
#[test]
fn a_rebuild_with_waste_but_no_shrink_explains_no_shrink() {
    let receipt = receipt(
        CompactPath::Rebuild,
        1,
        2,
        100,
        10,
        4_096,
        0,
        CompactKeptWaste::default(),
        64,
        CompactResidue::default(),
        1_000,
        2,
    );
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "physicalShrink" && zero.reason == "no-shrink"));
    assert!(
        receipt
            .zero_reasons
            .iter()
            .all(|zero| !(zero.field == "physicalShrink" && zero.reason == "no-waste")),
        "a zero delta with dead bytes is not no-waste"
    );
}
