# Closing verification: §16 VF and N rows — evidence and limits (2026-09-18)

Closing pass for the Stage 5 acceptance matrix's VF (verification) and N (new)
rows, as published in `stage-5-report.md` §16 "Stage 5 matrix - final".
Read-only on the repository except this file; build artifacts under
`core/target` and scratch under `/tmp` only.

- Frozen tree: `git rev-parse HEAD` → `4d887a6b9e74ee3e3fa863d09f8b6494d5d79f0f`
  (exit 0); `git status --short` → empty (exit 0). `core/AGENTS.md`, `core/README.md`,
  `core/docs/` ignored per the tasking (note: at this HEAD they are in fact committed,
  in `680115bcd`, which sits between the remedy commit and the frozen HEAD; no VF/N row
  cites them).
- Scope note: the tasking named N-1..N-12; §16's Stage 5 matrix also carries **N-13**
  (`layerfs-storage` unsafe boundary). N-13 was verified alongside the rest.
- Ancestry: `git merge-base --is-ancestor 3ecb952c8 HEAD` → exit 0 (the round-4 remedy
  commit is in the frozen tree); `git merge-base --is-ancestor 99743b2cf HEAD` → exit 0
  (the round-4 tree the verifiers ran on is an ancestor). The verify reports ran on
  `99743b2cf` (pre-remedy) and re-verified remedies at `3ecb952c8`; both precede HEAD,
  and `git diff --name-only 3ecb952c8..HEAD` shows only docs/evidence plus the
  `codec.rs` FFI doc inventory (committed as `1884e3eca`, i.e. the N-13 remedy).

## Verdicts, group by group

### VF group — **CONFIRMED as recorded in §16** (exceptions: none verdict-changing)

| Row | §16 status | Evidence check |
| --- | --- | --- |
| VF-1 | PASS (round 2) | round-2 review `stages-1-5-review-20260917T230700Z.md:927` (§4.1, line 860) records PASS |
| VF-2 | PASS (round 2) | same review `:928` PASS |
| VF-3 | PASS | `verify-VF3.md` exists; verdict PASS (`:18`), RE-VERIFICATION (`:264-321`) green — both strengthened tests re-run: `filesystem_ordering` 12 passed / `filesystem_sorted` 6 passed, exit 0; final verdict PASS (`:323-335`). Named case also verified by `verify-R2-F6-AT4-F25.md` (R2-F25 PASS) |
| VF-4 | PASS | `verify-N16-VF4.md` final verdicts post-remedy PASS (`:245-250`); RE-VERIFICATION commands all exit 0 (`:202-210`: filesystem_limits 7 passed, storage_limits 5 passed, greps confirm deletions). Walk row separately: `verify-R2-F7.md` FINAL VERDICT PASS (`:211-217`) with post-erratum re-verification green (`:164-209`) |
| VF-5 | PASS | `verify-VF5-VF6-F5.md:15` PASS — receipt sha256 reproduced, source commit an ancestor, ratios recomputed from the receipt's own ns |
| VF-6 | **NOT_RUN - owner disposition** | addendum `stage-5-verification-addendum-20260917.md` §6 `:127-143`: stays NOT_RUN, owner quote "defer it to stage 6", deferral to Stage 6 (#171), "not a waiver and not a PASS". Not counted as PASS anywhere: §16 totals `stage-5-report.md:799-802` ("81 PASS … 1 NOT_RUN"), round-2 review `:930` NOT_RUN. My own sweep of every `complete-operation`/`complete operation` hit in `docs/roadmap/0.1/0.1.7/component-decoupling/*.md` found only disclaimers, NOT_RUN statements and requirement/future-contract language (e.g. `filesystem-tree.md:526`, `canonical-objects.md:180`, `stage-5-completion-report-20260917.md:333`); no "as fast as"/"faster than" claim in any `stage-5-*.md`. No Stage 5 document claims complete-operation performance |
| VF-7 | PASS | `verify-R2-F4-F15.md` §1 verdicts all PASS (`:21-29`); the drift-column and scope-switch findings are recorded there as minor/non-blocking (`:151-171`), matching §16's "recorded" phrasing |
| VF-8 | PASS (round 2) | round-2 review `:934` PASS |

### N group — **CONFIRMED as recorded in §16** (exceptions: three labeling gaps, below)

| Row | §16 status | Evidence check |
| --- | --- | --- |
| N-1 | PASS | `verify-R2-F1-F2-F3-F14.md` verdicts PASS (`:34-39`: R2-F1/F2/F3 — the `--case`/`--mode` selection rows — plus F14) |
| N-2 | PASS | `verify-R2-F4-F15.md` R2-F4 rows PASS (`:23-26`); receipt `per-commit-loc-reread.log` rows reproduce |
| N-3 | PASS | `verify-VF5-VF6-F5.md` §B/§C verify the addendum cites the eligible `T143008Z` receipt (hash `:23`, ancestor `:24`, §5.1 rows `:54`) |
| N-4 | PASS | `verify-R2-F6-AT4-F25.md:9-13` R2-F6/AT-4 PASS; boundary re-confirmed by my own run (below) |
| N-5 | PASS | `verify-R2-F7.md` FINAL VERDICT PASS (`:211`) |
| N-6 | PASS | `verify-R2-F8-N6.md` all five claims PASS (`:13-19`); `verify-R2-F8-scaling.md:17` Verdict PASS (receipt reproduced); counting-allocator suite re-run green by me (below) |
| N-7 | PASS | `verify-R2-F10-F23-F24.md:20` PASS with caveats C1/C2 (matching §16's three-caveat summary with F24's C3) |
| N-8 | PASS | `verify-F11-F27-N14.md:14` PASS; RE-VERIFICATION at `3ecb952c8` final verdicts PASS (`:298-304`) |
| N-9 | PASS | `verify-R2-F12-F13.md` R2-F12/N-9 PASS |
| N-10 | PASS | `verify-R2-F12-F13.md` R2-F13/N-10 PASS |
| N-11 | PASS | `verify-N11-F21.md:8` PASS; its F6 qualification (`:96-105`) is exactly §16's "byte ceiling lives at C1's payload wave … count-only … seam arithmetic" wording |
| N-12 | **NOT_APPLICABLE** | reason "no DELETE exists except failed-save cleanup" verified in code: the only `DELETE FROM` statements are `core/crates/layerfs-storage/src/sqlite/cleanup.rs:65,120`, in `cleanup::abandon` ("Bounded cleanup of a definitely failed, unpublished save", `:1-2`), called only from `cas/owner.rs:938`. Round-2 review `:946` records the same reason |
| N-13 | PASS | `verify-N13.md` Final verdict PASS (`:259-261`; 19/19 FFI inventory re-verified `:218-257`); the codec doc diff that was uncommitted at re-verification time is now committed (`1884e3eca`, ancestor of HEAD) |

## Deciding suites, spot-run by me (all `--locked`, frozen tree)

| Command | Exit | Result |
| --- | --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_limits --locked` | 0 | **7 passed / 0 failed** (incl. `a_read_wave_is_accepted_at_4096_demands_and_refused_at_4097`) |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-storage --test storage_limits --locked` | 0 | **5 passed / 0 failed** (incl. `the_group_count_ceiling_is_named_with_its_derivation`) |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_ordering_scan --locked` | 0 | **2 passed / 0 failed** (incl. the counting-allocator case `lookups_allocate_nothing_after_the_tiers_are_built`) |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_failure the_attribute_value_bound_is_the_chunk_maximum_at_its_boundary --locked` | 0 | **1 passed / 0 failed** |
| `/tmp/walk-close-probe` (public-API probe: `build_filesystem`, `FileBacking`, flat build) | 0 | `MAXIMUM_CYCLE_CHECK_ENTRIES = 4096`; children=4096 → **Ok (entries_examined=4096)**; children=4097 → **Err(InvalidRecord("cycle check work limit"))** — the corrected §6 figures reproduced exactly |

Other commands (exit codes): `git rev-parse HEAD` 0; `git status --short` 0 (empty);
`git merge-base --is-ancestor 3ecb952c8 HEAD` 0; `git merge-base --is-ancestor
99743b2cf HEAD` 0; `git log`/`git diff --name-only 3ecb952c8..HEAD` 0; read-only
`grep`/`sed`/`cat` over the cited reports, receipts and code 0 each. One probe compile
iteration failed (E0308 String vs &str in my own scratch code, fixed; not repository code).

## Three §6 limits rows spot-checked against their boundary cases

1. **Attribute value ≤ 32,768** (`stage-5-report.md:429`):
   `filesystem_failure.rs:549` `the_attribute_value_bound_is_the_chunk_maximum_at_its_boundary`
   asserts `MAXIMUM_ATTRIBUTE_VALUE_BYTES == cdc::MAXIMUM_CHUNK_BYTES` (limits.rs:58 →
   32,768 at `gear.rs:17`), writes exactly 32,768 and reads it back whole, and refuses
   32,769 with `ObjectLimitExceeded{limit, actual}`. Ran green (exit 0).
2. **Whole-tree walk entries ≤ 4,096** (`stage-5-report.md:433`, corrected figures):
   reproduced through the public API — a build stating exactly 4,096 bindings is
   accepted (entries_examined=4096), 4,097 is the first refusal
   (`InvalidRecord("cycle check work limit")`). Matches `verify-R2-F7.md`'s probe and
   the §13.2 erratum. The in-repo bracketing case
   (`filesystem_bounds.rs:743` `the_cycle_check_work_limit_is_reachable_and_reported`,
   probes limit−8 / limit+512) passes inside the 10/10 suite.
3. **Read-wave demands ≤ 4,096** (`stage-5-report.md:436`):
   `filesystem_limits.rs:221` `a_read_wave_is_accepted_at_4096_demands_and_refused_at_4097`
   — 4,096 demands pass the count check and reach the provider (empty store answers
   `MissingObject`, proving the count check did not refuse), 4,097 refused
   `ObjectLimitExceeded{limit: 4_096, actual: 4_097}`. Ran green (exit 0).

## §6 limits-table falsification (row without a class / boundary case / derived note)

All 19 rows of `stage-5-report.md:420-440` carry a Kind (enforced / format /
configurable / derived-unverified / not-claimed / measured). Enforced rows have both-side
boundary cases (name 255/256, path 4,096/4,097 bytes and 256/257 components, symlink
4,096/4,097, attribute domain 64/65 + key 255/256, attribute value 32,768/32,769,
read wave object+byte, walk entries 4,096/4,097, read-wave demands 4,096/4,097 —
all in the suites run above or in `filesystem_bounds`/`storage_limits`); the two
derived rows (Ordering tiers, Pack group count) carry labelled derived notes with
arithmetic and pinned values; the format rows are pinned by
`filesystem_limits::the_limits_this_suite_cannot_bound_are_named_with_their_reason`
(31/8,192 with the fan-out arithmetic) and the codec/leaf malformed-fixture suites.
**No §6 row lacks a class, boundary case or derived note.** The storage work budgets
(`ENCODE/DECODE_WORKSPACE_BYTES`, cache windows) are declared non-Stage-5 rows by scope
(`stage-5-report.md:845-851`, `verify-N16-VF4.md:235-240` records the same reading).

## Round-4 README "What this directory proves" — claims vs cited receipts

Checked row by row (`README.md:15-29`): `ordering-scaling.log` matches the README's
counters and elapsed figures exactly (448/960/1,984/3,968 spilled; rows_read
2,198/6,684/19,960/59,007; runs 14/30/62/124; peak 66,432/139,008/284,160/568,320 B;
elapsed 16.3/32.7/79.6/157.6 ms; and these work counters are identical to the review's
pre-fix grid at `stages-1-5-review-20260917T230700Z.md:314-317`, elapsed there
15.8/35.6/87.6/154.5 ms — exactly as the README states). `check-cargo-test.log` carries
`filesystem_ordering_scan` (2 passed, incl. the zero-allocation case), `provider_errors`
(3 passed, the three absence-vs-failure discriminations the README lists) and
`connection_profile` (4 passed, the three refusals + pass). `simultaneous-memory.log`
matches the TR-5 row figure for figure. `per-commit-loc-reread.log` matches the R2-F4 row
(six drift commits, merge line 732→732, scope-switch pins). `stage-5-report.md` §2
(`:219-225`) carries the R2-F15 totals exactly as the README quotes them.
`check-final-cargo-test.log` totals **65 result blocks, 434 passed, 0 failed** —
matching §16 `:757`. **No README row is contradicted by its cited receipt.** Three
format-level imprecisions found, none substance-changing (below).

## Findings (falsification pass)

1. **Labeling gaps (minor, not verdict-changing):** three cited verifier reports never
   name their §16 row id: `verify-R2-F1-F2-F3-F14.md` (N-1 — verified there as R2-F1/F2/F3,
   which is N-1's substance), `verify-R2-F6-AT4-F25.md` (N-4 — verified as R2-F6/AT-4),
   `verify-VF5-VF6-F5.md` (N-3 — verified as §B/§C's addendum-citation checks). The
   substance of each row is verified and ends PASS; only the row label is absent.
2. **Stale pointer (minor):** `README.md:10` says "Code tree | `9327f6695` (`head.txt`)"，
   but `head.txt` records `134b8df73…` — it was legitimately updated in commit
   `99743b2cf` when the round-4 check logs were retained (`git log --follow` on head.txt:
   created `9327f6695…` in `134b8df73`, updated in `99743b2cf`). `134b8df73` is
   docs-only over `9327f6695` (`per-commit-loc-reread-2.log`: "134b8df73 18797->18797 +0"),
   so the claim's substance (the code tree) is unchanged; the parenthetical pointer is stale.
3. **Overstated log format (minor):** `README.md:108` "each with its exit code" — only
   `check-boundary.log` prints a literal `EXIT=0`; the other seven `check-*.log` and all
   eight `check-final-*.log` record successful output (cargo `Finished`/`test result: ok`,
   Python `OK`, empty fmt/diff) without an explicit exit line. All runs are green by
   their output; the phrasing overstates the format.
4. **Pre-existing, already recorded by the round's own verifier:** the drift annotation
   column quoted at `README.md:22` / §2 mixes before-/after-/delta-drift formulas
   (`verify-R2-F4-F15.md:151-160`, finding 1); the disclosed and recomputed columns
   themselves are exact. Not a README-vs-receipt contradiction (the receipt prints no
   drift column at all; every quoted figure is derivable from its numbers).
5. **Task-premise notes (no action):** the premise said `core/AGENTS.md`, `core/README.md`
   and `core/docs/` are uncommitted; at the frozen HEAD they are committed (`680115bcd`)
   and the working tree is clean — ignored per instructions, and no VF/N row cites them.
   The premise's "N-1..N-12" undercounts the matrix, which also carries N-13 (verified).

## UNVERIFIED (could not or did not check)

1. **The measurements behind the receipts were not re-run by this pass** (per the
   closing-pass mandate of spot-runs only): the ordering-scaling grid's elapsed times,
   the TR-5 coexistence counters, and the VF-5 comparison ratios at `eb42c1347` are
   accepted from the receipts' recorded counters and the two measurement verifiers'
   own reproductions (`verify-R2-F8-scaling.md`, `verify-TR5.md`,
   `verify-VF5-VF6-F5.md` §B/§F). My runs cover the three deciding suites and the
   three §6 boundary cases only.
2. **Issue #171's live state/content on the tracker** — the VF-6 deferral is verified
   as recorded in the repository documents (addendum §6, §16, terminal handoff), not
   against the tracker itself.
3. **The round-2 review's own §4.1 evidence artifacts** (e.g. its `cargo-test.log` with
   104 tests) — confirmed the VF-1/VF-2/VF-8/N-12 rows exist there with the cited
   statuses; the underlying artifacts were not re-derived.
4. **Full-workspace suite, clippy, fmt and the boundary guard were not re-run** by me
   (the `check-final-*.log` receipts record them green on the final tree; I verified
   their content and totals). My cargo runs cover the four named targets only.
5. **`verify-R2-F17-F18.md`, `verify-R2-F19-F20-F22-F26.md`, `verify-TR5.md`** cover
   non-VF/N rows (R2-*, TR-5) and were only confirmed to exist; their verdicts are
   outside this section's rows.

Repo writes by this pass: this file only. Build artifacts under `core/target`;
probe scratch under `/tmp/walk-close-probe/` and `/tmp/walk-close-probe-target/`.
