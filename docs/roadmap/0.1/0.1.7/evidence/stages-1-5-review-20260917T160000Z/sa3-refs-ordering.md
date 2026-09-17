# SA3 — Stage 5 reference accounting + ordering subsystem (OrderingBacking / OrderingRun)

Reviewer: SA3 (independent). Snapshot: HEAD `c99a8d9f9e05a20ecc8e47b11b8f22fa791664e6`, branch `main`, clean tree.
Scope: `core/crates/layerfs-content/src/filesystem/references/*`, `filesystem/update.rs`, the ordering
call sites in `filesystem/input.rs` + the reducer entry points, plus the stage-5 docs.
Method: read-only source/document inspection (read/grep/glob). No cargo build/test/clippy was run by me
(the lead reviewer owns those); every test citation below is a *source* citation, not a run result.
Product source was not modified; this file is the only artifact written.

Severity-ordered findings (details after the table):
F1 HIGH resource/complexity — `RunStore::find` is a linear scan from the run start, called once per
touched/released serial → Θ(n²) row decodes and Θ(n) 16 KiB reader allocations on the spilled path,
and those reads are invisible to the reported counters.
F2 HIGH/contract — the operation's declared `ordering_bytes` ceiling is not a hard bound: `consolidate`
empties `levels` before merging, so `reserve` checks only the output; the only hard cap is the backing
capacity, and `capacity_bytes()` is never read. `peak_live_runs` does not measure the merge it is
asserted to measure.
F3 MEDIUM — `release_zero_count` performs per-serial base/state reads for the frontier serials, and
`touched_serials` materialises every touched serial in one Vec; neither is covered by a declared ceiling.
F4 LOW — dead accounting fields (`CheckedInput::{additions,removals,declared_new}`, `ReferenceReducer::is_touched`);
`removals` is never populated.
F5 LOW — post-failure account is zeroed although bytes remain on disk (`held_bytes`/`owns_storage` report
clean; `cleanup_failed` is the sole signal and the operation drops the error-path release result).
F6 LOW — `FileBacking` run names are a fixed per-instance sequence with `create_new`; a stale file from a
crashed run makes every later backing over the same directory fail permanently.
F7 LOW — the `"new inode removal"` error branch (reachable from the release traversal) is untested and
inconsistent with the documented `"new inode without binding"` label; the contract does not state which
one a legal create-inside-removed-directory input gets.

## Check table

| # | Check | Location | Evidence | Status | Gap |
| --- | --- | --- | --- | --- | --- |
| 1.1 | Additions observed before removals, per changed name | `core/crates/layerfs-content/src/filesystem/update.rs:196-214` | `if let Some(next) = after { note_retained_binding }` then `if let Some(previous) = before { note_removed_binding }` (update.rs:205-212); only the edits stream is observed (`sorted/merge.rs:211` inside the `delta_first` arm), so each changed name is seen once with (before, after) | PASS | — |
| 1.2 | Additions before removals across directories/waves | `update.rs:166-256` then `update.rs:287-311` | every directory merge runs in the `directories` phase; the zero-count release runs only after all of them; counts are evaluated once at the end (`reduce.rs:509-543`), so a cross-directory move cannot transiently reach zero | PASS | — |
| 1.3 | New counts derived from retained final bindings, not a caller assertion | `reduce.rs:88-98, 106-117, 218-232` | `declare_new` registers the serial; `entry` creates `Row::Count{count:0}`; only `note_retained_binding` increments it; `finish_row` overwrites the value's own count with the tally (`reduce.rs:503-506`; grammar note `record.rs:17-20`). Test: `tests/filesystem_topology.rs:252-269` (forged count 99 not trusted) | PASS | — |
| 1.4 | Are new counts derived ONCE (no second, differently derived count)? | `reduce.rs:106-117` vs `validate.rs:100-168` | the *final* count has exactly one derivation site per row kind (`reduce.rs:503-506` for Count, `reduce.rs:515` for Effect). A second, unrelated addition tally exists in validation (`validate.rs:150-159`) but is used only for the multiple-parents check; its sibling `removals` is dead (F4) | PASS (with F4) | duplicated tally kept in `CheckedInput`; `removals` never written |
| 1.5 | Aliases outside the changed set included | `reduce.rs:227-231, 509-543`; `update.rs:411-418` | existing inodes get `Row::Effect{delta}` and the final count is `base.namespace_ref_count + delta` (reduce.rs:515), i.e. the stored total is kept, not recomputed. Tests: `tests/filesystem_hardlinks.rs:84-147` (third alias in a third directory raises 2→3 and the untouched `a/x` reads 3), `:244-285` | PASS | — |
| 1.6 | Newest pending value overrides the base value | `reduce.rs:154-163, 461-481` | `state`/`entry` read pending first, the run second; for an effect row the pending value supplies kind/content/metadata while the count is forced from the base record + delta (reduce.rs:470-475) | PASS | — |
| 1.7 | Moved-out child survives subtree removal | `release.rs:44-60, 129-150`; `update.rs:287-311` | the move-in is counted during the directory phase and the release decrement lands on the same accumulated row, so the derived count stays ≥1; tests `filesystem_hardlinks.rs:187-241`, `filesystem_bounds.rs:393-455` (601 released, moved-out alias keeps count 1) | PASS | verified with `None` backing in hardlinks (`tests/support/filesystem.rs:335-337`); the bounds case uses a backing but does not spill (default pending 4096, ~31 touched) |
| 1.8 | Externally hardlinked file survives | `reduce.rs:515` | same base+delta rule; a file whose other alias is in an untouched directory keeps the stored total. Tests `filesystem_hardlinks.rs:244-285` (removing one alias leaves count 1; removing the last removes the inode) | PASS | parity case `wide` carries a real `link` binding (fixtures manifest `manifest.rs:281`) compared against the sealed reference final records |
| 1.9 | Zero-count release is bounded | `release.rs:103-119`; `update.rs:298-310` | one listing page at a time (`page_entries=64`, `page_bytes=MAXIMUM_PAGE_BYTES=8192`, `limits.rs:10`), one `lookup_many` wave per page, unrelated subtrees untouched; test `filesystem_bounds.rs:413-425` (`pages <= released/64+4`, `peak_depth==2`) | PASS (listing) / INCOMPLETE (frontier reads, F3) | frontier serials are read one at a time (release.rs:67-85, 133) and `touched_serials` returns one Vec of all touched serials (reduce.rs:170-201) |
| 1.10 | Old roots still readable | `update.rs:351-359` (COW emit only) | `tests/filesystem_failure.rs:143-145, 404-409` (`old root intact`), `:325-327` | PASS | no ordering-specific case (the ordering paths never rewrite base objects: `objects.emit` only) |
| 1.11 | No emitted tree object becomes unreachable on a successful update | `update.rs:335-359` | root is emitted last with the inode table as its reference; the reference fixture set compares the whole reachable object set (`tests/filesystem_reference.rs:106-160, 275-301`) | PASS | ordering runs are not canonical objects (they live in the caller's backing, backing.rs:181-191) |
| 2.1 | Compact typed record size, from source | `references/record.rs:26-38` | `ROW_BYTES = 96`; layout 0..8 serial, 8 version, 9 tag, 10..12 length, 12 flag, 13..21 tally, 21..94 value (73 = `INODE_VALUE_BYTES`, `object/inode_leaf.rs:24`), 94..96 reserved zero = 96. Byte-level test `tests/filesystem_ordering.rs:65-135` (asserts 94..96 zero, tag/flag/version/length rejection) | PASS | — |
| 2.2 | Actual threshold crossing value | `reduce.rs:25, 218-248` | default `DEFAULT_MAXIMUM_PENDING = 4096` rows; `entry` spills only when the map already holds `>= maximum_pending` and a new serial arrives (reduce.rs:233-240), so in-memory peak is 4096 rows; crossing is planned work, not an error; `FilesystemResources::check` rejects 0 (`input.rs:90-94`) | PASS | — |
| 2.3 | Tier carry / merge, newest wins | `runs.rs:152-233, 260-319`; `merge.rs:99-159` | a spill writes one sorted batch and merges every lower tier under it (runs.rs:194-224); `consolidate` merges newest tier first (runs.rs:270-308); `merge_runs` takes the newer row on a tie and skips the older duplicate (merge.rs:120-141). Tests `filesystem_ordering.rs:270-330` (`find(5)` returns the newest count 12 after 12 carries) | PASS | doc comment "Level 0 is the newest data" (runs.rs:150-151) states a weaker invariant than the code relies on (lower index = newer); see observation O1 |
| 2.4 | Tombstone precedence (key written in an old run, deleted in a new one; and the inverse) | `reduce.rs:120-129, 218-232, 391-424, 509-534`; `merge.rs:120-141`; `runs.rs:236-252` | there is no tombstone tag (`record.rs:30-32` tags are Count/Effect). A deletion is an Effect row whose derived count reaches ≤0 (reduce.rs:528-534) or a Count row with count 0 (error for a non-root, reduce.rs:493-498). The newest row always *contains* the older total because `entry` re-reads a spilled row into pending before mutating it (reduce.rs:218-241); the merge prefers the newer row on a tie and the newer tier sits at the lower index; the final stream prefers pending over the run for an equal serial (reduce.rs:393-422). Correct in both directions | PASS | deletion is only decidable against the base record + accumulated delta; a row never carries "deleted" by itself (see observation O2) |
| 2.5 | Overflow / truncation behaviour | `record.rs:79-157`; `runs.rs:79-105, 160-171`; `merge.rs:106-152` | fixed 96-byte rows, no truncation; `checked_add/checked_sub` on tally, deltas, pending bytes, output rows and level counting (runs.rs:80-82, 93-95, 202-205; merge.rs:106-109); `>= MAXIMUM_LEVELS (32)` → `ResourceUnavailable("ordering tiers")` (runs.rs:164-168); a short/truncated run fails `read_exact` → `ContentError::Io` (backing.rs:264-269) | PASS | — |
| 2.6 | Simultaneous old/new runs accounted | `runs.rs:194-231, 270-317` | INCOMPLETE/FAIL — see F2: inputs are taken out of `levels` before merging, so neither the byte check nor `peak_live_runs` sees input+output coexistence | FAIL (reporting + enforcement) | handoff requires "Account simultaneous old/new runs during merge, all temporary bytes" (`stage-5-handoff.md:316-323`) |
| 2.7 | Backing quota value + enforcement site | `backing.rs:26, 89-106, 251-262` | `DEFAULT_BACKING_CAPACITY_BYTES = 256 * 1024 * 1024`; `Account::reserve` (backing.rs:91-106) is the accumulating held/peak check; `FileRun::append` reserves `bytes.len()` **before** `write_all` (backing.rs:251-254) and returns the reservation when the write fails (255-258) | PASS | `capacity_bytes()` is never read by product code (only `backing.rs:62, 212` + the test double), so the caller's capacity is never reconciled with the declared `ordering_bytes`; an undersized backing is discovered mid-operation, not "before required work" |
| 2.8 | Operation ceiling enforced BEFORE allocation | `runs.rs:152-173`; `runs.rs:199-217` | `charge_pending` + `reserve` run before `create_run` (runs.rs:156-173); each merge output is reserved before `merge_runs` creates it (runs.rs:202-217); test `filesystem_ordering.rs:391-459` (ceiling of one row fails the operation with no root) | PASS (ordering of the check) | the checked quantity omits the in-flight inputs (F2) |
| 2.9 | Cleanup on success / error / drop | `update.rs:129-143, 347-350`; `backing.rs:220-241, 280-295` | success: `drop(rows)`, flag set, then the checked `reducer.release()` (update.rs:347-350); error: one `backing.release()` guarded by `cleanup_attempted` (update.rs:124-143); drops remove remaining run files and record failures (backing.rs:233-241, 280-295). Tests `filesystem_ordering.rs:461-585, 587-717`; external probe `evidence/stage-5-ordering-corrected-20260917T064142Z/probe_corrected.rs:97-112` + `probe_corrected.stdout:2` (`release_calls=1`, `files_bytes_after_return=0`, no root) | PASS | F5: a failed release zeroes the account while files may remain |
| 2.10 | Error-triggered alternate route / fallback / retry | `runs.rs:47-66, 129-137, 152-155`; `update.rs:125-143` | no retry loop, no alternate algorithm: without a backing the first spill fails `ResourceUnavailable("ordering backing")` (runs.rs:130-137); an empty spill and a single-run consolidation are no-ops, not fallbacks; a failing cleanup fails the operation (update.rs:350) | PASS | — |
| 3.1 | Where the run actually spills: memory or disk | `backing.rs:243-278` | `FileRun` is an ordinary `std::fs::File` (`OpenOptions::write+read`, backing.rs:184-191) in a caller-supplied directory; the only in-memory side is the bounded pending `BTreeMap` (reduce.rs:57); no in-memory `OrderingRun` exists in product code | PASS | the C1 harness uses `FileBacking::new(&config.output)` with defaults (`examples/filesystem_timing_c1.rs:293-296`) |
| 3.2 | Exact byte accounting | `backing.rs:251-260`; `runs.rs:79-88` | run storage: exactly `bytes.len()` charged per append, i.e. 96/row. Pending: `rows * ROW_BYTES` charged, i.e. a row-count model of a `BTreeMap<u64, Row>` whose enum carries `Option<InodeValue>` (73-byte payload) plus node overhead; `charge_pending` charges the pending rows twice (once as memory, once as the run they become; the extra `reserve` at runs.rs:158 is redundant), and `reserve` is a per-call check, not an accumulated account | INCOMPLETE | no `size_of`/allocator measurement; the in-memory residency of the pending map is unmeasured (contrast `content-io-memory-audit.md:126`, "a charged threshold is not total peak memory") |
| 3.3 | Quota constants | `runs.rs:30-34`; `backing.rs:26`; `reduce.rs:25-27` | `DEFAULT_ORDERING_BYTES = 64 MiB`; `DEFAULT_BACKING_CAPACITY_BYTES = 256 MiB`; `DEFAULT_MERGE_BUFFER_BYTES = 16 KiB` (reader buffer = 170 rows × 96 = 16,320 B, merge.rs:59-64); `MAXIMUM_LEVELS = 32`; `DEFAULT_MAXIMUM_PENDING = 4096` rows; `DEFAULT_BASE_BATCH = 32` (final run buffer 32×96×4 = 12,288 B, reduce.rs:360) | PASS | — |
| 4.1 | Code that decides in-memory vs spilled runs | `reduce.rs:218-248`; `runs.rs:152-233`; `backing.rs:179-202` | `entry`: if the serial is absent from pending, look it up (`runs.find`), spill iff `pending.len() >= maximum_pending`, then insert into pending (reduce.rs:219-241); `spill` writes the batch, takes lower tiers, merges, installs at the first free level (runs.rs:152-233) | PASS | — |
| 4.2 | Tombstone precedence code path (old-run write then new-run delete, and inverse) | `reduce.rs:154-163, 218-241, 486-545`; `merge.rs:120-141` | see 2.4 — accumulation into the newest row, then newer-tier/newer-row precedence. A consolidation is forced before the zero-count scan (`reduce.rs:170-171`) and before the final stream (`reduce.rs:262`) | PASS | F1 makes this path quadratic in touched serials |

## Finding detail

**F1 (HIGH, resource; not a wrong result).** `RunStore::find` (`runs.rs:236-252`) iterates the tiers
newest-first and, for the tier whose `[first,last]` range covers the serial, builds a fresh `RunReader`
and reads rows **from offset 0** until it reaches the key (`merge.rs:56-92`; `RunReader::new` allocates
`(buffer/96).max(1)` rows = 16,320 B at the default, `merge.rs:58-67`). `find` is called by
`ReferenceReducer::state` (`reduce.rs:154-158`) and `entry` (`reduce.rs:220`) and `is_touched`
(`reduce.rs:150`). After `touched_serials` consolidates to a single run (`reduce.rs:171`),
`zero_count_serials` calls `state` once per touched serial (`update.rs:406-423`) and
`release_zero_count` calls `state`/`note_removed_binding` once per released child (`release.rs:67, 133`),
so an operation touching n inodes on the run-backed path performs Θ(n²) row decodes and Θ(n) 16 KiB
allocations. Trigger: any operation whose touched-serial count exceeds `maximum_pending_records`
(default 4096; the shipped evidence uses 1) with a backing — exactly the "record-backed workload" the
handoff requires to be tested (`stage-5-handoff.md:325-327`). Observed vs expected: results stay
correct (rows are found), but the lookup work is quadratic and unbounded by any declared ceiling;
the handoff requires bounded merge *and* descriptor work ("Keep bounded pending maps, merge buffers,
run descriptors and release cursors", `stage-5-handoff.md:316-319`). Consequence: a
10^5-inode spilled update costs ~10^10 row decodes; the largest verified input in the stage-5 evidence
is 303 inodes with ~31 touched serials, so the defect is invisible to every collected case.
Additionally `MergeWork::rows_read` ("Rows read back from runs", `merge.rs:31-32`) is only incremented
by `merge_runs` (`merge.rs:116`) and `copy_run` (`runs.rs:409`) — never by `find` or
`visit_newest_first` (`runs.rs:333-373`), which are `&self` readers, so the counter the evidence uses
(`filesystem_bounds.rs:507-509`) reports a small fraction of the actual run reads.
Smallest remedy: give `find` a bounded cursor (advance a per-tier reader monotonically across the
`touched_serials` sweep, which is already ascending) instead of restarting at offset 0, count those
reads in `MergeWork::rows_read`, and add one test asserting `rows_read <= c * touched` for a spilled
update.

**F2 (HIGH/contract).** `RunStore::reserve` (`runs.rs:91-105`) checks
`run_bytes() + pending_bytes + bytes <= limit`, and `run_bytes()` (`runs.rs:140-146`) sums only the
runs currently installed in `levels`. `spill` removes the lower tiers from `levels` before merging
(`runs.rs:194-198`) and keeps the new batch and each merge output outside `levels` until line 224, so
the merge inputs are never in the checked quantity; `consolidate` moves *every* live run into `sources`
first (`runs.rs:270-275`), so during consolidation `run_bytes() == 0` and each check (`runs.rs:282-286`)
degenerates to "this output ≤ limit". Counterexample: k tiers whose total is just under
`ordering_bytes = 64 MiB` can be consolidated while the operation physically owns the accumulated
output plus the current input plus the next output (≈2× the rows); every check passes and the failure
only arrives from the backing's own accumulating account, whose default is 256 MiB
(`backing.rs:26, 91-106`). `OrderingBacking::capacity_bytes` exists (`backing.rs:62, 212`) but no
product code reads it, so nothing checks that the caller's backing can hold the declared
`ordering_bytes` — an undersized backing fails mid-operation rather than "before required work"
(`stage-5-handoff.md:322-323`). Reporting is affected too: `peak_run_bytes`/`peak_live_runs` are
computed from `levels` *after* the merge loop (`runs.rs:226-231, 315-317`), so they exclude the
runs that coexist during a merge, contradicting `MergeWork::peak_live_runs` ("Largest number of live
runs at once", `merge.rs:39-41`) and the assertion comment "an old and a new run coexist during a
merge" (`filesystem_bounds.rs:502-505`), which passes because a hole-filling spill leaves two *tiers*
occupied (`runs.rs:159-172`). `peak_run_bytes` is rescued only because `note_physical_peak`
(`runs.rs:108-112`) folds in the backing's own `peak_bytes()` — a caller-supplied backing that reports
0 would leave the operation's only peak number understated. The completion report's ceiling row
(`stage-5-completion-report-20260917.md:78`) describes the ceiling as covering only "pending rows,
live runs and the output a merge is about to create", which matches the code but not the handoff
requirement at `stage-5-handoff.md:318-319`. Smallest remedy: keep the inputs in the checked set
(count them in `reserve` until their handles are dropped), consult `capacity_bytes()` once at entry
and fail if it cannot cover `ordering_bytes`, and set `peak_live_runs`/`peak_run_bytes` at the moment
inputs and output coexist.

**F3 (MEDIUM).** `release_zero_count` claims "one bounded listing page at a time and one bounded
base-record wave per page" (`release.rs:4-7`), and the page path does use `lookup_many`
(`release.rs:127-128`); but every frontier serial popped from `pending` costs one `reducer.state`
(which may scan a run) and one single-record `base_record` read (`release.rs:66-93`), and each child
that reaches zero is re-read through `reducer.state` (`release.rs:133`). `touched_serials`
(`reduce.rs:170-201`) materialises one `Vec<u64>` of *all* touched serials before the scan
(`update.rs:402-405`) — the largest per-operation structure in the reducer and the only one with no
declared ceiling (it is merely counted afterwards, `update.rs:403`). Consequence: provider demand
waves grow with the released inode count instead of with pages, and peak memory is 8 B × touched
inodes (declared in `ReferenceWork::serials_scanned`) rather than a declared bound. Smallest remedy:
batch the frontier serials into `lookup_many` waves like the page path, or state the touched-serial
vector as an explicit declared input bound.

**F4 (LOW).** `CheckedInput` exposes `additions`, `removals` and `declared_new`
(`validate.rs:85-90`); `removals` is created empty (`validate.rs:101`) and never written,
`declared_new` duplicates `input.new_inodes` (`validate.rs:179`) and is not read anywhere
(`update.rs:171, 183` and `reduce.rs:58` use `input.new_inodes`/their own set). `ReferenceReducer::is_touched`
(`reduce.rs:145-151`) has no caller in `core/`. These are dead accounting surfaces next to the live
one, which invites the "two places compute a count" failure mode. Smallest remedy: delete the unused
fields/method (or populate `removals` and use it).

**F5 (LOW).** On a failed cleanup `FileBacking::release` runs `self.account.held.set(0)` regardless of
the outcome (`backing.rs:220-230`), and `discard_paths` takes the whole path set
(`backing.rs:156-158`), so after a removal failure `held_bytes()` returns 0 and
`owns_storage()` returns false while a run file is still on disk; `cleanup_failed()` (`backing.rs:216-218`)
is the only signal, and on the operation's error path the release result is deliberately discarded
(`let _ = backing.release();`, `update.rs:137-139`). A caller (or test) that checks the two natural
accessors gets a false "clean" reading. `Drop` has the same shape (`backing.rs:233-241, 280-295`).
Smallest remedy: leave `held`/the path set intact when a removal fails (the bytes were not returned),
so the accessors agree with `cleanup_failed`.

**F6 (LOW).** `create_run` builds `layerfs-ordering-{:08}.run` from a counter that restarts at 0 for
every `FileBacking::new`/`with_capacity` (`backing.rs:117-143, 179-191`) and opens it with
`create_new(true)`. A file left by a crashed operation (no `Drop`) therefore makes every later
backing over that directory fail `ResourceUnavailable("ordering run file")` on its first run — no unique
suffix, no adoption, no cleanup of foreign leftovers. The logical contract is respected (the trait
carries no path, `backing.rs:53-77`), but the single shipped implementation hardcodes the convention
that `filesystem-tree.md:262-264` keeps out of the logical layer. Smallest remedy: derive the name from
an unpredictable/unique token, or adopt-and-truncate an existing file of the exact expected name.

**F7 (LOW, untested edge).** `note_removed_binding` rejects a `Row::Count` with
`InvalidRecord("new inode removal")` (`reduce.rs:120-129`). The only reachable trigger is the release
traversal (`release.rs:131`): an existing directory whose last binding is removed in the same operation
that also binds a newly declared inode under it. The release walks the directory's *new* content root,
finds the new child and fails the whole operation — with a label distinct from the documented
"new inode without binding" that the same situation produces on the non-release path
(`reduce.rs:486-498`; tested at `filesystem_hardlinks.rs:288-307` and `filesystem_topology.rs:234-250`).
Nothing in `filesystem-tree.md`/`stage-5-handoff.md` states which outcome such an input gets
(dropping the unreachable new record vs failing), and no test covers the branch. Smallest remedy:
decide the behaviour, document it, make the label consistent, add one test.

## Observations (not defects)

O1 `RunStore::spill`'s comment "Level 0 is the newest data" (`runs.rs:150-151`) states a weaker
invariant than the code needs. The real invariant is *lower tier index = newer*: a spill installs its
batch at the first free index and merges every lower index under it (`runs.rs:159-224`), which is what
makes the ascending iteration in `find` (`runs.rs:237`) and `visit_newest_first` (`runs.rs:337-342`)
newest-first. A future change made on the strength of that comment (e.g. scanning a single "level 0")
would break precedence. The same imprecision appears in `merge.rs:39-41` for `peak_live_runs`.

O2 Deletions have no record tag: the grammar is Count/Effect only (`record.rs:30-32`), so a row is
never self-describing as a tombstone; deletion is derived as `base_count + delta <= 0`
(`reduce.rs:528-534`). Within one operation this is sound because the delta always accumulates into
the newest row, but it means a corrupted/mis-ordered run silently changes a resurrection into a
deletion (or vice versa) with no grammar-level check. `stage-5-handoff.md:309-311` asks for
"value/tombstone/effect and precedence as required" — the derived design satisfies it, worth recording
as the chosen interpretation.

O3 `LAYERFS_TRACE` gated `eprintln!` tracing lives in product source on the reference-accounting path
(`update.rs:196-199, 215-217, 312-314, 332-334`), including one `std::env::var` lookup per observed
binding inside the `observe` closure (`update.rs:197`). It is a debug affordance in shipped code and a
per-binding env lookup in a hot closure; the product-boundary guard's rules
(`core/tools/check_product_boundary.py:8-13`) do not cover it.

O4 Documentation vs code: the completion report's claim "byte/quota accounting ... verified"
(`stage-5-completion-report-20260917.md:65`) and the ceiling row at `:78` are stronger than the code
supports (F2); item 3's "rows read by merges and copies are counted" (`:44`) is literally true but is
the number used as the read-work evidence. Items 1, 2, 4 and 5 of that table (owned bytes, checked
cleanup, 96-byte grammar, newest-first consolidation) match the current source.

## Not run / coverage limits

- `NOT_RUN`: no cargo build/test/clippy by this reviewer; every test citation is a source citation.
- The resources gate at `stage-5-verification.md:104-106` ("`rows_spilled == 0` whenever
  `maximum_pending_records` exceeds the touched row count") is a source-level assertion only in the
  tests read here (`filesystem_ordering.rs:256-261`).
- No collected case exercises a spilled operation with more than ~31 touched serials
  (`filesystem_bounds.rs:461-539`), so F1/F2 are code-level findings without a measured
  counterexample; the smallest missing evidence is one spilled update whose `rows_read`, `peak_run_bytes`,
  `peak_live_runs` and backing `peak_bytes` are compared against the declared ceilings.
