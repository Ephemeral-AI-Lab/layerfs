//! A Save stops trying a chunk delta base its trials keep losing against, and
//! keeps trying one that wins: chunks appended past the old end of a file are
//! all offered the old last chunk, which incompressible bytes never beat and
//! text resembling it does.
#[path = "support/memory_metadata.rs"]
mod metadata;

use layerfs_content::{AdvisoryPredecessors, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::encoding::delta::select::{LosingBases, LOSING_BASE_TRIALS};
use layerfs_storage::{Storage, StoragePolicy, WriteOutcome};
use std::sync::Arc;

/// Incompressible bytes, distinct per seed.
fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// Log lines: messages drawn from 48 templates of a few dozen words, with
/// varying fields, so every chunk repeats phrases its base also holds.
fn text(first_line: u64, len: usize) -> Vec<u8> {
    const WORDS: &str = "request worker cache commit branch socket timeout retry \
        payload session upstream accepted rejected pending flushed mount \
        overlay journal segment compacted handshake client server quota \
        latency snapshot replica lease expired renewed index partition";
    let words: Vec<&str> = WORDS.split(' ').collect();
    let template = |k: u64| {
        let mut state = k.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        (0..14)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                words[(state % 32) as usize]
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let templates: Vec<String> = (0..48).map(template).collect();
    let mut out = String::new();
    let mut line = first_line;
    while out.len() < len {
        let r = line.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 7;
        out.push_str(&format!(
            "2026-11-07T{:02}:{:02}:{:02}Z INFO [worker-{:02}] {} id={}\n",
            (line / 3600) % 24,
            (line / 60) % 60,
            line % 60,
            r % 16,
            templates[((r >> 8) % 48) as usize],
            (r >> 16) % 100_000,
        ));
        line += 1;
    }
    out.truncate(len);
    out.into_bytes()
}

fn chunk(raw: &[u8], base: Option<ObjectId>) -> FinalizedObject {
    let canonical = layerfs_content::file::mapping::encode_chunk_object(raw).unwrap();
    let object = FinalizedObject::new(ObjectRole::Chunk, canonical).unwrap();
    match base {
        Some(base) => object.with_predecessors(AdvisoryPredecessors::explicit(base).unwrap()),
        None => object,
    }
}

/// Stores `base` in one Save, then `chunks` in a second, each offered `base`.
/// Returns the second Save's outcome after every chunk read back.
fn append(base: &[u8], chunks: &[Vec<u8>]) -> WriteOutcome {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    metadata.state.lock().unwrap().policy = StoragePolicy::new(1, 131_072, 16, 4);
    let storage = Storage::new(metadata.clone()).unwrap();
    let base = chunk(base, None);
    let save = storage.begin_save().unwrap();
    save.accept(base.clone()).unwrap();
    save.finish().unwrap();
    let objects: Vec<_> = chunks
        .iter()
        .map(|raw| chunk(raw, Some(base.id())))
        .collect();
    let save = storage.begin_save().unwrap();
    for object in &objects {
        save.accept(object.clone()).unwrap();
    }
    let outcome = save.finish().unwrap();
    let reader = Storage::new(metadata).unwrap();
    let reader = reader.reader().unwrap();
    for object in &objects {
        assert_eq!(
            reader.read_objects(&[object.id()]).unwrap(),
            vec![object.canonical().to_vec()]
        );
    }
    outcome
}

#[test]
fn incompressible_chunks_stop_trying_a_base_that_keeps_losing() {
    let chunks: Vec<_> = (1..=200).map(|seed| noise(seed, 16 * 1024)).collect();
    let outcome = append(&noise(0, 16 * 1024), &chunks);
    let delta = outcome.delta;
    // Eight losses in a row, then one trial after gaps of 1, 2, 4, ... offers.
    let trials = u64::from(LOSING_BASE_TRIALS) + 7;
    assert_eq!(delta.trials, trials, "{delta:?}");
    assert_eq!(delta.full_losses, trials, "{delta:?}");
    assert_eq!(delta.losing_skipped, 200 - trials, "{delta:?}");
    assert_eq!(delta.prefix_selected, 0, "{delta:?}");
    assert_eq!(delta.chunk_depths[0], 200, "{delta:?}");
    assert_eq!(outcome.full_records, 200, "{outcome:?}");
}

#[test]
fn text_resembling_its_base_keeps_every_delta() {
    let chunks: Vec<_> = (1..=200).map(|n| text(n * 300, 16 * 1024)).collect();
    let outcome = append(&text(0, 16 * 1024), &chunks);
    let delta = outcome.delta;
    assert_eq!(delta.trials, 200, "{delta:?}");
    assert_eq!(delta.prefix_selected, 200, "{delta:?}");
    assert_eq!(delta.losing_skipped, 0, "{delta:?}");
    assert_eq!(delta.chunk_depths[1], 200, "{delta:?}");
}

#[test]
fn a_skipped_base_is_tried_again_and_a_win_resumes_every_trial() {
    // Twenty incompressible chunks, then fifty that resemble the base.
    let mut chunks: Vec<_> = (1..=20).map(|seed| noise(seed, 16 * 1024)).collect();
    chunks.extend((1..=50).map(|n| text(n * 300, 16 * 1024)));
    let delta = append(&text(0, 16 * 1024), &chunks).delta;
    // Offers 1-8 lose; 10, 13 and 18 are retried and lose; 27 is the next
    // retry, the first text chunk tried, and wins: 21-26 were skipped.
    assert_eq!(delta.losing_skipped, 1 + 2 + 4 + 2 + 6, "{delta:?}");
    assert_eq!(delta.full_losses, 11, "{delta:?}");
    assert_eq!(delta.prefix_selected, 50 - 6, "{delta:?}");
}

#[test]
fn the_memo_backs_off_doubling_and_forgets_a_base_on_a_win() {
    let base = ObjectId::from_bytes(&[7; 32]).unwrap();
    let mut memo = LosingBases::new();
    let mut tried = Vec::new();
    for offer in 1..=40_u32 {
        if memo.admits(base) {
            tried.push(offer);
            memo.note(base, false);
        }
    }
    assert_eq!(tried, [1, 2, 3, 4, 5, 6, 7, 8, 10, 13, 18, 27]);
    memo.note(base, true);
    assert!((0..10).all(|_| memo.admits(base)));
    // Another base is unaffected by this one's losses.
    let other = ObjectId::from_bytes(&[8; 32]).unwrap();
    for _ in 0..LOSING_BASE_TRIALS {
        memo.note(base, false);
    }
    assert!(memo.admits(other));
    assert!(!memo.admits(base));
}
