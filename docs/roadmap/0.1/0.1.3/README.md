# LayerFS 0.1.3

> **Status:** Draft filesystem-workload benchmark release; no scenario is
> registered until its source, fixture, runner, and evidence identities are
> frozen together.
>
> **Compatibility:** Preserve the released 0.1.x contract and every frozen
> `fs-bench-pro` control.

## Problem statement

The earlier v0.1.3 draft incorrectly treated public SDK operations as the
benchmark subjects. That would measure method-call overhead instead of the
filesystem workloads LayerFS exists to run.

v0.1.3 instead needs one bounded, reproducible matrix for payload, edit,
namespace, link, metadata, and tool workloads at the simplest LayerFS history
topology. The fixed topology separates workload cost from the repeated-Commit
history and Branch fan-out that v0.1.4 owns.

## Goal

Register 56 timed and 14 proof-only LayerFS lifecycle cases across exactly 11
filesystem workload families. Every new operation-based curve executes nested
prefixes of 1, 10, and 100 scheduled operations before one Commit. Frozen rows
retain their meanings; payload and namespace-scale families retain their
declared load-unit exceptions.

The complete release knows 82 definitions:

```text
56 timed LayerFS cases
+ 14 proof-only LayerFS cases
+ 12 frozen fs-bench controls
= 82 definitions
```

The 12 controls remain a separate regression/comparison lane. They never enter
v0.1.3 family counts, family budgets, or `registered_total_ns`.

## Files to read

- [0.1.x roadmap](../README.md)
- [Append-only benchmark contract](../benchmarking.md)
- [v0.1.1 scope](../0.1.1/README.md)
- [v0.1.2 scope](../0.1.2/README.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Benchmark workload helper](../../../../benchmark/fs-bench-pro/workload.rs)
- [Benchmark runner custody pattern](../../../../benchmark/fs-bench-pro/run.sh)

## Scope and exclusions

Each new case uses:

```text
one Store and one Client
  -> one LayerStack
  -> one genesis Layer
  -> one Branch
  -> one real-FUSE Workspace when a filesystem is projected
  -> one fresh workload process
  -> one final unpromoted Commit for each new v0.1.3 row
  -> End, fresh reconnect, and exact verification
```

The benchmark subjects are filesystem workload families, not SDK-operation
microbenchmarks. Fixture, Store, Client, container, source sealing, and report
preparation remain outside timed regions.

Inherited frozen rows retain their exact historical lifecycle, including
`edit16` and its multiple Commits. They are explicit compatibility exceptions,
not permission to add another repeated-Commit curve. v0.1.3 excludes Commit-
history depth as a new scaling dimension, multiple Branches, Branch fan-out,
competing publication, conflict resolution, and history-sensitive pagination.
[v0.1.4](../0.1.4/README.md) owns those shapes. v0.1.3 also does not redefine
the v0.1.1 namespace-admission matrix or the v0.1.2 prepend and capture
contracts.

## Family totals and budgets

The target and hard columns cover one complete candidate campaign for the
family: three fresh samples per timed case plus one execution of each proof
case. Proofs have no latency distribution. Preparation and reporting remain
outside the family budget and are recorded separately.

| # | Family contract | Timed | Proof | Target | Hard |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1 | [Payload create and read](payload-create-read.md) | 8 | 0 | 20 s | 40 s |
| 2 | [Same-count file edits](same-count-file-edits.md) | 5 | 0 | 10 s | 20 s |
| 3 | [Prepend and range-copy](prepend-range-copy.md) | 6 | 7 | 20 s | 40 s |
| 4 | [Namespace initialization and scale](namespace-initialization-scale.md) | 4 | 0 | 30 s | 60 s |
| 5 | [Tiny-file churn](tiny-file-churn.md) | 9 | 0 | 10 s | 20 s |
| 6 | [Directory construction and traversal](directory-construction-traversal.md) | 6 | 0 | 10 s | 20 s |
| 7 | [Git and tool workflow](git-tool-workflow.md) | 3 | 0 | 15 s | 30 s |
| 8 | [Count-changing file edits](count-changing-file-edits.md) | 6 | 4 | 18 s | 35 s |
| 9 | [Namespace mutation](namespace-mutation.md) | 3 | 0 | 10 s | 20 s |
| 10 | [Link/inode topology](link-inode-topology.md) | 3 | 0 | 8 s | 15 s |
| 11 | [Mixed load-bearing workload](mixed-load-bearing-workload.md) | 3 | 3 | 15 s | 30 s |
| **LayerFS total** | **11 families** | **56** | **14** | **166 s** | **330 s** |

One complete LayerFS campaign therefore targets at most 3 minutes and has a
hard ceiling of 6 minutes. A family or complete campaign that crosses its hard
ceiling is invalid release evidence, not a result to hide or average away.

## Shared load, seeds, and lifecycle timing

The geometric multiplier is `a = 10`. An operation family uses one scheduled
operation as its load unit and freezes nested prefixes of 1, 10, and 100
operations. The 1-operation schedule is the first operation of the 10-operation
schedule; the 10-operation schedule is the first ten operations of the
100-operation schedule. All scheduled operations run before one Commit.
Payload create uses 1/10/100 MiB, and namespace initialization retains its
frozen 100/1,000/10,000/100,000-file exception.

Use exactly these UTF-8 seed labels:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

New randomized schedules use a family- and scenario-domain-separated SHA-256
counter stream and take nested prefixes. Candidate evidence contains exactly
three total fresh timed samples per case, one per seed. Existing frozen control
rows retain their existing fixtures and sampling semantics.

Every timed case reports the workload interval and complete workflow wall. The
fixed lifecycle component is Workspace Create, Commit or `UpToDate`
acknowledgement, End, fresh reconnect, and verification; it must be at most
500 ms after excluding the declared workload terms. The complete-case target
is:

```text
0.5 s
+ payload_bytes / 100 MiB/s
+ affected_paths / 10,000 paths/s
+ same_count_edits / 100 edits/s
+ count_changing_edits / 50 edits/s
```

Round only enough to avoid false precision. Apply only denominators present in
the declared workload, but every applicable floor is mandatory: at least
100 MiB/s payload, 10,000 paths/s, 100 same-count edits/s, and 50
count-changing edits/s.

## Issue structure

Keep issue count demand-driven:

1. Create one parent v0.1.3 roadmap issue.
2. Create one shared harness/registry issue.
3. Create a family issue only when that family is scheduled; the 11 documents
   do not authorize 11 speculative issues.
4. Create focused fix issues only after the baseline identifies independent
   root causes.
5. Create one verification/publication issue last.

Assign scheduled issues to `@yifanxuaaa`. Every issue includes **Problem
statement**, **Goal**, **Files to read**, and **Acceptance criteria**.

## Acceptance criteria

- [ ] The registry contains exactly the 56 timed and 14 proof-only LayerFS
  cases in the 11 linked family contracts.
- [ ] The 12 frozen controls remain separate, unchanged, and excluded from
  family budgets and `registered_total_ns`.
- [ ] Every new 1/10/100 schedule is a nested prefix for each of the three
  frozen seeds.
- [ ] Candidate evidence contains exactly three fresh timed samples per case
  and one execution per proof case; every valid result is retained.
- [ ] Each applicable workload meets its payload, path, same-count-edit, and
  count-changing-edit floor and its fixed lifecycle bound.
- [ ] Each family and the complete LayerFS campaign fit their target and hard
  budgets with preparation reported separately.
- [ ] Every new case uses public filesystem behavior, real FUSE where
  projected, one final Commit, exact semantic oracles, fresh reopen
  verification, and deterministic cleanup; inherited frozen rows retain their
  exact historical lifecycle.
- [ ] Sync/barrier data is passive evidence attached to the owning workload,
  never a separate family, timed case, or proof case.
- [ ] Baselines run before optimization; only measured shared root causes
  receive implementation issues and focused regression checks.
- [ ] No v0.1.3 change alters the five-table Store schema, canonical bytes or
  identities, CDC profile, public semantics, released daemon compatibility,
  or existing resource bounds.
- [ ] New Commit-depth scaling and Branch fan-out remain assigned to v0.1.4.
