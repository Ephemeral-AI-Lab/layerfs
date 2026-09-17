# Stages 3–4 verification contract (frozen before collection)

> **Status:** frozen measurement/verification contract for
> [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) (Stage 3) and
> [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169) (Stage 4).
> Target LayerFS v0.1.7; not a released contract.

This document freezes what a Stages 3–4 claim may rest on. It was written before
any comparison collection and reuses the existing harness mechanisms
(`benchmark/fs-bench-pro/`, the shared Cargo target, the receipt/ledger rules in
[docs/general/benchmark_rules.md](../../../../general/benchmark_rules.md)). No new
framework, runner or monitoring subsystem is introduced by this batch.

## 1. Frozen identities

| Item | Value |
| --- | --- |
| Candidate source | the commit that adds this file (recorded in the ledger entry) |
| Reference source | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a` |
| Reference role | source/oracle input only; never a candidate dependency, include, linked fallback or alternate runtime path |
| Toolchain | `cargo +1.85.1`, `cargo +1.96.0` for `fmt` |
| Candidate packages | `layerfs-content` (C1), `layerfs-storage` (C2) |
| Construction worker | single (default wiring); every run exports `LAYERFS_CONSTRUCTION_WORKERS=1`; the namespace-init exception does not apply to any case here |
| Cache/state | declared per case; fixtures are prepared outside the timed scope and never re-prepared inside it |
| Acknowledgement | the case is complete only when the save's final transaction was acknowledged and the readback was authenticated |

## 2. Frozen cases

Sizes are exact byte counts; seeds are the deterministic generators the candidate
tests use (`noise` = fixed xorshift stream, `patterned` = index-derived bytes).

| Family | Case | Inputs | Success criterion |
| --- | --- | --- | --- |
| Payload FULL/DELTA | `whole-file-delta-win` | 100 000-byte base, 1 000 changed bytes, explicit predecessor | one PREFIX record; readback equals the supplied canonical bytes |
| Payload FULL/DELTA | `whole-file-delta-lose` | unrelated base and target of equal size | one trial, FULL selected, readback exact |
| Payload FULL/DELTA | `chunk-delta-win` | 200 000-byte file chunk pair | PREFIX selected; readback exact |
| Edit single | `small-overwrite` | base `T/2`, one 512-byte overwrite | root equals a fresh construction of the final bytes |
| Edit single | `chunked-overwrite` | base `2T`, one 4 096-byte overwrite | readback equals the independent model |
| Edit transitions | `grow` / `shrink` / `empty` | `T-1` + `T` inserted / `2T` - `3T/4` deleted / full delete | exact root for the small and empty cases, exact bytes for the chunked case |
| Edit batch | `multi-edit` | three ordered edits in current-result coordinates | model equality, root equality |
| No-op | `equal-replacement` / `empty-stream` | byte-identical replacement / no edit | the base root is returned unchanged and no object is emitted |
| Policy | `cutoff-128k/256k/1m` | boundary lengths `T-1`, `T`, `T+1` | representation boundary follows the configured cutoff |
| Policy | `singleton-1m` | 1 048 575-byte incompressible whole-file record | stored in its own pack, read back exactly, one database file on disk |
| Index/footprint | `store-bytes` | each pipeline case | total retained pack bytes and database bytes reported; a DB size delta is never reported as write I/O |

### 2.1 Registry status: every declared case is RUN or NOT_RUN (added 2026-09-17, W8.4)

The cases §2 declares are executed by the product's own external test targets, one
sample per case, with the fixture built inside the case. "RUN" below means the case
executed in this batch and its success criterion is an assertion in the named case;
a command that cannot run is `NOT_RUN` with its reason. Nothing here is a
performance claim - these are correctness and footprint rows.

| Declared case | Product case that runs it | Status |
| --- | --- | --- |
| `whole-file-delta-win` | `delta_payload::an_explicit_predecessor_produces_a_readable_prefix_record` | RUN |
| `whole-file-delta-lose` | `delta_payload::an_unrelated_candidate_loses_the_cost_comparison`, `delta_payload::an_absent_or_ineligible_candidate_selects_full` | RUN |
| `chunk-delta-win` | `delta_payload::the_chunk_lane_honours_its_own_depth_cap`, `delta_payload::a_fifty_link_chunk_chain_is_admitted_and_read` | RUN |
| `small-overwrite` | `edit_single::overwrite_in_the_middle`, `edit_single::overwrite_at_the_head_and_the_tail` | RUN |
| `chunked-overwrite` | `edit_single::a_chunked_base_reads_and_edits_without_a_full_pass` | RUN |
| `grow` / `shrink` / `empty` | `edit_transitions::every_conversion_direction_reaches_the_fresh_construction_root`, `edit_transitions::exact_boundaries_at_every_accepted_cutoff`, `edit_single::insert_and_delete_change_the_length_exactly`, `edit_single::complete_deletion_returns_the_empty_representation` | RUN |
| `multi-edit` | `edit_batch::several_separated_edits_apply_in_order`, `edit_batch::many_small_edits_are_applied_in_one_pass`, `edit_pipeline::a_multi_edit_chunked_stream_round_trips_in_current_result_coordinates` | RUN |
| `equal-replacement` / `empty-stream` | `edit_noop::an_equal_replacement_preserves_the_base_root`, `edit_noop::an_empty_stream_returns_the_base_root_untouched` | RUN |
| `cutoff-128k/256k/1m` | `policy_capacity::boundaries_follow_the_configured_cutoff_not_a_frozen_one`, `policy_capacity::every_supported_cutoff_is_persisted_and_reopened_unchanged`, `edit_transitions::exact_boundaries_at_every_accepted_cutoff` | RUN |
| `singleton-1m` | `policy_capacity::a_larger_incompressible_whole_file_record_uses_the_singleton_lane`, `memory_bounds::the_supported_incompressible_singletons_are_stored_and_read_back`, `memory_bounds::a_complete_file_singleton_never_creates_a_payload_file` | RUN |
| `store-bytes` | `memory_bounds::the_retained_footprint_reports_pack_bodies_and_database_bytes` | RUN (new in W8.4) |
| matched v0.1.6 campaign (payload, storage, memory, latency gates) | none | **NOT_RUN** - the owner decision in the closeout report's E1 is open, so no addendum was committed and no matched arm was collected; see §4 |
| read amplification on the representation transition | none | **NOT_RUN** - no case exists; the residual gap the acceptance report names, unmeasured in this batch |

The `store-bytes` row is real, not a placeholder. One sample of a deterministic
1 048 583-byte chunked fixture (`construct_file`), saved through a real `Store`, then
measured on the closed database:

```text
MEASURED store-bytes: raw=1048583 canonical=1052271 objects=60 inserted=60 packs=6 \
  pack_bodies=1052818 largest_pack=259777 database=1204224 files=["store_bytes.sqlite"]
```

Read as: the 60 canonical objects (1 052 271 B) are framed into six pack bodies
totalling 1 052 818 B, the largest 259 777 B - inside the ordinary 262 144-byte pack
limit - and all of it lives in rows of the single 1 204 224-byte database file, with
no pack, payload or spool file on disk. Pack bodies are therefore reported apart from
the database that contains them, and no database size delta is presented as write
I/O. Command:
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --test memory_bounds the_retained_footprint_reports_pack_bodies_and_database_bytes -- --nocapture`
(raw stdout in `w8/w8-verify.log`).

## 3. Measurement rules for any later comparison

1. One sample per case per arm unless an owner-approved campaign says otherwise;
   no best-of selection; diagnostics are labelled and reported beside the gate.
2. A fresh `--output` per run; receipts are append-only and never overwritten.
3. Both arms must match on public operation semantics, inputs, algorithm profile,
   cache state, work/acknowledgement boundary and harness treatment. Where the
   v0.1.6 public API cannot expose an equivalent C1 or C2 operation, the result is
   a non-comparative diagnostic and the speedup is **unproven**.
4. Complete command wall time ≤ 15 s, declared exceptions ≤ 25 s, verification
   ≤ 60 s. A case that cannot fit is reused from a qualifying receipt or recorded
   `NOT_RUN` with its measured wall time.
5. Cache state is declared and enforced equally; `INELIGIBLE` rows are reported,
   never dropped; cold and warm rows are never pooled.
6. Resource reporting covers retained pack/database/value-group footprints,
   written versus rewritten bytes, and the simultaneous live capacities of every
   lane. `SQLite cache_size` is not a memory cap and RSS is not a universal
   safety claim.

## 4. Current state of collection

**No v0.1.6 comparison campaign was collected for this batch, and none is claimed.**
On 2026-09-17 the owner passed the two qualification rows (G13, G15) by written
waiver, unmeasured, on the finding that no flaw is open in the recorded audits; the
qualification itself is deferred to Stage 6 (#171). The waiver and its scope are
recorded in `stages-3-4-closeout-report.md` §1 and §2.
Three rounds of receipts exist and are retained as they were produced:

| Round | Contents | Status |
| --- | --- | --- |
| [`evidence/stages-3-4-smoke-20260916T210931Z/`](../evidence/) | first single-sample smoke runs of the three modes | wiring only, admission-ineligible; unchanged |
| [`evidence/stages-3-4-fingerprint-collision-20260917T021500Z/`](../evidence/) | the searched 64-bit fingerprint collision pair and the search log | correctness fixture for the pooled candidate filter |
| [`evidence/stages-3-4-timing-20260917T031000Z/`](../evidence/) | 21 declared arms (pooled lane, 15 edit lanes, two timing on/off pairs) with `ledger.md`, `tool-identities.txt` and timing trees | single sample per case, debug profile, in-process fixtures, no warm-cache credit; a wiring and correctness demonstration, explicitly **not** performance evidence |
| [`evidence/stages-3-4-matched-c1-20260917T050000Z/`](../evidence/) | the matched C1 localized-edit pair (reference v0.1.6 against the candidate) with every observation in its `ledger.md` | identical edited root in all observations; the elapsed times interleave, so the latency gate stays **unqualified**; storage boundaries differ and memory is unmeasured |

The round's declarations were committed before collection in
[`stages-3-4-measurement-addendum.md`](stages-3-4-measurement-addendum.md); every arm
finished inside the 15 s per-command budget (longest 3.155 s) and the timing on/off
pairs print identical product lines.

**Disclosure (added 2026-09-17, W8.7).** The longest arm, `e1c-pooled-512`, is
**telemetry-clipped**: its `stdout.log` prints `pooled.save 3.105s [incomplete]`,
its timing tree carries `"incomplete": true` on the root and on one
`storage.accept`, and its `timings:` line states that detail was clipped rather
than zero, leaving 57.5 % of that scope unattributed. The receipt and its tree stay
exactly as produced and are never re-labelled; only its 3.155 s *wall time* is used
above, as budget accounting. The round is a wiring and correctness demonstration
and no performance claim rests on any of its arms.

The families this contract lists (payload FULL/DELTA, transitions, pooling reuse
and turnover, pack boundaries, grouped reads and failure cleanup) are covered by the
external test targets named in the
[report](stages-3-4-report.md), and the component entry points are
`core/crates/layerfs-storage/examples/measure_edits.rs` and
`measure_pooled.rs`. The measured performance, storage and memory gates against
v0.1.6 remain open. The matched C1 pair is the only reference comparison collected; it
proves reference-equivalent roots but does not resolve latency at n=1, does not align the
byte-accounting boundary and measures no memory.

## 5. Continuation adjustment: acceptance remains in Stages 3–4

The preceding collection statement records the initial partial implementation;
its smoke receipts remain unchanged. The current [continuation prompt](stages-3-4-continuation-prompt.md)
requires pooling and exact stored-tree edit proof, then the original Stage 3–4
performance/storage/resource gates before closing #168/#169. The earlier reference
to later qualification does not defer those issue requirements to Stage 6.

Before new benchmark implementation or collection, commit a versioned specification
addendum with the missing pooling/window/reopen families, reference-root partition
oracles, physically realized localized-edit scaling cases, exact per-case cache/
index state, matching successful operations, identities, numerical/resource limits
and acknowledgement boundaries. The table above and an unspecified per-case cache
state are insufficient for the complete claim. No new measurements or numerical
targets are asserted by this scheduling adjustment.

## 6. Component-specific continuation order

The later [completion report](stages-3-4-completion-report.md) records implemented
pooling and an initial edit oracle. Follow the [three continuation prompts](stages-3-4-continuation-prompt.md):
independent pooling boundary/reopen/chain checks and eligible component measurements
can proceed before D. Combined edit qualification requires aligned oracle inputs
and the implemented stored-tree algorithm. Final acceptance must validate the
identity/reuse eligibility of every component receipt against the final artifact.
This clarification changes scheduling only; it neither qualifies current results
nor relaxes any correctness, cache, performance or resource requirement.

## 7. Final review and acceptance state

The seven required deliverables and a criterion-by-criterion verdict are collected in
[`stages-3-4-final-review.md`](stages-3-4-final-review.md). That document is a
**self-review** by the implementing agent: independence is not claimed, and the
independent pass described by
[`stages-3-4-reviewer-handoff.md`](stages-3-4-reviewer-handoff.md) remains open.
