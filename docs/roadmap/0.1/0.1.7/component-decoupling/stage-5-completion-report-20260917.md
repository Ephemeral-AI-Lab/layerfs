# Stage 5 completion report (2026-09-17)

> **Status:** Implementation report for
> [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170) under
> [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165). It supersedes
> the status line of [stage-5-report.md](stage-5-report.md) while that document's
> earlier measurements and open items stay as they were written; the corrections
> to its stale claims are appended there.

## 1. Source identity and scope

| Field | Value |
| --- | --- |
| Continuation start | `979fbd5bcd46352f36dcc5f4a8ffd23786111328` (the commit the retained blocker audit investigated; the audit's own text records one extra leading `0`) |
| Final source | the commit that carries this file; the comparison campaign in §5 ran on `3b4941f1ededc6407504438cfe0bd5059fc159f9` with a clean tree |
| Reference oracle | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`, root workspace only |
| Preserved inputs | the investigator's uncommitted `stage-5-blocker-investigation-20260917.md`, `stage-5-continuation-handoff.md`, the routing edits and `evidence/stage-5-blocker-audit-20260917T055334Z/` are committed as received, not modified |
| Write profile | compact scoped-inline only; other profiles fail with `UnsupportedProfile` |

## 2. The confirmed defects, and two more this work found

Raw diagnostic: [attempt-2/probe.stdout](../evidence/stage-5-blocker-audit-20260917T055334Z/attempt-2/probe.stdout).
Replay of that client on the fixed source:
[stage-5-ordering-fixes-20260917T063837Z](../evidence/stage-5-ordering-fixes-20260917T063837Z/)
(its defect assertions no longer hold: exit 101 at the accounting assertion, with
`held_bytes=88 peak_bytes=88` where it demanded `(0, 0)`), and the corrected client
[stage-5-ordering-corrected-20260917T064142Z](../evidence/stage-5-ordering-corrected-20260917T064142Z/)
(exit 0: `success=false runs_created=14 release_calls=1 files_bytes_after_return=0`).

| # | Defect | Fix | Permanent regression |
| --- | --- | --- | --- |
| 1 | `FileBacking` never counted the bytes it owned: an 88-byte run reported `held=0 peak=0` | one shared account owns held/peak/capacity; an append reserves before it writes; a dropped run returns its bytes and removes its file; a removal failure is recorded | `filesystem_ordering::growth_is_reserved_before_it_happens_and_cleanup_returns_the_bytes`, `::a_removal_that_fails_is_visible_instead_of_being_hidden_in_drop` |
| 2 | a build created 14 runs, returned success, never called a deliberately failing release and left 2,200 bytes behind | the operation closes its row stream, then runs a checked cleanup, then emits the root: a cleanup failure fails the operation with no root object; the failure path makes one attempt and reports its own error | `::append_read_flush_and_release_failures_fail_the_operation_without_a_root`, `::a_successful_operation_releases_its_ordering_resources_once` |
| 3 | the returned result reported `runs_created=0 merges=0 peak_run_bytes=0` after real spills | `finish` snapshots the combined work (store + reducer), and rows read by merges and copies are counted | `::a_successful_operation_releases_its_ordering_resources_once`, `filesystem_bounds::every_reported_owner_is_nonzero_where_work_happened_and_zero_where_it_did_not` |
| 4 | **found here:** the 88-byte ordering row overlapped its own fields — the value's count bytes were written where the value flag and tally lived, so a decoded row did not describe its bytes | rows are a fixed 96 bytes with disjoint fields and one authoritative tally | `filesystem_ordering::records_are_fixed_width_and_every_malformed_field_is_rejected` |
| 5 | **found here:** consolidation merged tiers newest-last, so an older row could win a shared key and a newer accumulated count was lost, making a spilled update reach a different root | tiers are consolidated newest first, each older tier merged under the rows newer tiers already contributed | `filesystem_ordering::the_pending_threshold_changes_only_where_the_rows_live` |

Defects 4 and 5 were latent in the previously reported "parity passes" state: the
sealed fixtures never exercised a one-record pending map, and no test compared a
spilled run against a bounded one.

## 3. Checkpoints and gates

| Checkpoint | Status | Evidence |
| --- | --- | --- |
| A. Regressions, then fixes for the confirmed defects | complete | §2, the two replay directories, four regression targets |
| B. Four missing matrices | complete | `filesystem_ordering` (8), `filesystem_failure` (5), `filesystem_bounds` (4), C2 `filesystem_failure` (4) |
| C. Correct and freeze the qualification contract | complete | [verification](stage-5-verification.md) banner corrected; [addendum](stage-5-verification-addendum-20260917.md) frozen before §5 rows |
| D. Valid comparisons and real pipeline proof | component rows complete; complete-operation comparison `NOT_RUN` with its source-backed reason | §5, §6 |
| E. Verification, report, review | complete except the independent review | §7, §8 |

| Gate | Status |
| --- | --- |
| G1 three confirmed defects fixed with corrected external regressions | PASS |
| G2 grammar, precedence, tier boundaries, byte/quota accounting, checked cleanup verified | PASS |
| G3 C1 and C2 failure matrices pass, earlier roots intact | PASS |
| G4 whole-operation resource ownership and localized work established | PASS for the reported owners and the declared ceilings; **not** a measured whole-process memory bound (§4) |
| G5 canonical/profile/schema, hardlink/topology/attributes and Stage 1–4 regressions | PASS on the final source (§7) |
| G6 independent timers with real bodies, honest scopes, unchanged on/off behaviour | PASS (`filesystem_timing`, phase list in §6) |
| G7 applicable matched performance rows have valid evidence; non-comparative alternatives justified | component rows PASS with one row slower than the reference; complete-operation comparison **NOT_RUN**, justified in §6 by the reference's absent public surface; no complete-operation performance claim is made |
| G8 core checks, reports, LOC accounting, evidence identities | PASS except the independent review (§8) |

## 4. Resource ownership (separate from correctness)

| Owner | Ceiling | What is counted | Evidence |
| --- | --- | --- | --- |
| Sorted merge scratch | `FilesystemResources::scratch_bytes` (default 4 MiB − 1) | every page, row, decoded wire and canonical output before allocation | `peak_scratch_bytes` in both merge counters |
| Ordering bytes | `FilesystemResources::ordering_bytes` (default 64 MiB) | pending rows, live runs and the output a merge is about to create; reserved before creation | `references.runs.peak_run_bytes`, the operation-ceiling case |
| Physical backing | `FileBacking` capacity (default 256 MiB) | held/peak bytes actually owned; growth refused before it happens | backing counters cross-checked in `filesystem_ordering` |
| Touched-serial collection | one `u64` per touched inode | the single collection the operation performs, sized by the supplied changes plus released inodes | `references.serials_scanned` |
| Release cursors | one per released directory level | 64-entry pages | `release.pages`, `release.peak_depth` |
| Object boundary | none: reads are borrowed | one outstanding group | `objects.read_waves`, `peak_wave()` |

The declared ceilings bound the owners the operation reports. This is **not** a
measured bound on the whole process: caller input vectors, the provider's own
buffers, the consumer's retained objects and the file cache are outside the
operation and are counted by their owners, not here.

## 5. Collected rows

**Component primitives** (release, one sample per case per arm, clean tree at
`3b4941f1e`, receipt
`evidence/stage-5-component-comparison-20260917T073017Z/run-1/receipt.json`):

| Case | Reference ns | Candidate ns | Ratio | Identity |
| --- | ---: | ---: | ---: | --- |
| small 200 files / 20 change pairs | 436,000 | 141,375 | 0.324 | MATCH |
| wide 2,000 / 200 | 2,800,292 | 972,125 | 0.347 | MATCH |
| large-few-changes 20,000 / 20 | 3,895,708 | 4,286,541 | **1.100** | MATCH |

All six identities (base directory, base table, base root, updated directory,
updated table, final root) matched in every case. The candidate is slower on the
third case; that row is reported as measured.

**C1, C2 and integrated** (one sample each,
`evidence/stage-5-pipeline-timing-20260917T073302Z/`):

| Row | Scope | Elapsed |
| --- | --- | ---: |
| C1 `directory-update` | native operation, no database, real ordering backing | 3,039,208 ns |
| C2 `inode-update` | supplied objects, admission only (8 objects, 2 packs, 1 commit, 202 pooled values) | 14,604,375 ns |
| integrated `subtree-remove` | construction + bounded handoff + save acknowledgement | 17,534,000 ns |
| integrated read-back | labelled separately | 90,583 ns |

## 6. What is not claimed

- **No complete-operation comparison.** The reference has no public entry point
  that takes final sorted bindings plus typed values and returns a filesystem root;
  its filesystem update is private to `layerfs-workspace/src/changes.rs` behind a
  store, snapshot reader, workspace id and spool. This row is `NOT_RUN` with that
  reason. No complete-operation speed claim follows from §5.
- **No cold-cache, pack-footprint or simultaneous-memory claim.** The component
  family is a warm in-process fixture; the pipeline rows are correctness and
  structure evidence.
- **No whole-process memory bound.** §4 bounds the owners the operation reports.
- Two component cases are faster and one is 10% slower; nothing here is averaged
  across cases.

## 7. Verification actually run on the final source

| Command | Result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked` | PASS (all targets) |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | PASS |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | PASS |
| `python3 core/tools/check_product_boundary.py` | PASS (115 production files) |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | PASS |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | PASS |
| `python3 tools/production_loc.py --files` | see §9 |
| `git diff --check` | PASS |
| reference fixture generator (`-p layerfs-content --test stage5_reference_fixtures`) | PASS; re-running leaves the tree clean |
| Stages 1–5 review checklist | self-review performed, see §8 |

## 8. Independent review status

The acceptance pass recorded here is the **implementation agent's own review**
against the [Stages 1–5 checklist](stages-1-5-reviewer-handoff.md). It found and
fixed two items during this continuation: the row-layout overlap (§2.4) and the
consolidation precedence inversion (§2.5), plus a repeated-cleanup call in the
failure path. A review performed by an **independent** reviewer has not happened in
this session and is still outstanding; nothing in this report should be read as
substituting for it.

## 9. Size accounting

Production LOC from `python3 tools/production_loc.py --json`, same counter and
scope for every snapshot (production implementation only; tests, examples,
fixtures and tooling excluded):

| Scope | Stage 5 start | Previous report | Final | Delta from start |
| --- | ---: | ---: | ---: | ---: |
| C1 `layerfs-content` | 4,487 | 11,001 | 11,209 | +6,722 |
| C2 `layerfs-storage` | 5,941 | 5,964 | 5,964 | +23 |
| C1 + C2 | 10,428 | 16,965 | 17,173 | +6,745 |
| Telemetry | 732 | 732 | 732 | 0 |
| Core total | 11,160 | 17,697 | 17,905 | +6,745 |

Per-commit first-parent accounting is in each commit message. Files above or below
the file plan's recommended ranges are listed in
[stage-5-report.md](stage-5-report.md#2-source-tree-and-size); this continuation
moved `references/backing.rs` (116→295), `references/runs.rs` (247→434),
`references/reduce.rs` (465→579), `references/record.rs` (117→166) and
`sorted/format.rs` (603→728) further above their ranges because one owner now
accounts the bytes and the grammar has disjoint fields. Every production file stays
under the 999-physical-line ceiling and every `mod.rs` under 200 lines.

## 10. Remaining concerns and boundaries

1. **Independent review** (§8) is outstanding.
2. **Complete-operation comparison** stays `NOT_RUN`; Stage 6 (#171) qualifies the
   whole core and is the natural owner if an equivalent reference surface is built.
3. **Non-compact profiles** remain refused by design; no converter is planned.
4. **The large-few-changes component case is 10% slower than the reference.** The
   cause was not profiled; the row is reported rather than explained away, and no
   optimization was claimed for it.
5. Nothing in this continuation implements a Workspace/runtime shape, an Apple or
   APFS codec, an adapter, a plugin registry or a new crate; #165/#171/#172 stay
   open and no release or tag was produced.
