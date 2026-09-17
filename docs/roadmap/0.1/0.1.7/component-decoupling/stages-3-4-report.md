# Stages 3–4 implementation report

> **Status: implementation complete for Stages 3–4.** Every functional acceptance
> item of [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) (Stage 3,
> physical encoding and pooling) and
> [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169) (Stage 4, localized
> edits) has production code and an external test that drives that code, and the
> sealed v0.1.6 reference agrees in all nine edit cases. What is *not* complete is
> measurement, not implementation: two qualification gates remain open and are named
> in §7 — #168's simultaneous index/codec/SQL memory instrumentation, and #169's
> "existing-or-better qualified latency/storage/memory" item, which the matched C1
> pair could not resolve at one sample per arm (identical roots, interleaving times).
> Implementation completeness is not an acceptance verdict: **both issues stay open**,
> and parent [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165) and
> Stages 5–7 stay open with them.

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
* **Physical metadata pooling (E): IMPLEMENTED.** `object/inode_leaf.rs` carries
  the supplied canonical grammar (73-byte value, 31-byte header, 81-byte row,
  44-byte pooled prefix, 12-byte pooled row, 100 rows per leaf);
  `encoding/pool/*` writes value groups in the v6 lane and FULL/COPY-INSERT leaf
  records, assigns ordinals in first-encounter order, keeps the bounded
  131 072-entry index window and reads pooled leaves back through authenticated
  groups. `metadata_value_groups` is populated by every pooled save, and
  `metadata_pool`, `metadata_pool_index`, `metadata_window` and
  `metadata_fingerprint_collision` exercise it. **Correction (2026-09-17):** the
  original §1 text in this document said "NOT IMPLEMENTED ... the table remains
  empty", which contradicted this document's own §5/§6 tables and the source; the
  independent review recorded the contradiction and this paragraph replaces it.
  The two lane-assignment deviations from the design table (pooled value groups in
  v6, pooled leaf records in the v1 ordinary lane; v5 refused by scope) are
  recorded in `physical-encoding-and-packing.md`.
* **Larger records and lanes.** A whole-file record whose planned compact form
  cannot fit a normal pack is planned into a singleton pack (v7) with the same
  frame bytes and the same FULL/PREFIX choice. The pooled-metadata lane (v6) is
  written by every pooled save: value groups are encoded there, while the pooled
  leaf records themselves are stored in the v1 ordinary lane and distinguished by
  the `objects.object_role` column.
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
* **Decoded frontier (D): IMPLEMENTED.** `file/edit/tree.rs` holds one owned
  representation per unfinished node: a page the canonical builder emitted is held
  as the finalized object it already is, and a node the operation built from
  decoded parts is held decoded under an operation-local key and encoded and hashed
  exactly once, in `commit_node`, after the commit walk proves it final. A stored
  subtree stays `ObjectId + checked summary` and is read only when a split or join
  needs its boundary, so unchanged mapping pages keep their identity and are never
  re-encoded; a draft a later split or join supersedes is released.
  **Correction (2026-09-17):** the original §1 text said the frontier was PARTIAL
  and that stored-node split/concat reuse was not implemented; the review recorded
  that this contradicted §5/§6 and the source, and this paragraph replaces it.
  Reference-root equivalence for large → large edits is established by
  `edit_reference`'s nine sealed cases (re-verified by the review and by this
  batch).

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
Production LOC (corrected counter, at the batch tip c56c28dd1)
  core      11058   layerfs-content 4463, layerfs-storage 5863, layerfs-telemetry 732
  reference 65417   unchanged by this batch (coexistence, not removal)
  combined  76475

Production LOC (corrected counter, at this section's original snapshot aa4b5a9e4)
  core      10983   layerfs-content 4388, layerfs-storage 5863, layerfs-telemetry 732
  reference 65417   combined 76400
```

**Correction (2026-09-17).** This section was written against `dfd54fd8e` and
5 commits before the reviewed snapshot `91c3a0741`, and it quoted the counter's
uncorrected reference subtotal. The independent review found two counter defects
(`tools/production_loc.py`): any `#[cfg(...)]` merely *containing* "test" removed
production code (**956** reference lines over-removed as this batch measured it;
the independent 2026-09-16 review recounted **1 027** over the same rule), and
test-only files reached by a `#[cfg(test)] mod x;` declaration were counted as
product code (**4 083** lines as this batch measured it; the review recounted
**3 737**). **Corrected 2026-09-17 (C11):** the reviewed text attributed
"956 / 4 083" to the earlier review; those are this batch's own measurements and the
review's are the pair above. Both stand as their own measurement, and neither is
re-labelled. Both defects are fixed, with a focused tool test for each
defect, and the same corrected counter is applied to the pre-Stage-3 base
(`c38961f2f`, core 6 152), to this snapshot and to every commit since. The core
subtotal is unchanged by the fix: `core/crates/*/src` contains no `cfg` attribute
at all. The corrected numbers at the closeout commit are the block above; the
closeout report carries the same figures with the commands that produced them.

**Correction (2026-09-17, C9).** The block above was the `aa4b5a9e4` (W9) snapshot's
totals and the section presented them as the batch's. Both snapshots are now in the
block, each labelled, and the tip is the one the batch closes on: core **11 058** in
75 files, reference 65 417, combined **76 475**. A later commit's counter change
(`6566a95a3`) also moved the reference subtotal from 68 476 to 65 417, so any
combined figure quoted before it is stale by 3 059. Section 9.2 carries the same
correction for the disjoint package table.

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

Larger production files (production / physical) at this section's original
snapshot: `cas/owner.rs` 710 / 890, `file/edit/tree.rs` 552 / 645,
`encoding/codec.rs` 508 / 632, `file/cdc/gear.rs` 494 / 538,
`file/edit/apply.rs` 391 / 447, `cas/store.rs` 395 / 530, `pack/layout.rs` 381 / 466.
Every production file is under the 999-line ceiling and every `lib.rs`/`mod.rs` is
under 200 lines (`storage/src/lib.rs` 11 / 30 is the largest declaration file).

**Snapshot note (2026-09-17, C7).** Those figures belong to the snapshot this
section was written against; the independent review measured the same two files at
the reviewed tip as `cas/owner.rs` 911 and `file/edit/tree.rs` **901** physical
lines, and at the time of this correction they are 949 and 913. The caps still
hold, and the boundary guard (`python3 core/tools/check_product_boundary.py`)
re-scans them on every round rather than relying on this table. Section 9's per-file
table is the same snapshot's; its `file/edit/tree.rs` row states 805 physical lines,
which is the W9 measurement, not the reviewed tip's 901.

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
| `edit_batch` | 7 | `metadata_pool` | 14 |
| `edit_bounds` | 9 | `metadata_pool_index` | 5 |
| `edit_localized` | 5 | `metadata_window` | 2 |
| `edit_model` | 6 | `metadata_chain` | 2 |
| `edit_noop` | 7 | `metadata_fingerprint_collision` | 2 |
| `edit_reference` | 2 | `delta_payload` | 13 |
| `edit_single` | 9 | `delta_chains` | 9 |
| `edit_timing` | 4 | `policy_capacity` | 8 |
| `edit_transitions` | 5 | `physical_formats` | 6 |
| `file_complete` | 14 | `edit_pipeline` | 5 |
| `file_read` | 8 | `cas_reuse` | 8 |
| `inode_leaf` | 6 | `cas_roundtrip` | 6 |
| `object_identity` | 10 | `core_pipeline` | 5 |
| `streaming` | 9 | `memory_bounds` | 7 |
| `timing` (C1) | **7** | `pack_locator` | 8 |
| | | `persistence_failure` | 7 |
| | | `visibility` | 7 |
| | | `timing` (C2) | 4 |
| | | telemetry (`timer*`) | 37 |

**Correction (2026-09-17).** The `timing` (C1) cell said 11; the target runs 7
(counted from the workspace run's own output, and 7 is what the closeout evidence
records). The other cells above are the counts the same run reports at the closeout
commit, so this table and the code agree. The global total in this document's §6 was
correct for its own snapshot.

Retained evidence: `evidence/stages-3-4-oracle-20260916T222738Z-corrected/` (eight
reference edit cases) and `evidence/stages-3-4-oracle-20260917T034500Z/` (the ninth,
`repartition-80-100`, in its own generation directory so the earlier receipts stay
untouched),

**Correction (2026-09-17, R43).** The case `repartition-80-100` is named for its
*input* - a file of 80 extents concatenated with a file of 100 - and not for the
sealed base pages, which are **89 and 90** (`base_pages` in its fixture): the
canonical construction rebuilds the join and repartitions it, so an 80/100 base
partition is not reachable through the product at all. The edited 90 + 90 partition
and the surviving right leaf *are* asserted against the sealed fixture, and
`edit_reference::the_candidate_reproduces_the_reference_root_and_partition` now
asserts the **base** partition against the sealed one as well, so the label and the
fixture cannot drift apart unnoticed. The fixture itself is retained unchanged. `evidence/stages-3-4-fingerprint-collision-20260917T021500Z/`
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
| Existing-or-better qualified latency/storage/memory | — | `evidence/stages-3-4-matched-c1-20260917T050000Z/` | **INCOMPLETE**: the matched C1 pair returns the identical reference root, but four observations interleave (reference 224 875–351 666 ns, candidate 186 000–468 333 ns) and n=1 cannot qualify the gate; storage is not a like-for-like boundary and memory is unmeasured |

## 7. Unmet criteria and honest gaps

1. **No matched v0.1.6 timing comparison exists, and none is claimed.** The two
   products do not share a public edit surface to time, so the only reference claim is
   the sealed *result* oracle (`edit_reference`, `evidence/stages-3-4-oracle-*`). Speed
   and storage *improvements* over v0.1.6 remain unproven; the measurement round in
   `evidence/stages-3-4-timing-20260917T031000Z/` is a wiring demonstration in the
   debug profile with one sample per case, not a qualification.
2. **Transition read amplification is unmeasured, not non-compliant.** A whole-file
   base streams retained bytes and replacements through the complete builder, and a
   chunked base whose result falls below the cutoff assembles retained ranges without
   reading a discarded range - exactly what the handoff's §3.C prescribes
   (`file/edit/apply.rs:106-109`), with no fallback route and no second runtime mode;
   dispatch is on the viewed representation alone. What this batch does not have is a
   *measurement* of how many bytes those two transitions read per byte they produce:
   `edit_localized` establishes the demand set for the large → large stored-tree
   route, and the transition routes are covered for correctness only. **Correction
   (2026-09-17):** the original text called this a rebuild defect; the review showed
   the code does what the handoff prescribes, so the residual gap is stated instead.
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
6. **The matched comparison does not qualify latency, storage or memory.** The
   matched C1 pair in `evidence/stages-3-4-matched-c1-20260917T050000Z/` proves the
   candidate returns the reference root for the same base and edit, but its four
   observations interleave (reference 224 875–351 666 ns, candidate 186 000–468 333 ns),
   so with one sample per arm neither "existing-or-better" nor "slower" is supported.
   Storage is not a like-for-like boundary (the candidate emits the replacement
   content inside the timed window; the reference wrote 13 826 B against the
   candidate's 47 357 B) and memory is unmeasured. The smallest missing case is an
   owner-approved repeated-sample campaign (n ≥ 5 per arm) on this fixture, plus an
   aligned byte-accounting boundary.
7. **The admitted-FULL cache is a heuristic.** Its min-hash sketch can miss a
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

## 9. Per-file actual sizes (file-plan §4)

*(Renumbered from `## 8.` on 2026-09-17; see the note at the end of this section. The earlier `## 8. What was removed, retained, relocated, deferred` above keeps its number, so `§8.2` in older documents is now `§9.2`.)*

The file plan's §4 requires this table: `path | action | before production LOC |
actual after | signed delta | recommended final range | below/within/above |
physical lines | explanation and responsibility`. It was missing from this
document; the independent review recorded the omission, and the table below is it,
recomputed at the Stages 3-4 closeout commit with the **corrected** counter
(`tools/production_loc.py`, applied identically to the pre-Stage-3 base
`c38961f2f` and to the current tree). **Snapshot note (2026-09-17, C7):** the
"current tree" is the W9/`aa4b5a9e4` snapshot this table was computed against, not
the batch tip and not the reviewed snapshot; the `file/edit/tree.rs` row below
carries 805 physical lines where the reviewed tip measures 901 and this round
measures 913. The table is retained as the W9 measurement, labelled, and the tip's
per-file figures are the review's §3.1 and the boundary guard's scan.

`before` is the plan's base column, which the review verified equals `c38961f2f`
for these files. `after` is production LOC; `physical` is every physical line,
including comments and blanks, which is the 999-line ceiling measure. A plan row
whose file does not exist is marked *merged or folded*: the review assessed each of
those merges as justified and not a saving claim, and §5 of this document carries
the same assessment.

| path | action | before | after | delta | recommended | verdict | physical | responsibility |
| --- | --- | ---: | ---: | ---: | --- | --- | ---: | --- |
| `core/crates/layerfs-content/src/lib.rs` | Update | 16 | 24 | +8 | 18–35 | within | 41 | Public edit and object reexports |
| `core/crates/layerfs-content/src/error.rs` | Update | 116 | 120 | +4 | 120–190 | within | 174 | Edit/input/capacity errors |
| `core/crates/layerfs-content/src/policy.rs` | Update | 106 | 135 | +29 | 150–250 | below | 226 | Validated configurable policy and derived C1 capacities |
| `core/crates/layerfs-content/src/object/mod.rs` | Update | 12 | 23 | +11 | 14–24 | within | 29 | Declarations/reexports |
| `core/crates/layerfs-content/src/object/id.rs` | Retain | 89 | 77 | -12 | 89 | changed | 105 | Frozen identity |
| `core/crates/layerfs-content/src/object/codec.rs` | Update | 107 | 107 | +0 | 107–150 | within | 138 | Canonical limits independent of routing cutoff |
| `core/crates/layerfs-content/src/object/access.rs` | Update | 18 | 18 | +0 | 40–90 | below | 36 | Bounded shared authenticated owners; ordered demands |
| `core/crates/layerfs-content/src/object/output.rs` | Update | 111 | 121 | +10 | 100–170 | within | 187 | Moved final bytes and bounded advisory predecessors |
| `core/crates/layerfs-content/src/object/predecessor.rs` | New | 0 | 64 | +64 | 45–85 | within | 102 | Bounded candidates and explicit provenance |
| `core/crates/layerfs-content/src/object/inode_leaf.rs` | New | 0 | 307 | +307 | 160–260 | above | 392 | Checked compact inode-leaf grammar needed for C2 pooling only |
| `core/crates/layerfs-content/src/file/mod.rs` | Update | 10 | 17 | +7 | 14–24 | within | 23 | File operation reexports |
| `core/crates/layerfs-content/src/file/content.rs` | Update | 195 | 197 | +2 | 150–230 | within | 254 | Capacity-aware complete/whole construction |
| `core/crates/layerfs-content/src/file/read.rs` | Update | 111 | 111 | +0 | 90–160 | within | 127 | Read using an operation-local authenticated view |
| `core/crates/layerfs-content/src/file/view.rs` | New | 0 | 86 | +86 | 90–160 | below | 112 | Open/authenticate base once; scoped logical reads |
| `core/crates/layerfs-content/src/file/cdc/mod.rs` | Retain | 5 | 5 | +0 | 5 | exact | 10 | Frozen CDC exports |
| `core/crates/layerfs-content/src/file/cdc/gear.rs` | Retain | 494 | 494 | +0 | 494 | exact | 538 | Frozen table and algorithm |
| `core/crates/layerfs-content/src/file/mapping/mod.rs` | Update | 15 | 15 | +0 | 18–30 | below | 20 | Mapping reexports |
| `core/crates/layerfs-content/src/file/mapping/types.rs` | Update | 182 | 182 | +0 | 180–250 | within | 248 | Checked summaries, extent bounds and roles |
| `core/crates/layerfs-content/src/file/mapping/codec.rs` | Update | 303 | 303 | +0 | 280–380 | within | 334 | Frozen canonical node grammar; reuse checked decode |
| `core/crates/layerfs-content/src/file/mapping/build.rs` | Update | 236 | 309 | +73 | 220–330 | within | 391 | Final child-first streaming emission |
| `core/crates/layerfs-content/src/file/mapping/read.rs` | Update | 210 | 210 | +0 | 180–290 | within | 241 | Grouped node/payload acquisition and shared repeated demand |
| `core/crates/layerfs-content/src/file/edit/mod.rs` | New | 0 | 15 | +15 | 8–16 | within | 20 | Edit declarations/reexports |
| `core/crates/layerfs-content/src/file/edit/input.rs` | New | 0 | 286 | +286 | 90–160 | above | 390 | Checked ordered edit stream and stable replacement range capability |
| `core/crates/layerfs-content/src/file/edit/apply.rs` | New | 0 | 388 | +388 | 130–230 | above | 450 | Known final-size dispatch and sequential edit operation |
| `core/crates/layerfs-content/src/file/edit/compare.rs` | New | 0 | 67 | +67 | 80–140 | below | 86 | Bounded applicable no-op comparison and replay |
| `core/crates/layerfs-content/src/file/edit/split.rs` | New | 0 | 23 | +23 | 160–260 | below | 32 | Path-local split preserving slices/subtrees |
| `core/crates/layerfs-content/src/file/edit/concat.rs` | New | 0 | 19 | +19 | 210–350 | below | 29 | Join/coalesce/partition/root-collapse rules |
| `core/crates/layerfs-content/src/file/edit/finish.rs` | New | 0 | 38 | +38 | 110–190 | below | 54 | Proven finality, child-first sealing and final root |
| `core/crates/layerfs-storage/src/lib.rs` | Update | 11 | 11 | +0 | 14–28 | below | 30 | Storage/encoding public reexports only |
| `core/crates/layerfs-storage/src/error.rs` | Update | 105 | 105 | +0 | 120–200 | below | 154 | Explicit physical/dependency/capacity failures |
| `core/crates/layerfs-storage/src/policy.rs` | Update | 148 | 193 | +45 | 180–300 | within | 328 | Resolved profile, cutoff/depth/work/frame capacities |
| `core/crates/layerfs-storage/src/cas/mod.rs` | Update | 11 | 11 | +0 | 14–24 | below | 16 | CAS declarations/reexports |
| `core/crates/layerfs-storage/src/cas/store.rs` | Update | 327 | 385 | +58 | 260–420 | within | 518 | Scoped store/read/save ownership and bounded Store-owned caches |
| `core/crates/layerfs-storage/src/cas/owner.rs` | Update | 364 | 711 | +347 | 320–520 | above | 911 | Writer state, physical lanes, publication/cleanup invariants |
| `core/crates/layerfs-storage/src/cas/batch.rs` | Update | 58 | 58 | +0 | 90–160 | below | 87 | Byte/count admission and planned singleton |
| `core/crates/layerfs-storage/src/cas/save.rs` | Update | 49 | 52 | +3 | 90–170 | below | 76 | Batched membership/bases and moved input; no per-object clone |
| `core/crates/layerfs-storage/src/cas/membership.rs` | Update | 42 | 32 | -10 | 70–140 | below | 47 | Exact reuse/collision under valid ownership |
| `core/crates/layerfs-storage/src/cas/dependencies.rs` | Update | 62 | 60 | -2 | 90–170 | below | 86 | Logical references plus selected physical dependencies |
| `core/crates/layerfs-storage/src/cas/read.rs` | Update | 65 | 74 | +9 | 120–220 | below | 101 | Grouped acquisition, retained ceiling and shared decoded owners |
| `core/crates/layerfs-storage/src/cas/finish.rs` | Update | 16 | 16 | +0 | 20–50 | below | 25 | Final drain, acknowledgement and one terminal disposition |
| `core/crates/layerfs-storage/src/encoding/mod.rs` | Update | 9 | 11 | +2 | 12–24 | below | 17 | Encoding exports |
| `core/crates/layerfs-storage/src/encoding/full.rs` | Update | 112 | 177 | +65 | 110–190 | within | 213 | Capacity-aware existing FULL alternatives |
| `core/crates/layerfs-storage/src/encoding/decode.rs` | Update | 103 | 170 | +67 | 100–180 | within | 192 | Explicit physical dispatch and canonical authentication |
| `core/crates/layerfs-storage/src/encoding/codec.rs` | Retire | 367 | 508 | +141 | 0 | changed | 633 | Move and extend into codec/; count relocation once |
| `core/crates/layerfs-storage/src/encoding/delta/mod.rs` | New | 0 | 4 | +4 | 8–16 | below | 8 | Payload delta declarations/reexports |
| `core/crates/layerfs-storage/src/encoding/delta/record.rs` | New | 0 | 201 | +201 | 100–180 | above | 240 | WHOLE_FILE/CHUNK FULL/PREFIX framing |
| `core/crates/layerfs-storage/src/encoding/delta/select.rs` | New | 0 | 276 | +276 | 160–280 | within | 380 | One trial, role-aware cost/eligibility/work accounting |
| `core/crates/layerfs-storage/src/encoding/delta/read.rs` | New | 0 | 224 | +224 | 170–300 | within | 298 | Iterative dependency reconstruction and intermediate checks |
| `core/crates/layerfs-storage/src/encoding/delta/candidates.rs` | New | 0 | 126 | +126 | 100–180 | within | 166 | Admitted-FULL cache/signatures and ordered bounded candidate acquisition |
| `core/crates/layerfs-storage/src/encoding/pool/mod.rs` | New | 0 | 9 | +9 | 8–16 | within | 14 | Physical pooling exports |
| `core/crates/layerfs-storage/src/encoding/pool/value_group.rs` | New | 0 | 77 | +77 | 140–240 | below | 103 | Build/hash/compress exact value-group body once |
| `core/crates/layerfs-storage/src/encoding/pool/index.rs` | New | 0 | 203 | +203 | 160–280 | within | 261 | Bounded BTreeSet window, sync, equality candidates and invalidation |
| `core/crates/layerfs-storage/src/encoding/pool/leaf.rs` | New | 0 | 101 | +101 | 140–240 | below | 140 | Canonical rows to pooled ordinals and inverse mapping |
| `core/crates/layerfs-storage/src/encoding/pool/delta.rs` | New | 0 | 261 | +261 | 160–280 | within | 287 | Pooled COPY/INSERT selection/codec with exact cost rules |
| `core/crates/layerfs-storage/src/encoding/pool/read.rs` | New | 0 | 310 | +310 | 160–280 | above | 369 | Bounded value-group authentication and per-chain work accounting |
| `core/crates/layerfs-storage/src/pack/mod.rs` | Update | 10 | 13 | +3 | 14–24 | below | 18 | Pack declarations/reexports |
| `core/crates/layerfs-storage/src/pack/layout.rs` | Update | 321 | 396 | +75 | 280–440 | within | 489 | Checked explicit format/locator grammar including selected capacities |
| `core/crates/layerfs-storage/src/pack/placement.rs` | Update | 123 | 113 | -10 | 130–230 | below | 150 | Fit/base chronology before assembly |
| `core/crates/layerfs-storage/src/pack/assemble.rs` | Update | 195 | 223 | +28 | 180–300 | within | 252 | Borrowed groups; assemble one selected write |
| `core/crates/layerfs-storage/src/sqlite/mod.rs` | Update | 8 | 10 | +2 | 10–20 | within | 15 | SQLite exports |
| `core/crates/layerfs-storage/src/sqlite/connection.rs` | Update | 50 | 50 | +0 | 50–80 | within | 72 | Selected no-WAL/no-sync/zero-retry profile |
| `core/crates/layerfs-storage/src/sqlite/schema.rs` | Update | 223 | 232 | +9 | 220–340 | within | 267 | Explicit schema/profile compatibility and new valid policy ranges |
| `core/crates/layerfs-storage/src/sqlite/lookup.rs` | Update | 118 | 141 | +23 | 130–220 | within | 177 | Paged locator/base lookups and required indexes |
| `core/crates/layerfs-storage/src/sqlite/write.rs` | Update | 83 | 83 | +0 | 120–220 | below | 117 | Atomic packs/locators/base/catalogue writes |
| `core/crates/layerfs-storage/src/sqlite/cleanup.rs` | Update | 58 | 70 | +12 | 80–150 | below | 100 | Known-owned reverse dependency cleanup and cache invalidation |
| `core/crates/layerfs-storage/src/sqlite/pool.rs` | New | 0 | 118 | +118 | 120–200 | below | 162 | Bounded value-group catalogue queries/writes under one ceiling |
| `core/crates/layerfs-storage/sql/schema.sql` | Update | 46 | 48 | +2 | 80–130 | below | 69 | Exact roles/policy/constraints/indexes; retain publication watermark |
| `core/crates/layerfs-content/src/file/mapping/predecessor.rs` | New | 0 | — | — | 100–180 | merged or folded into a sibling (see the closeout report) | — | Bounded reference cursor with original/current coordinate distinction |
| `core/crates/layerfs-content/src/file/edit/frontier.rs` | New | 0 | — | — | 180–300 | merged or folded into a sibling (see the closeout report) | — | Owned decoded unfinished nodes and charged bounds |
| `core/crates/layerfs-storage/src/encoding/codec/mod.rs` | New | 0 | — | — | 8–16 | merged or folded into a sibling (see the closeout report) | — | Codec declarations/reexports |
| `core/crates/layerfs-storage/src/encoding/codec/profile.rs` | New | 0 | — | — | 80–140 | merged or folded into a sibling (see the closeout report) | — | Pinned parameters and checked workspace/frame capacities |
| `core/crates/layerfs-storage/src/encoding/codec/encode.rs` | New | 0 | — | — | 220–340 | merged or folded into a sibling (see the closeout report) | — | Reused bounded FULL/PREFIX and group compression workspace |
| `core/crates/layerfs-storage/src/encoding/codec/decode.rs` | New | 0 | — | — | 260–420 | merged or folded into a sibling (see the closeout report) | — | Frame checks, prefix lifetimes and bounded decompression |
| `core/crates/layerfs-storage/src/pack/read.rs` | New | 0 | — | — | 100–190 | merged or folded into a sibling (see the closeout report) | — | Grouped body/record views; avoid repeated group decode |
| `core/crates/layerfs-storage/src/pack/singleton.rs` | New | 0 | — | — | 90–160 | merged or folded into a sibling (see the closeout report) | — | Budget-checked consuming singleton assembly |
| `core/crates/layerfs-content/src/file/edit/tree.rs` | Unplanned | 0 | 627 | +627 | — | unplanned addition | 805 *(W9 snapshot; 901 at the reviewed tip, 913 now — see the snapshot note)* | see the closeout report |
| `core/crates/layerfs-telemetry/src/lib.rs` | Unplanned | 0 | 3 | +3 | — | unplanned addition | 16 | see the closeout report |
| `core/crates/layerfs-telemetry/src/timer/format.rs` | Unplanned | 0 | 69 | +69 | — | unplanned addition | 82 | see the closeout report |
| `core/crates/layerfs-telemetry/src/timer/json.rs` | Unplanned | 0 | 115 | +115 | — | unplanned addition | 136 | see the closeout report |
| `core/crates/layerfs-telemetry/src/timer/mod.rs` | Unplanned | 0 | 8 | +8 | — | unplanned addition | 25 | see the closeout report |
| `core/crates/layerfs-telemetry/src/timer/recording.rs` | Unplanned | 0 | 232 | +232 | — | unplanned addition | 291 | see the closeout report |
| `core/crates/layerfs-telemetry/src/timer/report.rs` | Unplanned | 0 | 170 | +170 | — | unplanned addition | 249 | see the closeout report |
| `core/crates/layerfs-telemetry/src/timer/scope.rs` | Unplanned | 0 | 135 | +135 | — | unplanned addition | 206 | see the closeout report |

### 9.1 Directory totals

| Directory | before | actual after | delta | recommended | verdict |
| --- | ---: | ---: | ---: | --- | --- |
| `layerfs-content/src/` | 2336 | 4388 | +2052 | 3632–5522 | within |
| `layerfs-content/src/object/` | 337 | 717 | +380 | 555–868 | within |
| `layerfs-content/src/file/` | 1761 | 3392 | +1631 | 2789–4179 | within |
| `layerfs-content/src/file/cdc/` | 499 | 499 | +0 | 499 | within |
| `layerfs-content/src/file/mapping/` | 946 | 1019 | +73 | 978–1460 | within |
| `layerfs-content/src/file/edit/` | 0 | 1463 | +1463 | 968–1646 | within |
| `layerfs-storage/src/` | 3038 | 5815 | +2777 | 5008–8578 | within |
| `layerfs-storage/src/cas/` | 994 | 1399 | +405 | 1074–1874 | within |
| `layerfs-storage/src/encoding/` | 591 | 2658 | +2067 | 2096–3602 | within |
| `layerfs-storage/src/encoding/codec/` | 0 | 0 | +0 | 568–916 | below |
| `layerfs-storage/src/encoding/delta/` | 0 | 831 | +831 | 538–956 | within |
| `layerfs-storage/src/encoding/pool/` | 0 | 961 | +961 | 768–1336 | within |
| `layerfs-storage/src/pack/` | 649 | 745 | +96 | 794–1344 | below |
| `layerfs-storage/src/sqlite/` | 540 | 704 | +164 | 730–1230 | below |
| `layerfs-storage/sql/` | 46 | 48 | +2 | 80–130 | below |

### 9.2 Disjoint package totals at the closeout commit

**Correction (2026-09-17, C9).** This table certified core **10 983** / combined
**76 400** — the `aa4b5a9e4` (W9) snapshot — while the batch tip and W10 say
**11 058** / **76 475**. Both are recorded below, each against the snapshot it
belongs to; the tip is the one the batch closes on.

At the batch tip (`c56c28dd1`, the last product commit of Stages 3–4):

| Scope | Production LOC | Files |
| --- | ---: | ---: |
| C1 `layerfs-content` | 4 463 | 29 |
| C2 `layerfs-storage` including its 48 SQL LOC | 5 863 | 39 |
| telemetry | 732 | 7 |
| **core (three packages)** | **11 058** | **75** |
| reference `crates/` (unchanged coexistence) | 65 417 | 193 |
| **combined product** | **76 475** | **268** |

For comparison, the same counter over the `aa4b5a9e4` snapshot this section was
originally written against:

| Scope | Production LOC | Files |
| --- | ---: | ---: |
| C1 `layerfs-content` | 4 388 | 29 |
| C2 `layerfs-storage` including its 48 SQL LOC | 5 863 | 39 |
| telemetry | 732 | 7 |
| **core (three packages)** | **10 983** | **75** |
| reference `crates/` (unchanged coexistence) | 65 417 | 193 |
| **combined product** | **76 400** | **268** |

The reference subtotal is quoted with the corrected counter, which is the point of
the W9.5 correction: the counter's uncorrected rule removed 956 shipped reference
lines and counted 4 083 lines of test-only modules that live under `src/`. Both
are fixed and both are covered by a focused tool test. The core subtotal is
unchanged by the correction, because `core/crates/*/src` contains no `cfg`
attribute at all.

### 9.3 Files that are not plan rows

The plan has 75 rows; 67 exist as planned, 8 were merged rather than created
(`file/mapping/predecessor.rs`, `file/edit/frontier.rs`, `encoding/codec/{mod,
profile,encode,decode}.rs`, `pack/read.rs`, `pack/singleton.rs`), and **one** file
exists without a plan row: `file/edit/tree.rs`.

**Correction (2026-09-17, C8).** The reviewed text listed eight files "without a
plan row". Six of them **have** plan rows — `object/inode_leaf.rs`,
`file/edit/split.rs`, `file/edit/concat.rs`, `file/edit/finish.rs`,
`sqlite/pool.rs` and `pack/layout.rs` (`stages-3-4-file-plan.md:100,118,119,120,221,232`)
— and `encoding/pool/*` has plan rows at `:214–219`, which the reviewed sentence
half-conceded in a parenthesis. That list was therefore not a list of unplanned
files but of files whose row it had failed to match. `file/edit/tree.rs` is the
only genuine unplanned Stage 3–4 addition, and the independent review reached the
same conclusion (F-21(b)). The unplanned addition is in the table above with
`Unplanned`; the review's §5.2 assessed it and `object/inode_leaf.rs` as the two
with real weight, and neither is a thin wrapper.
