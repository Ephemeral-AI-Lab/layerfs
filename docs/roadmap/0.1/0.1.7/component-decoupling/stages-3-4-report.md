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
| Schema | `application_id = 1279677261`, `user_version = 3`; v1/v2 rejected, never migrated |
| Tables/columns | four tables, twenty columns (unchanged); role range widened to 1…6 for the new `InodeLeaf` role |
| Pack framings | v1 ordinary, v2 native, v4 compact whole-file, v6 pooled metadata, v7 singleton; v3/v5/unknown rejected |
| Pack bounds | 256 KiB (v1/v2/v4/v6), `16 MiB + 4096` (v7, one group) |
| Group body bound | 64 KiB (v1/v2/v4), 16 KiB (v6), singleton pack limit (v7) |
| Payload codec | unchanged pinned Zstandard parameters; whole-file window log 18 at/below the 256 KiB cutoff, 20 above; frame bound `max(135168, raw + raw/128 + 1024)` |
| Chain budgets | 512 KiB canonical, 256 KiB encoded, independent of depth |
| Read ceiling | the publication watermark, applied to every dependency and cache read |

`objects.object_role = 6` is reserved for the pooled inode leaf the metadata lane
will use; no such object can be produced yet, so the value is a declared format
capability rather than a live path.

## 3. Actual file tree and LOC

Produced with `python3 tools/production_loc.py` (same counter and classification
for both snapshots; before = commit `c38961f2f` tree materialized with
`git archive HEAD`, after = this working tree).

```text
Production LOC: 74628 -> 77136 (delta +2508)
  core       6152 -> 8660   (layerfs-content 2336 -> 3572, layerfs-storage 3084 -> 4356,
                             layerfs-telemetry 732 -> 732)
  reference 68476 -> 68476  (unchanged)
```

| Directory | Before | Recommended | After |
| --- | ---: | ---: | ---: |
| `core/crates/layerfs-content/src/` | 2 336 | 3 632–5 522 | 3 572 |
| `core/crates/layerfs-content/src/file/` | 1 761 | 2 789–4 179 | 2 872 |
| `core/crates/layerfs-content/src/file/edit/` (new) | 0 | 968–1 646 | 897 |
| `core/crates/layerfs-content/src/file/mapping/` | 946 | 978–1 460 | 1 011 |
| `core/crates/layerfs-content/src/object/` | 337 | 555–868 | 421 |
| `core/crates/layerfs-storage/src/` | 3 038 | 5 008–8 578 | 4 310 |
| `core/crates/layerfs-storage/src/encoding/` | 591 | 2 096–3 602 | 1 600 |
| `core/crates/layerfs-storage/src/encoding/delta/` (new) | 0 | 538–956 | 749 |
| `core/crates/layerfs-storage/src/cas/` | 994 | 1 074–1 874 | 1 106 |
| `core/crates/layerfs-storage/src/pack/` | 649 | 794–1 344 | 742 |
| `core/crates/layerfs-storage/src/sqlite/` | 540 | 730–1 230 | 563 |
| `core/crates/layerfs-storage/sql/` | 44 | 80–130 | 46 |

The total lands **below** the recommended range, because checkpoint E (metadata
pooling: `object/inode_leaf.rs`, `file/mapping/predecessor.rs`,
`encoding/pool/*`, `sqlite/pool.rs`, `pack/read.rs`, `pack/singleton.rs`) was not
implemented and because `pack/read.rs` was again merged into
`encoding/decode.rs` (record extraction and reconstruction are one operation).
No file was split or padded to reach an estimate; every production file is under
the 999-line ceiling and every `lib.rs`/`mod.rs` is under 200 lines.

New production files: `file/view.rs`, `file/edit/{mod,input,compare,frontier,split,concat,finish,apply}.rs`,
`object/predecessor.rs`, `encoding/delta/{mod,record,select,read,candidates}.rs`,
`encoding/pool/mod.rs` (empty declaration module, see §7).

## 4. Commands and results

All commands were run from the repository root.

```sh
python3 core/tools/check_product_boundary.py                      # PASS: 68 production files scanned
python3 -m unittest discover -s core/tools -p 'test_*.py'         # PASS: 5 tests
python3 -m unittest discover -s tools -p 'test_production_loc.py' # PASS: 13 tests
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check   # PASS
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked  # PASS
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings  # PASS
git diff --check                                                  # PASS (no whitespace errors)
```

Targeted suites (each command run separately, actual counts):

| Target | Tests | Target | Tests |
| --- | ---: | --- | ---: |
| `edit_batch` | 7 | `delta_payload` | 10 |
| `edit_bounds` | 8 | `delta_chains` | 8 |
| `edit_noop` | 7 | `policy_capacity` | 6 |
| `edit_single` | 9 | `physical_formats` | 5 |
| `edit_timing` | 4 | `edit_pipeline` | 4 |
| `edit_transitions` | 5 | `cas_reuse` | 8 |
| `file_complete` | 14 | `cas_roundtrip` | 6 |
| `file_read` | 8 | `core_pipeline` | 5 |
| `object_identity` | 9 | `memory_bounds` | 7 |
| `streaming` | 8 | `pack_locator` | 7 |
| `timing` (C1) | 7 | `persistence_failure` | 7 |
| | | `timing` (C2) | 4 |
| | | `visibility` | 5 |

C1 86 tests, C2 82 tests, telemetry unchanged. No target is empty and no test was
disabled. The three smoke commands and their observed values are in the evidence
README; `measure_edits` also has the `batch` and `shrink` cases.

`tools/preflight.sh` was **not** run: it is permanently retired (ledger L32), and
no aggregate wrapper was substituted.

## 5. #168 criterion table

| Criterion | Code | Test | Evidence |
| --- | --- | --- | --- |
| Configurable cutoff with checked ranges | `content/policy.rs`, `storage/policy.rs` | `edit_transitions::exact_boundaries_at_every_accepted_cutoff`, `policy_capacity::*` | accepted 128 KiB/256 KiB/1 MiB end-to-end; unsupported values rejected before creation |
| Depths configurable, `0` disables delta, increased value reaches its boundary | `policy.rs`, `encoding/delta/select.rs` | `delta_payload::depth_zero_disables_prospective_delta_entirely`, `delta_chains::an_increased_depth_is_honoured_up_to_its_boundary` | 16-edge chain stored and read at depth 16; boundary refusal at depth 2 |
| Chain-work ceilings independent of depth | `encoding/delta/read.rs`, `select.rs` | `delta_chains::a_chain_that_would_exceed_its_work_budget_is_never_stored` | a dependent that could not be reconstructed is stored FULL instead |
| WHOLE_FILE explicit predecessor and cache rule | `delta/select.rs`, `delta/candidates.rs` | `delta_payload::{an_explicit_predecessor_*, the_admitted_full_cache_*}` | PREFIX stored and read back; cache hit inside one save |
| CHUNK first-supplied-candidate rule | `delta/select.rs` | `delta_payload::native_delta_uses_the_first_supplied_eligible_candidate_only` | two candidates, one trial |
| Complete-cost comparison including base identity | `delta/record.rs`, `full.rs` | `delta_payload::an_unrelated_candidate_loses_the_cost_comparison` | FULL selected by policy, not by failure |
| Exact reuse before any trial | `cas/save.rs`, `membership.rs` | `delta_payload::exact_reuse_is_decided_before_any_trial` | `trials == 0` on a duplicate |
| Failed acquisition never becomes FULL | `delta/select.rs`, `cas/owner.rs` | `delta_payload::a_corrupt_base_fails_the_save_instead_of_selecting_full` | corrupt base pack → save fails |
| Iterative chain reconstruction with authentication | `delta/read.rs` | `delta_chains::{a_corrupt_intermediate_*, a_cyclic_dependency_*, a_wrong_role_dependency_*}` | corrupt/missing/cyclic/wrong-role dependencies each rejected |
| Explicit format matrix, no decoder probing | `pack/layout.rs` | `physical_formats::*` | v1/v2/v4/v6/v7 recognized; v0/v3/v5/v8/v99 rejected |
| Larger incompressible whole-file singleton at 1 MiB | `encoding/full.rs`, `pack/assemble.rs` | `policy_capacity::a_larger_incompressible_whole_file_record_uses_the_singleton_lane` | 1 048 575-byte record stored, read back, one database file on disk |
| **Metadata pooling (values, ordinals, pooled FULL/DELTA, bounded index)** | — | — | **NOT IMPLEMENTED** |

## 6. #169 criterion table

| Criterion | Code | Test | Evidence |
| --- | --- | --- | --- |
| Ordered normalized edits in current-result coordinates | `file/edit/input.rs` | `edit_batch::several_separated_edits_apply_in_order`, `many_small_edits_are_applied_in_one_pass` | 200 edits in one pass equal the independent model and a fresh construction |
| Stable replacement ranges, overlap/inapplicable rejection | `input.rs` | `edit_batch::an_overlapping_stream_is_rejected_before_any_work`, `edit_single::an_inapplicable_range_is_rejected_before_any_work` | explicit `InvalidEdit` labels |
| Single edits: overwrite/insert/delete/append, head/middle/tail | `edit/apply.rs` | `edit_single::*` | root equals a fresh construction |
| Complete deletion and empty stream | `edit/finish.rs` | `edit_single::complete_deletion_returns_the_empty_representation`, `edit_timing` | defined empty form |
| Transitions at every accepted cutoff | `edit/apply.rs`, `policy.rs` | `edit_transitions::every_conversion_direction_reaches_the_fresh_construction_root` | 0/1/T-1/T/T+1 at 128 KiB, 256 KiB, 1 MiB; small→large, large→small, any→empty |
| No-op preservation and bounded compare + replay | `edit/compare.rs` | `edit_noop::*` | equal replacement returns the base root; shifted coordinates handled; long equal prefix then mismatch constructs |
| Occupancy 63/64/127/128/129 and streaming 192/193 | `mapping/build.rs` | `edit_bounds::{join_occupancy_at_every_boundary, streaming_flush_boundaries_and_height_growth}` | every non-root page holds 64…128 entries after the edit |
| 80+100 join stays canonical | `mapping/build.rs` | `edit_bounds::a_join_of_two_full_pages_stays_canonical` | joined mapping and both edits pass the page validator |
| Bounded frontier, many files/edits, late failures | `edit/frontier.rs` | `edit_bounds::{many_files_and_many_edits_stay_bounded, the_retained_frontier_does_not_grow_with_the_file, a_source_that_ends_early_*, a_consumer_that_rejects_*}` | bounded retained entries, one error returned once |
| Real DB-free edit timing with enabled/disabled equivalence | `edit_timing.rs` | `edit_timing::*` | same reads and result with recording on and off |
| Real edit → C2 → reopen → readback | `cas/*`, `edit_pipeline.rs` | `edit_pipeline::*` | exact bytes after reopen; the edit stores far fewer objects than the file holds |
| **Owned decoded frontier with stored-node split/concat reuse and the finality proof** | `edit/frontier.rs` (partial) | — | **PARTIAL: no stored-node reuse, no reference-root equivalence for large → large** |

## 7. Unmet criteria and honest gaps

1. **#168 metadata pooling is not implemented.** Nothing consumes
   `metadata_value_groups`; there is no inode-leaf grammar, no value group
   builder/digest, no ordinal assignment, no pooled FULL/COPY-INSERT record, no
   pooled reader and no bounded `BTreeSet<(fingerprint, ordinal)>` index. The
   `encoding/pool/mod.rs` module is a declaration-only module with no
   implementation. The candidate never had the reference's disposable SQL
   fingerprint index, so there was nothing to replace with the bounded ordered set;
   the ordered set itself is simply absent. Consequently **#168 is incomplete** and
   no evidence exists for the pooling families.
2. **#169 stored-node split/concat reuse is not implemented.** The edit frontier is
   one owned decoded structure and the resulting partition is canonical, but the
   mapping is re-derived from the retained extent sequence instead of mutating the
   stored tree. Unchanged mapping pages are re-encoded; unchanged *chunk payloads*
   are retained and referenced, so no payload is re-CDC'd and no old content is
   rewritten. A large → large edit therefore does not reproduce a reference
   (v0.1.6) root in general, and no such equivalence is claimed. **#169 is
   incomplete.**
3. **The 80+100 → 90+90 repartition proof is not established.** The
   `edit_bounds::a_join_of_two_full_pages_stays_canonical` test proves the joined
   and edited mappings satisfy the canonical page partition, but it does not prove
   the reference's specific repartition, because the reference algorithm was not
   ported.
4. **Same-save delta candidates are ineligible.** A base that this save accepted
   but has not stored yet is not read for a trial: sealing a group for an optional
   optimisation would cost packing granularity. The object is stored FULL by
   policy. Cross-operation (stored) bases and explicit predecessors are used.
5. **No comparison campaign.** No matched v0.1.6 arm, cache contract or registered
   performance campaign was collected for these stages; the smoke runs are wiring
   evidence only. No speedup, storage-reduction or memory-reduction claim is made.
6. **Memory evidence is declared-capacity evidence.** Live capacities and bounded
   caches are asserted by tests; no heap/RSS attribution was collected, and the
   selected SQLite cache size is not presented as a memory cap.
7. **The admitted-FULL cache is a heuristic.** Its min-hash sketch can miss a
   genuinely similar candidate (bounded candidate loss, never a correctness risk);
   `delta_payload::the_admitted_full_cache_supplies_a_candidate_within_one_save`
   documents the content shape it works for.

## 8. What was removed, retained, relocated, deferred

* **Removed in effect (not by deletion):** the frozen single-value policy check;
  the "DELTA is not implemented" limitation; the assumption that one pack limit
  covers every record.
* **Retained unchanged:** the publication watermark and its regression tests, the
  pending-group same-save read path, the exact CAS comparison, the frozen CDC
  profile and mapping grammars, MEMORY journal / synchronous OFF / zero busy
  timeout, and the 999/200-line and product-purity rules.
* **Relocated:** the reference's `encoding/codec.rs` responsibilities stayed in one
  file (profile + two workspaces) instead of splitting into `codec/`; record
  extraction and reconstruction stayed merged in `encoding/decode.rs` rather than
  a new `pack/read.rs`.
* **Deferred:** everything in §7, plus filesystem-tree construction, Workspace COW,
  FUSE, host/daemon transport and cloud support (Stages 5–7, still open).
* **Reference source:** untouched; no dependency, include, fallback binary or
  alternate runtime path points at `crates/`.
