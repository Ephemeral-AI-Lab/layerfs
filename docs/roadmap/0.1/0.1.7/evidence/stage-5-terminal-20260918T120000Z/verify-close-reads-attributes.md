# Closing verification pass: RD / AT / C2 / TR rows of the Stage 5 matrix (§16)

| | |
| --- | --- |
| Rows | §16 "Stage 5 matrix - final" (`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md:769-777`): RD-1..RD-6, AT-1..AT-7, C2-1..C2-4, TR-1..TR-5 |
| Tree | `4d887a6b9e74ee3e3fa863d09f8b6494d5d79f0f` (`git rev-parse HEAD`, exit 0; matches the task's frozen commit exactly) |
| Working tree | clean before and after (`git status --porcelain`, exit 0, empty both times) |
| Method | read-only on tracked files; every verdict below is reproduced on this tree from suites, public-API clients and code reading — a roadmap report, suite log, commit message or verifier report alone is never the evidence |

## Group verdicts

| Group | Verdict | Row-level exceptions |
| --- | --- | --- |
| **RD** (RD-1..RD-6) | **CONFIRMED PASS** | none — every row reproduces; RD-6's F7 caveat is closed (ceiling declared with tight figures at `limits.rs:86`, enforced in `validate.rs`, both reproducers in `filesystem_bounds` pass 10/10) |
| **AT** (AT-1..AT-7) | **CONFIRMED PASS** | none — AT-4 (round-2 FAIL, remediated round 3 in `2fe2a4642`) reproduces on the final tree: 32,768 accepted and read back whole, 32,769 refused by the declared bound, through the public emit path (s5check A1-A5) and the product boundary case |
| **C2** (C2-1..C2-4) | **CONFIRMED PASS** | none — C2-2 keeps its documented scope limit (caller-authorized value roots are not object dependencies; contract `admission-and-persistence.md`, pin `filesystem_pipeline.rs:577`), exactly as §3E's 2026-09-17 correction records |
| **TR** (TR-1..TR-5) | **CONFIRMED PASS** | none — TR-1/TR-2 (round-2 FAIL, remediated round 3) and TR-5 (round-2 INCOMPLETE, remediated round 4) all reproduce on the final tree; the extended §5 coexistence table's new rows match the code (below) |

No row in this section is UNVERIFIED. Residual scope limits are listed under UNVERIFIED.

## 1. Identity and drift between the receipt trees and the final tree

`git log --oneline -8` (exit 0): the receipts' trees are ancestors — round-3 `2fe2a4642` /
`5e2a20a0c`, round-4 `9327f6695` / `134b8df73` / `99743b2cf` / `3ecb952c8` / `1884e3eca` — and
HEAD `4d887a6b9` adds only docs commits (`1884e3eca` codec module doc, `680115bcd` core docs,
`5b93c3184` final matrices, `4d887a6b9` check logs). Drift check over every code path this
section depends on:

- `git diff --stat 134b8df73..HEAD -- core/crates/layerfs-content/src/filesystem/{references,attributes} core/crates/layerfs-content/src/filesystem/{update,limits}.rs core/crates/layerfs-content/tests/filesystem_{read,attributes,bounds,failure}.rs core/crates/layerfs-storage/tests/filesystem_pipeline.rs core/crates/{layerfs-content,layerfs-storage}/examples core/crates/layerfs-telemetry/` (exit 0) → **only `limits.rs` changed, 26+/12−** — the R2-F7 erratum remedy (doc text and figure corrections; verified by `verify-R2-F7.md` §10 at `3ecb952c8`).
- `git diff --stat f288d2af7..HEAD -- .../validate.rs .../read.rs .../inode/read.rs .../directory/read.rs .../provider.rs` (exit 0) → validate.rs 5+/3− (doc + alias only, per `verify-R2-F7.md` §1); the read paths unchanged since the round-2 reviewed tree.

So every suite, example and client this section cites runs against code byte-identical to what
the round-3/round-4 verifiers exercised, apart from `limits.rs` documentation whose enforced
figures were themselves re-verified below.

## 2. Deciding suites re-run on the final tree (all exit 0)

| # | Command | Exit | Result |
| --- | --- | --- | --- |
| 1 | `git rev-parse HEAD` | 0 | `4d887a6b9e…` (exact match) |
| 2 | `git status --porcelain` | 0 | empty (clean), re-checked after all work |
| 3 | `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_read --locked` | 0 | **4 passed / 0 failed** |
| 4 | `cargo +1.85.1 test … -p layerfs-content --test filesystem_attributes --locked` | 0 | **11 passed / 0 failed** |
| 5 | `cargo +1.85.1 test … -p layerfs-content --test filesystem_bounds --locked` | 0 | **10 passed / 0 failed** |
| 6 | `cargo +1.85.1 test … -p layerfs-storage --test filesystem_pipeline --locked` | 0 | **6 passed / 0 failed** |
| 7 | spot: `… --test filesystem_failure the_attribute_value_bound --locked -- --nocapture` | 0 | **1 passed / 0 failed** |
| 8 | spot: `… --test filesystem_read listing_is_bounded_by_bytes --locked -- --nocapture` | 0 | **1 passed / 0 failed** |
| 9 | `… --test filesystem_failure --locked` (RD-5's deciding suite) | 0 | **8 passed / 0 failed** |
| 10 | `… -p layerfs-telemetry --test timer --locked` (TR-2/T-R4 basis) | 0 | **21 passed / 0 failed** |
| 11 | `… -p layerfs-content --test filesystem_timing --locked` (TR-3) | 0 | **2 passed / 0 failed** |
| 12 | `… -p layerfs-storage --test filesystem_pipeline --locked -- --list` | 0 | 6 names incl. `a_caller_authorized_value_root_is_not_an_object_dependency` |

Note: §4's round-2 table counted `filesystem_attributes` 9 and `filesystem_pipeline` 3 tests;
the final tree carries 11 and 6 (remediation rounds added cases). §16's own basis is the round-4
suite — `check-final-cargo-test.log` in this directory holds 65 `test result: ok` blocks with no
non-ok block — and the per-row deciding suites above are green on the final tree regardless.

## 3. Falsification reproductions (attempted; every property held)

**(a) AT-4 — 32,768 accepted / 32,769 refused through the public emit path.**
Round-3 client `…/stage-5-terminal-20260918T020000Z/diagnostics/s5check`, `cargo run --quiet`
(exit 0), prints verbatim: `A1 declared attribute bound 32768 bytes`, `A2 bound equals the chunk
maximum: true`, `A3 at 32768 bytes: read back 32768 bytes, all 0x5a: true`, `A4 at 32769 bytes:
Err(ObjectLimitExceeded { limit: 32768, actual: 32769 })`, `A5 refused by the declared bound:
true`. Code: the bound is derived, not restated — `core/crates/layerfs-content/src/filesystem/limits.rs:58`
(`MAXIMUM_ATTRIBUTE_VALUE_BYTES = crate::file::cdc::MAXIMUM_CHUNK_BYTES`) from
`core/crates/layerfs-content/src/file/cdc/gear.rs:17` (`MAXIMUM_CHUNK_BYTES = 32_768`); write-path
enforcement at `core/crates/layerfs-content/src/filesystem/attributes/value.rs:26-31`; boundary
case `the_attribute_value_bound_is_the_chunk_maximum_at_its_boundary`
(`tests/filesystem_failure.rs`) passes (command 7); §6's limits row states 32,768 at
`stage-5-report.md:429`.

**(b) RD-4 — bounded count and byte pagination.** `filesystem_read::listing_is_bounded_by_bytes_as_well_as_count`
(`core/crates/layerfs-content/tests/filesystem_read.rs:135-182`), run with `--nocapture`
(command 8, 1 passed): a 15-byte row fits exactly one entry at a 15-byte bound; every bound
1..14 is refused `ObjectLimitExceeded { limit: bound }` (a refusal, not a silent empty page);
count bound and byte bound both apply; zero count is refused `InvalidRecord("listing limit")`;
a resumed listing drains 20 entries across pages of 4 with no repeat and no skip. The property
reproduces; only the round-2 review's P3 figures (row width 16, 8 entries) are stale — the case
was rewritten for the final tree and the substance is intact.

**(c) TR-1 — the case selects the operation, real bodies inside the timed region.** Examples
built `--workspace --locked --examples` (exit 0); fresh `--output` dirs under /tmp; all exit 0:
- `filesystem_timing_c1 --case empty` → `objects read 0 … emitted 1 bytes 129`,
  `prepared_objects 3` — byte-identical to receipt `case-selection/c1-attribute-case/empty.log`.
- `filesystem_timing_c1 --case attributes` → `emitted 5 bytes 507`, the `attribute reads:` and
  `attribute_patch:` lines, `readback 21 bytes "timed attribute value"`, `prepared_objects 11` —
  byte-identical to `c1-attribute-case/attributes.log`.
- `measure_filesystem --mode c2 --case empty` → `case_root 91d21521…`;
  `--case attributes` → `case_root 622262df…` — distinct roots, matching the round-3 receipts
  (`case-selection/c2-*.log`, six distinct roots confirmed by `sort | uniq -c`, each count 1).

**(d) TR-2 — bounded reports, honest clipping.** The telemetry contract is pinned by the timer
suite (command 10, 21 passed, incl. `node_budget_clips_detail_and_keeps_running_the_operation`,
`depth_budget_clips…`, `disabled_and_clipped_reports_are_distinguishable`); s5check D1-D8 print
the three distinct states with a panicked child reporting `Unknown`. `measure_components`
error plumbing re-checked: an 8 MiB (maximum legal) input runs exit 0, and a re-run against a
pre-existing `--timings` file exits **1** with `Error: "--timings … already exists"` — the same
`main → Result` plumbing `save_timings`'s `is_incomplete` gate uses. The clipped-run receipt's
impossibility with legal inputs is stated in this directory's `README.md:58-72`, as §16 requires.

**(e) TR-5 — simultaneous memory and backing.** The round-4 client rebuilt from the final tree
(`s5term`, release, exit 0) and run as `s5term coexist` (exit 0): every deterministic counter is
identical to `simultaneous-memory.log` — pairs 2000, spilled 3968, rows_read 59007, rows_writ
25760, runs 124, peak_owned/peak_back 568320, held_now 0, emitted 14, pend 64, dir_scr 376110,
ino_scr 349820, tiers 5; only `elapsed_ns` varies (wall time, not part of the row). The receipt's
pin `head.txt` = `134b8df73…` is an ancestor of HEAD and the counter code is unchanged since
(§1 drift check).

## 4. The extended §5 coexistence table vs the code (TR-5)

`stage-5-report.md:395-408` extends the original four-row table with the verifier's findings;
every new row matches the final tree:

- **merge-reader buffers** (`:400`, finding F1): `merge_runs` holds two fresh `RunReader`s —
  `core/crates/layerfs-content/src/filesystem/references/merge.rs:200,213-214` — each with a
  `RunScan` buffer rounded down to whole rows (`merge.rs:70-75`: 16,384 → 170 × 96 = 16,320 B).
  `consolidate` moves every run out of its tiers and merges (`runs.rs:381-434`) **before**
  `reset_scans()` (`runs.rs:438`), so the two merge readers coexist with the retained tier scans
  during the phase's final consolidate — ≈ 7 × 16,320 = 114,240 B in this 5-tier fixture,
  ≤ (32 + 2) buffers in general (`MAXIMUM_LEVELS = 32`, `runs.rs:41`). ✓
- **`FinalRows` stream** (`:401`, finding F2): `run_buffer` is allocated
  `vec![0; base_batch.max(1) * ROW_BYTES * 4]` at `reduce.rs:366` with
  `DEFAULT_BASE_BATCH = 32` (`reduce.rs:27`) and `ROW_BYTES = 96` → 32 × 96 × 4 = 12,288 B; the
  pending map is re-collected as a `Vec<Row>` at `reduce.rs:349`; `lookahead`/`wave` are bounded
  by `base_batch` (`reduce.rs:397`). The stream is created inside the references phase and
  consumed lazily inside the inodes phase (`update.rs:337-356`), so it coexists with the inodes
  scratch. ✓
- **pre-phase owners** (`:404-408`, finding F3): the touched-serials `Vec<u64>` is built at
  `update.rs:464`, count-refused via `maximum_touched_serials` (`update.rs:465-473`) and reported
  by `note_serials_scanned` — byte-counted nowhere, exactly as disclosed; the release traversal's
  prefetched/cursors (`references/release.rs:70-77`) precede the references phase
  (`update.rs:313-336`). ✓
- The four original rows re-verified by `verify-TR5.md` and re-reproduced above; scratch phases
  are sequential (`update.rs:160,179,337,354,369,374`) and no process-level figure is claimed
  (`simultaneous-memory.log` last line; report `:414-416`).

## 5. Other findings

- **Citation drift only, no evidence failure.** `verify-R2-F6-AT4-F25.md` cites `limits.rs:53`,
  §6 row `:406` and `MAXIMUM_WALK_ENTRIES` at `limits.rs:72`; on the final tree these live at
  `limits.rs:58`, `stage-5-report.md:429` and `limits.rs:86` — the `3ecb952c8` remedy shifted
  lines while keeping the derivations and values (`32_768`, `4_096`) intact, re-reproduced here.
- **RD-6 / F7:** the walk ceiling is declared once (`limits.rs:86`, alias `validate.rs`) with the
  corrected tight figures ("a build stating 4,096 bindings is accepted and 4,097 is the first
  refused… the base-tree walk charges the rest of the tree beside it first", report `:433`);
  both reproducers pass in `filesystem_bounds` (command 5), and `s5check` B1-B3 print
  `declared walk ceiling 4096`, `build of 4088 files: Ok(4088)`, `build of 4104 files:
  Err(InvalidRecord("cycle check work limit"))`.
- **TR-4's round-2 citation** `provider.rs:60-68` resolves to
  `core/crates/layerfs-storage/src/cas/provider.rs:67-98` on the final tree — provider calls
  carry the caller's `TimingScope` (`:73,98`); substance unchanged.
- **TR-1 nuance** (recorded by `verify-R2-F1-F2-F3-F14.md` and echoed in §16's verifier table):
  the round-3 commit message's "2 read / 6 emitted" is an aggregate across three printed lines;
  the receipts and my re-runs print the accurate per-line figures. The row's substance (real
  change inside the timed region; counters differ from `--case empty`) holds.

## 6. Write footprint (read-only constraint)

The only repository file written is this one. Build artifacts went to gitignored targets
(`core/target`, the diagnostics clients' own `s5check/target`, `s5term/target` — the same
locations the round-4 verifiers used) and `/tmp` (all outputs, the recovered `s5term` binary).
One accidental artifact: a misresolved relative `CARGO_TARGET_DIR` during the `s5term` build
created a nested untracked build directory under `diagnostics/s5term/docs/…`; it was removed
immediately after the binary was copied to `/tmp`, and `git status --porcelain` is empty before
and after all work (exit 0, no tracked file touched).

## 7. UNVERIFIED

1. **The full workspace suite** (§16's "65 result blocks, 434 tests passed, 0 failed") was not
   re-run in aggregate by me; I read `check-final-cargo-test.log` (65 `test result: ok` blocks,
   no non-ok block) and re-ran this section's deciding suites individually. Aggregate greenness
   is the round-4 receipt's claim, re-confirmed only for the suites named above.
2. **The TR-5 receipt-era binary's source seal** — its md5 was never recorded before a sibling
   rebuild (verify-TR5 §8.1); my mitigation is a fresh build from the final tree reproducing
   every deterministic counter exactly, with the counter code byte-identical since the receipt's
   pin (`134b8df73`).
3. **Which instant attains `peak_run_bytes`, and the runtime count of live tier scans at the
   final consolidate** — not observable through the public counters (verify-TR5 §8.2-8.3); the
   structural bounds (≤ `peak_live_runs` scans + 2 merge readers) are code-established, not
   counter-exposed.
4. **`measure_components`' `is_incomplete` branch itself** never executes with any legal input
   (unreachable, per this directory's README §"A clipped-run receipt … cannot exist"); its
   non-zero exit is established by the identical error plumbing, demonstrated empirically
   (§3d), not by triggering the branch.
5. **Round-2 review line citations for RD-1..RD-3/RD-5/AT-1..AT-3/AT-5..AT-7/C2-3** (product
   `path:line`s on the round-2 tree) were not each re-walked; the deciding suites those rows name
   are green on the final tree, and the drift check in §1 shows the read/attribute/provider paths
   unchanged since the round-2 reviewed tree.
