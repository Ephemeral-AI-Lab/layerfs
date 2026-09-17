# Verification: R2-F8 / R9 / N-6 — one reader (and buffer) per ordering tier

- **Verifier:** delegated code/documentation verification subagent (parent `session-be47d8b4-d2c4-42c2-a6a0-5c58781db918`).
- **Date:** 2026-09-17T19:03Z (local 2026-09-18 ~03:03).
- **Repository:** `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, `git rev-parse HEAD` =
  `99743b2cff2470e6634874d7ee14b9d37d0ba16e` (round-4 fix `9327f6695` in its history).
- **Scope:** code + documentation verification only. The timing grid was NOT run (a
  separate verifier owns measurement). No repository file was modified, staged,
  committed or reverted by this verifier; the only file written is this one.

## Verdict summary

| # | Claim | Verdict |
|---|---|---|
| 1 | `RunStore::find` keeps one `RunScan` (with retained buffer) per tier alive across lookups; continuing lookups served from the retained buffer; restarting lookups keep the buffer and only invalidate its bytes | **PASS** |
| 2 | Dead states gone: `LookupScan::settled` (round 3) and `LookupScan::total` (round 4) no longer exist; `RunReader::seek_from` removed with no remaining callers | **PASS** |
| 3 | Module docs match code: runs.rs module doc one-reader-per-tier bound (`MAXIMUM_LEVELS` buffers of `merge_buffer` bytes, dropped on run replacement), `find` doc describes buffer reuse, merge.rs `RunScan`/`RunReader` docs describe the split | **PASS** |
| 4 | Product test proves the property from outside: counting global allocator; 12 spills leave several tiers live; ascending sweep over all 768 serials reads the spilled rows exactly once; mixed continuing/restarting wave of 832 lookups performs ZERO heap allocations | **PASS** |
| 5 | Ordering suite pins the semantics: restarted scans return the same rows (`run_lookup_answers_every_serial_in_every_order` and the new restarted-scan case) | **PASS** |

## Repository identity and working-tree notes

- Task stated commit `99743b2cf3a869b7d8897a1f16b82d742aeedc40`. `git cat-file -t` on that
  full hash exits 128 (no such object). Actual HEAD is `99743b2cff2470e6634874d7ee14b9d37d0ba16e`
  — the 9-character prefix `99743b2cf` matches; the task's hash tail appears to be a typo.
  All findings below are from HEAD's tree.
- `head.txt` in this evidence directory records `134b8df73...` (one commit behind HEAD);
  this verifier's `git rev-parse HEAD` at verification time returned `99743b2cff...`.
- At the start of this verification `git status --porcelain` was empty (clean tree). The
  cargo test run below compiled only `layerfs-content` (no dependency recompiled), i.e. it
  tested HEAD sources. During this session, **concurrent verifiers** (sharing the workspace)
  modified `core/crates/layerfs-storage/src/encoding/codec.rs`,
  `docs/roadmap/0.1/0.1.7/component-decoupling/physical-encoding-and-packing.md` and
  `stage-5-report.md`, and added their own `verify-*.md` files. None of those are files cited
  below; `runs.rs`, `merge.rs`, `record.rs`, `inode_leaf.rs`, and both test files are untouched
  versus HEAD. Those concurrent changes are not this verifier's work.

## Commands run (all read-only; exit codes in brackets)

1. `git rev-parse HEAD` [0] → `99743b2cff2470e6634874d7ee14b9d37d0ba16e`
2. `git status --porcelain | head -20` [0] → empty (clean) at start
3. `git log --oneline -3` [0] → `99743b2cf`, `134b8df73`, `9327f6695`
4. `git cat-file -t 99743b2cf3a869b7d8897a1f16b82d742aeedc40` [128] → object does not exist
5. `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_ordering_scan --test filesystem_ordering --locked` [0]
   - `filesystem_ordering`: **12 passed**; 0 failed (includes `run_lookup_answers_every_serial_in_every_order`)
   - `filesystem_ordering_scan`: **2 passed**; 0 failed (`lookups_allocate_nothing_after_the_tiers_are_built`, `a_restarted_scan_returns_the_same_rows_as_a_fresh_one`)
6. `grep -rn "seek_from" core/ --include="*.rs"` [1] → **zero matches**
7. `grep -rn "seek_from" --include="*.rs" .` (excluding `/target/`) → **zero matches** repo-wide
8. `grep -n "settled" core/crates/layerfs-content/src/filesystem/references/runs.rs` [0] → single match, line 534, doc prose only
9. `grep -n "total:" core/crates/layerfs-content/src/filesystem/references/runs.rs` [1] → zero matches
10. `grep -n "settled\|total" core/crates/layerfs-content/src/filesystem/references/merge.rs` [1] → zero matches
11. `grep -n "reset_scans" .../runs.rs` [0] → definition L120; calls L231, L299, L438, L512, L521
12. `grep -n "levels.push\|levels.clear\|scans.push\|scans.clear\|MAXIMUM_LEVELS" .../runs.rs` [0] → L25, L41, L121, L223, L229, L321, L333, L437, L440
13. `git log --oneline -S "settled" -- .../runs.rs` [0] → `9327f6695`, `2fe2a4642`, `8701eae12`
14. `git log --oneline -S "total:" -- .../runs.rs` [0] → `9327f6695`, `8701eae12`
15. `git log --oneline -S "seek_from" -- core/` [0] → `9327f6695`, `8701eae12`
16. `git show 2fe2a4642 -- .../runs.rs | grep "settled\|total"` [0] → shows removal of `-    settled: u64,`
17. `git show 9327f6695 -- .../runs.rs | grep -E "^[-+].*total|^[-+].*settled"` [0] → shows removal of `-    total: u64,`
18. `git show 9327f6695 -- .../merge.rs | grep -E "^[-+].*seek_from|^[-+].*fn "` [0] → shows `-    pub fn seek_from(run: &'a Run, buffer_bytes: usize, offset: u64) -> ContentResult<Self>` removed, `+    pub fn start(&mut self, run: &Run, offset: u64) -> ContentResult<()>` added
19. `git show 9327f6695 -- .../runs.rs` (find diff) [0] → removed `-            let mut reader = RunReader::seek_from(run, self.merge_buffer, from)?;`, added `+            let scan = self.scans[index].get_or_insert_with(|| LookupScan::new(buffer_bytes));`
20. `grep -n "fn read_at" core/crates/layerfs-content/tests/support/filesystem.rs` [0] → L555; body fills the caller-provided `buffer` slice (no heap)
21. `grep -n "ROW_BYTES\|pub enum Row\|fn decode" .../record.rs` [0] → L26 `ROW_BYTES: usize = 96`, L42 `enum Row`, L109 fixed-width `decode`
22. `InodeValue` struct inspection (`core/crates/layerfs-content/src/object/inode_leaf.rs` L84) [0] → plain inline struct (`InodeKind`, `u64`, `ObjectId`, `ObjectId`) — no heap

## Claim 1 — one reader (and buffer) per tier: PASS

`find` acquires each tier's scan from a per-tier `Option` slot in the store, not by
constructing a reader (all in `core/crates/layerfs-content/src/filesystem/references/runs.rs`):

- L47-52 — the store holds `scans: Vec<Option<LookupScan>>`, "One resumable lookup scan
  per tier, parallel to `levels`. A tier's scan - and with it the tier's retained reader
  buffer - is created by the first lookup that reaches a live run in that tier, and empty
  tiers never allocate one."
- L332-336 — acquisition:
  ```rust
  while self.scans.len() <= index {
      self.scans.push(None);
  }
  let buffer_bytes = self.merge_buffer;
  let scan = self.scans[index].get_or_insert_with(|| LookupScan::new(buffer_bytes));
  ```
  The scan (and its buffer) persists in `self.scans` across `find` calls; only the first
  in-range lookup into a live tier constructs one.
- L341-344 — the resume/restart match:
  ```rust
  match scan.resume {
      Some(resume) if serial >= resume => {}
      _ => scan.reader.start(run, 0)?,
  }
  ```
  A continuing request (serial at or beyond the cursor) does nothing — it is served by
  `scan.reader.next(run.handle.as_ref())` (L347) out of the retained buffer. A restarting
  request repositions via `RunScan::start`, which (merge.rs L94-97) sets
  `offset`/`remaining` and `filled = 0; consumed = 0` — the buffer `Vec` is untouched.
- merge.rs L66-69 (`RunScan::new` doc): "The buffer is rounded to whole rows and is never
  reallocated; a restart only invalidates the bytes it holds." L84-85 (`start` doc):
  "Positioning invalidates the buffered bytes; it does not allocate."
- merge.rs L117-127 (`RunScan::next`) serves from the retained buffer whenever
  `consumed != filled`, and refills with one bounded `handle.read_at` only when the buffer
  is exhausted — "The row is served from the retained buffer when it is already held and
  from one bounded `read_at` when it is not." (L114-116)
- History proof of "instead of rebuilding a reader per call": the round-4 diff (command 19)
  removed `let mut reader = RunReader::seek_from(run, self.merge_buffer, from)?;` — a
  per-lookup reader rebuild — and replaced it with the `get_or_insert_with` retention above.
- The scans vector drops a tier's buffer when its run is replaced: `reset_scans`
  (L116-122, `self.scans.clear()` at L121, "Drops every tier's scan, keeping the tiers
  themselves. Called whenever a tier's run is replaced: the position and the retained
  buffer belong to the run that was scanned, not to the tier.") is called from
  **spill** (L231 before the new run is written, and L299 after it is installed),
  **consolidate** (L438), **take_single_handle** (L512) and **release** (L521). Clearing
  drops every `LookupScan` → its `RunScan` → its `buffer: Vec<u8>`.

## Claim 2 — dead states gone: PASS

- `LookupScan` today (runs.rs L546-553) has exactly two fields — `reader: RunScan` and
  `resume: Option<u64>` — no `settled`, no `total`.
- `grep -n "total:" runs.rs` → no matches. `grep -n "settled" runs.rs` → one match,
  L534, prose inside the `LookupScan` doc comment ("a request the tier already settled
  is answered from the buffer the scan holds") — an English verb, not a field or state.
- `grep -rn "seek_from" core/ --include="*.rs"` → exit 1, zero matches; repo-wide
  (outside `/target/`) also zero. `RunReader`'s remaining surface is `new/rewind/next/
  run_offset/rows` (merge.rs L160-194).
- Removal history, reproduced: round 3 = `2fe2a4642` (message: "Round 3 of the Stage 5
  terminal handoff") removed `-    settled: u64,` (command 16); round 4 = `9327f6695`
  (message: "Round 4 ... plus the second dead state the round-3 cleanup left behind")
  removed `-    total: u64,` (command 17) and `-    pub fn seek_from(...)` (command 18).

## Claim 3 — module documentation matches the code: PASS

- runs.rs module doc, L22-27: "**One reader per tier.** A lookup does not build a reader:
  each tier keeps a [`RunScan`](merge::RunScan) with its own retained buffer for the
  lifetime of the tier's run, so lookups are amortized reads from a buffer the store
  already owns. At most `MAXIMUM_LEVELS` such buffers exist at once, each bounded by the
  declared merge buffer, and every one of them is dropped when its tier's run is replaced
  or released."
- `find` doc, L310-323, describes the buffer reuse and the restart semantics: "a lookup
  continues the scan that the previous lookup left in place instead of rebuilding a
  reader. An ascending sweep therefore neither allocates nor re-reads bytes it already
  holds; only a request the cursor has passed restarts the run from the front, and that
  restart keeps the buffer and invalidates its bytes." and "The lookup scans retain at
  most `MAXIMUM_LEVELS` buffers of `merge_buffer` bytes each, bounded by the tier count
  and dropped whenever a tier's run is replaced."
- merge.rs `RunScan` doc, L46-51, states the split: "The scan owns its buffer; the run's
  storage is borrowed per call. That split is what lets the run store keep one reader per
  tier alive across lookups: the buffer is allocated once, the cursor state is the scan's
  fields, and the handle comes from whichever run the tier holds today." `RunReader` doc,
  L149-154: "The reader is a fresh [`RunScan`] plus the run's handle, for callers - the
  merge and visit paths - that read a whole run once and drop the reader. The run store's
  lookup path uses `RunScan` directly so its buffer survives the call."
- Doc-vs-code line-by-line check of the `MAXIMUM_LEVELS` bound: `scans` does grow per tier
  *index* (L325 `for index in 0..self.levels.len()`, L332-334 push up to `index`), so the
  vector holds at most `levels.len()` entries. `levels.len()` never exceeds
  `MAXIMUM_LEVELS = 32` (L41): the only growth site is spill L228-230
  ```rust
  if level == self.levels.len() {
      self.levels.push(None);
  }
  ```
  guarded by L218-227:
  ```rust
  let level = self.levels
      .iter()
      .position(Option::is_none)
      .unwrap_or(self.levels.len());
  if level >= MAXIMUM_LEVELS {
      return Err(ContentError::ResourceUnavailable { what: "ordering tiers" });
  }
  ```
  (`consolidate` pushes at L440 only after `levels.clear()` at L437, so len ≤ 1 there.)
  Therefore `scans.len() ≤ levels.len() ≤ MAXIMUM_LEVELS`, and allocated buffers are
  further bounded by live tiers (empty slots `continue` at L326-328 before any scan is
  created). The module doc's "at most `MAXIMUM_LEVELS`" and `find`'s tighter "bounded by
  the tier count" are both true. Each buffer is sized from `self.merge_buffer` (L335,
  `LookupScan::new(buffer_bytes)` → `RunScan::new` rounds down to whole rows, merge.rs
  L70-75; `RunStore::new` clamps `merge_buffer.max(ROW_BYTES)` at L82, so a buffer never
  exceeds the declared merge buffer).
- Nuance (not a mismatch): `reset_scans` drops **every** tier's scan whenever **any**
  run is replaced (spill/consolidate/take_single_handle/release), a coarser policy than
  "its tier's run". The module doc's statement ("every one of them is dropped when its
  tier's run is replaced or released") remains true — each buffer is dropped no later
  than its own run's replacement — and `reset_scans`' own doc (L116-119) states the
  coarse policy explicitly.

## Claim 4 — the product test proves the property from outside: PASS

`core/crates/layerfs-content/tests/filesystem_ordering_scan.rs`, module doc L1-6: "One
reader per tier: a lookup neither allocates nor rebuilds a reader. ... after the tiers
are built an arbitrary mix of continuing and restarting requests performs no heap
allocation at all. A counting global allocator makes that property observable from
outside the store."

- Counting global allocator: L19-46 (`Counting` forwards to `System`, incrementing
  `ALLOCATIONS` on `alloc` and `realloc`), installed with `#[global_allocator]` at L45-46.
  It counts **process-wide**, so the assertions cover every heap path the lookup wave
  touches (product, backing shim, and harness alike).
- Twelve spills leave several tiers live: L69-78 (`BATCHES = 12`, `ROWS = 64`, one
  `store.spill(&pending)` per batch), L80-83:
  ```rust
  assert!(
      store.live_runs() >= 2,
      "the fixture must leave several tiers live"
  );
  ```
- Warmup bound (allocations to create the tier scans are bounded by the tier count):
  L89-99, warmup lookups are `[1, total / 2, total]`, then
  ```rust
  let warmup_allocated = ALLOCATIONS.load(AtomicOrdering::Relaxed) - warmup_before;
  assert!(
      warmup_allocated <= live_tiers * 2 + 4,
      "creating {live_tiers} tier scans allocated {warmup_allocated} times"
  );
  ```
  The comment (L85-88) states the intent: creation "is bounded by the number of live
  tiers, and it happens once per tier rather than once per lookup."
- Ascending sweep over all 768 serials reads exactly the spilled rows once: `total =
  BATCHES * ROWS = 768` (L79), sweep L106-109, assertion L110-111:
  ```rust
  let sweep_reads = store.work().rows_read - read_before;
  assert_eq!(sweep_reads, total, "the ascending sweep is one pass");
  ```
  `rows_read` is charged per row read inside `find` (runs.rs L348), so the sweep is
  measured as exactly one pass over the spilled rows (768 rows across the live tiers).
- Mixed continuing/restarting wave of 832 lookups performs ZERO heap allocations: the
  measured wave is the 768-lookup ascending sweep (L106-109) plus the 64-lookup
  restarting wave `for serial in (1..=ROWS).rev()` (L116-119) — 768 + 64 = **832**, with
  `allocations_before` captured at L105 before the sweep. The wave is genuinely mixed:
  the sweep itself begins with restarts (`find(1)` repositions tier 3's scan left at
  resume 385 by the warmup; `find(513)` repositions tier 2's scan left at resume 769),
  then continues; the descending wave restarts on every lookup. Assertion L121-125:
  ```rust
  let allocated = ALLOCATIONS.load(AtomicOrdering::Relaxed) - allocations_before;
  assert_eq!(
      allocated, 0,
      "the lookup wave allocated {allocated} times after the tier readers existed"
  );
  ```
- Supporting allocation-freedom of the success path (checked, not just trusted):
  `RecordingBacking::read_at` (tests/support/filesystem.rs L555) fills the caller's
  buffer slice; `Row` is a fixed-width 96-byte record (`record.rs` L26, L42, L109) whose
  `InodeValue` is a plain inline struct (`inode_leaf.rs` L84-93) — `decode` needs no heap.
- Both tests pass (command 5, exit 0).

## Claim 5 — the ordering suite still pins the semantics: PASS

`core/crates/layerfs-content/tests/filesystem_ordering.rs` L774-846,
`run_lookup_answers_every_serial_in_every_order`: six overlapping spill batches
(L785-799) so "a serial can be held by more than one tier and precedence matters"; truth
is a fresh full walk via `visit_newest_first` (L800-810); then the same serials are
demanded in five orders — the original, reversed, interleaved front/back, `vec![keys[0]; 8]`,
and a 3× cycle (L811-823). For every order (L825-836):
```rust
for serial in order {
    let found = store.find(serial).expect("find");
    let row = found.unwrap_or_else(|| panic!("serial {serial} must be found"));
    assert_eq!(row.serial(), serial);
    ... assert_eq!(delta, truth[&serial], "serial {serial} must carry the newest accumulated effect");
}
```
and serials no tier holds are absence, not some other row (L837-843). The reversed,
interleaved and repeated orders repeatedly demand serials behind each tier's cursor, so
restarted scans must return exactly what a fresh walk returns. The new
`a_restarted_scan_returns_the_same_rows_as_a_fresh_one`
(filesystem_ordering_scan.rs L128-157) pins the same property directly: a forward walk of
96 serials, then the same serials demanded again in reverse — "every answer must equal
what a scan started from the front returns" — plus absence for `ROWS + 1`.

## Falsification attempts

(a) **Dead states.** `grep "total:"` in runs.rs: zero matches. `grep "settled"`: one match
at L534 — prose in a doc comment ("a request the tier already settled is answered from
the buffer the scan holds"), not a field; the struct (L546-553) has only `reader` and
`resume`. `seek_from`: zero matches in `core/` and zero repo-wide outside `/target/`.
Both dead states and the removed method are gone from code; the surviving English word
"settled" could mislead a naive grep, but it names the cursor semantics, not a state.

(b) **`MAXIMUM_LEVELS` doc claim.** Verified true (details under Claim 3): scans grow per
tier index, but `levels.len()` is capped at `MAXIMUM_LEVELS` by the spill gate at
runs.rs L218-230 — the only `levels.push(None)` (L229) is reached only when
`level < MAXIMUM_LEVELS`, and `level ≤ levels.len()`. So with `levels.len()` tiers the
scans vector holds at most `levels.len() ≤ MAXIMUM_LEVELS` entries; the doc's bound holds
and `find`'s doc adds the tighter live-tier bound.

(c) **Lookup patterns that would still allocate after warmup.** Two exist:
1. *First touch of a live tier warmup never reached* — e.g. if warmup had touched only
   serials in tier 3's range `[1,512]`, a later serial in tier 2's range `[513,768]`
   would run `get_or_insert_with(LookupScan::new)` (buffer `vec![0; rows*ROW_BYTES]`,
   merge.rs L75) plus possible `scans.push` growth (L332-334). **Covered by construction
   in this test**: the warmup `[1, total/2, total]` spans both live tiers of the 12-spill
   fixture (popcount(12) = 2 live tiers: level 2 `[513,768]`, level 3 `[1,512]`), and the
   warmup assertion bounds exactly these creations. With a different spill count (e.g. 7
   spills → 3 live tiers) a 3-point warmup could miss a middle tier and the zero-allocation
   wave would fail — the property is fixture-conditional, but the once-per-tier creation
   cost is documented and asserted, not hidden.
2. *A run replaced mid-wave* — any `spill`/`consolidate`/`take_single_handle`/`release`
   between lookups drops every scan (reset_scans at L231/299/438/512/521), so the next
   lookup re-allocates its tier's scan. **Impossible within the tested wave by
   construction**: `find` never mutates `levels` or replaces runs, and the test performs
   all 12 spills before warmup with no replacement inside the 832-lookup wave. The test
   does not cover interleaved spill-then-lookup allocation; that pattern is outside the
   claim's stated scope ("after the tiers are built ... wave of 832 lookups"), and the
   docs state the drop-on-replacement behaviour explicitly.
Everything else on the success path is allocation-free by construction (fixed-width
inline `Row` decode, `read_at` into the caller's buffer, `start`/`next` touch no heap),
and the process-wide zero assertion passed empirically.

## UNVERIFIED

- **Timing/measurement grid** — explicitly out of scope for this verifier (a separate
  verifier owns the measurement). No wall-time, memory-peak or scaling numbers were
  produced or checked here.
- **Runtime buffer accounting** — "at most `MAXIMUM_LEVELS` buffers of `merge_buffer`
  bytes" was verified by code reading (structural bound), not by measuring live heap at
  runtime.
- **The task's stated commit hash** `99743b2cf3a869b7d8897a1f16b82d742aeedc40` — not a
  git object (exit 128); verified instead against HEAD `99743b2cff24...` (matching
  9-char prefix). Whether the tail is a typo cannot be resolved from inside the repo.
- **clippy / fmt / `core/tools/check_product_boundary.py`** — not run (not requested;
  this verifier's mandate was the two named test binaries plus code/doc reading). The
  owner's `check-*.log` files in this directory were not independently validated.
- **Interleaved spill/lookup allocation behaviour** — reasoned about (falsification (c)2)
  but not exercised by any test; no product test asserts allocation behaviour across a
  run replacement.
- **`RunReader` merge/visit call sites beyond the four found** (`merge_runs`,
  `visit_newest_first`, `copy_run`, `visit_run`) — confirmed by reading, not by a
  exhaustive use-def tool.
