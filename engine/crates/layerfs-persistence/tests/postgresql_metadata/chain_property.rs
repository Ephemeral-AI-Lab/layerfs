//! #758 phase 2, property: over random file histories (file counts, sizes,
//! edit or rewrite per version, in-batch delta bases, histories well past the
//! depth caps), every stored list is the record-header chain, every strict
//! read sees no stale hint and returns what the default port returns, and a
//! file's cold read costs one chain locate whatever its history length.
//! `SEEDS` is fixed so the target fits its 100 s bound.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/pg/chains.rs"]
mod chains;

use chains::*;
use layerfs_content::FinalizedObject;
use layerfs_storage::Storage;

/// Histories checked; each is a fresh Store.
const SEEDS: u64 = 40;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
}

fn one_history(url: &str, seed: u64) {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let store = Store::new(url, &format!("prop{seed}"));
    let storage = storage(&store.handles);
    let count = rng.range(1, 5) as usize;
    let size = rng.range(4_000, 60_000) as usize;
    let versions = rng.range(18, 34) as usize;
    let mut files = Files::new(count, size, seed + 100);
    let mut history = Vec::new();
    for version in 0..versions {
        let rewrite = rng.range(0, 9) == 0;
        let mut objects = files.emit(&storage, rewrite);
        // Now and then an in-batch chain of two new objects.
        if rng.range(0, 3) == 0 {
            let base = whole(&noise(seed * 1000 + version as u64, size));
            let mut bytes = base.canonical().to_vec();
            let at = rng.range(0, bytes.len() as u64 - 9) as usize;
            bytes[at..at + 8].copy_from_slice(b"758-prop");
            let child = hinted(FinalizedObject::new(base.role(), bytes).unwrap(), &base);
            history.push(child.id());
            objects.extend([base, child]);
        }
        save(&storage, objects);
        history.extend(files.latest());
    }
    assert_eq!(
        store.scalar("SELECT count(*) FROM object_location WHERE ancestors IS NULL"),
        0,
        "seed {seed}"
    );
    assert_lists_exact(&store);
    let handles = store.open();
    let latest = files.latest();
    let (strict, cost) = cold_read(&handles, &history, true);
    let plain = {
        let port = Storage::new(handles.storage.clone()).unwrap();
        port.reader().unwrap().read_objects(&history)
    };
    assert_eq!(strict.unwrap(), plain.unwrap(), "seed {seed}");
    assert_eq!(cost.stale, 0, "seed {seed}: {cost:?}");
    assert!(
        cost.round_trips <= cost.a8_budget() + cost.prepares,
        "seed {seed}: {cost:?}"
    );
    // One file's cold read past the caps: one wave, at most statement 1,
    // statement 2 and one group read, however long the history.
    let _ = cold_read(&handles, &latest[..1], true);
    let (one, cost) = cold_read(&handles, &latest[..1], true);
    one.unwrap();
    assert_eq!(cost.waves, 1, "seed {seed}: {cost:?}");
    assert_eq!(cost.stale, 0);
    assert!(
        cost.round_trips <= 3 && cost.round_trips <= cost.a8_budget(),
        "seed {seed}: {cost:?}"
    );
}

#[test]
fn random_histories_keep_exact_lists_and_flat_strict_reads() {
    let Some(url) = url() else { return };
    let seeds = std::env::var("LAYERFS_758_PROPERTY_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(SEEDS);
    for seed in 1..=seeds {
        one_history(&url, seed);
    }
}
