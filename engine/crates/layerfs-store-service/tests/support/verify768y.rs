//! Independent #768 "ids are never reused" bookkeeping (not the author's
//! harness). At a point where no Store service is open on the Store (so no
//! Storage handle is alive and nothing is kept), every ordinal and pack id
//! below the counters that no stored row uses is lost for good. Such a gap
//! must stay unused for the rest of the Store's life: a later stored value
//! block or pack in it is a reused id. [`Lost::closed_point`] checks the
//! gaps of every earlier closed point and records the current ones;
//! [`Lost::check`] checks without recording (any time, also with services
//! open).
use super::adversary::TestStore;
use super::verify768x::{alloc, allocation_failures, pack_ids, value_blocks};

/// Inclusive ranges lost at earlier closed points.
#[derive(Default)]
pub struct Lost {
    pub ordinals: Vec<(i64, i64)>,
    pub packs: Vec<(i64, i64)>,
}

/// Inclusive gaps of `[1, end)` that the sorted inclusive `used` ranges
/// leave.
fn gaps(used: &[(i64, i64)], end: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut next = 1;
    for &(lo, hi) in used {
        if lo > next {
            out.push((next, lo - 1));
        }
        next = next.max(hi + 1);
    }
    if end > next {
        out.push((next, end - 1));
    }
    out
}

fn total(ranges: &[(i64, i64)]) -> i64 {
    ranges.iter().map(|(lo, hi)| hi - lo + 1).sum()
}

impl Lost {
    /// Every earlier lost range is still unused, the counters stand past
    /// every stored id, and no duplicate allocation exists.
    pub fn check(&self, store: &TestStore, what: &str) {
        let kind = store.kind;
        let blocks = value_blocks(store);
        let packs = pack_ids(store);
        for &(lo, hi) in &self.ordinals {
            if let Some(b) = blocks.iter().find(|b| !(b.1 < lo || b.0 > hi)) {
                panic!(
                    "{kind:?} {what}: ordinals {lo}..={hi} were lost at an earlier closed point \
                     but block {b:?} uses them (a reused id)"
                );
            }
        }
        for &(lo, hi) in &self.packs {
            if let Some(p) = packs.iter().find(|p| (lo..=hi).contains(*p)) {
                panic!(
                    "{kind:?} {what}: pack ids {lo}..={hi} were lost at an earlier closed point \
                     but pack {p} uses one (a reused id)"
                );
            }
        }
        let a = alloc(store);
        assert!(
            a.next_ordinal > a.last_ordinal && a.next_pack > a.last_pack,
            "{kind:?} {what}: a counter stands at or below a stored id: {a:?}"
        );
        let failures = allocation_failures(store);
        assert!(failures.is_empty(), "{kind:?} {what}: {failures:#?}");
    }

    /// At a point where no service is open: [`Self::check`], then records
    /// the current gaps. Returns the lost ordinal and pack id totals.
    pub fn closed_point(&mut self, store: &TestStore, what: &str) -> (i64, i64) {
        self.check(store, what);
        let a = alloc(store);
        let blocks = value_blocks(store);
        let packs: Vec<(i64, i64)> = pack_ids(store).into_iter().map(|p| (p, p)).collect();
        let ordinals = gaps(&blocks, a.next_ordinal);
        let pack_gaps = gaps(&packs, a.next_pack);
        assert!(
            total(&ordinals) >= total(&self.ordinals) && total(&pack_gaps) >= total(&self.packs),
            "{:?} {what}: fewer ids lost than at an earlier closed point",
            store.kind
        );
        self.ordinals = ordinals;
        self.packs = pack_gaps;
        (total(&self.ordinals), total(&self.packs))
    }
}
