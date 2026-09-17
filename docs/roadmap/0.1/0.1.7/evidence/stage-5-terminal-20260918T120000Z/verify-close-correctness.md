# verify-close-correctness: Stage 5 §16 correctness groups (CI, PS, SI, SC, WT, RA, OR)

Closing verification pass, 2026-09-18. Scope: the correctness rows of
`docs/roadmap/0.1/0.1.7/component-decoupling/stage-5-report.md` §16 "Stage 5
matrix - final" — the CI, PS, SI, SC, WT, RA and OR groups — as published
"PASS (round 2)" (CI/PS/SI/SC/WT/RA) and "PASS" (OR). The round-2 review
(`stages-1-5-review-20260917T230700Z.md` §4.1) verified them against tree
`f288d2af7`; this pass re-verifies them on the final tree.

| | |
| --- | --- |
| HEAD | `4d887a6b9e74ee3e3fa863d09f8b6494d5d79f0f` (matches the tasking; `git rev-parse HEAD`, exit 0) |
| Method | read-only on the repository: git inspection, source reading, `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` (artifacts under `core/target` only), one throwaway script under `/tmp`; this file is the only repository write |
| Out of scope | `core/AGENTS.md`, `core/README.md`, `core/docs/` (owner's uncommitted in-session additions, per tasking) |

## Verdicts

| Group | Rows | Verdict |
| --- | --- | --- |
| CI (codec/identity) | CI-1..CI-6 | **PASS** |
| PS (profile/schema) | PS-1..PS-4 | **PASS** |
| SI (scoped identity) | SI-1..SI-4 | **PASS** |
| SC (sorted construction) | SC-1..SC-6 | **PASS** |
| WT (whole-tree topology) | WT-1..WT-6 | **PASS** |
| RA (reference accounting) | RA-1..RA-7 | **PASS** |
| OR (ordering) | OR-1..OR-8 | **PASS** — OR-8's caveat resolution confirmed |

All 41 rows in this section hold on the final tree. **UNVERIFIED list: empty.**
Row-level qualifications (none changes a verdict) are in "Findings" below.

## Commands and exit codes

Toolchain: `cargo +1.85.1` (cargo 1.85.1). cwd = repository root unless noted.

| # | Command | Exit | Result |
| --- | --- | --- | --- |
| 1 | `git rev-parse HEAD` | 0 | `4d887a6b9e…` |
| 2 | `git cat-file -t f288d2af7` | 0 | commit (review base exists) |
| 3 | `git log --oneline f288d2af7..HEAD` | 0 | 18 commits; product commits touching this section's paths: `2fe2a4642` (round 3), `6c00e0f53`, `9327f6695`, `3ecb952c8` (round 4) |
| 4 | `git log --oneline f288d2af7..HEAD -- core/crates/layerfs-content/src/object/inode_leaf.rs …/sorted/format.rs …/sorted/finish.rs …/tests/filesystem_codec.rs …/tests/fixtures/` | 0 | **empty** — CI's cited artifacts are byte-unchanged since the review tree |
| 5 | same for `identity.rs input.rs references/reduce.rs references/release.rs references/record.rs tests/{filesystem_topology,filesystem_reference,filesystem_hardlinks}.rs` | 0 | **empty** — SI/RA/OR-1 cited artifacts unchanged |
| 6 | `git show <c> -- <paths>` for `2fe2a4642 6c00e0f53 9327f6695 3ecb952c8` | 0 each | diffs read in full; see "Deltas since the review tree" |
| 7 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_codec --test filesystem_profile --test filesystem_sorted --test filesystem_reference --test filesystem_topology --test filesystem_hardlinks --test filesystem_updates --test filesystem_ordering` | 0 | codec **9**/0, profile **2**/0, sorted **6**/0, reference **2**/0, topology **17**/0, hardlinks **6**/0, updates **6**/0, ordering **12**/0 — all passed, 0 failed (first run piped through `tail -60`, which hid three suites' lines; re-run as #8 to capture them) |
| 8 | `… --test filesystem_codec --test filesystem_hardlinks --test filesystem_ordering` (re-run) | 0 | 9/6/12 passed, 0 failed |
| 9 | `… -p layerfs-content --test fixture_seal` | 0 | 2 passed (read-only seal guard; first combined attempt with `--test filesystem_pipeline` in `-p layerfs-content` exited 1 — *no test target named `filesystem_pipeline` in that crate*; it lives in `layerfs-storage`. Tooling error on my side, corrected) |
| 10 | `… -p layerfs-storage --test metadata_pool --test metadata_window --test filesystem_pipeline --test cas_reuse` | 0 | 15/2/6/9 passed, 0 failed (PS-3, RA-7) |
| 11 | `… -p layerfs-content --test inode_leaf --test object_identity` | 0 | 6 and 11 passed (review cited 4 and 9; both suites grew) |
| 12 | `… -p layerfs-content --test filesystem_ordering_scan --test filesystem_attributes` | 0 | 2 and 11 passed |
| 13 | `… --test filesystem_codec inode_pages_match_the_reference_bytes` (targeted, CI-3 falsification) | 0 | 1 passed, 8 filtered |
| 14 | `… --test filesystem_topology cycle` (targeted, WT-2 falsification) | 0 | 4 passed, 13 filtered |
| 15 | `python3 /tmp/verify_ci3_fixture.py` (independent fixture-byte parse) | 0 | assertions all held (a first version exited 1 on **my own** wrong envelope offsets — a bug in my script, not in the product; corrected by anchoring on the `LFS6INT\0` magic) |
| 16a | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content` (whole crate) | 0 | 33 result blocks, 229 passed, 0 failed |
| 16b | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` (whole core workspace) | 0 | **65 result blocks, 434 passed, 0 failed — matches §16's preamble exactly** (the 65 blocks are the three crates together; the `-p layerfs-content` run above is the 33-block subset) |

Suite totals re-observed on the final tree vs the round-2 citations: codec 9
(=9), profile 2 (=2), sorted 6 (=6), reference 2 (=2), topology 17 (=17),
hardlinks 6 (=6), updates 6 (=6), ordering 12 (=12), inode_leaf 6 (was 4),
object_identity 11 (was 9). Nothing shrank; nothing failed.

## Deltas since the review tree (git, read)

Only four product commits touched this section's paths since `f288d2af7`;
every delta is value-preserving for these rows:

- `2fe2a4642` (round 3) — `validate.rs`: `MAXIMUM_CYCLE_CHECK_ENTRIES`
  re-declared as `limits::MAXIMUM_WALK_ENTRIES`, same 4,096 value, doc only
  beside it (WT rows unaffected). `references/runs.rs`: the dead, never-read
  `LookupScan::settled` field deleted (review F9); lookup/resume/`rows_read`
  semantics untouched. `sorted/page.rs`: `MAXIMUM_SCRATCH_BYTES` re-pointed to
  `limits::MAXIMUM_OPERATION_SCRATCH_BYTES` (same 4 MiB). `limits.rs`,
  `attributes/value.rs`, `object/mod.rs`, `object/output.rs`: outside these
  groups' claims or doc/constant consolidation.
- `6c00e0f53` (round 4) — `sqlite/schema.rs`: the `application_id`/`user_version`
  pragma reads now go through the closed `Pragma` enum (R2-F23); the profile and
  schema checks are unchanged (PS-1 re-verified below). `object/access.rs`,
  `sqlite/connection.rs`: provider/pragma work, not these rows.
- `9327f6695` (round 4) — `references/merge.rs` + `runs.rs`: the one-reader-per-tier
  hoist (R2-F8/F9). `RunReader`'s buffered state became `RunScan` (buffer kept
  across lookups); `seek_from` deleted with its only caller. `merge_runs`,
  `MergeWork` accounting, `visit_newest_first` (now `runs.rs:460`) and the
  newest-first/tombstone semantics are untouched; `rows_read` charging is
  explicitly preserved in the new module doc (`runs.rs`, "One reader per tier").
  Pinned by `filesystem_ordering_scan` (2 passed: zero-allocation lookup wave).
- `3ecb952c8` (round 4 remedy) — `limits.rs`/`sorted/budget.rs`: constant
  consolidation (`MAXIMUM_TREE_LEVEL = file::mapping::MAX_LEVEL` = 31; dead
  `4 MiB - 1` twin deleted). `tests/filesystem_ordering.rs` and
  `tests/filesystem_sorted.rs`: **test additions only** (the boundary/remedy
  cases); both suites green.

## Findings (path:line, on the final tree)

**CI — PASS.**
- CI-1/CI-2: the one grammar is `encode_leaf_value`/`decode_leaf_value`
  (`core/crates/layerfs-content/src/object/inode_leaf.rs:244,282`); the only
  callers are the two routes (`inode_leaf.rs:193,220` and
  `core/crates/layerfs-content/src/filesystem/sorted/format.rs:376,481`) — grep,
  unchanged since review.
- CI-3: `LEAF_ROW_BYTES = 81` (`inode_leaf.rs:32`); header `count * 81` written
  (`inode_leaf.rs:222,256`) and validated; sealed fixtures exist
  (`core/crates/layerfs-content/tests/fixtures/filesystem/codec-inode-leaf.bin`,
  `codec-inode-branch.bin`, + 8 more). **Falsified positively, twice**: (a) the
  targeted golden-bytes test passed (`inode_pages_match_the_reference_bytes`:
  decode fixture → assert structure → re-encode → byte-equal, and the ObjectId
  hash matches the manifest); (b) my own byte-level parse of both fixtures,
  independent of the product code: leaf `subtree_bytes = 162 = 2 × 81`,
  branch `subtree_bytes = 10,368 = 128 × 81`, leaf body exactly `31 + 2 × 81`
  bytes, serials `[7, 9]` strictly ascending. The 73-vs-81 resolution reproduces
  from the raw sealed bytes.
- CI-4: codec 9, `inode_leaf` 6, `object_identity` 11 — all pass.
- CI-5: unordered serials rejected on both routes ("inode key order",
  `inode_leaf.rs:250` encode and `:343` decode).
- CI-6: the generator is `#[ignore]`d and refuses to run without
  `LAYERFS_SEAL_FIXTURES=1`; `fixture_seal.rs` is read-only (2 tests pass, hash
  the sealed set, and assert the gate stays in the generator's source,
  `tests/fixture_seal.rs:82-95`).

**PS — PASS.**
- PS-1: open-time checks unchanged: `application_id`/`user_version` pinned
  (`core/crates/layerfs-storage/src/sqlite/schema.rs:139-141`) and the widened
  role constraint required textually — `&[("objects", "CHECK (object_role
  BETWEEN 1 AND 13)")]` at `schema.rs:157`. The round-4 pragma-enum change did
  not touch these checks.
- PS-2: no `ALTER`/migration statement in `core/crates/layerfs-storage/src/`
  or its SQL (grep; the only hits are doc comments stating "never migrated",
  e.g. `policy.rs:26`).
- PS-3: `metadata_pool` 15, `metadata_window` 2, `cas_reuse` 9 all pass
  (`-p layerfs-storage`).
- PS-4: `filesystem_profile` 2 pass, including
  `a_foreign_profile_is_refused_before_a_root_exists`.

**SI — PASS.**
- SI-1: `InodeScope`/`InodeIdentity` with the serial bound at
  `core/crates/layerfs-content/src/filesystem/identity.rs:19-52`
  (`MAXIMUM_INODE_SERIAL = i64::MAX`); the allocator-precondition contract at
  `filesystem/input.rs:5-15`. Unchanged since review.
- SI-2: range enforced (`identity.rs:47-50`); exercised by `object_identity`
  (11 pass).
- SI-3: no `ObjectId::for_bytes` from a serial anywhere in `filesystem/` —
  the three `for_bytes` sites hash profile-description bytes
  (`filesystem/root.rs:25`), scope bytes (`root.rs:32`) and canonical encoded
  page bytes (`sorted/page.rs:352`), never a serial.
- SI-4: counts derived, caller counts never trusted —
  `a_caller_asserted_count_is_never_trusted` passes (topology); `reduce.rs`
  unchanged since review.

**SC — PASS.**
- SC-1/SC-6: `filesystem_sorted` 6 pass, including
  `initial_construction_needs_no_provisional_seed`; `sorted/finish.rs`
  unchanged since review.
- SC-2: `filesystem_reference` 2 + `filesystem_updates` 6 pass.
- SC-3: the cited case exists and passes —
  `exact_sizing_partitions_match_the_encoder_and_the_tail_rebalances`
  (`tests/filesystem_attributes.rs:277`; suite 11 pass).
- SC-4: height collapse covered by `filesystem_updates` +
  `filesystem_topology` (all pass).
- SC-5: the counters exist and are asserted
  (`sorted/page.rs:47,51,322`, `sorted/merge.rs:183`;
  `a_content_only_change_leaves_every_directory_page_untouched` passes).

**WT — PASS.**
- WT-1: `filesystem_topology` 17 pass.
- WT-2 **falsified positively**: the targeted `cycle` filter ran 4 tests, all
  refused their cycles; the bodies apply genuine multi-move cycles through the
  public entry points (`tests/support/filesystem.rs:299,337` wrap
  `layerfs_content::filesystem::build_filesystem` / `update_filesystem`) —
  e.g. `a_cycle_formed_by_moving_a_directory_below_itself_is_refused`
  (`tests/filesystem_topology.rs:179-209`) applies a three-part move cycle
  (`d→e→d`), and `an_update_cycling_two_declared_new_directories_is_refused`
  (`:605-`) builds a cycle entirely inside one batch.
- WT-3: sorted/unique change lists checked at `filesystem/input.rs:32-43`;
  duplicate/reused identities refused (topology cases pass).
- WT-4/WT-5/WT-6: single-parent, root-operation and disconnected-input cases
  all present and passing (`validate.rs` membership/topology walk; the only
  delta since review is the constant re-point, same value).

**RA — PASS.**
- RA-1/RA-2: `filesystem_reference` 2 pass
  (`construction_matches_the_sealed_reference_roots_and_pages`,
  `updates_match_the_sealed_reference_operation`); `references/reduce.rs`
  unchanged since review.
- RA-3: `filesystem_hardlinks` 6 pass.
- RA-4: newest-pending-overrides-base carried by
  `visit_newest_first` (`references/runs.rs:460`) and merge precedence
  (`merge.rs`); `tier_carries_keep_the_newest_row_and_accumulate_counts`
  passes (ordering suite).
- RA-5: hardlinks + topology cases pass.
- RA-6: `ReleaseWork` with `peak_depth` (`references/release.rs:22,34,141`);
  `growth_is_reserved_before_it_happens_and_cleanup_returns_the_bytes` and
  `a_successful_operation_releases_its_ordering_resources_once` pass.
- RA-7: `filesystem_pipeline` 6 pass in `-p layerfs-storage`, including the
  reopen case.

**OR — PASS, with OR-8's caveat resolution confirmed.**
- OR-1: `records_are_fixed_width_and_every_malformed_field_is_rejected` passes;
  `references/record.rs` unchanged since review.
- OR-2/OR-3: `the_pending_threshold_changes_only_where_the_rows_live`,
  `tier_carries_keep_the_newest_row_and_accumulate_counts` pass.
- OR-4: `LengthOverflow` on every count/length growth
  (`references/backing.rs:102`, `merge.rs:210`, `reduce.rs:115`);
  `the_operation_ceiling_is_enforced_before_the_rows_are_written` passes.
- OR-5: capacity checked before work (`references/runs.rs:94-111`,
  `check_capacity`).
- OR-6: round 2 could only verify this "PASS (source)" — the failure path had
  not been re-run. On the final tree the failure paths are test-covered and
  pass: `a_removal_that_fails_is_visible_instead_of_being_hidden_in_drop` and
  `append_read_flush_and_release_failures_fail_the_operation_without_a_root`
  (`tests/filesystem_ordering.rs:363,463`) beside the success/drop cases. The
  final matrix's plain PASS is therefore *stronger* than the round-2 evidence.
- OR-7: no retry/fallback path in content src (grep; the hits are doc comments
  that state the absence, e.g. `object/output.rs:190`).
- OR-8 (the caveat row): **resolution confirmed.** The counters describe the
  work — `rows_read` is charged at every read (`references/runs.rs` lookup doc),
  and §15 states the amplification honestly: "the read amplification is still
  superlinear… O(n log n) in the change count at a fixed pending ceiling. No
  O(changes) claim is made" (`stage-5-report.md` §15). I cross-checked
  `evidence/stage-5-terminal-20260918T120000Z/ordering-scaling.log` against that
  paragraph: spilled 448/960/1,984/3,968, `rows_read` 2,198/6,684/19,960/59,007,
  `rows_written` 1,588/4,328/10,896/25,760, runs 14/30/62/124, peak owned bytes
  66,432/139,008/284,160/568,320, elapsed 16.3/32.7/79.6/157.6 ms with the
  per-doubling ×3.04/×2.99/×2.96 ratios printed in the log itself — every figure
  matches §15's narrative. `verify-R2-F8-scaling.md` (same directory) records
  the receipt reproduced bit-identically, and `verify-R2-F8-N6.md` exists and
  decides the code rows. §16's OR row cites exactly these.

## Qualifications (no verdict change)

1. **Citation line drift.** Several §4.1 line references have drifted on the
   final tree (e.g. RA-4's `merge.rs:165-192` region now holds the reader
   wrappers after the `RunScan` hoist; `visit_newest_first` sits at
   `runs.rs:460`). Every cited *property* was re-located by content and holds;
   only the line numbers aged.
2. **Suite growth.** `inode_leaf` (4→6) and `object_identity` (9→11) grew since
   the review; the six deciding suites hold their cited counts exactly.
3. **Command log honesty.** Four of my own invocations needed attention: the
   first `tail -60` hid three suites' result lines (re-run), a
   `--test filesystem_pipeline` was aimed at the wrong crate (it is a
   `layerfs-storage` target), my first fixture parser used wrong envelope
   offsets, and the first whole-suite tally (#16a, `-p layerfs-content`, 33
   blocks/229 tests) was the wrong denominator for §16's preamble — the
   preamble's 65 blocks/434 tests is the whole core workspace, re-run as #16b
   and reproduced exactly. All four were my tooling, all corrected, all re-run
   green; none touches the product or the matrix rows.
4. **OR-6 was weaker in round 2** ("PASS (source)", failure path not re-run);
   the final tree's suite now covers failure and drop paths, so the §16 plain
   PASS is supported — this is an upgrade, not a regression.

## UNVERIFIED

None. Every row in CI-1..CI-6, PS-1..PS-4, SI-1..SI-4, SC-1..SC-6, WT-1..WT-6,
RA-1..RA-7 and OR-1..OR-8 was confirmed on the final tree by artifact
inspection, suite runs, targeted falsification or independent reproduction.
