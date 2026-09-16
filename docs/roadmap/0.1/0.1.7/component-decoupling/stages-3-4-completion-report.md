# Stages 3-4 completion report: pooling, stored-tree edits and qualification

> **Status:** additive completion report for
> [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) and
> [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169), continuing
> [stages-3-4-report.md](stages-3-4-report.md). Read with the
> [completion handoff](stages-3-4-completion-handoff.md) and the
> [verification contract](stages-3-4-verification.md).

Start state: `24ef187d4` (the Stage 3-4 implementation commit series), working tree
clean except the owner's uncommitted continuation documents. Reference identity is
the pinned v0.1.6 source `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`;
`git diff 44cf748 -- crates/layerfs-content` is empty, so the oracle below runs the
sealed reference, not a later revision.

| Assignment | State |
| --- | --- |
| E - pooled physical metadata | **Implemented**; three required test targets pass |
| D - stored-tree split/concat and finality | **Incomplete**; the sealed oracle exists, the base is proven equivalent, and six exact counterexamples are recorded |
| Qualification | **Not started**; the versioned addendum is required before collection and no campaign was run |

Neither issue is closed. #168 still misses its remaining policy/storage/performance
criteria, and #169's reference-root requirement is unmet.

## 1. Assignment E: pooled physical metadata

### 1.1 What was implemented

| Step | Implementation |
| --- | --- |
| Checked canonical input grammar | `core/crates/layerfs-content/src/object/inode_leaf.rs`: the 73-byte inode value, the 31-byte page header, the 44-byte canonical prefix, the pooled physical layout, the pooled value object, and the derived physical length |
| Value groups | `encoding/pool/value_group.rs` builds the exact group body once (ordinal-ordered FULL records), hashes it **before** compression, and stores the compressed frame only when the retained rule says it is smaller |
| Pooled records | `encoding/pool/leaf.rs`: tag 0 (physical body) and tag 1 (base, output length, instruction count, COPY/INSERT program) - exactly the reference tags, read under the `InodeLeaf` role |
| COPY/INSERT delta | `encoding/pool/delta.rs`: the reference producer with its fixed 4096-bucket/four-slot seed table over sixteen-byte seeds, a bounded match budget, the instruction grammar and its validator |
| Bounded ordered set | `encoding/pool/index.rs`: `BTreeSet<(fingerprint, ordinal)>`, at most 131,072 retained entries, whole-window reset, catalogue replay on a cold start, invalidation on any failed save; the fingerprint only filters candidates and full value bytes decide |
| Reader | `encoding/pool/read.rs`: rebuilds the pooled body from its delta chain, authenticates each value group against its catalogue digest, resolves ordinals and rebuilds the canonical leaf; the same captured ceiling applies to value-group packs |
| Catalogue | `sqlite/pool.rs`: `next_ordinal`, `group_for`, `catalogue`, `insert_group`, all bounded queries |
| Persistence integration | ordinals are assigned in first-encounter order, groups are placed in the v6 lane, their catalogue rows share the save's transaction with the leaf row, cleanup cascades through `object_packs`, and `store_policy.metadata_delta_max_depth` (schema `user_version` 4, twenty-one columns) gives pooling its own persisted depth |

### 1.2 Evidence

| Required case | Test | Result |
| --- | --- | --- |
| Supplied leaf -> real SQLite -> authenticated reopen, exact bytes and ID | `metadata_pool::a_supplied_leaf_round_trips_through_sqlite_and_reopen` | PASS |
| Repeated values inside and across leaves and saves use the same ordinals | `metadata_pool::repeated_values_reuse_the_same_ordinals_across_leaves_and_saves`, `metadata_pool_index::a_shared_value_keeps_the_smallest_ordinal_across_leaves_and_reopen` | PASS (4 reused / 4 new; later saves 0 new) |
| 165-value group boundary | `metadata_pool::values_cross_a_group_boundary_at_one_hundred_sixty_five` | PASS (two leaves of 100 values, one group each, third leaf reads back) |
| Pooled FULL and winning/losing COPY/INSERT | `metadata_pool::{a_losing_delta_trial_stores_the_leaf_in_full, a_pooled_leaf_survives_a_store_reopen_with_its_chain}` | PASS (four-leaf delta chain, plus a cost loss) |
| Bad digest / missing catalogue / private group visibility | `metadata_pool::{a_corrupt_value_group_is_rejected_once, a_missing_catalogue_row_is_rejected, an_unfinished_private_value_group_is_not_visible_to_a_reader}` | PASS |
| Failure invalidation and cleanup | `metadata_pool_index::{a_failed_save_invalidates_the_set_instead_of_reusing_phantom_ordinals, a_failed_save_removes_its_private_value_groups}` | PASS; retained content stays readable |
| Own persisted depth, rejection above the range | `policy_capacity::the_pooled_metadata_depth_is_persisted_and_checked_separately` | PASS (12 and 50 accepted, 51 rejected before any file exists) |
| Grammar rejection matrix | `inode_leaf::*` (6 tests) | PASS |
| Bounded retained set | `metadata_pool_index::the_retained_set_is_bounded_by_entries_not_by_file_size` | PASS (1,000 retained entries for ten leaves) |

### 1.3 Gaps in E

* **The 131,072-entry window boundary and its cold-start replay are not exercised.**
  Reaching them needs 131,072 stored values (about 794 groups, 1,310 leaves). The
  replay rule is implemented (catalogue-only, no payload read) and is reported as a
  coverage gap, not a verified boundary.
* **Fingerprint-collision coverage is unreachable externally**: the reference
  fixture for it is not available to a candidate test, so equality is only proven
  through the full-value comparison path plus ordinary reuse cases.
* **Pooled depth above 12 is untested with a real chain**; the boundary at 50 is
  accepted and persisted only.
* No performance, storage or memory measurement was collected for pooling.

## 2. Assignment D: stored-tree edits

### 2.1 The oracle is established

`crates/layerfs-content/examples/rope_edit_oracle.rs` (a reference-tree **oracle
input**, not product code and not a candidate dependency) builds a base with the
reference constructor, applies the reference's `FileMutationBatch`, and prints the
resulting root, logical length and decoded page partition as JSON. Fixtures for six
cases are committed under
[`evidence/docs/roadmap/0.1/0.1.7/evidence/stages-3-4-oracle-20260916T214846Z`](../evidence/docs/roadmap/0.1/0.1.7/evidence/stages-3-4-oracle-20260916T214846Z/README.md).

`core/crates/layerfs-content/tests/edit_reference.rs` builds the same base with the
candidate constructor, applies the same edit through `apply_edits`, and compares the
root, the partition and the surviving leaf identities.

**The candidate's base construction is reference-equivalent in all six cases**
(`base_root == oracle base_root`), which isolates the gap to the edit itself.

### 2.2 Counterexamples (this target fails today, by design)

| Case | Oracle root / partition | Candidate root / partition | Surviving leaves |
| --- | --- | --- | --- |
| `join-80-100` | `333d5497...`, `2 / 96 / 97` | `523276d8...`, `2 / 128 / 65` | none / none |
| `untouched-sibling` | `c60cf658...`, `4 / 73 / 74 / 69 / 86` | `f122073c...`, `3 / 128 / 87 / 87` | oracle keeps `c5a59b6c`; candidate keeps **none** |
| `interior-multi-level` | `57e0a51c...`, `4 / 102 / 103 / 98 / 99` | `7a61fffd...`, `4 / 128 / 128 / 73 / 73` | both keep one prefix leaf (`fc8b3286`) because the flush alignment coincides |
| `height-growth` | `ae823b68...`, `4 / 128 / 72 / 78 / 79` | `2f1439fb...`, `3 / 128 / 128 / 101` | both keep `fc8b3286` |
| `root-collapse` | `ba1ba757...`, `21` | `ba1ba757...`, `21` | **exact match** |
| `batch-normalized` | `793895c5...`, `51` | `c4204dad...`, `51` | none / none; the same entry count with **different leaf bytes**, so the update stream itself differs, not only the tree shape |

Raw comparison output:
[`candidate-comparison.txt`](../evidence/docs/roadmap/0.1/0.1.7/evidence/stages-3-4-oracle-20260916T214846Z/candidate-comparison.txt).

The `untouched-sibling` row is the core gap: the reference leaves an untouched leaf in
place with its identity, while the candidate re-derives the mapping from the retained
extent sequence and emits a fresh node. `batch-normalized` shows a second,
independent difference in the *extent* stream for an update with a deletion, which
must be resolved before any tree work can match.

### 2.3 Defect found and fixed while establishing the oracle

The oracle immediately exposed a real candidate bug: **appending to a chunked file
failed** with `InvalidEdit { what: "unreached segment" }`, because the extent walk
stops exactly at the end of the last extent and never reaches a replacement whose
base range is the empty range at end of file. `file/edit/apply.rs` now applies a
trailing pure insertion and rejects any other unreached segment. This is why the
oracle was worth building before the tree work: the small-file append path passed,
the chunked one did not.

### 2.4 Remaining D work and the next concrete action

1. **Resolve the `batch-normalized` extent difference first.** Until the update
   stream produces the reference's extent sequence, no tree-shape comparison can
   pass. Next action: dump the reference's and the candidate's extent list
   (chunk id, source offset, length) for that fixture and compare element by
   element; the likely difference is whether a replacement run is chunked
   independently or across the retained boundary.
2. **Introduce the two-state representation** (`StoredSubtree { id, summary }` versus
   owned decoded entries with checkpointed summaries) and port the reference's
   split/concat/coalesce, half partition, root context and collapse rules from
   `crates/layerfs-content/src/file/rope/edit.rs` (904 lines), reading `state.rs`,
   `build.rs` and the extent codec completely first.
3. **Prove finality per emission rule** and bound the live state over supported
   height/fanout, both boundary paths, builder levels, pending output and
   authenticated reads, then replace the large->large mapping walk entirely - no
   parallel mode, no error fallback.
4. Re-run `edit_reference` until all six cases match, then the existing edit
   targets, then the node-work instrumentation gate.

## 3. Qualification

Not started, and deliberately so: the completion handoff requires the reference
exactness gate before qualification. The committed
[verification contract](stages-3-4-verification.md) and its continuation section
already state the missing families; a versioned addendum with exact pooling and
stored-tree cases, cache/index state, identities, numerical limits and
acknowledgement boundaries must be committed **before** any collection. No
measurement was run for this continuation, so no number here is a performance claim,
and the earlier smoke receipts keep their stated meaning.

## 4. Commands and production LOC

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test edit_reference
cargo +1.85.1 run  --locked -p layerfs-content --example rope_edit_oracle -- --case join-80-100
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
```

| Target | Tests | Result |
| --- | ---: | --- |
| `inode_leaf` | 6 | PASS |
| `metadata_pool` | 9 | PASS |
| `metadata_pool_index` | 5 | PASS |
| `policy_capacity` (extended) | 7 | PASS |
| `edit_model` (required target, added here) | 6 | PASS |
| `edit_reference` | 2 | **FAIL: six recorded counterexamples** |
| every other existing target | unchanged | PASS |

Per-commit production LOC. Every snapshot below was re-measured from the committed
tree with `git archive <rev>` and `python3 tools/production_loc.py --root <tree>
--detail`, so the numbers are the audited classification for the actual commits:

```text
24ef187d4 -> da8ee5769   E, pooled metadata
  core       8660 -> 10399   (delta +1739)
    layerfs-content  3572 -> 3891   (+319)
    layerfs-storage  4356 -> 5776  (+1420)
    layerfs-telemetry 732 -> 732
  reference 68476 -> 68476; combined 77136 -> 78875 (delta +1739)

da8ee5769 -> b49931570   D step 1, oracle and the end-of-file append fix
  core      10399 -> 10415   (delta +16; layerfs-content 3891 -> 3907 only)
  reference 68476 -> 68476; combined 78875 -> 78891 (delta +16)
  The oracle example, the fixtures and the failing external target are test/oracle/
  documentation scope and are not counted.

b49931570 -> 315a339fa   the required edit_model target
  core      10415 -> 10415   (delta 0; test-only)
  combined  78891 -> 78891   (delta 0)

Current committed tree: core 10415 across 75 production files
  layerfs-content 3907 (29 files), layerfs-storage 5776 (38 files, of which
  runtime SQL 48 in one file), layerfs-telemetry 732 (7 files).
  Recursive scopes: content/src/file/edit 913, content/src/file/mapping 1011,
  content/src/object 739, storage/src/encoding 2560 (pool 926, delta 768),
  storage/src/cas 1401, storage/src/sqlite 702, storage/src/pack 739.
```

**Correction to an earlier commit message.** The commit message of `da8ee5769`
reported the per-package subtotals as `layerfs-content 3572 -> 3949` and
`layerfs-storage 4356 -> 5718`. Re-measuring that committed tree gives
`3891` and `5776`. The core total (`8660 -> 10399`) and the combined total
(`77136 -> 78875`) in that message are correct; only the two per-package subtotals
were transcribed from a filtered working-tree listing. The audited values are the
ones above, and they are what any comparison should use.

## 5. Continuation

Exact source identity for a resumed run: this commit's tree; the oracle fixtures
under `docs/roadmap/0.1/0.1.7/evidence/docs/roadmap/0.1/0.1.7/evidence/stages-3-4-oracle-20260916T214846Z/`; the failing target
`edit_reference`; the reference revision `44cf748...`. The first concrete action is
section 2.4.1, then the two-state representation of section 2.4.2. Nothing in this
continuation removes an acceptance criterion, and no issue may be closed on the
strength of this report.
