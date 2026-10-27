//! The write recognizer over a content-built base: gate, follow, search, the
//! unmatched budget, forward sources and memory-only passes.
mod common;

use common::{fixture_with, Fixture};
use layerfs_overlay::{WriteClaims, RUN_BYTES, WRITE_CLAIMS};
use layerfs_workspace::{
    BaseView, CanonicalCache, CanonicalClient, Recognition, RecognizeWork, WriteRun,
    RECOGNIZE_BYTES, UNMATCHED_BYTES,
};
use std::sync::Arc;

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;
/// The large regular file of the fixture.
const FILE: u64 = 8;

fn noise(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    let mut bytes = Vec::with_capacity(length + 8);
    while bytes.len() < length {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.extend_from_slice(&state.to_le_bytes());
    }
    bytes.truncate(length);
    bytes
}
struct Base {
    fixture: Fixture,
    cache: Arc<CanonicalCache>,
    view: BaseView,
}
fn base(bytes: Vec<u8>) -> Base {
    let fixture = fixture_with(bytes);
    let cache = Arc::new(CanonicalCache::new(8 * MIB));
    let client = Arc::new(CanonicalClient::with_cache(
        Arc::new(fixture.store.clone()),
        Some(Arc::new(fixture.store.clone())),
        cache.clone(),
    ));
    let view = BaseView::open(client, fixture.root, fixture.scope).unwrap();
    Base {
        fixture,
        cache,
        view,
    }
}
impl Base {
    fn resident(&self) -> BaseView {
        self.view
            .with_client(Arc::new(CanonicalClient::resident(self.cache.clone())))
    }
}
/// Writes `content[from..]` at its own offsets in requests of `request`
/// bytes and returns the claims of each, checked against the committed bytes.
/// Every request is published under the next revision.
fn rewrite(
    base: &Base,
    run: &mut WriteRun,
    work: &mut RecognizeWork,
    content: &[u8],
    from: usize,
    request: usize,
) -> Vec<(u64, WriteClaims)> {
    rewrite_published(base, run, work, content, from, request, true)
}
/// The same; `published` says whether the run learns of each publication.
fn rewrite_published(
    base: &Base,
    run: &mut WriteRun,
    work: &mut RecognizeWork,
    content: &[u8],
    from: usize,
    request: usize,
    published: bool,
) -> Vec<(u64, WriteClaims)> {
    let committed = &base.fixture.bytes;
    let mut claimed = Vec::new();
    for (index, start) in (from..content.len()).step_by(request).enumerate() {
        let data = &content[start..content.len().min(start + request)];
        // The revision of the request before this one, and of this one.
        let (earlier, revision) = (100 + index as i64, 101 + index as i64);
        if !run.wants(&base.view, data.len()) {
            continue;
        }
        let Recognition::Decided(claims) =
            run.recognize(&base.view, FILE, start as u64, data, work)
        else {
            panic!("a client with a provider needs no base");
        };
        if published {
            run.published(start as u64, (start + data.len()) as u64, revision);
        }
        let Some(claims) = claims else {
            continue;
        };
        assert!(!claims.ranges.is_empty() && claims.ranges.len() <= WRITE_CLAIMS);
        // Only a claim under the earlier request's revision begins before
        // this request, and never before that request or one chunk back.
        let mut at = start as u64;
        if claims.ranges[0].offset < at {
            assert!(published && index > 0);
            assert_eq!(claims.after, Some(earlier));
            at = (start - request.min(RUN_BYTES)) as u64;
        }
        for range in &claims.ranges {
            assert!(range.length > 0 && range.offset >= at, "{range:?}");
            at = range.offset + range.length;
            assert!(at <= (start + data.len()) as u64, "{range:?}");
            let (file, source) = (range.offset as usize, range.source_offset as usize);
            let length = range.length as usize;
            assert_eq!(
                content[file..file + length],
                committed[source..source + length],
                "{range:?}"
            );
        }
        claimed.push((start as u64, claims));
    }
    claimed
}
/// File bytes the claims cover. A claim that reaches back over an earlier
/// request repeats bytes that request already claimed; they count once.
fn total(claimed: &[(u64, WriteClaims)]) -> u64 {
    let (mut bytes, mut covered) = (0, 0);
    for range in claimed.iter().flat_map(|(_, claims)| &claims.ranges) {
        let end = range.offset + range.length;
        bytes += end.saturating_sub(range.offset.max(covered));
        covered = covered.max(end);
    }
    bytes
}

#[test]
fn the_gate_passes_only_a_chunked_committed_file_and_a_write_of_one_chunk() {
    let base = base(noise(MIB, 3));
    let (mut work, data) = (RecognizeWork::default(), noise(64 * KIB, 5));
    // The small whole file, a directory, a symlink and a serial the base
    // does not hold: one pass each, then no further pass.
    for serial in [2, 1, 3, 99] {
        let mut run = WriteRun::default();
        assert!(run.wants(&base.view, data.len()));
        assert_eq!(
            run.recognize(&base.view, serial, 0, &data, &mut work),
            Recognition::Decided(None)
        );
        assert!(!run.wants(&base.view, data.len()), "serial {serial}");
    }
    assert_eq!(
        (work.passes, work.hashed_bytes, work.scanned_bytes),
        (4, 0, 0)
    );
    // A write below one minimum chunk is never examined.
    let run = WriteRun::default();
    assert!(!run.wants(&base.view, RECOGNIZE_BYTES - 1));
    assert!(run.wants(&base.view, RECOGNIZE_BYTES));
    // Another base starts over, whatever the run decided before.
    let other = self::base(noise(MIB, 4));
    let mut run = WriteRun::default();
    run.recognize(&base.view, 2, 0, &data, &mut work);
    assert!(!run.wants(&base.view, data.len()) && run.wants(&other.view, data.len()));
}

#[test]
fn a_shifted_tail_is_claimed_in_whole_chunks_by_one_search_and_then_followed() {
    let committed = noise(3 * MIB, 7);
    let base = base(committed.clone());
    let middle = MIB + 12_345;
    for (label, content, from) in [
        (
            "insert",
            {
                let mut content = committed.clone();
                content.splice(middle..middle, noise(4096, 9));
                content
            },
            middle,
        ),
        (
            "delete",
            {
                let mut content = committed.clone();
                content.drain(middle..middle + 4096);
                content
            },
            middle,
        ),
        (
            "prepend",
            {
                let mut content = noise(4096, 11);
                content.extend_from_slice(&committed);
                content
            },
            0,
        ),
    ] {
        for request in [64 * KIB, 128 * KIB] {
            let (mut run, mut work) = (WriteRun::default(), RecognizeWork::default());
            let claimed = rewrite(&base, &mut run, &mut work, &content, from, request);
            let written = (content.len() - from) as u64;
            let bytes = total(&claimed);
            println!(
                "RECOGNIZE {label} request={request} written={written} claimed={bytes} \
                 requests={} {work:?}",
                claimed.len()
            );
            // The chunk that crosses a request's end is completed by the
            // next one: what stays unclaimed is the edit and the chunks cut
            // by it and by the first request.
            assert!(
                written - bytes <= 2 * 64 * KIB as u64,
                "{label}: {bytes} of {written}"
            );
            // Without a known publication a request keeps only the chunks
            // that lie wholly in it.
            let (mut alone, mut idle) = (WriteRun::default(), RecognizeWork::default());
            let whole =
                rewrite_published(&base, &mut alone, &mut idle, &content, from, request, false);
            assert!(whole.iter().all(|(_, claims)| claims.after.is_none()));
            assert!(
                total(&whole) * 2 > written && total(&whole) < bytes,
                "{label}"
            );
            // One search finds the shift; every later request follows it.
            assert!(work.scanned_bytes <= 4 * 64 * KIB as u64, "{work:?}");
            assert!(work.hashed_bytes < written + 64 * KIB as u64, "{work:?}");
            // Sources move forward through the file. A claim that reaches
            // back repeats the end of the one before it and goes on from it.
            let (mut start, mut source) = (0, 0);
            for range in claimed.iter().flat_map(|(_, claims)| &claims.ranges) {
                let end = range.source_offset + range.length;
                assert!(
                    range.source_offset >= start && end > source,
                    "{label}: {range:?}"
                );
                (start, source) = (range.source_offset, end);
            }
        }
    }
}

/// A program that grows a file moves its tail from the end backward: the
/// requests arrive in falling offsets, as the benchmark's writer and
/// `memmove` send them.
#[test]
fn a_tail_moved_from_the_end_backward_is_claimed_across_requests() {
    let committed = noise(3 * MIB, 7);
    let base = base(committed.clone());
    let middle = MIB + 12_345;
    let mut content = committed.clone();
    content.splice(middle..middle, noise(4096, 9));
    let from = middle + 4096;
    for (request, published) in [(64 * KIB, true), (128 * KIB, true), (64 * KIB, false)] {
        let (mut run, mut work) = (WriteRun::default(), RecognizeWork::default());
        let mut covered = vec![false; content.len()];
        let (mut end, mut revision, mut reached) = (content.len(), 100_i64, 0);
        while end > from {
            let start = from.max(end.saturating_sub(request));
            let data = &content[start..end];
            assert!(run.wants(&base.view, data.len()));
            let Recognition::Decided(claims) =
                run.recognize(&base.view, FILE, start as u64, data, &mut work)
            else {
                panic!("a client with a provider needs no base");
            };
            if let Some(claims) = claims {
                assert!(claims.ranges.len() <= WRITE_CLAIMS);
                let last = claims.ranges.last().unwrap();
                // Only a claim under the revision of the request above ends
                // after this request, and at most one chunk after it.
                let to = (last.offset + last.length) as usize;
                if to > end {
                    assert!(published && to <= end + RUN_BYTES, "{last:?}");
                    assert_eq!(claims.after, Some(revision));
                    reached += 1;
                }
                for range in &claims.ranges {
                    assert!(
                        range.offset as usize >= start && range.length > 0,
                        "{range:?}"
                    );
                    let (file, source) = (range.offset as usize, range.source_offset as usize);
                    let length = range.length as usize;
                    assert_eq!(
                        content[file..file + length],
                        committed[source..source + length]
                    );
                    assert_eq!(file - source, 4096, "{range:?}");
                    covered[file..file + length].fill(true);
                }
            }
            revision += 1;
            if published {
                run.published(start as u64, end as u64, revision);
            }
            end = start;
        }
        let written = content.len() - from;
        let bytes = covered.iter().filter(|byte| **byte).count();
        println!(
            "RECOGNIZE backward request={request} published={published} written={written} \
             claimed={bytes} reached={reached} {work:?}"
        );
        if published {
            // Every boundary but the first is joined: what stays unclaimed
            // is the top request before its first found chunk and the chunk
            // the edit cut.
            assert!(written - bytes <= 2 * 64 * KIB, "{bytes} of {written}");
            assert!(reached >= written / request - 2, "{reached}");
        } else {
            assert!(reached == 0 && bytes * 2 > written && bytes * 10 < written * 9);
        }
        assert!(
            work.hashed_bytes < (written + 2 * 64 * KIB) as u64,
            "{work:?}"
        );
    }
}

/// The cells a writer job would store for a tail moved down by one cell in
/// requests that begin on cell boundaries: a claim is whole cells, so two
/// claims that meet inside a cell without being one range leave that cell.
#[test]
fn an_aligned_delete_leaves_no_stored_cell_between_claims() {
    const CELL: usize = 4096;
    let committed = noise(96 * MIB, 41);
    let base = base(committed.clone());
    let p = 16 * MIB;
    let mut content = committed.clone();
    content.drain(p..p + CELL);
    let request = 64 * KIB;
    let (mut run, mut work) = (WriteRun::default(), RecognizeWork::default());
    let mut stored = vec![false; content.len().div_ceil(CELL)];
    let mut log: Vec<(usize, Vec<(u64, u64)>)> = Vec::new();
    for (index, start) in (p..content.len()).step_by(request).enumerate() {
        let data = &content[start..content.len().min(start + request)];
        let Recognition::Decided(claims) =
            run.recognize(&base.view, FILE, start as u64, data, &mut work)
        else {
            panic!("a client with a provider needs no base");
        };
        run.published(
            start as u64,
            (start + data.len()) as u64,
            101 + index as i64,
        );
        stored[start / CELL..(start + data.len()).div_ceil(CELL)].fill(true);
        let ranges = claims.map_or(Vec::new(), |claims| claims.ranges);
        for range in &ranges {
            let (from, to) = (
                range.offset as usize,
                (range.offset + range.length) as usize,
            );
            stored[from.div_ceil(CELL)..to / CELL].fill(false);
        }
        log.push((
            start,
            ranges
                .iter()
                .map(|r| (r.offset, r.offset + r.length))
                .collect(),
        ));
    }
    let cells: Vec<usize> = (0..stored.len()).filter(|cell| stored[*cell]).collect();
    println!(
        "RECOGNIZE aligned delete stored cells={} {work:?}",
        cells.len()
    );
    for cell in cells.iter().take(6) {
        let at = cell * CELL;
        let near: Vec<_> = log
            .iter()
            .filter(|(start, _)| start + 2 * request > at && *start < at + 2 * request)
            .collect();
        println!("RECOGNIZE stored cell at {at} ({}): {near:?}", at % request);
    }
    assert!(cells.len() <= 12, "{}", cells.len());
}

#[test]
fn a_whole_rewrite_follows_the_unchanged_part_and_finds_the_shift_after_an_edit() {
    let committed = noise(2 * MIB, 13);
    let base = base(committed.clone());
    let mut content = committed.clone();
    // An equal-length overwrite, then an insert further on.
    content[300 * KIB..300 * KIB + 100].fill(0x5a);
    content.splice(MIB..MIB, noise(300, 15));
    let (mut run, mut work) = (WriteRun::default(), RecognizeWork::default());
    let claimed = rewrite(&base, &mut run, &mut work, &content, 0, 128 * KIB);
    let bytes = total(&claimed);
    println!(
        "RECOGNIZE rewrite claimed={bytes} of {} {work:?}",
        content.len()
    );
    assert!(bytes * 4 > content.len() as u64 * 3, "{bytes}");
    let shifts: std::collections::BTreeSet<i64> = claimed
        .iter()
        .flat_map(|(_, claims)| &claims.ranges)
        .map(|range| range.offset as i64 - range.source_offset as i64)
        .collect();
    assert_eq!(shifts.into_iter().collect::<Vec<_>>(), [0, 300]);
}

#[test]
fn bytes_that_match_nothing_stop_the_search_after_a_fixed_budget() {
    let base = base(noise(4 * MIB, 17));
    let content = noise(4 * MIB, 19);
    let (mut run, mut work) = (WriteRun::default(), RecognizeWork::default());
    let claimed = rewrite(&base, &mut run, &mut work, &content, 0, 64 * KIB);
    println!("RECOGNIZE unmatched {work:?}");
    assert!(claimed.is_empty());
    assert_eq!(work.passes, UNMATCHED_BYTES / (64 * KIB as u64));
    assert_eq!(work.scanned_bytes, 2 * UNMATCHED_BYTES);
    // Only a length that happens to fit two candidates is hashed.
    assert!(work.hashed_bytes < UNMATCHED_BYTES / 4, "{work:?}");
    assert!(!run.wants(&base.view, 64 * KIB));
}

#[test]
fn a_shift_that_would_move_the_source_backward_is_not_claimed() {
    let committed = noise(2 * MIB, 23);
    let base = base(committed.clone());
    let (mut run, mut work) = (WriteRun::default(), RecognizeWork::default());
    // The unchanged first half, in place.
    let claimed = rewrite(&base, &mut run, &mut work, &committed[..MIB], 0, 128 * KIB);
    assert!(total(&claimed) > MIB as u64 / 2);
    // Then bytes below the last claim that equal content 4 KiB further on.
    let (at, data) = (512 * KIB, &committed[516 * KIB..644 * KIB]);
    let outcome = run.recognize(&base.view, FILE, at as u64, data, &mut work);
    assert_eq!(outcome, Recognition::Decided(None));
    // The same bytes with no earlier claim are found.
    let mut fresh = WriteRun::default();
    let Recognition::Decided(Some(claims)) =
        fresh.recognize(&base.view, FILE, at as u64, data, &mut work)
    else {
        panic!("a fresh run finds the shift");
    };
    assert_eq!(
        claims.ranges[0].source_offset - claims.ranges[0].offset,
        4 * KIB as u64
    );
}

#[test]
fn a_memory_only_pass_asks_for_the_base_and_records_nothing() {
    let committed = noise(MIB, 29);
    let base = base(committed.clone());
    let resident = base.resident();
    let data = &committed[100 * KIB..228 * KIB];
    let (mut run, mut work) = (WriteRun::default(), RecognizeWork::default());
    let demand = || {
        base.fixture
            .store
            .demand
            .load(std::sync::atomic::Ordering::Relaxed)
    };
    let before = demand();
    assert_eq!(
        run.recognize(&resident, FILE, 100 * KIB as u64, data, &mut work),
        Recognition::NeedsBase
    );
    assert_eq!(format!("{run:?}"), format!("{:?}", WriteRun::default()));
    assert_eq!(demand(), before);
    // The pass over the provider reads mapping pages and no chunk.
    let served = || {
        base.fixture
            .store
            .served
            .load(std::sync::atomic::Ordering::Relaxed)
    };
    let payload = served();
    let Recognition::Decided(Some(first)) =
        run.recognize(&base.view, FILE, 100 * KIB as u64, data, &mut work)
    else {
        panic!("the unchanged bytes are found");
    };
    assert!(
        served() - payload < 64 * KIB as u64,
        "{}",
        served() - payload
    );
    // Now the pages are in memory: the same pass decides without a provider.
    let (mut again, before) = (WriteRun::default(), demand());
    let outcome = again.recognize(&resident, FILE, 100 * KIB as u64, data, &mut work);
    assert_eq!(outcome, Recognition::Decided(Some(first)));
    assert_eq!(demand(), before);
    assert_eq!(work.needs_base, 1);
}
