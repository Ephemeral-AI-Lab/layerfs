# TR-5 verification: simultaneous memory and backing during one update

- **Verdict: PASS** (with findings — see §7; no published figure is contradicted).
- Claim under test (receipt `simultaneous-memory.log`, this directory): what one
  update operation holds at once during its references phase, measured with the
  operation's own counters, each with scope, unit and producing counter — pending
  rows (64 rows = 6,144 B, `references.peak_pending` × 96 B/row), ordering bytes
  (568,320 B, `references.runs.peak_run_bytes`: live runs + spilled-but-unmerged
  inputs + pending + reserved outputs), live-tier scan buffers (5 × 16,384 B =
  81,920 B, `references.runs.peak_live_runs` × `merge_buffer_bytes`), caller
  backing peak (`FileBacking::peak_bytes`) — plus sequential-phase scratch peaks
  (directories 376,110 B, inodes 349,820 B), with **no** process-level RSS, cgroup
  or page-cache figure claimed. Published as the §5 table "Simultaneous memory and
  backing during one update (TR-5)" in
  `docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md`.

## 1. Identity and environment

| Item | Value |
| --- | --- |
| HEAD at verification start (2026-09-17T19:36Z) | `3ecb952c8ed530706700e09647f42ee51bf09f98` (matches the task's `3ecb952c8`) |
| HEAD at evidence-file time (2026-09-17T19:53Z) | `1884e3ecadaba2bc158d2995cfef79faca4f04fe` — a sibling agent landed the then-uncommitted `codec.rs` doc fix mid-verification; the repo is under active sibling work (many modified docs, untracked `core/docs/`) |
| Receipt's own pin (`head.txt`) | `134b8df73c9672544e93e99dee3ce93991ef2047` — the commit that landed `simultaneous-memory.log`; the file is unchanged from there through current HEAD |
| Working tree at start | **NOT clean** (task premise said clean): ` M core/crates/layerfs-storage/src/encoding/codec.rs`, a comment-only FFI-inventory diff (11+/10−, no code); that exact diff was committed by the sibling as `1884e3eca` at ≈2026-09-17T19:38:43Z |
| Counter-relevant code, `134b8df73` → `1884e3eca` | Identical for every path this claim depends on (`references/runs.rs`, `record.rs`, `reduce.rs`, `merge.rs`, `release.rs`, `backing.rs`, `filesystem/update.rs`, `filesystem/input.rs`, the s5term client). The only product changes are `limits.rs` (docs; `MAXIMUM_TREE_LEVEL` re-derived from `file::mapping::MAX_LEVEL`, same value 31; unused `DEFAULT_OPERATION_SCRATCH_BYTES` twin deleted) and `sorted/budget.rs` (dead accessor `default_limit()` deleted) — behavior-neutral, confirmed by reading both diffs |
| Measurement binary (runs A/B) | `/tmp/layerfs-s5term-target/release/s5term`, mtime 2026-09-17T18:33:47Z (predates the receipt recording at ≈18:38Z ⇒ the receipt's own build); md5 not recorded before a sibling rebuilt it |
| Measurement binary (run C) | Same path rebuilt 2026-09-17T19:47Z by a sibling, md5 `5c3bb8a1f93e0eb522662c392915fcb8` |

## 2. Commands and exit codes

All read-only on the repository; the only file written in the repository is this
one. Working directory `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.

1. `git rev-parse HEAD && git status --porcelain` — exit 0 — `3ecb952c8ed5…`; one modified file (`codec.rs`, comment-only).
2. `pgrep -fl "cargo|s5term"` — exit 1 — nothing concurrent at 19:36Z.
3. `git diff core/crates/layerfs-storage/src/encoding/codec.rs` — exit 0 — doc-comment FFI inventory only (`ZSTD_compressBound`, `ZSTD_estimateCCtxSize_usingCParams` added to the module doc); no code.
4. `git log/merge-base/show --stat` checks — exit 0 — receipt landed in `134b8df73`, unchanged since; `134b8df73` is an ancestor of HEAD; `git diff --stat 134b8df73 3ecb952c8` touches only `limits.rs`, `sorted/budget.rs`, tests and docs among product paths.
5. **Run A (diagnostic — concurrent)**: `pgrep -fl "cargo|s5term"` found PIDs 64044/64047, a sibling's `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content` already active, then `/tmp/layerfs-s5term-target/release/s5term coexist` — exit 0 — output in §3. The pre-check at 19:36Z was clear; the sibling's cargo test started in the interval and was concurrent with this run.
6. **Run B (gate sample — quiet)**: `pgrep` quiet immediately before and after; `/tmp/layerfs-s5term-target/release/s5term coexist` — exit 0 — output in §3.
7. Source reads (read tool / `grep` / `sed`, all exit 0): `references/runs.rs`, `references/record.rs`, `references/reduce.rs`, `references/release.rs`, `references/merge.rs`, `references/backing.rs`, `filesystem/update.rs`, `filesystem/input.rs`, `sorted/page.rs`, `s5term/src/main.rs`, receipt, `head.txt`, `git-status.txt`, report §5.
8. `git show 3ecb952c8:docs/…/stage-5-report.md | sed -n '386,408p'` — exit 0 — the committed TR-5 table, identical to the worktree copy read earlier; worktree-vs-`3ecb952c8` diff for the report is 136 ± lines elsewhere (sibling edits in flight).
9. `git show 1884e3eca --stat`, `git diff HEAD -- …codec.rs` — exit 0 — resolved the codec.rs story: `3ecb952c8` had not staged the module-doc fix; `1884e3eca` landed it; worktree then matched HEAD for that file.
10. **Run C (post-rebuild confirmation — quiet)**: `pgrep` quiet before/after; rebuilt binary (md5 `5c3bb8a1…`) `/tmp/layerfs-s5term-target/release/s5term coexist` — exit 0 — output in §3.

## 3. Measurement outputs (grid row columns: pairs, spilled, elapsed_ns, rows_read, rows_writ, runs, peak_owned, held_now, peak_back, emitted, pend, dir_scr, ino_scr, tiers)

Receipt (line 5): `2000  3968  139433375  59007  25760  124  568320  0  568320  14  64  376110  349820  5`

- Run A (concurrent, exit 0): `2000  3968  223985083  59007  25760  124  568320  0  568320  14  64  376110  349820  5`
- Run B (quiet, exit 0): `2000  3968  162377834  59007  25760  124  568320  0  568320  14  64  376110  349820  5`
- Run C (quiet, rebuilt binary, exit 0): `2000  3968  173640167  59007  25760  124  568320  0  568320  14  64  376110  349820  5`

All three runs print the derived block: pending 64 rows = 6,144 B; ordering bytes
568,320 B; 5 tiers × 16,384 B = 81,920 B; backing peak = peak_back column;
directories 376,110 B; inodes 349,820 B; "no process-level RSS, cgroup or
page-cache figure is claimed". Byte-identical to the receipt except `elapsed_ns`.

### Counter comparison (deterministic figures)

| Counter | Receipt | Run A | Run B | Run C | Match |
| --- | --- | --- | --- | --- | --- |
| `references.peak_pending` | 64 | 64 | 64 | 64 | ✓ |
| pending bytes (64 × 96) | 6,144 B | 6,144 B | 6,144 B | 6,144 B | ✓ |
| `references.runs.peak_run_bytes` | 568,320 | 568,320 | 568,320 | 568,320 | ✓ |
| `references.runs.peak_live_runs` | 5 | 5 | 5 | 5 | ✓ |
| scan buffers (5 × 16,384) | 81,920 B | 81,920 B | 81,920 B | 81,920 B | ✓ |
| `FileBacking::peak_bytes` (peak_back) | 568,320 | 568,320 | 568,320 | 568,320 | ✓ |
| `directories.peak_scratch_bytes` | 376,110 | 376,110 | 376,110 | 376,110 | ✓ |
| `inodes.peak_scratch_bytes` | 349,820 | 349,820 | 349,820 | 349,820 | ✓ |
| spilled / rows_read / rows_written / runs_created / emitted / held_now | 3968 / 59007 / 25760 / 124 / 14 / 0 | same | same | same | ✓ |
| `elapsed_ns` (wall time, not part of TR-5) | 139,433,375 | 223,985,083 | 162,377,834 | 173,640,167 | varies, expected |

**No difference in any deterministic counter.** Concurrency disclosure: run A
overlapped a sibling's `cargo test -p layerfs-content` (PIDs 64044/64047) that
started after the 19:36Z pre-check; it is reported as a diagnostic. Runs B and C
ran with the machine quiet (pgrep exit 1 before and after each).

## 4. Semantics verified against the source

(a) **`peak_run_bytes` scope** — VERIFIED. `owned_bytes()` =
`run_bytes()` (live runs) + `pending_bytes` + `merge_input_bytes`
(spilled-but-unmerged inputs): `core/crates/layerfs-content/src/filesystem/references/runs.rs:143-147`.
`reserve()` adds the about-to-be-created output bytes to that owned set and
records the maximum (`runs.rs:150-164`); `charge_pending()` charges the pending
map (`runs.rs:130-139`, reserving 2× because the pending rows and the run they
become must both fit); `spill()` charges merge inputs (`runs.rs:253-264`) and
reserves each merge output (`runs.rs:268-276`); `note_physical_peak()` folds the
backing's own peak into the counter (`runs.rs:167-171`). The receipt's wording
"live runs + spilled-but-unmerged inputs + pending + reserved outputs" is exactly
what the code measures.

(b) **96 B/row** — VERIFIED. `pub const ROW_BYTES: usize = 96;` at
`core/crates/layerfs-content/src/filesystem/references/record.rs:26` (grammar doc
at record.rs:3-15). 64 × 96 = 6,144 B ✓.

(c) **merge buffer default and derived bound** — VERIFIED. `DEFAULT_MERGE_BUFFER_BYTES = 16 * 1024`
(`runs.rs:37`), wired as the `FilesystemResources` default `merge_buffer_bytes`
(`filesystem/input.rs:75`). One retained scan per live tier, each bounded by the
merge buffer (`runs.rs:52`, `321-323`, `332-336`); the buffer allocation rounds
down to whole rows (170 × 96 = 16,320 B ≤ 16,384 B, `merge.rs:70-75`), so 16,384
B/tier is a correct conservative bound. At most `MAXIMUM_LEVELS = 32` tiers
(`runs.rs:41`; refusal at `runs.rs:223`), so "≤ 32 × 16 KiB" (= 524,288 B) and
"at most MAXIMUM_LEVELS × merge buffer" are stated correctly **for the scan
buffers** — see finding F1 for the merge-reader caveat.

(d) **Sequential phases; scratch does not coexist with the references phase** —
VERIFIED. `phases.phase("validate"|"directories"|"references"|"inodes"|"cleanup"|"root.encode")`
at `filesystem/update.rs:160, 179, 337, 354, 369, 374`. The scratch `Budget`
lives inside the per-call sorted `Engine` (`sorted/page.rs:154-166`), and
`peak_scratch_bytes` is snapshotted when that engine finishes
(`sorted/finish.rs:108`), so no directories/inodes scratch lease is alive during
the references phase. Caveat (finding F2): the references phase's output stream
is consumed *inside* the inodes phase (`update.rs:346-356`), so ordering stream
state coexists with the inodes scratch — the phases are sequential, not
memory-isolated. Note also `zero_count_serials`/`release_zero_count`
(`update.rs:310-336`) run between the directories and references phases, outside
any named phase scope.

(e) **No process-level figure** — VERIFIED. Receipt line 14 and report §5
(stage-5-report.md:406-408) both state it; every published number is an
operation counter (`ReferenceWork`, `MergeWork`, `SortedWork`, `FileBacking`).

## 5. Report §5 TR-5 table (stage-5-report.md:386-408, committed at 3ecb952c8)

| Report row | Report value | Receipt | Code | Match |
| --- | --- | --- | --- | --- |
| pending rows | 64 rows = 6,144 B, `references.peak_pending` × 96 B/row, scope "the reducer's pending map" | same | reduce.rs:49,57; record.rs:26 | ✓ |
| ordering bytes | 568,320 B, `references.runs.peak_run_bytes`, scope "live runs + spilled-but-unmerged inputs + pending + reserved outputs" | same | runs.rs:143-164; merge.rs:43 | ✓ |
| live-tier scan buffers | 5 tiers = 81,920 B, `references.runs.peak_live_runs` × `merge_buffer_bytes`, "one retained reader buffer per live tier, ≤ 32 × 16 KiB" | same | runs.rs:41,52,321-336; input.rs:75 | ✓ |
| caller backing | 568,320 B peak, `FileBacking::peak_bytes`, scope "the physical owner of the run files" | same | backing.rs:269-273 | ✓ |
| scratch leases | directories 376,110 B, inodes 349,820 B, "held in their own sequential phases, not during the references phase" | same | update.rs:269-272,361; page.rs:53 | ✓ |
| phases / no-process-level | validate, directories, references, inodes, cleanup, root.encode; "No process-level RSS, cgroup or page-cache figure is claimed" | same | update.rs:160-374 | ✓ |

Arithmetic: 64 × 96 = 6,144 ✓; 5 × 16,384 = 81,920 ✓; 568,320 / 96 = 5,920 row-equivalents, and 568,320 = peak_back (the backing peak folded into `peak_run_bytes` by `note_physical_peak`). **No mismatch found.**

## 6. Reproduction command

```sh
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
pgrep -fl "cargo|s5term"   # must be empty
/tmp/layerfs-s5term-target/release/s5term coexist
```

## 7. Findings (falsification: omitted simultaneous owners)

None of these contradicts a published figure; each is an owner the table does not
list. Reported with path:line as requested.

- **F1 — merge reader buffers coexist with the tier scans during consolidate
  (inside the references phase).** Each `merge_runs` call holds two fresh
  `RunReader` buffers (`core/crates/layerfs-content/src/filesystem/references/merge.rs:213-214`,
  each ≈16,320 B). `consolidate()` takes every run out of its tier
  (`runs.rs:391-399`) and merges (`runs.rs:400-434`) **before** dropping the tier
  scans (`reset_scans()` at `runs.rs:438`), so during the references phase's
  final consolidate up to `peak_live_runs` scan buffers + 2 merge-reader buffers
  are live at once: for this fixture ≈7 × 16,320 ≈ 114,240 B, versus the table's
  81,920 B scan-buffer row. The row is correctly scoped to scan buffers, but the
  table omits the merge-reader owner, and the derived cap "at most MAXIMUM_LEVELS
  × merge buffer" can be exceeded in the general case (32 scans + 2 readers = 34
  buffers); that extreme needs ~2³²−1 spills and is not fixture-reachable.
- **F2 — `FinalRows` stream state, allocated inside the references phase, is in
  no row.** `run_buffer` = `base_batch`(32) × 96 × 4 = 12,288 B
  (`core/crates/layerfs-content/src/filesystem/references/reduce.rs:366`), the
  taken pending map re-collected as `Vec<Row>` (`reduce.rs:349`), plus
  `lookahead`/`wave`/`serials`/`bases` bounded by `base_batch` (reduce.rs:327,
  397-458) — together ≈15 KB class. The stream persists into the inodes phase and
  coexists with the inodes scratch peak (`update.rs:346-356`).
- **F3 — pre-phase owners (outside the references-phase scope, disclosed for
  completeness).** The touched-serials collection `Vec<u64>`
  (`update.rs:463-474`; counter `serials_scanned`, reduce.rs:47,210-212; ≈2,001 ×
  8 B here) coexists with pending+runs+scans between the directories and
  references phases and is byte-counted nowhere — it is only count-refused via
  `maximum_touched_serials` (`update.rs:465-473`). The release traversal's
  `queue`/`cursors`/`pending`/`prefetched`
  (`references/release.rs:70-77`) likewise precede the references phase; in this
  fixture no serial reaches zero count, so it holds ~nothing.

## 8. UNVERIFIED

1. **The receipt-era binary's source seal.** Its md5 was not recorded before a
   sibling's 19:47Z rebuild replaced it. Mitigation: the client and every
   counter-relevant source file are byte-identical from `134b8df73` through
   `1884e3eca`, and two independently built binaries (receipt-era build and the
   19:47Z rebuild) reproduced every deterministic counter exactly.
2. **Which instant attains `peak_run_bytes` = 568,320** (the final consolidate
   inside the references phase vs. the earlier `touched_serials` consolidate
   between phases). Not observable through the public counters; the four owner
   categories do demonstrably coexist during the references phase (finish's
   consolidate holds live runs + merge inputs + reserved output + the pending map
   + scans + backing).
3. **The runtime count of live tier scans at the final consolidate** —
   structurally bounded by `peak_live_runs` = 5; no counter exposes it.
4. **Stability of stage-5-report.md beyond the verified snapshot** — siblings are
   editing it now; the TR-5 table was verified in the committed `3ecb952c8` copy
   (identical to the worktree copy read at 19:44Z). Later edits were not reviewed.
5. **`elapsed_ns`** — wall time, intentionally not compared (not part of TR-5).
6. Note, not a gap: no warm/cold cache contract applies — the fixture is an
   in-memory `Bag` and TR-5 claims counters, not timed performance.
