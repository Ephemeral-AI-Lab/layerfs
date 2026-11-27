//! #88: of several ring entries sharing as many signature hashes with an
//! object, the newest is proposed. A file saved under a new name each time
//! should delta against its previous version, not the oldest one admitted.
use layerfs_content::ObjectId;
use layerfs_storage::encoding::delta::candidates::Candidates;

fn id(byte: u8) -> ObjectId {
    ObjectId::from_bytes(&[byte; 32]).unwrap()
}
/// A sorted signature from eight hash values; `u64::MAX` is unused.
fn sig(mut hashes: [u64; 8]) -> [u64; 8] {
    hashes.sort_unstable();
    hashes
}

#[test]
fn an_equal_overlap_proposes_the_newer_entry_whatever_its_identity() {
    // Two entries reached through different hashes, each sharing three with
    // the query. The older one has the smaller identity, which used to win.
    let mut ring = Candidates::new().unwrap();
    ring.insert(id(1), sig([11, 12, 13, 101, 102, 103, 104, 105]));
    ring.insert(id(9), sig([14, 15, 16, 201, 202, 203, 204, 205]));
    let query = sig([11, 12, 13, 14, 15, 16, 301, 302]);
    assert_eq!(ring.find(id(50), &query), Some(id(9)));
    // A still newer entry with the same overlap takes over.
    ring.insert(id(5), sig([301, 302, 17, 401, 402, 403, 404, 405]));
    let query = sig([11, 12, 14, 15, 301, 302, 501, 502]);
    assert_eq!(ring.find(id(50), &query), Some(id(5)));
}

#[test]
fn a_higher_overlap_still_wins_over_a_newer_entry() {
    let mut ring = Candidates::new().unwrap();
    ring.insert(id(9), sig([11, 12, 13, 14, 101, 102, 103, 104]));
    ring.insert(id(1), sig([15, 16, 201, 202, 203, 204, 205, 206]));
    let query = sig([11, 12, 13, 14, 15, 16, 301, 302]);
    assert_eq!(ring.find(id(50), &query), Some(id(9)));
    // The object itself is never its own candidate.
    assert_eq!(ring.find(id(9), &query), Some(id(1)));
}

#[test]
fn recency_follows_insertion_across_the_ring_wrap() {
    let mut ring = Candidates::new().unwrap();
    ring.insert(id(1), sig([11, 12, 13, 101, 102, 103, 104, 105]));
    // Fill the ring with unrelated entries until it wraps past the first slot
    // and the first entry is overwritten; then two related entries remain.
    for n in 0..8_191_u64 {
        let base = 1_000_000 + n * 16;
        ring.insert(id(7), std::array::from_fn(|k| base + k as u64));
    }
    ring.insert(id(2), sig([11, 12, 13, 106, 107, 108, 109, 110]));
    ring.insert(id(3), sig([14, 15, 16, 111, 112, 113, 114, 115]));
    let query = sig([11, 12, 13, 14, 15, 16, 301, 302]);
    assert_eq!(ring.find(id(50), &query), Some(id(3)));
}
