# LayerFS 0.1.2

> **Status:** Implementation in progress. The family runner format and universal
> regular-file edit engine are implemented; edit families and Store-footprint
> admission remain sequential follow-up work. No new scenario is registered
> until its operation, fixture, load, timing, schema, and performance receipts
> are frozen together.

## Release structure

v0.1.2 has five ordered sub-issues:

0. [adapt `fs-bench-pro` to the family format](fs-bench-pro-format.md);
1. [implement the universal Workspace regular-file edit engine](universal-file-edit-engine.md);
2. [complete the same-count edit performance family](same-count-file-edits.md);
3. [complete the count-changing edit performance family](count-changing-file-edits.md); and
4. [complete total durable Store-footprint evidence](store-footprint-efficiency.md).

The first item is shared implementation, not a benchmark family. The two edit
families classify workloads by the semantic difference under test: file length
is preserved or changed. Store footprint remains separately accounted evidence.

## GitHub tracking

| Order | Issue |
| ---: | --- |
| Parent | [#12 — v0.1.2 universal file editing and performance-family release](https://github.com/Ephemeral-AI-Lab/layerfs/issues/12) |
| 0 | [#17 — adapt `fs-bench-pro` to family-local performance runners](https://github.com/Ephemeral-AI-Lab/layerfs/issues/17) |
| 1 | [#14 — implement the universal Workspace regular-file edit engine](https://github.com/Ephemeral-AI-Lab/layerfs/issues/14) |
| 2 | [#13 — build and optimize same-count file-edit performance](https://github.com/Ephemeral-AI-Lab/layerfs/issues/13) |
| 3 | [#15 — build and optimize count-changing file-edit performance](https://github.com/Ephemeral-AI-Lab/layerfs/issues/15) |
| 4 | [#16 — measure and optimize total durable Store footprint](https://github.com/Ephemeral-AI-Lab/layerfs/issues/16) |

## Fixed performance environment

All timed file-edit rows use one environment:

```text
MacBook host
-> host-resident LayerStackStore and public Client
-> Docker Desktop
-> managed Linux container
-> real LayerFS FUSE projection
-> one fresh workload process
-> explicit Commit and End
```

Environment is a campaign identity, not a scenario-name axis. There is no
POSIX-versus-native, FUSE-versus-materialization, macOS-versus-Linux, or driver
matrix in v0.1.2 performance results. Record exact host hardware/OS, Docker and
engine versions, image digest, container kernel and limits, FUSE capabilities,
Store placement, cache profile, fixture digest, source seal, schema, and
acknowledgement boundary for every run.

The edit engine is intentionally projection-independent. One untimed
materialization/FUSE conformance group prevents driver coupling, but v0.1.2
makes performance claims only for the fixed Docker/FUSE path.

## Benchmark-family layout

All code stays in the existing [`benchmark/fs-bench-pro`](../../../../benchmark/fs-bench-pro/)
harness. The exact [v0.1.2 harness format](fs-bench-pro-format.md) gives each
family one pure definition module and one runner:

| Family | Definition | Runner |
| --- | --- | --- |
| `init_namespace` v0.1.1 | `families/init_namespace.rs` | `run-namespace.sh` |
| Same-count edits | `families/edit_same_count.rs` | `run-edit-same-count.sh` |
| Count-changing edits | `families/edit_count_changing.rs` | `run-edit-count-changing.sh` |
| Store footprint | `families/store_footprint.rs` | `run-store-footprint.sh` |

`run-namespace.sh` is an existing family runner and retains its exact v0.1.1
operation/schema/evidence meaning; issue 0 only extracts its pure definitions
and adds descriptive aliases such as `namespace-10000-files-300mb` while
preserving the frozen `namespace-10000` raw ID.
Shared lifecycle, Docker/FUSE setup, fixture generation, custody, receipt,
verification, and reporting code remains shared. Do not create another
benchmark crate or copy shared helpers into a family.

Each runner requires `--case` unless `--all` is explicit and supports:

```text
--mode performance   # default; selected case/seed only, no full verifier
--mode verify        # selected exact byte/root/reopen/resource verification
--mode admission     # explicit full family plus separate verification
```

This makes the development loop one selected performance case instead of the
full benchmark suite. Performance mode still rejects process failure,
incomplete operation count, wrong reported final length, timeout, OOM, swap, or
cleanup failure. Full digest/root/reopen work belongs only to verification and
never enters performance timing.

## Performance inventory

| Work item | Timed performance IDs | Separate verification/conformance | Development runner |
| --- | ---: | ---: | --- |
| Universal edit implementation | 0 registered family rows | 7 groups + focused owner-side checks | focused tests/cases |
| Same-count family | 14 | 1 group | `run-edit-same-count.sh` |
| Count-changing family | 25 | 4 groups | `run-edit-count-changing.sh` |
| Store footprint | 0 mutation rows | 3 controls x 3 fresh Stores per source/candidate | `run-store-footprint.sh` |
| **Mutation total** | **39** | **12 groups** | — |

One candidate performance arm contains 117 samples:

```text
39 timed IDs * 3 seeds
```

A full paired unchanged/candidate collection contains 234 executions. That full
collection is admission work, not the normal development loop.

## Descriptive scenario naming

New IDs use:

```text
<operation>-<position>-<size-or-delta>-ops-<count>
```

Examples:

```text
overwrite-head-4k-ops-100
append-tail-4k-ops-100
insert-middle-4k-ops-10
truncate-tail-4k-ops-1
sparse-write-past-eof-gap-60k-payload-4k-ops-100
replace-middle-grow-2k-to-4k-ops-10
```

Do not put fixed environment or internal route names such as `posix`, `native`,
`docker`, or `fuse` in a scenario ID. Frozen historical IDs retain their raw
names and receive clearer report display names.

## Paired family design

The same-count family provides reusable controls:

| Same-count control | Count-changing comparison |
| --- | --- |
| Head overwrite | Head prepend |
| Middle overwrite | Middle insert/delete and grow/shrink replacement |
| Tail overwrite | Tail append/truncate and sparse write-past-EOF |

Each count-changing receipt contains `paired_same_count_control_id`. A pair
holds fixture, seed, positional intent, operation count, supplied bytes where
possible, one-Commit topology, environment, cache, limits, timing, and schema
constant. It deliberately varies deleted versus inserted length.

## v0.1.0 integration without rewriting history

Migration means assigning frozen rows to their permanent family, not renaming
or changing them:

| Frozen row | Permanent family |
| --- | --- |
| `small-edit` | v0.1.2 same-count anchor |
| `edit16` | v0.1.2 same-count anchor |
| `prepend-temp-copy-rename` | v0.1.2 count-changing anchor |
| `cold-create-32m` | v0.1.3 payload create/read anchor |
| `read-32m` | v0.1.3 payload create/read anchor |

v0.1.3 reruns both completed v0.1.2 edit families unchanged and adds no new
head/middle/tail, append/prepend, insertion/deletion, truncate, sparse, or
unequal-replacement members.

## Universal edit architecture

Use one internal range replacement:

```text
FileEdit(node, start, delete_len, replacement)
Replacement = Inline | Zero | Spool
```

Ordinary FUSE write/truncate and optional owner-side
`WorkspaceFileRangeEdit` lower into one balanced implicit piece tree. Commit
emits maximal ascending replacement runs into the existing
`FileMutationBatch`/structural splice and performs one inode upsert.

There is no `copy_file_range`, OS-specific edit operation, second edit log,
second canonical editor, CDC suffix-resynchronizer, borrowed-source mode,
partial completion, or byte-copy fallback.

## Performance versus verification

The benchmark is for latency and throughput. Performance samples retain:

- Create-through-End and phase walls;
- operations completed and operations per second;
- payload throughput where the workload copies or supplies meaningful bytes;
- FUSE/spool traffic, piece/tree work, candidate/admission work;
- CPU, RSS, cgroup peak, swap, timeout/OOM, and cleanup validity.

Full byte hashing, canonical-root comparison, fresh reopen, adversarial
boundaries, failure injection, and materialization equality are separate
verifier work. They are not run in the default development mode and never
contribute to a performance distribution. A result cannot be published as
release evidence until its explicit verifier passes.

## Baselines and provisional targets

Retained v0.1.1 anchors:

| Metric | Median / gate |
| --- | ---: |
| Workspace Create | 14.550 ms / <=20 ms |
| `small-edit` Commit | 4.503 ms / <=6 ms |
| `cold-create-32m` complete | 131.774 ms / <=150 ms |
| `edit16` complete | 156.446 ms / <=200 ms |
| `prepend-temp-copy-rename` complete | 223.763 ms / <=250 ms |
| `read-32m` complete | 141.418 ms / <=150 ms |
| registered total | 653.401 ms / <=700 ms |
| inner write throughput | 505.6 MB/s / >=314.6 MB/s |
| host peak RSS | 97.1 MB / <=128 MiB |

For every new performance row, run three alternating seed pairs. When the two
arms intentionally have identical commit/product/harness/workload identity,
use one prepared daemon and alternate A/A labels; this removes daemon-instance
state as an undeclared variable while every row still receives a fresh Store,
Branch, Workspace, and workload process. Distinct baseline/candidate source
identities require distinct sealed containers. Require:

```text
median(candidate / unchanged baseline) <= 1.05
```

Any pair above `1.10` requires phase/counter disposition. Require at least a
20-percent improvement only when baseline evidence proves a defect and the
retained implementation claims to optimize it.

Provisional family budgets, to be frozen after unchanged-source measurement:

| Family | One source arm | Paired baseline/candidate | Separate verifier timeout |
| --- | ---: | ---: | ---: |
| Same-count, 42 samples | 3 / 6 s target/hard | 6 / 12 s | 20 s |
| Count-changing, 75 samples | 10 / 20 s | 20 / 40 s | 40 s |
| **Edit total** | **13 / 26 s** | **26 / 52 s** | **60 s** |

The universal implementation's conformance timeout is separately 30 seconds,
so complete admission verification has a 90-second aggregate timeout. That is
not a performance target and is never run implicitly during development.

Focused owner-side 32 MiB prepend remains:

```text
range edit + Commit/refresh target / hard  50 / 75 ms
complete lifecycle target / hard          80 / 110 ms
old payload/FUSE/spool transfer            0
```

## Issue structure

Create one parent release issue with five ordered sub-issues:

0. formalize `run-namespace.sh` as the v0.1.1 namespace family and adapt
   `fs-bench-pro` to family definitions, selected-case performance, and separate
   verify/admission modes;
1. implement the universal Workspace regular-file edit engine;
2. build, baseline, and optimize same-count file-edit performance;
3. build, baseline, and optimize count-changing file-edit performance; and
4. baseline and optimize total durable Store footprint.

The implementation issue owns shared code and conformance. Each performance
family issue owns its definition file, runner, frozen IDs, baseline, measured
optimization disposition, candidate evidence, verifier, and publication. Do
not split benchmark creation from optimization or create one issue per row.

## Compatibility boundary

Preserve the five-table Store schema, canonical encodings and identities, CDC
profile, existing public SDK/CLI behavior, daemon/FUSE protocol, visibility and
acknowledgement, ordinary metadata/namespace semantics, and existing resource
ceilings. `WorkspaceFileRangeEdit` is additive and owner-side.

## Acceptance criteria

- [ ] Issue 0 lands the shared benchmark format before the implementation and
  family issues collect evidence.
- [ ] One shared implementation issue, two complete edit performance families,
  and one Store-footprint family finish in v0.1.2.
- [ ] All benchmark files and runners live under `benchmark/fs-bench-pro` and
  reuse shared harness helpers.
- [ ] Each family runner defaults to one explicit performance case/seed and
  requires `--all` for full admission.
- [ ] Exactly 39 timed edit IDs, 12 separate verification/conformance groups,
  and three Store controls are frozen.
- [ ] Frozen v0.1.0 rows retain their identities and v0.1.3 inherits the two
  complete edit families without adding members.
- [ ] Performance timing contains no digest/root/reopen/failure/materialization
  verifier work.
- [ ] One environment-independent edit engine serves ordinary write/truncate
  and owner-side range editing with no fallback or alternate canonical path.
- [ ] Paired regression, frozen anchor, provisional family, throughput, RSS,
  zero-swap, and cleanup gates pass.
- [ ] Explicit verification passes before evidence is published.
