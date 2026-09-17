# verify-R2-F8-scaling: the ordering scaling receipt (R2-F8 / N-6)

Verifier: read-only verification subagent, 2026-09-18 ~03:41-03:50 +0800.
Scope: the receipt `ordering-scaling.log` (this directory) against the round-2
review's pre-fix P1 grid and `stage-5-report.md` §15's "ordering scaling receipt,
stated honestly" paragraph. Read-only on the repository; the only writes were
under `/tmp` (binary backup, quiet-window script, the client's own backing dirs)
and this file.

| | |
| --- | --- |
| HEAD at tasking | `3ecb952c8ed530706700e09647f42ee51bf09f98` (matches the 9-char tasking prefix) |
| HEAD when the grid ran | `1884e3ecadaba2bc158d2995cfef79faca4f04fe` (= `3ecb952c8` + one doc-only commit in `layerfs-storage/codec.rs`, landed mid-verification at 03:38:43; `layerfs-storage` is not linked into this client) |
| Product src state | `core/crates` and `diagnostics/s5term` clean at both commits (dirty files were docs/evidence only) |
| Binary | rebuilt at HEAD `1884e3eca`, sha256 `4d58afde0c32743016edd13028241d1f40634b30f5d1fc6da09fac90a5198407` — **bit-identical** to the prebuilt 02:33:47 binary (same sha256), which was built from the receipt's code tree `9327f6695` |

## Verdict: **PASS**

Every deterministic counter in the receipt is reproduced exactly — by the
receipt's own binary (run A) and by a clean re-run of a bit-identical rebuild at
the current HEAD (run B) — and is identical to the review's pre-fix grid in every
column the review printed. The ~×3.0 per-doubling read ratio is reproduced; the
amplification is honestly stated as still superlinear with the O(n log n)
tiered-merge reason and no O(changes) claim; elapsed is single-sample noise
against the review's numbers, with one caveat noted in finding 5.

## Concurrency record (task precondition)

- Session start `pgrep -fl "cargo|s5term"` → exit 1 (nothing active).
- Run A (03:45:30): pgrep **exit 0 — a sibling verifier's `cargo +1.85.1 test
  --manifest-path core/Cargo.toml --locked` was active** (PIDs 64587, 65314).
  Run A's **elapsed column is therefore contaminated** and is excluded from the
  noise assessment; its deterministic counters are unaffected (and matched).
- Waited for quiet (`/tmp/wait-quiet-check.sh`, polls pgrep every 10 s): "QUIET
  after ~0s" at 03:47:01.
- Run B (03:47:44, the gate sample): pgrep exit 1 — **nothing concurrent**. The
  rebuild (03:47:3x) was the only cargo process in that window.

## Commands and exit codes

| # | Command (cwd = repo root unless noted) | Exit |
| --- | --- | --- |
| 1 | `git rev-parse HEAD && git status --porcelain \| head -5` | 0 — `3ecb952c8…`; tree **not** clean: ` M core/crates/layerfs-storage/src/encoding/codec.rs` (see finding 1) |
| 2 | `pgrep -fl "cargo\|s5term"` (session start) | 1 — nothing concurrent |
| 3 | `ls` evidence dir, client dir, `/tmp/layerfs-s5term-target/release/s5term` | 0 — binary present, 887,664 B, mtime 2026-09-18 02:33:47 |
| 4 | `git diff core/crates/layerfs-storage/src/encoding/codec.rs` | 0 — doc-comment-only change (FFI inventory wording) |
| 5 | `git log 134b8df73..HEAD`, `git log -1`, `git diff --stat 9327f6695..HEAD -- core/crates/layerfs-content/src core/crates/layerfs-telemetry/src` | 0 — receipt `head.txt` tree `134b8df73` is docs-only on top of code tree `9327f6695`; the only src delta since the binary's build is `limits.rs` + `sorted/budget.rs` (remedy commit `3ecb952c8`) |
| 6 | `git diff 134b8df73..3ecb952c8 -- …/limits.rs …/budget.rs`; `grep MAX_LEVEL …/mapping/types.rs` | 0 — delta is value-preserving constant alias (`MAXIMUM_TREE_LEVEL = file::mapping::MAX_LEVEL`, and `MAX_LEVEL: u8 = 31` at `mapping/types.rs:17`), dead-code deletion (`DEFAULT_OPERATION_SCRATCH_BYTES`, `Budget::default_limit`) and doc comments |
| 7 | `cargo +1.85.1 --version` | 0 — cargo 1.85.1 (d73d2caf9 2024-12-31) |
| 8 | fixture-shape grep of the review client `stage15-diag/src/main.rs` | 0 — shape equivalent (see falsification) |
| 9 | Run A: `date; pgrep -fl "cargo\|s5term"; /tmp/layerfs-s5term-target/release/s5term grid` | grid 0; pgrep 0 (**concurrent cargo test — elapsed contaminated**) |
| 10 | `sh /tmp/wait-quiet-check.sh` | 0 — quiet at 03:47:01 |
| 11 | `shasum -a 256` prebuilt binary; `cp` it to `/tmp/s5term-prebuilt-backup-9327f6695`; `git rev-parse HEAD`; `git status --porcelain -- core/crates …/diagnostics/s5term` | 0 — `4d58afde…`; HEAD `1884e3eca`; product/client tree clean |
| 12 | rebuild (cwd = `…/diagnostics/s5term`): `CARGO_TARGET_DIR=/tmp/layerfs-s5term-target cargo +1.85.1 build --release --locked` | 0 — "Finished `release` profile [optimized] in 4.40s"; recompiled `layerfs-content` + `s5term`; rebuilt sha256 = `4d58afde…` (bit-identical) |
| 13 | Run B: `date; pgrep -fl "cargo\|s5term"; /tmp/layerfs-s5term-target/release/s5term grid` | grid 0; pgrep 1 (**quiet — gate sample**) |
| 14 | `python3` ratio/delta computations | 0 — tables below |

## Full grid outputs (verbatim)

Run A — prebuilt binary (the receipt's own artifact; elapsed contaminated by a
concurrent cargo test):

```
s5term: round-4 terminal-handoff probes (public API only)
== grid: ordering scaling, maximum_pending_records = 64, base 4000 files ==
 pairs  spilled    elapsed_ns  rows_read  rows_writ   runs  peak_owned    held_now   peak_back   emitted    pend dir_scr ino_scr     tiers
   250      448      12304500       2198       1588     14       66432           0       66432         7      64  280036  328156         3
   500      960      33652917       6684       4328     30      139008           0      139008         8      64  344652  328156         4
     doubling 250 -> 500: rows_read x3.04, elapsed x2.74
  1000     1984      73395000      19960      10896     62      284160           0      284160        10      64  375990  328156         5
     doubling 500 -> 1000: rows_read x2.99, elapsed x2.18
   2000     3968     146192916      59007      25760    124      568320           0      568320        14      64  376110  349820         5
     doubling 1000 -> 2000: rows_read x2.96, elapsed x1.99
```

Run B — clean gate sample, rebuilt (bit-identical) binary, nothing concurrent:

```
s5term: round-4 terminal-handoff probes (public API only)
== grid: ordering scaling, maximum_pending_records = 64, base 4000 files ==
 pairs  spilled    elapsed_ns  rows_read  rows_writ   runs  peak_owned    held_now   peak_back   emitted    pend dir_scr ino_scr     tiers
   250      448      14185084       2198       1588     14       66432           0       66432         7      64  280036  328156         3
   500      960      31375208       6684       4328     30      139008           0      139008         8      64  344652  328156         4
     doubling 250 -> 500: rows_read x3.04, elapsed x2.21
  1000     1984      69274417      19960      10896     62      284160           0      284160        10      64  375990  328156         5
     doubling 500 -> 1000: rows_read x2.99, elapsed x2.21
   2000     3968     144112416      59007      25760    124      568320           0      568320        14      64  376110  349820         5
     doubling 1000 -> 2000: rows_read x2.96, elapsed x2.08
```

## Deterministic-counter comparison (review pre-fix / receipt / run A / run B)

Review sources: `stages-1-5-review-20260917T230700Z/diagnostics/diag-run.log:4-7`
(P1) and the review's own restatement
`…/component-decoupling/stages-1-5-review-20260917T230700Z.md:312-317` (F8) and
`:1154-1159` (§6.2, adds `backing peak` = the receipt's `peak_back` and
`backing held after` = the receipt's `held_now`). Receipt:
`ordering-scaling.log:4-9`. The review's client printed no `pend`, `dir_scr`,
`ino_scr` or `tiers` columns; those are compared receipt-vs-rerun.

| pairs | spilled | rows_read | rows_written | runs | peak_owned | held_now | peak_back | emitted | pend | dir_scr | ino_scr | tiers |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 250 | 448 = 448 = 448 = 448 | 2,198 ×4 | 1,588 ×4 | 14 ×4 | 66,432 ×4 | 0 ×4 | 66,432 ×4 | 7 ×4 | 64 r/rB | 280,036 r/rB | 328,156 r/rB | 3 r/rB |
| 500 | 960 ×4 | 6,684 ×4 | 4,328 ×4 | 30 ×4 | 139,008 ×4 | 0 ×4 | 139,008 ×4 | 8 ×4 | 64 r/rB | 344,652 r/rB | 328,156 r/rB | 4 r/rB |
| 1,000 | 1,984 ×4 | 19,960 ×4 | 10,896 ×4 | 62 ×4 | 284,160 ×4 | 0 ×4 | 284,160 ×4 | 10 ×4 | 64 r/rB | 375,990 r/rB | 328,156 r/rB | 5 r/rB |
| 2,000 | 3,968 ×4 | 59,007 ×4 | 25,760 ×4 | 124 ×4 | 568,320 ×4 | 0 ×4 | 568,320 ×4 | 14 ×4 | 64 r/rB | 376,110 r/rB | 349,820 r/rB | 5 r/rB |

("×4" = identical across review, receipt, run A, run B; "r/rB" = receipt and both
re-runs agree, no review counterpart.) **No counter differs anywhere.** Zero
counter differences out of 4 rows × 12 columns.

## Elapsed comparison (single samples, ns)

| pairs | review (diag-run.log) | receipt | run A (contaminated) | run B (clean) | receipt−review | runB−review | runB−receipt |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 250 | 15,831,958 | 16,316,625 | 12,304,500 | 14,185,084 | +3.1% | −10.4% | −13.1% |
| 500 | 35,641,708 | 32,709,000 | 33,652,917 | 31,375,208 | −8.2% | −12.0% | −4.1% |
| 1,000 | 87,571,000 | 79,641,750 | 73,395,000 | 69,274,417 | −9.1% | −20.9% | −13.0% |
| 2,000 | 154,513,875 | 157,575,875 | 146,192,916 | 144,112,416 | +2.0% | −6.7% | −8.5% |

Per-doubling ratios — `rows_read`: **3.04 / 2.99 / 2.96 in all four sources**
(review's own arithmetic agrees, review md:319). Overall 59,007/2,198 = 26.85×
for 8× pairs = n^1.582, matching §15's "≈ n^1.58". Elapsed: review 2.25 / 2.46 /
1.76 (recomputed from its table); receipt 2.00 / 2.43 / 1.98; run B 2.21 / 2.21 /
2.08 — all in the ~2-2.5 band, no directional pattern.

## §15 paragraph check, claim by claim (`stage-5-report.md:683-700`)

1. "repeats the round-2 review's P1 grid … (4,000-file base,
   `maximum_pending_records = 64`, release profile, one sample per case)"
   (685-687) — **verified**: client source (below), release build log, single
   `measured_update` per pair count; the review client's fixture shape is
   source-equivalent (`stage15-diag/src/main.rs:148-190`).
2. "work counters are **identical** to the review's pre-fix grid" with the five
   counter lists (687-690) — **verified exactly**, including `peak_back` and
   `held_now` via the review's §6.2 table (review md:1154-1159).
3. "per-doubling read ratio remains ~×3.0 (2,198 → 59,007 over three doublings,
   ≈ n^1.58)" (691-692) — **verified**: 26.85× ⇒ 2.99×/doubling ⇒ n^1.582.
4. "read amplification is still superlinear, and the reason is the tiered merge
   itself, which is O(n log n) … at a fixed pending ceiling" (692-694) —
   superlinearity **verified** (×3.0 > ×2.0 per doubling in every sample). The
   O(n log n) attribution is the mechanism's class; note the review itself
   expected ~2.1×/doubling for that class at these sizes (review md:323-324) and
   3.0× was observed — §15 claims no fix and no speedup, only the mechanism, so
   the statement is honest; the finite-size exponent (n^1.58) sits above the
   class's small-size expectation. Observation, not a defect.
5. "No O(changes) claim is made." (695) — **verified**; the sentence is present
   and no such claim appears in §15 or the receipt README.
6. "Elapsed times are 16.3 / 32.7 / 79.6 / 157.6 ms against the review's
   15.8 / 35.6 / 87.6 / 154.5 ms - single samples on the same machine, within
   sample noise, no faster claim" (695-697) — numbers **verified** against
   `ordering-scaling.log:4-9` and `diag-run.log:4-7`; receipt-vs-review deltas
   are within ±9%; my clean re-run is −7% to −21% (caveat in finding 5); §15
   makes no directional claim, so nothing published overstates.
7. "the per-lookup 16 KiB allocation and buffer re-read are gone (the
   counting-allocator test asserts a zero-allocation lookup wave…)" (697-700) —
   the test exists (`core/crates/layerfs-content/tests/filesystem_ordering_scan.rs`)
   and ran in `check-cargo-test.log:269`; its re-run is `verify-R2-F8-N6.md`'s
   scope, not this receipt's (see UNVERIFIED).

## Falsification checks (client `diagnostics/s5term/src/main.rs`)

- **2 changes per rename pair**: `rename_pairs` allocates `pairs * 2` (157) and
  pushes, per index, one unbind `(f%05d, None)` and one rebind `(r%05d, Some(serial))`
  (158-161) — exactly 2 changes per pair; the review's §6.2 says the same
  ("a rename contributes 2 keys", review md:1161-1162). **Holds.**
- **4,000-file base**: `fixture(4000)` is called inside the case loop (236);
  `fixture` builds 4000 files + 2 directories from an empty `Bag` (81-153). **Holds.**
- **pending forced to 64**: `FilesystemResources { maximum_pending_records: 64 }`
  (189-192); the printed `pend` column is 64 in the receipt and both re-runs. **Holds.**
- **fresh backing directory per case**: `backing_dir` does `remove_dir_all` +
  `create_dir_all` (169-174); each case gets `grid-{pairs}` (194) — distinct
  labels *and* freshly removed; the fixture build uses its own fresh
  `layerfs-s5term-fixture` dir per case (141, called once per `fixture()`). **Holds.**
- **no cross-case cache warming**: each case rebuilds its fixture from scratch
  (fresh `Bag`, `build_filesystem`, 82/137-148); the measured update reads only
  that case's in-memory `Bag` (`read_canonical_batch`, 57-61) and its own spill
  files in its own fresh directory — nothing a later case reuses; no warm-up run;
  one sample per case, no best-of. Within a case, fixture preparation is untimed
  and identical for every case (the sanctioned declared-preconditioning pattern).
  Each fixture rebuilds from scratch — confirmed. **Holds.**
- **public entry points only**: imports are public API of `layerfs-content` /
  `layerfs-telemetry` (17-32); standalone `[workspace]` (`Cargo.toml:9`); nothing
  in the repository imports it; dependencies are exactly those two crates
  (`Cargo.toml:7-8`) — `layerfs-storage` (the mid-session doc change) is not
  linked at all.

## Findings

1. **Task-premise drift, no measurement effect.** The tree was **not** clean at
   session start (`core/crates/layerfs-storage/src/encoding/codec.rs` modified —
   an in-flight doc-comment-only FFI-inventory edit, committed mid-session as
   `1884e3eca` at 03:38:43), and HEAD advanced `3ecb952c8` → `1884e3eca` during
   verification. The delta is doc-only and in a crate this client does not link;
   the rebuilt binary is bit-identical (`4d58afde…`). The receipt's counters
   therefore hold on the receipt tree (`9327f6695`), on `3ecb952c8`, and on
   `1884e3eca`.
2. **Binary/tree provenance, resolved empirically.** The prebuilt binary
   (02:33:47) predates the remedy commit `3ecb952c8` (03:33:58), which touched
   `layerfs-content/src/filesystem/limits.rs` and `…/sorted/budget.rs`. The diff
   is value-preserving (constant alias to `MAX_LEVEL = 31`, `mapping/types.rs:17`)
   plus dead-code deletion and docs; the rebuild at HEAD recompiled both crates
   and produced a **bit-identical** binary — the changes are codegen-neutral and
   the receipt's grid is valid for the current tree, not only the receipt's tree.
3. **Run A concurrency violation (mine).** Run A started while a sibling
   verifier's `cargo test --manifest-path core/Cargo.toml --locked` was active
   (pgrep exit 0, PIDs 64587/65314, 03:45:30). Its elapsed column is excluded
   from the noise assessment; deterministic counters are unaffected (identical
   to the clean run). The gate sample is run B (pgrep exit 1, 03:47:44). The
   sibling's run was not interrupted.
4. **Review-internal elapsed-ratio inconsistency (review's record, not the
   receipt's).** The review's F8 text prints "elapsed by 2.00 / 2.38 / 2.10"
   (`stages-1-5-review-20260917T230700Z.md:319-320`), but its own elapsed column
   (md:314-317, `diag-run.log:4-7`) yields 2.25 / 2.46 / 1.76. §15 and the
   receipt quote only the review's elapsed *values* (which are consistent), so
   nothing in the verified claims inherits this.
5. **Elapsed-noise caveat.** §15's quoted receipt numbers are within ±9% of the
   review's; my clean re-run sits −7% to −21% (the 1,000-pairs row, 69.3 vs
   87.6 ms, is beyond the ±15% same-binary sample spread observed between runs
   A and B at the 250 row: 12.3 vs 14.2 vs 16.3 ms). "Within single-sample
   noise" is fair for the receipt's own record and no faster claim is made; this
   one row is the weakest link in that phrasing.
6. **Column-scope of "identical".** The review's client printed no
   `pend`/`dir_scr`/`ino_scr`/`tiers` columns, so "counters identical to the
   review's grid" is exactly true for every column the review printed (spilled,
   rows_read, rows_written, runs, peak_owned, held-after, backing peak,
   emitted — the last three via `diag-run.log` and review md:1154-1159); the
   receipt-only columns were verified receipt-vs-rerun instead. No defect —
   stated so the claim's scope is precise.
7. **`head.txt` vs README code tree.** `head.txt` records `134b8df73` (the
   docs commit that landed the receipts, 02:42:44) while the README names the
   code tree `9327f6695` (README.md:10); `git diff --stat 9327f6695..134b8df73 --
   core/crates` is empty, so both describe the same product tree. Consistent
   once explained; noted for the record. The receipt itself ran at 02:38, four
   minutes before `head.txt` was written.

## UNVERIFIED

- **The review's original elapsed numbers (15.8/35.6/87.6/154.5 ms) cannot be
  re-measured** — they are the round-2 review's single-sample record
  (`diag-run.log:4-7`). I verified the recorded values match §15's quotes and
  that the review client's fixture shape is source-equivalent; nothing more.
- **Quiet-machine conditions for the review's run and the author's receipt run**
  — no concurrency record accompanies either log; run A shows how much a
  concurrent cargo test can perturb elapsed (12.3 vs 14.2/16.3 ms at the 250 row).
- **The exact historical invocations** — both READMEs document release-profile
  reproduction commands (review `README.md:62-64`, this directory's
  `README.md:39-44`); the actual commands used at the time are not re-playable.
- **The counting-allocator test's zero-allocation assertion** (§15:697-700) —
  not re-run here; its record is `check-cargo-test.log:269` and it belongs to
  `verify-R2-F8-N6.md`'s scope. I confirmed the test file exists and is wired.
- **The prebuilt binary's exact 02:33:47 build tree** — inferred (HEAD
  `9327f6695` at that minute; receipt-time `git-status.txt` shows no product-src
  dirt); the bit-identical rebuild at HEAD makes the distinction immaterial for
  the counters.
- **Machine-level elapsed fairness** (CPU frequency scaling, background load) —
  single samples without controls; the receipt makes no performance-gate claim
  on elapsed, so this does not affect the verdict.

Repo writes: none except this file. `/tmp` writes: binary backup
`/tmp/s5term-prebuilt-backup-9327f6695`, `/tmp/wait-quiet-check.sh`, and the
client's own `/tmp/layerfs-s5term-*` backing directories.
