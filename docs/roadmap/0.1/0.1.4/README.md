# LayerFS 0.1.4

> **Status:** Draft benchmark-completion release; no issues, harness, scenario
> IDs, or release candidate are admitted yet.
>
> **Compatibility:** Preserve the released 0.1.x contract and every previously
> registered benchmark row.

## Problem statement

The v0.1.3 matrix covers filesystem workloads and bounded retained-history
storage growth on one Branch. v0.1.4 extends that evidence to multiple Layers
and Branches, broader history queries, diffs, conflicts, and publication. LayerFS claims that
Fork is zero-copy, history is immutable, and new states are incremental
physically; those claims need one bounded multi-history benchmark matrix before
1.0.0.

## Goal

Create and run LayerFS-only benchmarks for multi-Layer and multi-Branch Commit
history, Fork, Add, Diff, paged Query, conflict, resolution, head movement,
historical reads, reopen, and storage reuse. Optimize each independent measured
bottleneck while preserving the scenario meanings frozen in v0.1.0-v0.1.3.

## Files to read

- [0.1.x roadmap](../README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [v0.1.3 Workspace and single-Branch deduplication plan](../0.1.3/README.md)
- [Public operation families](../../../../crates/layerfs-monitor/src/operation.rs)
- [Public SDK client](../../../../crates/layerfs-sdk/src/client.rs)
- [LayerStack Store lifecycle](../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- [Workspace reconciliation](../../../../crates/layerfs-workspace/src/reconcile.rs)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Store and Branch evaluator](../../../../tools/layerfs-eval/src/main.rs)

## Inherited single-Branch coverage

Reuse the v0.1.3 [single-Branch history family](../0.1.3/dedup-branch-history.md)
and [testing rules](../0.1.3/testing-rules.md). Do not recreate its localized,
hot-set, recurring-content, metadata-only, or unique-rewrite storage trajectories
under new identities. The families below add multi-Branch/multi-Layer semantics
or query/scaling questions absent from that inherited storage family.

## Scale profiles

Start with geometric profiles rather than a Cartesian matrix:

```text
Commit-history depth: 1, 10, 100
Branch fan-out:       1, 10, 100
```

The 1,000-Commit or 1,000-Branch case is an extended diagnostic only when a
smaller profile identifies a scaling question that it can answer. Do not run
every history depth against every Branch count, namespace size, and payload
size.

Use one fixed content fixture and a deterministic edit schedule with separate
independent, identical, disjoint, and overlapping changes. Fixture construction
stays outside measured operation boundaries.

## Draft benchmark families

Exact scenario IDs, fixtures, edit schedules, and sample counts are frozen only
when the parent benchmark issue is admitted.

| Family | Required cases | Question |
| --- | --- | --- |
| Commit history | depths 1, 10, 100 | Do Commit, reopen, head lookup, and historical reads remain bounded as history grows? |
| Historical reads | early, middle, latest | Do later publications leave every retained state byte-exact and directly readable? |
| Fork source | genesis Layer, later Layer, eligible Commit | Does Fork remain zero-copy and independent of represented payload size? |
| Branch fan-out | 1, 10, 100 Branches | What are latency, memory, and physical storage costs per empty and changed Branch? |
| Branch edits | independent, identical, disjoint, overlapping | Are reuse, isolation, and conflict results exact? |
| Add | `Added`, `UpToDate`, `NoChanges`, `HeadMoved` | Are the distinct publication outcomes correct and separately measurable? |
| LayerStack Diff | adjacent and distant Layers, empty Diff | Does cost follow changed results rather than complete represented state? |
| Branch Diff | adjacent and distant Commits, Branch versus Layer | Are paged results exact and bounded? |
| Query history | first page and continuation | Does pagination remain stable across growing Layers, Branches, and Commits? |
| Conflict lifecycle | enumerate, paginate if needed, resolve, Commit | Are competing changes explicit, bounded, and byte-exact after resolution? |
| Competing publication | controlled head movement | Does the losing operation report `HeadMoved` without corrupting either history? |
| Storage reuse | identical and localized changes across Branches | Does physical growth follow unique changed content rather than logical history size? |
| Fresh reopen | complete retained graph | Does a new Client recover exact heads, Layers, Commits, conflicts, and historical bytes? |

## Benchmark requirements

- Reuse the v0.1.3 runner, workload boundaries, result schema, and fixture
  conventions; extend them only where multi-history facts require new fields.
- Use public SDK operations and the ordinary Store/Workspace path.
- Keep per-operation latency separate from complete history-construction wall.
- Record history depth, Branch count, changed paths and bytes, queue/service
  time, CPU, peak RSS, Store growth, semantic bytes, candidate/inserted/reused
  objects and bytes, transaction maxima, result-page counts, and cleanup state.
- Verify exact final bytes, canonical roots, Branch heads, Layer order, Commit
  parents, Add outcome, Diff results, conflict choices, and fresh reopen.
- Retain every valid sample and every failed tier; do not shorten the registered
  population after observing a slow result.
- Use LayerFS-only iteration; external products are not comparators for
  LayerStack-specific Fork, Add, history, Diff, Query, or conflict semantics.

## Proposed GitHub issue structure

Create one parent v0.1.4 benchmark issue, then these bounded subissues:

1. multi-history fixture, deterministic edit schedule, and runner extension;
2. Commit depth, historical reads, and reopen;
3. Fork sources and Branch fan-out;
4. Add outcomes and controlled head movement;
5. LayerStack and Branch Diff;
6. Query pagination;
7. conflict enumeration, resolution, and competing publication;
8. storage reuse, resource scaling, and dedup analysis; and
9. accumulated regression and release closure.

Do not pre-create optimization issues. Run the baseline first and create one
focused issue per independent measured root cause. Every issue must contain
**Problem statement**, **Goal**, **Files to read**, and **Acceptance criteria**
and be assigned to `@yifanxuaaa`.

## Acceptance criteria

- [ ] Freeze the smallest deterministic multi-history fixture, edit schedule,
  scale profiles, and exact scenario table before release measurement.
- [ ] Complete the admitted 1/10/100 Commit-depth and Branch-fan-out profiles
  without a Cartesian matrix expansion.
- [ ] Prove Fork from Layer and eligible Commit adds no canonical payload copy
  solely because the Branch exists.
- [ ] Measure and verify `Added`, `UpToDate`, `NoChanges`, and `HeadMoved`
  separately.
- [ ] Prove adjacent, distant, and empty LayerStack/Branch Diff results exactly,
  including bounded pagination.
- [ ] Prove conflict enumeration, resolution, losing-head behavior, and final
  bytes without mutating retained losing history.
- [ ] Prove early, middle, and latest historical states remain directly
  readable after later Commits and Adds.
- [ ] Prove fresh reconnect recovers the complete retained graph and exact
  canonical state.
- [ ] Report incremental semantic and physical growth per Commit and Branch,
  including reuse for identical and localized changes.
- [ ] Retain bounded CPU, RSS, transaction, result-page, Store, and cleanup
  evidence for every applicable row.
- [ ] Record an optimized or measured/no-change disposition for every admitted
  family and add one focused regression check per retained optimization.
- [ ] Preserve and rerun every registered v0.1.0-v0.1.3 scenario without an
  unexplained regression.
- [ ] Admit the v0.1.4 rows into the append-only registry only after source,
  fixture, runner, and result identities are frozen together.
- [ ] Move incompatible work to v0.2.0 rather than weakening the 0.1.x
  contract.
- [ ] Create the immutable versioned manual, release record, checksums, and
  annotated tag only from a clean candidate that passes the release gates.

## Handoff to 1.0.0

After v0.1.4, the accumulated v0.1.0-v0.1.4 registry is the proposed benchmark
contract v1 for 1.0.0. The 1.0.0 candidate reruns that union; it does not rename,
retime, or weaken earlier rows to improve the final report.
