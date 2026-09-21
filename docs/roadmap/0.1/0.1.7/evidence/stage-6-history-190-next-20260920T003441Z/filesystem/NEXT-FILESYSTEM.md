# Next filesystem attribution and bounded optimization

Status: source investigation and implementation plan only. No new measurement,
build, product change, harness change, or Git mutation was performed by this
squad. Ownership is this document. Source was read from the working tree at
HEAD `10b9d4a6cf9d88267d508cb010cc82950e080d77`, including the preceding
parent-lookup change awaiting the coordinator's commit. Links below refer to
that working-tree layout; line references record the inspected version.

## Evidence that can be reproduced now

The archived candidate timing trees from
[`stage-6-history-190-opt-20260919T232858Z`](../../stage-6-history-190-opt-20260919T232858Z/README.md)
give the following exact sums. These are existing diagnostics, not new runs or
release claims.

| Recorded interval | Stride10 ns | Stride3 ns |
| --- | ---: | ---: |
| `validate` | 3,505,397,540 | 17,489,653,664 |
| `directories` | 1,633,180,084 | 4,958,796,085 |
| `references` | 742,543 | 2,171,293 |
| `inodes` | 4,482,754,167 | 15,739,979,668 |
| `cleanup` | 14,752 | 44,296 |
| `root.encode` | 30,666 | 70,086 |
| Filesystem parent minus these children | **5,210,486,416** | **15,511,671,619** |
| Complete filesystem interval | **14,832,606,168** | **53,702,386,711** |

The residual is an exact subtraction, not a measured duration assigned to any
one function. Its mechanisms cannot yet be ranked by time.

Reproduction, from the repository root, reads the original timing files:

```python
import json
from pathlib import Path

root = Path("docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-opt-20260919T232858Z/runs")
def nodes(node):
    return 1 + sum(nodes(child) for child in node["children"])

for tier in ("stride10", "stride3"):
    report = json.loads((root / f"candidate-history-{tier}/raw/timing.json").read_text())
    totals, residual = {}, 0
    for state in report["children"]:
        for node in state["children"]:
            if node["name"] == "filesystem":
                residual += node["elapsed_ns"] - sum(c["elapsed_ns"] for c in node["children"])
                for child in node["children"]:
                    totals[child["name"]] = totals.get(child["name"], 0) + child["elapsed_ns"]
    print(tier, nodes(report), residual, totals)
```

This also yields **257** nodes for stride10 and **797** for stride3 in the
archived candidate. The coordinator's **850**-node planning envelope is
conservative, not the node count in those archived files. Adding three scopes
per state gives at most `850 + 3 * 53 = 1009 < 1024`. Against the actual archived
tree it gives at most `797 + 159 = 956`. Keep the conservative envelope; do not
raise the timer cap or permit detailed stride1.

## What is outside the existing spans

Paths below are relative to the repository root.

| Source location | Unspanned work | Relevant bound or qualification |
| --- | --- | --- |
| `core/crates/layerfs-content/src/filesystem/update.rs:160` | Collect unreachable newly allocated parents. | Scans caller changes; not a traversal of the entire saved filesystem. |
| `update.rs:172`–179 | Construct/check reducer and register supplied values/new identities. | Existing pending, spill and ordering quotas apply. |
| `update.rs:308`–345 | Overlay rebuilt directory roots onto typed inode values; batch-read values absent from input and final retained parent window. | `batch = min(base_read_batch, MAXIMUM_READ_DEMANDS)` at line 180. |
| `update.rs:348`–359 | Select other supplied inode values and insert them into the reducer. | Preserve existing all-effects-before-values insertion order; the previous experiment showed quota regressions when changing this order. |
| `update.rs:363`–371 | Find initially zero-count serials. | `zero_count_serials` at line 505 consolidates/visits all touched reducer rows, checks the touched-set limit, then batch-reads their base records. |
| `update.rs:373`–385 | Release descendants of those serials. | `release_zero_count` traverses only released directories, with bounded listing pages and child lookup waves. It is not an unconditional full-tree scan. |
| `update.rs:407`–418 and 420–423 | Propagate errors, snapshot counters, close final rows, construct root value. | Keep as explicit residual rather than claiming zero cost. |

The touched-row pass already carries `PendingState` out of the consolidated
visit, avoiding a subsequent `state()` lookup for every row:
[`references/reduce.rs`](../../../../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs)
lines 168–207. Reimplementing that optimization would do nothing.

## Minimal production telemetry plan

Reuse the public, real-product
[`FilesystemPhases`](../../../../../../../core/crates/layerfs-content/src/filesystem/objects.rs)
(lines 127–164). Disabled phases execute the same body without reading a clock.
No benchmark flag, test-only branch, alternative algorithm, new telemetry
abstraction, per-inode timer, or cap change is needed.

Add exactly three coarse scopes in `run_body`:

1. **`metadata.overlay`**: wrap the complete block at lines 308–359 in
   `phases.phase(..., || -> ContentResult<()> { ...; Ok(()) })?`. Preserve all
   loop bodies and insertion order; the closure returns no new collection.
2. **`references.zero_count`**: wrap the existing `zero_count_serials(...)`
   call and return its existing tuple. Keep the counter update outside or
   inside consistently; name the exact boundary in the report.
3. **`references.release`**: wrap the existing `release_zero_count(...)`
   call and assign its existing `ReleaseWork`. Keep both scopes inside the
   existing `if checked.topology.table.is_some()` branch; the initial state
   legitimately has no zero-count/release scope.

Report the new spans and the **remaining** residual separately. These wrappers
do not include the setup/registration or postprocessing noted above. There is
no justification for forcing a zero residual by relabeling it as useful work.

The existing `FilesystemUpdateCounters` has `base_records_read` for the initial
zero-count pass and `release: ReleaseWork`. The current harness trace exports
reference-stream counters but does not expose all those release counters.
Export the already-public counters in the harness before proposing additional
product counters: zero-count base records; release pages, entries, base records,
released bindings, traversed-directory counter, and peak cursor depth. Preserve
their source meanings; for example `ReleaseWork.traversed_directories` is
incremented on the release loop's page-processing path (release.rs:183), so it
must not be relabeled as distinct directory identities without auditing it.

## Why `inodes` is not a pure tree-merge interval

```text
references scope
    reducer.finish()
      consolidate runs
      create FinalRows iterator
      initialize first pending/run row
    return iterator                        <-- references scope ends

inodes scope
    apply_inode_values(..., changes iterator)
       demand next change
          FinalRows.next_change()
             fill_wave() when necessary
                merge newest pending/run rows
                lookup_many(effect serials) <-- base reads happen here
                overlay authenticated base count
             finish_row()
       merge changes into inode tree
    return tree                            <-- inodes scope ends
```

Source: `update.rs:387`–405;
[`references/reduce.rs`](../../../../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs)
lines 270–291 (`finish`), 388–401 (`next_change`) and 404–498 (`fill_wave`).
Therefore the recorded **4,482,754,167 / 15,739,979,668 ns** cannot be attributed
solely to inode-page merging. Nor does the tiny `references` interval prove
reference processing is cheap.

Do not wrap `next_change` or each `fill_wave` with `FilesystemPhases.phase`:
each call allocates a timing node and would violate the bounded report shape.
Do not collect all final rows first to create a convenient timing boundary:
that changes memory use, streaming, error order and measured work. For the first
pass, label the interval **inode merge plus lazy reference reduction** and use
existing `ReferenceWork.base_waves/base_records_read`, run-read counters and
inode-page counters. If these identify lazy reduction as the next material
unknown, design a bounded aggregate production timing counter in a separate
review; the current three-scope plan intentionally does not pretend to split it.

## Candidates and the first falsifiers

1. **Measure zero-count and release before editing them.** The independent
   check is the three-span run above, with unchanged roots, bytes, errors and
   work counters. If both measured intervals are small relative to the residual,
   they cannot explain most of that residual. This directly falsifies a costly
   release-walk explanation without changing an algorithm.

2. **Avoid base demands for newly allocated rows in zero-count evaluation.**
   At `update.rs:527`–540, the code looks up every touched serial, but
   `PendingState::New` uses only its carried count and ignores `base`.
   `FinalRows.fill_wave` already avoids such demands by selecting only
   `Row::Effect` (reduce.rs:443–465). This is a bounded filtering opportunity:
   keep the existing wave size, filter demands to existing rows, and consume
   returned bases only for those rows. No cross-operation cache is needed.
   However, skipped acquisitions can change which malformed base pages are
   observed. Prove the required validation/error contract before applying it;
   roots alone are insufficient. The first proof should include a mixed new /
   existing wave, deleted existing inode, absent existing record, root count,
   and malformed required page. Count avoided demands separately from time.
   Its expected impact on this history workload is **not measured**.

3. **Do not reread a zero-count record solely to seed release, if a bounded
   transfer can preserve semantics.** `zero_count_serials` obtains base records
   at update.rs:529, discards them, and `release_zero_count` rereads starting
   records at release.rs:72–79. This is a source-confirmed repeated demand.
   Yet retaining every zero-count record would enlarge live memory beyond the
   existing serial vector. A bounded handoff may help, but must account for
   changed release ordering, reducer mutations, aliasing and quota refusals.
   Do not implement an unbounded map or begin release before all initial counts
   are known. The first falsifier is a small `references.release` time or few
   starting records; either weakens this as the next priority.

4. **Avoid claiming release is currently unbatched.** Starting records and
   listed child records are already fetched with `lookup_many`
   (`release.rs:72`, `153`), and child records are retained before descending
   (`release.rs:173`–177). A singleton fallback still exists, but its frequency
   is not measured. Also, the prefetched map is populated from *all* starting
   waves before consumption (lines 71–80), so bounded acquisition waves alone
   do not establish a one-wave live-memory bound. Audit that separately before
   copying the pattern.

5. **Do not add broad cross-phase reuse yet.** Zero-count derivation and lazy
   FinalRows can read the same existing records, but releases mutate reducer
   effects between them. Authenticated base values are immutable for the pinned
   root; effective counts are not. Any reuse must distinguish those values,
   stay within an explicit existing budget and preserve refusal behavior. The
   current read counters cannot determine duplicate-record frequency across
   these phases, so no size-proportional cache is justified.

## Acceptance for the next implementation

- Preserve complete-command limits, one sample per case per arm, fixed workers,
  lock/quiet-machine declarations, append-only evidence, and cache eligibility.
  The campaign coordinator owns runs; this squad initiated none.
- First establish telemetry-only equivalence, including timing-disabled output,
  node completeness and the 53-state cap. Then use identical instrumentation in
  baseline/candidate if an algorithm changes.
- Prove roots, byte-identical Store where expected, and exact quota/error parity.
  Keep the prior 56-case quota matrix: the rejected earlier implementation
  demonstrated why changed insertion order is not harmless.
- Report changed work counters with named semantics and measured intervals;
  provider elapsed overlaps filesystem phases and must not be added to them.
- No third-party modification, changed format, subtree-summary design, worker
  increase, widened memory/timeout policy or timing-admission claim is authorized
  by this document. Product changes require core checks and architecture updates
  under `core/AGENTS.md`.

Production source delta for this squad: **0** (one document only). No commit was
created; this statement is not a substitute for the coordinator's exact staged
tree production-LOC comparison.
