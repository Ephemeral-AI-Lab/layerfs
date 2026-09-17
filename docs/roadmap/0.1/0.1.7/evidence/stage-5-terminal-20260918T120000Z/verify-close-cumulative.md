# Closing verification: cumulative Stages 0-5 matrix and integration routes (§16)

Verifier: closing-pass subagent, read-only on the repository. Frozen commit
`4d887a6b9e74ee3e3fa863d09f8b6494d5d79f0f` (`git rev-parse HEAD`, exit 0).
`git status --porcelain` (exit 0) reports a clean tree: 0 lines. The only file
written by this pass is this one; build artifacts went under `core/target` and
`/tmp`. Section under test: `stage-5-report.md` §16, "Cumulative Stages 0-5
matrix - final" (:804-823) and "Cumulative integration routes - final"
(:825-833), against the round-2 record
`stages-1-5-review-20260917T230700Z.md` §4.2 (:962-1004) and §4.3 (:1006-1015).

## Verdicts

| Section | Verdict |
| --- | --- |
| Cumulative Stages 0-5 matrix - final (36 rows) | **CONFIRMED** — 34 PASS / 2 owner-WAIVED of 36; every remediated row's verifier report exists and ends in the claimed verdict; the waived rows are unchanged, excluded from the PASS count and never promoted; all three required spot-checks hold on the final tree. No row overstates its evidence. |
| Cumulative integration routes - final | **CONFIRMED** — the five round-2 PASS routes remain green on the final tree; the sixth ("measured route with real payloads") is recorded as following VF-6's owner disposition (deferred to Stage 6 / #171), with the real-payload correctness proof carried by `core_pipeline` and `filesystem_pipeline` tests, matching the round-2 review's own record. No route claim changed without a receipt. |

Row-level exceptions: none falsified. Qualifications and residual gaps are in
Findings and UNVERIFIED below.

## 1. Round-2 baseline reproduced

`stages-1-5-review-20260917T230700Z.md` §4.2 (:962-1004) records exactly the
baseline the task states: 36 rows, "Cumulative totals: **PASS 30, FAIL 3,
PARTIAL 1, owner-WAIVED 2**" (:1003). The FAIL rows are N-13 (:997), N-16
(:1000), N-17 (:1001); the PARTIAL row is N-14 (:998); the waived rows are S3-6
(:986) and S4-5 (:991). §4.3 (:1008-1015) records five PASS routes plus the
sixth INCOMPLETE: "`measure_filesystem` supplies synthetic content/metadata
roots … the tests above, not the harness, carry the real-payload proof" (:1015).

## 2. Remediated cumulative rows — verifier reports and final verdicts

All cited reports exist in
`docs/roadmap/0.1/0.1.7/evidence/stage-5-terminal-20260918T120000Z/` (`ls`,
exit 0).

| Row | Report | Initial verdict | Final verdict (report end) | §16 claim matches |
| --- | --- | --- | --- | --- |
| N-13 | `verify-N13.md` | FAIL — narrowly (FFI inventory 17/19, :13-21) | "Final verdict for N-13: PASS." (:259) after RE-VERIFICATION (:218-265) | yes (:797, :816) |
| N-14 | `verify-F11-F27-N14.md` | PASS with two imprecisions (:15) | RE-VERIFICATION at `3ecb952c8` final verdicts: N-14 **PASS**, R2-F11/N-8 PASS, F27 PASS (:298-304) | yes (:817) |
| N-16 | `verify-N16-VF4.md` | INCOMPLETE for both claims (:10-11) | RE-VERIFICATION final verdicts: N-16 **PASS**, VF-4 PASS (:245-250) | yes (:819) |
| N-17 | `verify-R2-F10-F23-F24.md` | R2-F23 / N-17 **PASS** with phrasing nuance N1 (:21, §254-353); caveats C1-C3 exactly as §16 summarizes | yes (:743, :820) | |
| TEL-4 (F12/F13) | `verify-R2-F12-F13.md` | all three verdicts PASS (:20-24) | yes (:814) | |
| S2-4 (F27 note) | `verify-F11-F27-N14.md` | F27 PASS at re-verification (:303) | yes (:809) | |
| N-15 (F7 caveat) | `verify-R2-F7.md` | PASS with boundary erratum (:7) | "FINAL VERDICT for R2-F7: PASS" after RE-VERIFICATION at `3ecb952c8` (:164-211) | yes (:818) |

Code-evidence carry-over to the frozen commit: `git diff --name-only
1884e3eca..4d887a6b9` filtered to non-docs paths exits 1 (nothing outside
`docs/`, `core/docs/`, `core/AGENTS.md`, `core/README.md`) — after the last
remedy commit (`1884e3eca`, the N-13 inventory doc) only documentation and
evidence changed, so every verifier's code citations carry verbatim to
`4d887a6b9`. The earlier remedies (`3ecb952c8`) are the commits the
re-verifications name. `connection.rs` (N-17) is untouched across
`99743b2cff..4d887a6b9`.

N-13 bridge check (its RE-VERIFICATION ran on an then-uncommitted doc edit):
`git diff 99743b2cff..4d887a6b9 -- core/crates/layerfs-storage/src/encoding/codec.rs`
(exit 0) is a single doc-only hunk adding `ZSTD_compressBound` and
`ZSTD_estimateCCtxSize_usingCParams` to the module-doc inventory, matching the
re-verification's "+11/-10" description; the frozen tree's module doc lists all
19 callable entry points.

## 3. Owner-waived rows (S3-6, S4-5)

- Status unchanged: §16 keeps them **owner-WAIVED** with "unchanged; not
  counted as PASS" (stage-5-report.md:811,813), matching round-2 (:986,991) and
  the round-3 remediation matrix (stage-5-matrix-remediation-20260917.md:115,120
  "owner-WAIVED \| owner-WAIVED \| unchanged from the review").
- Not counted as PASS: the §16 table's PASS rows count 34
  (S01 ×6 + S2 ×9 + S3-1..S3-5 ×5 + S4-1..S4-4 ×4 + TEL ×4 + X-1 + N-13..N-17
  ×5); the stated totals "34 PASS … 2 owner-WAIVED (excluded from the PASS
  count) … of 36" (:822-823) arithmetically reconcile (34 + 2 = 36).
- Not promoted: `grep -rn "S3-6\|S4-5" docs/roadmap/0.1/0.1.7/` (exit 0) finds
  every mention keeping them waived/unmeasured — round-1 review :1084/:1089
  ("a waiver is not a measurement"), round-2 review :539-540/:986/:991,
  remediation matrix :115/:120, terminal handoff
  stage-5-terminal-handoff-20260917.md:60 ("they stay waived … must not be
  promoted into evidence"), §16 :811/:813 and :852-853. No evidence directory
  mentions them at all.

## 4. Spot-checks of round-2 PASS rows on the final tree

| Row | Check | Result |
| --- | --- | --- |
| S2-5 (SQLite profile) | `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-storage --test connection_profile --test policy_capacity --locked` | exit 0; `connection_profile` 4 passed / 0 failed, `policy_capacity` 9 passed / 0 failed (incl. `the_connection_profile_is_declared_and_the_engine_maxima_are_the_hosts`) |
| S01-6 (C1 without SQLite) | `core/crates/layerfs-content/Cargo.toml` dependencies | only `blake3` + `layerfs-telemetry`; `rusqlite` appears only under `layerfs-storage` (`grep -rln`, exit 0) |
| X-1 (no retry/fallback/fsync/WAL) | greps over `core/crates/*/src/` | `fsync\|fdatasync\|sync_all\|sync_data\|sync_file_range` (exit 0): doc comments only (lib.rs:11-12, connection.rs:6-7); `\bWAL\b\|journal_mode` (exit 0): doc comments + `PRAGMA journal_mode = MEMORY` (connection.rs:33,55); non-comment `retry\|fallback\|resend` (exit 1): zero hits |
| TEL-3 (extra) | telemetry retention constants | `MAX_NODES = 1_024`, `MAX_DEPTH = 32`, `MAX_LABEL_BYTES = 128` at layerfs-telemetry/src/timer/recording.rs:14,17,20; `grep -c "\.child("` on cas/owner.rs → 0 |
| S2-7 (extra) | `finish(mut self)` | still `pub fn finish(mut self` at cas/store.rs:314 |

## 5. Integration routes

- Five PASS routes (round-2 §4.3 :1010-1014): every cited suite re-run on the
  frozen tree, all green — storage side exit 0 (`filesystem_pipeline` 6,
  `edit_pipeline` 6, `metadata_pool` 15, `filesystem_failure` 4,
  `persistence_failure` 8, `visibility` 8); content side exit 0
  (`edit_localized` 5, `edit_transitions` 7, `filesystem_hardlinks` 6,
  `filesystem_topology` 17); plus `core_pipeline` 6 (exit 0). First attempt
  named `edit_localized` under `-p layerfs-storage` and cargo errored ("no test
  target named `edit_localized`"); corrected by splitting packages. Test counts
  differ from round-2 where remedy commits added tests; §16 claims "suites
  green" (:827-828), which holds.
- Sixth route: §16 :827-833 records it as the complete-operation measurement
  row following VF-6's owner disposition — deferred to Stage 6 (#171) — and the
  real-payload correctness proof carried by tests, "exactly as the round-2
  review recorded". Receipts verified: the owner disposition is recorded
  verbatim in `stage-5-verification-addendum-20260917.md` §6 :125-143 ("VF-6: i
  think we can defer it to stage 6.", deferred to Stage 6/#171, "not a waiver
  and not a PASS"); `verify-VF5-VF6-F5.md` verdict table (:16) gives VF-6 PASS
  as governance ("deferred to Stage 6 / #171; not waived, not promoted") and
  its falsification §E (:82-90) found no complete-operation performance claim
  in any Stage 5 document. The named tests exist as claimed:
  `a_file_larger_than_five_mebibytes_survives_a_real_store` with
  `noise(6 * 1024 * 1024 + 1)` (core_pipeline.rs:252-285) and
  `a_real_tree_saves_reopens_and_reads_back_exactly` (filesystem_pipeline.rs:332),
  matching round-2's §4.3 :1015 and Q4 :545-553. No document promotes the route
  to PASS/measured (`grep -rn "measured route\|real payload\|real-payload"`,
  exit 0 — only §16:828-830 and the round-2 records).

## 6. Supporting totals

- `check-final-cargo-test.log` (the final-tree run §16 :757 cites): 65
  `test result: ok` blocks, all `0 failed`, summed passed = 434 — matches §16's
  "65 result blocks, 434 tests passed, 0 failed".
- `python3 core/tools/check_product_boundary.py` on the frozen tree: exit 0,
  "PASS: scanned 116 production Rust/SQL files".
- Limits suites re-run on the frozen tree: `filesystem_limits` exit 0 (7
  passed), `storage_limits` exit 0 (5 passed) — identical counts to the N-16
  re-verification (:206-207).

## Findings (path:line; none falsifies §16)

1. **Briefing/tree discrepancy (observation).** The task described
   `core/AGENTS.md`, `core/README.md`, `core/docs/` as uncommitted in-session
   additions; at the frozen commit they are committed (`680115bcd`, an ancestor
   of `4d887a6b9`) and `git status --porcelain` is empty. They are
   documentation-only, outside the matrix scope; no row's evidence touches
   them.
2. **N-13 re-verification predates its commit (bridged).** `verify-N13.md`
   :220-225 re-verified the inventory on uncommitted working-tree edits; the
   same caveats are recorded in `verify-F11-F27-N14.md` :342-353 and
   `verify-N16-VF4.md` :230-234. The edits were committed as `1884e3eca`; I
   confirmed the frozen tree carries the identical doc-only hunk, so the gap is
   closed on `4d887a6b9` (§2 above).
3. **head.txt vs README label (observation).**
   `stage-5-terminal-20260918T120000Z/head.txt` records `134b8df73` (the tree
   the round-4 `check-*.log` receipts ran on); the directory README's table
   labels the code tree `9327f6695` (the last code commit). A labeling nuance
   in the evidence README, not a §16 claim; the `check-final-*.log` files postdate
   it and the product tree is identical from `1884e3eca` onward.
4. **Route suite counts evolved (expected, not overstated).** Round-2 counts
   (metadata_pool 17, visibility 15, persistence_failure 6, edit_localized 13,
   edit_transitions 9) differ from the frozen tree (15, 8, 8, 5, 7) because
   remedy commits added/refactored tests. §16 claims "suites green", which I
   re-ran and confirmed; no §16 statement repeats the old counts.

## UNVERIFIED

1. **Issue #171's actual tracker state** (the Stage 6 deferral target) —
   outside the repository; not checkable read-only. Same limitation
   `verify-VF5-VF6-F5.md` :96 records. The deferral is verified only as
   recorded in the repository documents (addendum §6).
2. **The full 65-block workspace suite was not re-run by me.** The "434 passed,
   0 failed" figure is confirmed from `check-final-cargo-test.log` totals; I
   directly re-ran 17 suites/subsets (all green, exit 0), not the complete
   workspace pass.
3. **Round-2 PASS rows beyond the spot-checks** (S01-1..S01-5, S2-1..S2-3,
   S2-6..S2-9, S3-1..S3-5, S4-1..S4-4, TEL-1, TEL-2) stand on the round-2
   review's citations plus the green final-tree suites; I spot-checked S2-5,
   S01-6, X-1 (required) and TEL-3, S2-7 (extra) but did not re-derive every
   row's evidence from scratch.
4. **`verify-R2-F10-F23-F24.md` caveats C1/C2** (dependency-read collapse on a
   tampered store; locator-byte passthrough) were established by that
   subagent's `/tmp` probes and source reading; I did not re-run those probes.
   They are recorded caveats, not PASS conjuncts of N-17.
5. **Round-2 snapshot line numbers for files the remedy commits touched**
   (e.g., owner.rs drift noted in `verify-F11-F27-N14.md` :274-278) — I
   confirmed via git that the cited code files are byte-identical from the
   verifier trees to the frozen commit, but did not line-diff every round-2
   citation.

## Commands run (all from the repository root; read-only)

| # | Command | Exit | Key result |
| --- | --- | --- | --- |
| 1 | `git rev-parse HEAD` | 0 | `4d887a6b9e74ee3e3fa863d09f8b6494d5d79f0f` |
| 2 | `git status --porcelain` / `--short` | 0 | clean, 0 lines |
| 3 | `git log --oneline -3`; `git log --oneline 99743b2cff..4d887a6b9` | 0 | 5 commits: 3ecb952c8, 1884e3eca, 5b93c3184, 680115bcd, 4d887a6b9 |
| 4 | `git diff --stat/--name-only 99743b2cff..4d887a6b9` | 0 | product src changed only in limits.rs, budget.rs, codec.rs + 5 test files (the remedies) |
| 5 | `git diff --name-only 1884e3eca..4d887a6b9 \| grep -v '^docs/\|core docs'` | 1 | nothing outside docs/evidence after the last remedy |
| 6 | `git diff 99743b2cff..4d887a6b9 -- .../encoding/codec.rs`; `grep -oE "ZSTD_[A-Za-z0-9_]*" codec.rs` | 0 | doc-only inventory hunk; 19 entry points present |
| 7 | `ls` evidence dirs; `wc -l` six verifier reports | 0 | all cited verify-*.md exist |
| 8 | `grep -n "verdict\|RE-VERIFICATION\|^## "` six verifier reports | 0 | final verdicts as tabulated in §2 |
| 9 | `grep -rn "S3-6\|S4-5" docs/roadmap/0.1/0.1.7/` | 0 | every mention keeps them waived; no promotion |
| 10 | `cat core/crates/layerfs-content/Cargo.toml`; `grep -rln rusqlite core/crates/` | 0 | no SQLite in layerfs-content |
| 11 | `grep -rniE "fsync\|fdatasync\|sync_all\|sync_data\|sync_file_range"` core/crates/*/src | 0 | doc comments only |
| 12 | `grep -rniE "\bWAL\b\|journal_mode"` core/crates/*/src | 0 | doc comments + journal_mode = MEMORY |
| 13 | `grep -rniE "retry\|fallback\|resend"` core/crates/*/src piped to drop comment lines | 1 | zero non-comment hits |
| 14 | `cargo +1.85.1 test -p layerfs-storage --test connection_profile --test policy_capacity --locked` | 0 | 4 + 9 passed, 0 failed |
| 15 | `cargo +1.85.1 test -p layerfs-storage --test filesystem_pipeline edit_pipeline metadata_pool filesystem_failure persistence_failure visibility core_pipeline --locked` (first attempt also named edit_localized) | first attempt non-zero (no such target in storage); corrected run 0 | 6/6/15/4/8/8/6 passed, 0 failed |
| 16 | `cargo +1.85.1 test -p layerfs-content --test edit_localized edit_transitions filesystem_hardlinks filesystem_topology --locked` | 0 | 5/7/6/17 passed, 0 failed |
| 17 | `cargo +1.85.1 test -p layerfs-content --test filesystem_limits --locked`; `-p layerfs-storage --test storage_limits --locked` | 0 / 0 | 7 and 5 passed, 0 failed |
| 18 | `python3 core/tools/check_product_boundary.py` | 0 | PASS, 116 files |
| 19 | `grep -c/-oE` over `check-final-cargo-test.log` | 0 | 65 ok blocks, 0 failed, 434 passed total |
| 20 | `cat head.txt git-status.txt`; `git log -1 134b8df73 9327f6695` | 0 | identity records (Finding 3) |
| 21 | `grep -rn "measured route\|real payload\|real-payload"` + targeted `sed`/`grep` of addendum §6, round-2 §4.3/Q4, §14/§15 | 0 | route claims as analyzed in §5 |
| 22 | `grep` MAX_NODES/MAX_DEPTH/MAX_LABEL_BYTES, `.child(` on owner.rs, `finish(mut self` | 0 / 1 (count 0) / 0 | recording.rs:14,17,20; owner.rs 0 `.child(`; store.rs:314 |

Read-only tools (read/grep/glob) were used for all document inspection; no
tracked file was modified, staged, committed, pushed, checked out or reset, and
no issue state was changed.
