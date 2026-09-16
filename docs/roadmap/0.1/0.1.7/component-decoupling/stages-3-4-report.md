# Stages 3–4 implementation report

> **Status:** implementation report for
> [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) (Stage 3,
> physical encoding) and [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169)
> (Stage 4, localized edits). Parent [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165)
> and Stages 5–7 stay open. Both issues are **not** closed: §6 and §7 name the
> unmet criteria.

Read with the [handoff](stages-3-4-handoff.md), its
[file plan](stages-3-4-file-plan.md), the [verification contract](stages-3-4-verification.md)
and the Stages 0–2 [report](stages-0-2-report.md).

## 1. What was built

```text
C1  layerfs-content                     C2  layerfs-storage
  policy.rs    configurable T + depths      policy.rs    persisted policy, lanes, budgets
  file/view.rs one authenticated base       encoding/codec.rs  FULL + PREFIX (raw prefix)
  file/edit/   known edits                  encoding/delta/    record, select, chains, cache
    input      validated stream, plan       encoding/full.rs   FULL records, lane planning
    compare    bounded no-op + replay       pack/layout.rs     v1/v2/v4/v6/v7 grammars
    frontier   owned decoded unfinished     pack/placement.rs  planned lane placement
    split/concat/finish/apply               cas/owner.rs       selection + one write path
  mapping/build.rs  ExtentBuilder           cas/read.rs        chain reconstruction
```

Three modes remain independently runnable and were actually run
(`--mode c1|c2|pipeline`), now with edit cases
([smoke evidence](../evidence/stages-3-4-smoke-20260916T210931Z/README.md)).

### Stage 3 (#168)

* **Configurable policy with checked ranges.** The construction cutoff accepts
  powers of two from 128 KiB to 1 MiB; each role's dependency depth accepts
  `0` (delta disabled) up to `50`. The persisted policy row is validated on create
  and on open, and a Store is never migrated. Threshold selection, chain-work
  budgets and live-memory budgets are separate capacities: `policy_capacity.rs`
  asserts that raising the cutoff widens only the whole-file record/frame bounds.
* **Payload DELTA.** `WHOLE_FILE` considers its explicit predecessor first and the
  bounded admitted-FULL winner cache only when no listed candidate is acquired;
  `CHUNK` considers the first supplied eligible candidate and never the cache. At
  most one PREFIX trial happens per object, and the comparison is over the complete
  framed record cost including the base identity. Chains are reconstructed
  iteratively with bounded depth, encoded and canonical work; every intermediate is
  authenticated; chronology is enforced by locator order, so a cycle is not
  expressible.
* **Physical metadata pooling (E): NOT IMPLEMENTED.** No inode-leaf grammar, value
  groups, ordinals, pooled records, pooled reader or bounded index exists yet. The
  `metadata_value_groups` table is shipped and remains empty. This is the main
  unmet part of #168 (§6).
* **Larger records and lanes.** A whole-file record whose planned compact form
  cannot fit a normal pack is planned into a singleton pack (v7) with the same
  frame bytes and the same FULL/PREFIX choice. The pooled-metadata lane (v6) exists
  as a grammar and is exercised only by the format tests; nothing writes it yet.
* **Format matrix.** v1 ordinary, v2 native, v4 compact whole-file, v6 pooled
  metadata and v7 singleton are recognized by version; v3, v5 and unknown versions
  are rejected explicitly with no trial decode.

### Stage 4 (#169)

* **Known-edit construction.** One immutable base is opened and authenticated once
  per operation. Edits are an ordered stream in current-result coordinates with
  stable replacement ranges; the stream is validated before any work and never
  reordered or merged. One declared applicability rule is enforced: an edit may not
  reach back into bytes an earlier edit in the same stream introduced.
* **Dispatch on the declared final length.** Small → small assembles the final whole
  object in one allocation; small → large streams retained bytes and replacements
  through the complete builder (so the root equals a fresh construction); large →
  small assembles retained ranges without reading discarded ranges; large → large
  streams retained extents and CDC'd replacement runs into an owned decoded
  frontier; any → empty emits the defined empty representation.
* **No-op preservation.** An empty stream, or a stream whose replacements are all
  byte-identical to their base ranges, returns the base root itself. The comparison
  is bounded and stops at the first difference; a mismatch replays the same edits
  for construction and both passes stay charged.
* **Decoded frontier (D): PARTIAL.** There is one owned decoded unfinished
  representation per operation (`ExtentBuilder` wrapped by `EditFrontier`), retained
  entries are bounded by page capacity and height, and pages are sealed as soon as
  a later edit cannot reach them. Stored-node **split/concat reuse** is not
  implemented: an edit re-derives the mapping from the retained extent sequence, so
  unchanged mapping pages are re-encoded even though the chunk payloads they refer
  to are retained. Reference-root equivalence for large → large edits is therefore
  **not established** (§7).

## 2. Supported profile, formats and ranges

| Item | Accepted |
| --- | --- |
| Construction cutoff `T` | powers of two, 131 072 … 1 048 576; default 131 072 |
| Whole-file delta depth | 0 … 50; default 8; `0` disables prospective whole-file delta |
| Chunk delta depth | 0 … 50; default 4; `0` disables prospective chunk delta |
| Metadata delta depth | 0 … 50; default 8; the pooled-metadata dependency bound |
| Schema | `application_id = 1279677261`, `user_version = 4`; older Stores rejected, never migrated |
| Tables/columns | four tables, twenty-one columns (`metadata_delta_max_depth` added by schema 4) |
| Object roles | 1 … 6; role 6 (`InodeLeaf`) is produced by the pooled metadata lane |
| Pack framings | v1 ordinary, v2 native, v4 compact whole-file, v6 pooled metadata, v7 singleton; v3/v5/unknown rejected |
| Pack bounds | 256 KiB (v1/v2/v4/v6), `16 MiB + 4096` (v7, one group) |
| Group body bound | 64 KiB (v1/v2/v4), 16 KiB (v6), singleton pack limit (v7) |
| Payload codec | unchanged pinned Zstandard parameters; whole-file window log 18 at/below the 256 KiB cutoff, 20 above; frame bound `max(135168, raw + raw/128 + 1024)` |
| Payload chain budgets | 512 KiB canonical, 256 KiB encoded, independent of depth |
| Metadata chain budgets | 8 × 8 192 canonical (65 536), 17 × 8 193 encoded (139 281), 128 KiB match budget, 32 MiB decoded work |
| Pooled grammar | inode value 73 B, node header 31 B, leaf row 81 B, pooled prefix 44 B, pooled row 12 B, ≤100 rows per leaf, 165 values per group |
| Pooled index | `BTreeSet<(fingerprint, ordinal)>`, ≤131 072 entries, whole-window reset, catalogue replay on cold start, invalidate-on-failure |
| Read ceiling | the publication watermark, applied to every dependency and cache read |

## 3. Actual file tree and LOC

Produced with `python3 tools/production_loc.py --root <tree> --detail` (one counter
and classification for every snapshot; before = the commit's first parent
materialized with `git archive`, after = the staged or committed tree).

```text
Production LOC at HEAD dfd54fd8e
  core      10929   layerfs-content 4385, layerfs-storage 5812, layerfs-telemetry 732
  reference 68476   unchanged (coexistence, not removal)
  combined  79405
```

Directory totals (production LOC; parent directories include their children):

| Directory | After |
| --- | ---: |
| `core/crates/layerfs-content/src/` | 4 385 |
| `core/crates/layerfs-content/src/file/` | 3 366 |
| `core/crates/layerfs-content/src/file/edit/` | 1 391 |
| `core/crates/layerfs-content/src/file/mapping/` | 1 011 |
| `core/crates/layerfs-content/src/object/` | 739 |
| `core/crates/layerfs-storage/src/` | 5 764 |
| `core/crates/layerfs-storage/src/encoding/` | 2 587 |
| `core/crates/layerfs-storage/src/encoding/delta/` | 795 |
| `core/crates/layerfs-storage/src/encoding/pool/` | 926 |
| `core/crates/layerfs-storage/src/cas/` | 1 410 |
| `core/crates/layerfs-storage/src/pack/` | 739 |
| `core/crates/layerfs-storage/src/sqlite/` | 702 |
| `core/crates/layerfs-storage/sql/` | 48 |
| `core/crates/layerfs-telemetry/src/` | 732 |

Larger production files (production / physical): `cas/owner.rs` 710 / 890,
`file/edit/tree.rs` 552 / 645, `encoding/codec.rs` 508 / 632, `file/cdc/gear.rs`
494 / 538, `file/edit/apply.rs` 391 / 447, `cas/store.rs` 395 / 530,
`pack/layout.rs` 381 / 466. Every production file is under the 999-line ceiling and
every `lib.rs`/`mod.rs` is under 200 lines (`storage/src/lib.rs` 11 / 30 is the
largest declaration file).

Per-commit production LOC for this batch (first parent → committed tree):

| Commit | Subject | Before | After | Delta |
| --- | --- | ---: | ---: | ---: |
| `da8ee5769` | implement pooled physical metadata | 8 660 | 10 399 | +1 739 |
| `b49931570` | sealed reference edit oracle and chunked EOF append | 10 399 | 10 415 | +16 |
| `315a339fa` | required `edit_model` target | 10 415 | 10 415 | 0 |
| `2b2dbc028` | correct the per-package LOC subtotals | 10 415 | 10 415 | 0 |
| `01d9f70f3` | stored-tree copy-on-write edits | 10 415 | 10 893 | +478 |
| `5a0fef716` | bound pooled chains at admission and cover the window | 10 893 | 10 929 | +36 |
| `c255dcfaf` | declare the measurement round | 10 929 | 10 929 | 0 |
| `dfd54fd8e` | print the pooled policy and identities | 10 929 | 10 929 | 0 |

New production files in the batch: `object/inode_leaf.rs`,
`file/edit/{input,compare,split,concat,finish,tree}.rs`,
`encoding/pool/{mod,value_group,leaf,delta,index,read}.rs`, `sqlite/pool.rs`.
Deleted: `file/edit/frontier.rs` (95 production lines), the whole-mapping rebuild
route it existed for. Relocated/merged: record extraction and reconstruction stayed
in `encoding/decode.rs`; the reference `encoding/codec.rs` responsibilities stayed in
one file instead of a `codec/` directory.

## 4. Commands and results

All commands were run from the repository root at HEAD after the final commit.

```sh
python3 core/tools/check_product_boundary.py                       # PASS: 75 production files scanned
(cd core/tools && python3 -m unittest discover -v)                 # PASS: 5 tests
python3 tools/test_production_loc.py                               # PASS: 13 tests
cargo +1.85.1 fmt --all --manifest-path core/Cargo.toml --check    # PASS
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
                                                                   # PASS: 43 targets, 245 tests, 0 failures
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked -- -D warnings
                                                                   # PASS, no warnings
git diff --check                                                   # PASS (no whitespace errors)
```

`tools/preflight.sh` was **not** run: it is permanently retired (ledger L32), and no
aggregate wrapper was substituted.

Tests per target (actual counts from the run above):

| Target | Tests | Target | Tests |
| --- | ---: | --- | ---: |
| `edit_batch` | 7 | `metadata_pool` | 9 |
| `edit_bounds` | 8 | `metadata_pool_index` | 5 |
| `edit_localized` | 5 | `metadata_window` | 2 |
| `edit_model` | 6 | `metadata_chain` | 2 |
| `edit_noop` | 7 | `metadata_fingerprint_collision` | 2 |
| `edit_reference` | 2 | `delta_payload` | 10 |
| `edit_single` | 9 | `delta_chains` | 8 |
| `edit_timing` | 4 | `policy_capacity` | 7 |
| `edit_transitions` | 5 | `physical_formats` | 5 |
| `file_complete` | 14 | `edit_pipeline` | 4 |
| `file_read` | 8 | `cas_reuse` | 8 |
| `inode_leaf` | 6 | `cas_roundtrip` | 6 |
| `object_identity` | 9 | `core_pipeline` | 5 |
| `streaming` | 8 | `memory_bounds` | 7 |
| `timing` (C1) | 11 | `pack_locator` | 7 |
| | | `persistence_failure` | 7 |
| | | `visibility` | 5 |
| | | `timing` (C2) | 4 |
| | | telemetry (`timer*`) | 37 |

Retained evidence: `evidence/stages-3-4-oracle-20260916T222738Z-corrected/` (eight
reference edit cases) and `evidence/stages-3-4-oracle-20260917T034500Z/` (the ninth,
`repartition-80-100`, in its own generation directory so the earlier receipts stay
untouched), `evidence/stages-3-4-fingerprint-collision-20260917T021500Z/`
(the searched collision pair and the search log),
`evidence/stages-3-4-timing-20260917T031000Z/` (the declared measurement round with
its ledger and `tool-identities.txt`), and the earlier
`evidence/stages-3-4-smoke-20260916T210931Z/` and
`evidence/stages-3-4-oracle-20260916T214846Z/` generations, retained as they were
with their claims marked superseded.

## 5. #168 criterion table

| Criterion | Code | Test | Evidence |
| --- | --- | --- | --- |
| Configurable cutoff with checked ranges | `content/policy.rs`, `storage/policy.rs` | `edit_transitions::exact_boundaries_at_every_accepted_cutoff`, `policy_capacity::*` | accepted 128 KiB/256 KiB/1 MiB end-to-end; unsupported values rejected before creation |
| Depths configurable, `0` disables delta, increased value reaches its boundary | `policy.rs`, `encoding/delta/select.rs` | `delta_payload::depth_zero_disables_prospective_delta_entirely`, `delta_chains::an_increased_depth_is_honoured_up_to_its_boundary` | 16-edge chain stored and read at depth 16; boundary refusal at depth 2 |
| Chain-work ceilings independent of depth | `encoding/delta/read.rs`, `select.rs`, `cas/owner.rs` | `delta_chains::a_chain_that_would_exceed_its_work_budget_is_never_stored`, `metadata_chain::maximal_records_hit_the_canonical_budget_before_the_depth_cap` | payload and pooled lanes both refuse a dependency a later read could not reconstruct |
| WHOLE_FILE explicit predecessor and cache rule | `delta/select.rs`, `delta/candidates.rs` | `delta_payload::{an_explicit_predecessor_*, the_admitted_full_cache_*}` | PREFIX stored and read back; cache hit inside one save |
| CHUNK first-supplied-candidate rule | `delta/select.rs` | `delta_payload::native_delta_uses_the_first_supplied_eligible_candidate_only` | two candidates, one trial |
| Complete-cost comparison including base identity | `delta/record.rs`, `full.rs` | `delta_payload::an_unrelated_candidate_loses_the_cost_comparison` | FULL selected by policy, not by failure |
| Exact reuse before any trial | `cas/save.rs`, `membership.rs` | `delta_payload::exact_reuse_is_decided_before_any_trial` | `trials == 0` on a duplicate |
| Failed acquisition never becomes FULL | `delta/select.rs`, `cas/owner.rs` | `delta_payload::a_corrupt_base_fails_the_save_instead_of_selecting_full` | corrupt base pack → save fails |
| Iterative chain reconstruction with authentication | `delta/read.rs` | `delta_chains::{a_corrupt_intermediate_*, a_cyclic_dependency_*, a_wrong_role_dependency_*}` | corrupt/missing/cyclic/wrong-role dependencies each rejected |
| Explicit format matrix, no decoder probing | `pack/layout.rs` | `physical_formats::*` | v1/v2/v4/v6/v7 recognized; v0/v3/v5/v8/v99 rejected |
| Larger incompressible whole-file singleton at 1 MiB | `encoding/full.rs`, `pack/assemble.rs` | `policy_capacity::a_larger_incompressible_whole_file_record_uses_the_singleton_lane` | 1 048 575-byte record stored, read back, one database file on disk |
| Pooled inode-leaf grammar and value identity | `object/inode_leaf.rs` | `inode_leaf::*` (6), `metadata_pool::*` | 73-byte values, 31-byte header, 81-byte rows, canonical re-encode check, pooled body round-trip |
| Exact value pooling, ordinals and digests | `encoding/pool/{value_group,leaf}.rs`, `sqlite/pool.rs` | `metadata_pool::{a_supplied_leaf_round_trips_through_sqlite_and_reopen, shared values keep the smallest ordinal}` | group body built once then hashed; 165 values per group; one group per admitted leaf |
| Pooled FULL / COPY-INSERT DELTA records | `encoding/pool/delta.rs`, `cas/owner.rs` | `metadata_pool::*`, `metadata_chain::*` | real COPY/INSERT programs; seven of eight leaves delta at 100 rows; FULL on depth/budget refusal |
| Bounded index window, wholesale reset, catalogue replay, invalidate-on-failure | `encoding/pool/index.rs`, `cas/owner.rs` | `metadata_window` (2), `metadata_pool_index` (5) | cap retained exactly at 131 072, whole-window release past it, 1 312-leaf real Store crossing, reopen replay, failed save cleans its groups |
| Candidate filter is a filter, not equality | `encoding/pool/index.rs` + search tool | `metadata_fingerprint_collision` (2) | two distinct valid values with one 64-bit fingerprint, never confused through the Store |
| Packing lanes for the pooled lane and singletons | `pack/{layout,assemble}.rs` | `physical_formats::*`, `pack_locator::*` | v6 pooled groups, v7 singleton, lane-specific pack bounds |

## 6. #169 criterion table

| Criterion | Code | Test | Evidence |
| --- | --- | --- | --- |
| Ordered normalized edits in current-result coordinates | `file/edit/input.rs` | `edit_batch::several_separated_edits_apply_in_order`, `edit_model::*` | 200 edits in one pass equal the independent model and a fresh construction |
| Stable replacement ranges, overlap/inapplicable rejection | `input.rs` | `edit_batch::an_overlapping_stream_is_rejected_before_any_work`, `edit_single::an_inapplicable_range_is_rejected_before_any_work` | explicit `InvalidEdit` labels |
| Single edits: overwrite/insert/delete/append, head/middle/tail | `edit/tree.rs`, `edit/apply.rs` | `edit_single::*` (9) | root equals a fresh construction; stored-tree route for every large → large shape |
| Complete deletion and empty stream | `edit/finish.rs` | `edit_single::complete_deletion_returns_the_empty_representation` | defined empty form |
| Transitions at every accepted cutoff | `edit/apply.rs`, `policy.rs` | `edit_transitions::every_conversion_direction_reaches_the_fresh_construction_root` | 0/1/T-1/T/T+1 at 128 KiB, 256 KiB, 1 MiB; small→large, large→small, any→empty |
| No-op preservation and bounded compare + replay | `edit/compare.rs` | `edit_noop::*` (7) | equal replacement returns the base root; shifted coordinates handled |
| Occupancy 63/64/127/128/129 and streaming 192/193 | `mapping/build.rs` | `edit_bounds::{join_occupancy_at_every_boundary, streaming_flush_boundaries_and_height_growth}` | every non-root page holds 64…128 entries after the edit |
| Stored-tree split/concat reuse (large → large) | `file/edit/tree.rs` | `edit_localized` (5), `edit_batch`, `edit_single` | one three-extent overwrite demands 3 retained payloads at 8 MB and 3 at 24 MB while the mapping grows 5 → 12 pages; untouched payloads are still referenced by identity |
| Bounded work, many edits/files, late failures, slow consumer | `edit/tree.rs`, `edit/apply.rs` | `edit_bounds::{many_files_and_many_edits_stay_bounded, a_source_that_ends_early_*, a_consumer_that_rejects_*}` | deferred-node ceiling, one error returned once |
| Emission finality | `edit/tree.rs` | `edit_batch::every_object_the_edit_emits_is_reachable_from_its_root`, `edit_single::the_base_objects_are_never_rewritten` | every emitted object is reachable from the returned root; base objects never rewritten |
| **80 + 100 → 90 + 90 partition case** | `edit/tree.rs`, `edit/apply.rs` | `edit_reference::repartition-80-100` | the reference repartitions the 80+100 join to exactly 90 + 90 with the far leaf surviving by identity; the candidate reproduces root, partition and survivor |
| Frozen reference equivalence | `edit/tree.rs`, `edit/apply.rs` | `edit_reference` against the sealed v0.1.6 oracle | all nine cases match the reference root, partition and surviving leaf identities exactly |
| Real DB-free edit timing with enabled/disabled equivalence | `edit_timing.rs`, `examples/measure_edits.rs` | `edit_timing::*` (4), `--timing on\|off` receipts | same reads, result and identities with recording on and off |
| Real edit → C2 → reopen → readback | `cas/*`, `edit_pipeline.rs`, `examples/measure_edits.rs` | `edit_pipeline::*` (4), the `e2-pipeline-*` receipts | exact bytes after reopen; C1-only and integrated runs return the same edited root |

## 7. Unmet criteria and honest gaps

1. **No matched v0.1.6 timing comparison exists, and none is claimed.** The two
   products do not share a public edit surface to time, so the only reference claim is
   the sealed *result* oracle (`edit_reference`, `evidence/stages-3-4-oracle-*`). Speed
   and storage *improvements* over v0.1.6 remain unproven; the measurement round in
   `evidence/stages-3-4-timing-20260917T031000Z/` is a wiring demonstration in the
   debug profile with one sample per case, not a qualification.
2. **Small → large and large → small still rebuild the mapping.** Only large → large
   takes the stored-tree route; a cutoff-crossing edit rebuilds because the resulting
   page shape is different work. There is no fallback and no second runtime mode, and
   the transition tests cover every accepted cutoff.
3. **Memory evidence is declared-capacity plus external accounting.** Product-reported
   live capacity (`Store::pool_index_entries/bytes`) and the external test accounting
   in `edit_bounds`, `edit_localized` and `memory_bounds` bound the live state; no
   heap/RSS attribution was collected, and the selected SQLite cache size is not
   presented as a memory cap.
4. **The fingerprint collision is a searched example, not a proof about the filter's
   distribution.** One genuine 64-bit collision was found after 435 008 316 steps and
   is retained as a fixture; that the filter's false-positive rate behaves as a random
   64-bit function over all inputs is an assumption, not a measured property.
5. **The pooled window boundary is covered by arithmetic and a real crossing.** The
   unit case reaches the exact cap, and the Store case crosses it with 1 312 leaves;
   the full 131 072-entry window is never filled by *distinct* identity-recorded rows
   in a single fixture, because the fixture that does fill it is exactly the crossing
   case reported.
6. **The admitted-FULL cache is a heuristic.** Its min-hash sketch can miss a
   genuinely similar candidate (bounded candidate loss, never a correctness risk);
   `delta_payload::the_admitted_full_cache_supplies_a_candidate_within_one_save`
   documents the content shape it works for.

## 8. What was removed, retained, relocated, deferred

* **Removed:** the whole-mapping rebuild edit route (`file/edit/frontier.rs` and the
  streaming plan reader behind it). Nothing else was deleted; the reference tree is
  untouched.
* **Retained unchanged:** the publication watermark and its regression tests, the
  pending-group same-save read path, the exact CAS comparison, the frozen CDC profile
  and mapping grammars, MEMORY journal / synchronous OFF / zero busy timeout, and the
  999/200-line and product-purity rules.
* **Relocated/merged:** record extraction and reconstruction stayed merged in
  `encoding/decode.rs` rather than a new `pack/read.rs`; the reference
  `encoding/codec.rs` responsibilities stayed in one file instead of splitting into
  `codec/`.
* **Deferred (Stages 5–7, still open):** filesystem-tree and namespace algorithms on
  top of the inode-leaf grammar, Workspace COW, FUSE, host/daemon transport, cloud
  support, and any release/tag/deployment claim. Stage 6 broadens qualification.
* **Reference source:** untouched; no dependency, include, fallback binary or
  alternate runtime path points at `crates/`. The two reference-tree additions
  (`examples/rope_edit_oracle.rs`, `examples/fingerprint_collision_search.rs`) are
  development/search tools for oracle generation and are never candidate
  dependencies.
