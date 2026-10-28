//! fs.grep reads a window of files' roots in batched demands (#354, #352
//! F-035). A batch never crosses its own byte bound, so it never reaches the
//! storage reader's per-demand limit, and the batches cover every file once,
//! in order.
use layerfs_content::{
    file::mapping::FILE_STATE_CANONICAL_BYTES, policy::WHOLE_FILE_CANONICAL_OVERHEAD,
};
use layerfs_storage::policy::READ_CANONICAL_BYTES_LIMIT;
use layerfs_store_service::read::cached::{batches, root_bound, ROOTS, ROOT_BYTES};

// The batch bound is well inside the storage reader's demand limit.
const _: () = assert!(ROOT_BYTES <= READ_CANONICAL_BYTES_LIMIT / 8);

fn check(lengths: &[u64]) {
    let runs = batches(lengths);
    let mut next = 0;
    for run in &runs {
        assert_eq!(run.start, next, "runs are consecutive");
        assert!(run.end > run.start, "no empty run");
        assert!(run.len() <= ROOTS);
        let bytes: usize = lengths[run.clone()].iter().map(|&l| root_bound(l)).sum();
        if run.len() > 1 {
            assert!(bytes <= ROOT_BYTES, "run {run:?} holds {bytes} bytes");
        }
        next = run.end;
    }
    assert_eq!(next, lengths.len(), "every file in one run");
}

#[test]
fn batches_stay_within_their_bounds() {
    // A root's bound covers a whole-file root and a file state.
    assert_eq!(
        root_bound(0),
        FILE_STATE_CANONICAL_BYTES.max(WHOLE_FILE_CANONICAL_OVERHEAD)
    );
    assert_eq!(root_bound(1000), 1000 + WHOLE_FILE_CANONICAL_OVERHEAD);
    assert_eq!(root_bound(u64::MAX), usize::MAX);

    check(&[]);
    assert!(batches(&[]).is_empty());
    // Small files: runs of exactly ROOTS.
    let small = vec![3000u64; 200];
    check(&small);
    assert_eq!(batches(&small).len(), 4);
    assert_eq!(batches(&small)[0], 0..ROOTS);
    // 128 KiB files: the byte bound splits before the count does.
    let large = vec![128 * 1024u64; 100];
    check(&large);
    assert!(batches(&large).iter().all(|run| run.len() < ROOTS));
    // A file at the exact bound goes alone, as does a huge chunked file;
    // its neighbours start and end runs of their own.
    let edge = (ROOT_BYTES - WHOLE_FILE_CANONICAL_OVERHEAD) as u64;
    let mixed = [10, edge, 10, 10, 1 << 40, 10, edge - 30, 5];
    check(&mixed);
    let runs = batches(&mixed);
    assert!(runs.contains(&(1..2)));
    assert!(runs.contains(&(4..5)));
    // Many empty files fit the count bound.
    check(&vec![0; 1000]);
}
