# LayerFS 0.1.3

> **Status:** Draft filesystem-workload benchmark release; no new scenario is
> registered until its complete family source, fixture, runner, and evidence
> identities are frozen together.
>
> **Compatibility:** Preserve the released 0.1.x contract and rerun every
> completed v0.1.0-v0.1.2 family without adding members to it.

## Problem statement

v0.1.2 owns and completes the same-count and count-changing file-edit
performance families after implementing one shared edit engine. v0.1.3 must not
continue those families. It
instead completes the remaining payload, namespace, CAS/CDC, tiny-file,
directory, tool, link, and composed filesystem workloads at the simplest
LayerFS history topology.

The fixed topology separates filesystem-workload cost from repeated Commit
history and Branch fan-out, which remain owned by v0.1.4.

## Goal

Complete 42 timed and 5 proof-only v0.1.3 cases across exactly 8 indivisible
families. Every new operation curve executes nested prefixes of 1, 10, and 100
scheduled operations before one Commit unless its family declares a frozen
load-unit exception.

The accumulated workload registry through v0.1.3 contains:

```text
39 timed v0.1.2 edit-family cases
+ 42 timed v0.1.3 family cases
= 81 timed cases

12 v0.1.2 verifier/conformance groups
+ 5 v0.1.3 family proofs
= 17 proof-only cases
```

With the 12 frozen `fs-bench` controls, the complete release knows 110 workload
definitions. The separately accounted v0.1.2 Store-footprint controls do not
enter these counts or `registered_total_ns`.

## Files to read

- [0.1.x roadmap](../README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [v0.1.1 scope](../0.1.1/README.md)
- [v0.1.2 release scope](../0.1.2/README.md)
- [v0.1.2 `fs-bench-pro` family format](../0.1.2/fs-bench-pro-format.md)
- [v0.1.2 same-count family](../0.1.2/same-count-file-edits.md)
- [v0.1.2 count-changing family](../0.1.2/count-changing-file-edits.md)
- [v0.1.2 universal edit implementation](../0.1.2/universal-file-edit-engine.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Benchmark workload helper](../../../../benchmark/fs-bench-pro/workload.rs)

## Scope and exclusions

Each new case uses one Store and Client, one LayerStack and genesis Layer, one
Branch, one real-FUSE Workspace when a filesystem is projected, one fresh
workload process, one final unpromoted Commit, End, fresh reconnect, and exact
verification. Fixture, Store, Client, container, source sealing, and report
preparation remain outside timed regions.

v0.1.3 introduces no new same-count, prepend, append, truncate, middle
insert/delete, sparse-growth, unequal-replacement, or count-changing-family
members. It reruns those
complete v0.1.2 families unchanged in a separate inherited regression lane.

Inherited rows retain their exact lifecycle. They do not authorize new Commit
depth, multiple Branches, Branch fan-out, competing publication, conflict
resolution, or history-sensitive pagination. v0.1.4 owns those shapes.

The v0.1.3 CAS/CDC rows measure exact sharing and resynchronization. They do not
replace or satisfy the unique-content namespace throughput or v0.1.2 durable
Store-footprint gates. Fixtures must not use owner-side Workspace file-range edit, reflink, clone,
shared backing storage, or precomputed product roots to manufacture reuse.

## v0.1.3-owned family totals and budgets

| # | Complete family contract | Timed | Proof | Target | Hard |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1 | [Payload create and read](payload-create-read.md) | 8 | 0 | 20 s | 40 s |
| 2 | [Namespace initialization, scale, and CAS/CDC deduplication](namespace-initialization-scale.md) | 7 | 2 | 55 s | 90 s |
| 3 | [Tiny-file churn](tiny-file-churn.md) | 9 | 0 | 10 s | 20 s |
| 4 | [Directory construction and traversal](directory-construction-traversal.md) | 6 | 0 | 10 s | 20 s |
| 5 | [Git and tool workflow](git-tool-workflow.md) | 3 | 0 | 15 s | 30 s |
| 6 | [Namespace mutation](namespace-mutation.md) | 3 | 0 | 10 s | 20 s |
| 7 | [Link/inode topology](link-inode-topology.md) | 3 | 0 | 8 s | 15 s |
| 8 | [Mixed load-bearing workload](mixed-load-bearing-workload.md) | 3 | 3 | 15 s | 30 s |
| **v0.1.3 total** | **8 families** | **42** | **5** | **143 s** | **265 s** |

The target and hard columns cover three fresh samples per timed case plus one
execution of each proof. Preparation and reporting remain outside family
budgets. Each inherited v0.1.2 source arm retains the provisional 13/26-second
edit-family target/hard budget; complete paired collection uses 26/52-second
accounting. Its separate verification/conformance timeout is 90 seconds. Do not
combine verification walls with performance distributions or publish one mixed
accumulated latency budget.

## Shared load, seeds, and lifecycle timing

The geometric multiplier is `a = 10`. A new v0.1.3 operation family uses one
scheduled operation as its load unit and freezes nested prefixes of 1, 10, and
100 operations. Payload create and namespace initialization retain their
declared exceptions.

Use exactly these labels for new v0.1.3 randomized schedules:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

The v0.1.2 mutation families retain their own frozen v0.1.2 seed labels and are
not regenerated under v0.1.3 identities.

Every timed case reports workload and complete workflow walls. The fixed
lifecycle component is at most 500 ms after excluding applicable workload
terms:

```text
0.5 s
+ payload_bytes / 100 MiB/s
+ affected_paths / 10,000 paths/s
+ same_count_edits / 100 edits/s
+ count_changing_edits / 50 edits/s
```

## Issue structure

Keep issue count demand-driven:

1. Create one parent v0.1.3 roadmap issue.
2. Create one shared harness/registry issue.
3. Create a family issue only when one of the eight complete families is
   scheduled.
4. Create focused implementation issues only after the baseline proves
   independent root causes.
5. Create one verification/publication issue last.

No v0.1.3 issue may reopen a completed v0.1.2 family or add a member to it.

## Acceptance criteria

- [ ] Exactly 8 v0.1.3-owned families contain 42 timed and 5 proof-only cases.
- [ ] The inherited v0.1.2 lane remains exactly 39 timed cases across two
  complete edit performance families plus 12 separate verifier/conformance
  groups.
- [ ] The accumulated workload campaign contains 81 timed and 17 proof-only
  cases, plus 12 separate frozen controls.
- [ ] Every new 1/10/100 schedule is a seed-bound nested prefix.
- [ ] Candidate evidence contains exactly three fresh timed samples per new case
  and one execution per new proof; every valid result is retained.
- [ ] Each applicable payload, path, same-count-edit, count-changing-edit, fixed
  lifecycle, family, and complete-campaign gate passes.
- [ ] Every new case uses public filesystem behavior, real FUSE where projected,
  one final Commit, exact oracles, fresh reopen, and deterministic cleanup.
- [ ] Deduplication rows retain exact chunk transcripts and distinguish
  within-import duplicate, preexisting reuse, and borrowed-by-result identity
  without using owner-side Workspace file-range edit during fixture construction.
- [ ] Baselines run before optimization and only measured shared root causes
  receive implementation changes.
- [ ] No change alters the five-table Store schema, canonical identities, CDC
  profile, public semantics, daemon compatibility, or resource bounds.
- [ ] New Commit-depth and Branch-fan-out work remains assigned to v0.1.4.
