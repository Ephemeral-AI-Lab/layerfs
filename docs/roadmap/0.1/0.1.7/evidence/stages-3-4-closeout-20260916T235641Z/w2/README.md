# W2 evidence — the pooled lane honours the publication watermark (G2)

## What the packet fixes

`core/crates/layerfs-storage/src/cas/owner.rs` passed `i64::MAX` as the read
ceiling at its four pooled read sites, so the pooled lane's stated invariant
("the publication watermark, applied to every dependency and cache read",
`stages-3-4-report.md:112`) rested entirely on the
`retained_pack_ceiling != highest_pack_id` precondition in
`MutationOwner::acquire`. The four sites now supply the owner's own
`self.ceiling`, which already includes every pack this save created:

| Site | Was | Now |
| --- | --- | --- |
| `cas/owner.rs:452-470` (`PoolIndex::find` in `select_pooled`) | `i64::MAX` | `self.ceiling` |
| `cas/owner.rs:531-543` (`PoolIndex::sync` in `sync_pool_index`) | `i64::MAX` | `self.ceiling` |
| `cas/owner.rs:650-652` (`lookup::location` in `pool_base`) | `i64::MAX` | `self.ceiling` |
| `cas/owner.rs:677-683` (`PoolReader::leaf_body` in `pool_base`) | `i64::MAX` | `self.ceiling` |

`encoding/pool/read.rs:70-84` also decides the ceiling **before** consulting its
decoded-value cache, so a group retained earlier in the wave cannot bypass the
ceiling. `VisibilityCeiling` is still the refusal, unchanged.

## Reachability analysis (stated plainly)

**No reachable state today hands the pooled lane a row above its own ceiling.**
`MutationOwner::acquire` (`owner.rs:162-171`) refuses to start whenever
`retained_pack_ceiling != highest_pack_id`, and the owner's ceiling starts at
that same highest pack id and only grows with packs the save itself writes. A
save's own early bounded commits therefore stay at or below its ceiling. The
change is a fail-closed statement of the invariant at the read sites, not a fix
for an observed leak - exactly as the review describes it. Both oracles below
were still built to fail if the threading is removed or replaced by a smaller
ceiling.

## Commands, exits and raw output

`w2-verify.log` (9 recorded commands, all exit 0):

1. `cargo +1.85.1 test ... -p layerfs-storage --test visibility` — **7 passed** (was 5), 2.2 s.
2. `--test metadata_pool` — 10 passed (was 9).
3. `--test metadata_chain` — 2 passed.
4. `--test metadata_window` — 2 passed, 8.5 s (the real 1 312-leaf crossing).
5. `--test metadata_pool_index` — 5 passed.
6. `--test edit_pipeline` — 4 passed.
7. `grep -n "i64::MAX\|self.ceiling" core/crates/layerfs-storage/src/cas/owner.rs` — the raw source receipt: the three remaining `i64::MAX` uses are `read_batch`, `resolve_location` and `Availability::validate`, none of which is a pooled read; the four pooled sites read `self.ceiling`.
8. `cargo +1.85.1 clippy --workspace --all-targets -- -D warnings` — exit 0.
9. production LOC pair — core 10938 → 10938 (delta 0; substituted arguments and a moved block, no new statements).

Whole-command wall times: 1.0-8.5 s, all inside the 15 s budget.

## The two oracles

* `visibility.rs::a_pooled_read_refuses_a_value_group_above_the_captured_ceiling`
  — behavioural. It drives a real save whose bounded early commit leaves a
  **value-group pack committed above the watermark**, then asserts that
  `PoolReader::group_values` (cold and cached), `PoolIndex::sync` and
  `PoolIndex::find` all return `VisibilityCeiling` for that row, and that the
  same calls succeed at the real published ceiling. A control in
  `w2-fails-without-fix.log` shows the cached-group half failing when the cache
  is consulted before the ceiling.
* `visibility.rs::the_pooled_lane_supplies_the_owners_ceiling_at_every_read_site`
  — structural. It reads the pooled lane's two source regions out of
  `cas/owner.rs` and rejects both an unbounded ceiling and a missing
  `self.ceiling` there. Control: with the four sites reverted, it fails with
  "the pooled lane's selection region passes an unbounded read ceiling".

`w2-fails-without-fix.log` records both control runs and their exit codes (101).

## Identities

| | |
| --- | --- |
| Source | commit `97414bac4` (first parent) plus the W2 change |
| Toolchain | `cargo 1.85.1 (d73d2caf9 2024-12-31)`, `rustc 1.85.1 (4eb2518d3 2025-03-15)` |
| Profile | `dev` (debug) test profile, `--locked` |
| Worker count | `LAYERFS_CONSTRUCTION_WORKERS=1` |
| Fixtures | in-process deterministic: `noise(len)` body of 5 MiB to force the bounded early commit, an 8-value canonical pooled inode leaf, `test.derived` values |
| Cache state | not a measurement; no timed phase, no sample count |
| Production LOC | core `10938 -> 10938` (delta 0) |

## What this artifact does not prove

* It does not demonstrate a reachable leak: none exists while `acquire` keeps
  refusing a Store whose watermark lags its highest pack. It proves the read
  sites now carry the ceiling and that the pooled read API refuses a row above
  it.
* It is not a measurement. No time, storage or memory number is claimed.
* `PoolReader::leaf_body` applies the ceiling to its **chain base** lookups and
  to the ordinal groups it resolves; a FULL root record is checked by the caller
  (`cas/read.rs:60-68`), not by `leaf_body` itself.
