# LayerFS 0.1.x benchmark contract

> **Status:** Current maintainer contract for 0.1.x evidence; not a released
> benchmark result or product contract.

The historical architecture is retained in
[the V2 replacement specification](../../research/history/v2-replacement/spec.md),
sections 17–19. The executable harness lives under
[`benchmark/fs-bench-pro`](../../../benchmark/fs-bench-pro/).

The immutable reportable 0.1.0 result remains in the
[0.1.0 benchmark record](../../../release-notes/0.1.0/benchmark-results.md).

## Problem statement

`fs-bench-pro` implements the registered 32 MiB payload campaign, but the
public benchmark surface remains incomplete. It does not yet provide accepted
release evidence for growing namespaces, the known prepend/capture paths,
diverse tiered filesystem workloads in a single-history topology, or
multi-Layer and multi-Branch history scaling. The earlier interrupted
9,000-empty-file probe is not reproducible release evidence.

## Goal

Grow one append-only LayerFS benchmark registry from v0.1.0 through v0.1.4
while keeping the registered payload and namespace campaigns independently
runnable. Each release adds one bounded family, freezes it, and reruns all
earlier registered rows:

1. v0.1.1: initialization and namespace scaling;
2. v0.1.2: prepend/range-copy and online Workspace capture;
3. v0.1.3: filesystem workload and exact CAS/CDC deduplication cases on one
   genesis Layer and one Branch;
   and
4. v0.1.4: multi-Layer and multi-Branch history.

## Files to read

- [`benchmark/fs-bench-pro/src/main.rs`](../../../benchmark/fs-bench-pro/src/main.rs)
  for the implemented payload lifecycle, receipts, and summary.
- [`benchmark/fs-bench-pro/workload.rs`](../../../benchmark/fs-bench-pro/workload.rs)
  for the existing ten-byte edit workload.
- [`benchmark/fs-bench-pro/run.sh`](../../../benchmark/fs-bench-pro/run.sh) for
  the LayerFS payload runner and evidence-custody pattern.
- [The v0.1.1 checklist](0.1.1/README.md) for release-level admission and
  compatibility gates.
- [The namespace-v2 specification](0.1.1/namespace-optimization-spec.md) for
  the proposed small-heavy fixture replacement and measured initialization /
  Workspace Create optimization gates.
- [The v0.1.2 README](0.1.2/README.md) for prepend and capture ownership.
- [The v0.1.3 README](0.1.3/README.md) and its linked family documents for the
  single-history workload and deduplication draft.
- [The v0.1.4 README](0.1.4/README.md) for the multi-history operation draft.
- [The frozen `fs-bench` workload](../../../benchmark/fs-bench/fs-bench.sh) for
  existing mounted-operation controls that must be reused rather than copied.

## Acceptance criteria

- The v0.1.1 namespace matrix has exactly `namespace-100`, `namespace-1000`,
  `namespace-10000`, and `namespace-100000`.
- Historical namespace-v1 retains 2,500 unique deterministic bytes per file.
  Active namespace-v2 uses the separately identified small-heavy
  125/200/300/500-MB profile and exact class/anchor equations in the v0.1.1
  specification. The two profiles are never pooled.
- Every timed namespace row uses real FUSE; materialization is used only for
  the untimed 10,000-file equality proof.
- A separate LayerFS-only `run-namespace.sh` supports one-case and `all` modes
  with immutable evidence and a fresh process per tier.
- Namespace rows never enter `registered_total_ns`.
- `run.sh` continues to own the implemented LayerFS payload campaign.
- Every later release reruns all previously registered rows.
- Once admitted, a scenario's ID, fixture, public operation sequence, timing
  boundary, acknowledgement semantics, oracle, sample rules, and result schema
  remain frozen through 1.0.0.
- A benchmark correction retains the old row and evidence and introduces a new
  scenario ID or schema version.
- v0.1.3 family documents and the v0.1.4 README remain drafts until their
  fixtures and exact matrices are admitted.

## Release sequence

| Release | New benchmark family | Optimization rule |
| --- | --- | --- |
| v0.1.0 | Registered 32 MiB payload lifecycle | Frozen baseline |
| v0.1.1 | Existing-directory initialization and namespace scale | Fix correctness/resource blockers and measured initialization or localized-Commit bottlenecks |
| v0.1.2 | Prepend/range-copy and online Workspace capture | Optimize only retained public-path failures or material opportunities |
| v0.1.3 | Diverse filesystem workloads; one genesis Layer and one Branch; no new history-depth axis | Use tiered seeded schedules; change code only where evidence warrants it |
| v0.1.4 | Multi-Layer, multi-Branch, and Commit-history scaling | Optimize measured history, fan-out, publication, diff, query, or conflict bottlenecks |

The accumulated v0.1.0-v0.1.4 registry is the proposed benchmark contract v1
for 1.0.0.

## Append-only freeze policy

Admission freezes the scenario definition, not one host's observed result.
Immutable results remain bound to their source, harness, fixture, host,
container, cache, and acknowledgement identities.

Never silently change an admitted row. If its definition is defective:

1. retain its evidence;
2. mark the row deprecated with the exact reason;
3. add a new scenario ID or result-schema version; and
4. run the old and new rows together once when practical.

Every release candidate runs its new rows plus all earlier registered rows.
LayerFS-only operations remain LayerFS-only unless another product exposes a
preregistered boundary with genuinely matched semantics.

## Test matrix authority

The tables in this file are the canonical `fs-bench-pro` scenario inventory.
Every scenario has a stable ID, fixture, projection, timed boundary, and
status. Adding, removing, or changing a scenario requires updating this file,
the harness self-check, and any affected report parser together.

Results do not belong in the matrix. Immutable results remain under
`benchmark-results/` and the applicable release record.

Status meanings:

- **Registered:** implemented and included in the existing LayerFS hard gates.
- **Admission:** planned or implemented release-admission evidence, excluded
  from registered totals until an explicit gate is accepted.
- **Proof:** correctness/resource evidence, not a latency score.

Only completed, verified, source-bound rows become registered. Work in a dirty
tree or an unverified runner remains admission work even if implementation
files exist.

## Execution lanes

Keep the campaigns separate so LayerFS iteration never waits for an unrelated
comparison product:

| Lane | Runner | Purpose | When to run |
| --- | --- | --- | --- |
| LayerFS payload | `run.sh` | Existing registered 32 MiB LayerFS campaign | Focused regression and candidate evidence |
| LayerFS namespace | `run-namespace.sh` | 0.1.1 namespace admission and ceiling | Every relevant LayerFS iteration; one selected tier or the full matrix |

Both active lanes are LayerFS-only. Release regression is evaluated by
rerunning every earlier registered LayerFS row; no external comparison runner
is part of `fs-bench-pro`.

The LayerFS-only runner supports one fast case and the full matrix
without calling the registered payload runner:

```text
benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID namespace-10000 1
benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID all 4
```

Both modes retain source/container seals and refuse to overwrite evidence. The
one-case mode is iteration evidence, not a release claim.

### Existing registered payload matrix

| Scenario ID | Fixture | Operation | Projection | Status |
| --- | --- | --- | --- | --- |
| `cold-create-32m` | Empty Layer | Create and commit one 32 MiB file | Real FUSE | Registered |
| `small-edit` | One 32 MiB file | One ten-byte overwrite and Commit | Real FUSE | Registered |
| `edit16` | One 32 MiB file | Sixteen ten-byte overwrite/Commit cycles | Real FUSE | Registered |
| `prepend-temp-copy-rename` | One 32 MiB file | Prepend ten bytes through temp-copy-rename | Real FUSE | Registered |
| `read-32m` | One 32 MiB file | Sequentially read the complete file | Real FUSE | Registered |

## Implemented payload timed boundary

The harness measures one local `LayerStackStore` through public SDK operations.
The benchmark, SDK, Store, Workspace runtime/spool, and FUSE `ProxyHost` run
natively on macOS. Every execution starts a fresh process in a real FUSE
Workspace served by one prepared Linux container.

The container owns only the control daemon, fresh helpers/processes, workload
binary, and fixture. It has no Store, runtime, result, binary, or fixture host
bind. It publishes only its authenticated daemon port on host `127.0.0.1`.

Store, image, container, and fixture preparation stay outside the timer.
Workspace Create, execution/output completion, Commit, and End stay inside.

Required lifecycle fields:

```text
workspace_create_ns
execution_ns
commit_api_ns
layerstack_visible_ns
workspace_end_ns
complete_lifecycle_ns
```

The harness may separately report the inner 32 MiB write interval, but it does
not substitute that interval for `execution_ns`. Store visibility is verified
through public `Client::query` after T4, so proof work changes none of the timed
equations.

Retain raw JSONL, source seal, Git state, host/container custody, and generated
reports under `benchmark-results/fs-bench-pro/runs`.

## Iteration and handoff

Run:

```bash
benchmark/fs-bench-pro/run.sh --self-check
```

Then start a fresh sealed campaign with the native macOS public SDK/Store and
one prepared FUSE-capable daemon container.

If a hard target fails, use the six lifecycle fields, Workspace Commit receipt,
FUSE read/write receipts, and independently reported inner-write interval to
locate the phase. Change the smallest shared root cause, rerun focused checks,
and create a new immutable run ID.

Never reuse a Workspace, process, Store, evidence directory, or earlier result
to improve a measurement. Run the smallest failing LayerFS-only case first.

## Implemented 0.1.1 namespace admission matrix

This section records the implemented namespace-v1 historical contract and the
active [namespace-v2 profile](0.1.1/namespace-optimization-spec.md).
Namespace-v2 keeps the same four scenario IDs, `all`, runner, real-FUSE
projection, and registered-total exclusion while using distinct schema,
fixture, digest, edit, and cache-profile identities. Namespace-v1 evidence is
never relabeled or pooled with namespace-v2.

| Scenario ID | Regular files | Directories | Namespace-v2 logical bytes | Projection | Status |
| --- | ---: | ---: | ---: | --- | --- |
| `namespace-100` | 100 | 1 | 125,000,000 (125 MB) | Real FUSE | Implemented admission |
| `namespace-1000` | 1,000 | 10 | 200,000,000 (200 MB) | Real FUSE | Implemented admission |
| `namespace-10000` | 10,000 | 100 | 300,000,000 (300 MB) | Real FUSE | Implemented admission |
| `namespace-100000` | 100,000 | 1,000 | 500,000,000 (500 MB) | Real FUSE | Implemented admission ceiling |

The prospective binding 100,000-file gate includes the authorized 10-percent
release tolerance: initialization at most 3.235294118 seconds, throughput at
least 153,000,000 B/s, and file rate at least 30,600/s. The 2.5-second / 200
MB/s preferred goal and 2.0-second / 250 MB/s stretch goal remain visible and
nonbinding. Historical evidence retains the target identity under which it was
captured.

`MB` is decimal; the exact byte count is authoritative.

`regular_files` excludes directories. The fixture manifest records regular
files, directories, logical bytes, and a deterministic digest separately.
Namespace-v2 allocates exact 1-percent empty, 79-percent tiny, 15-percent small,
and 5-percent medium counts after one 100-MB anchor per tier and a second anchor
at 100,000 files. Every nonempty file contains deterministic unique
path-derived bytes. Use 100 regular files per directory and one deterministic
non-anchor as the unchanged-length ten-byte edit target.

Namespace-v2 identities are:

```text
schema: fs-bench-pro-namespace-v3
fixture_profile: synthetic-small-heavy-v2
fixture_digest_profile: namespace-file-digest-tree-v2
edit_contract: content-only-normalized-mtime-v1
```

Issue [#11](https://github.com/Ephemeral-AI-Lab/layerfs/issues/11) owns the
bounded cold direct-admission continuation toward 200 MB/s. It may not change
this scenario matrix or use a benchmark scenario name in product behavior.

Materialization is not a timed `fs-bench-pro` row. Namespace-v1 retains its
historical untimed equality proof at 10,000 files / 25 MB. Active namespace-v2
uses the same proof boundary at 10,000 files / 300 MB. Materialization and real
FUSE must produce the same logical state and canonical root, and neither proof
is mixed into the real-FUSE performance matrix.

Fixture generation stays outside LayerFS timing. Each admission case measures
and emits these phases separately:

```text
layerstack_init_ns
branch_fork_ns
workspace_create_ns
edit_ns
commit_ns
workspace_end_ns
reopen_verify_ns
complete_product_ns
```

`complete_product_ns` starts immediately before
`Client::initialize_layerstack` with the Store and Client ready, and ends only
after reconnecting to the Store and completing exact reopen verification.
Fixture generation, Store creation, Client construction, container preparation,
and report generation are excluded and recorded as setup.

Evidence sources are explicit:

- phase wall times come from `Instant` boundaries in the namespace command;
- user/system CPU and peak RSS come from the OS process supervisor around each
  fresh tier process, with raw output retained;
- fixture files, data directories, and logical bytes come from the generated
  fixture manifest (`directories` excludes the fixture root);
- scanned, candidate, inserted, reused, and transaction fields come from
  LayerFS operation/storage receipts or new passive instrumentation;
- an unavailable field is an evidence error, never a silently emitted zero.

Initialization admission is bounded below 8,192 objects and 4 MiB per
transaction. The existing Workspace Commit bound remains below 128 objects and
4 MiB; namespace admission does not relax the registered payload campaign.

Run every tier in a fresh process so process peak RSS belongs to that tier.
Start with one exploratory sample. Retain at least three valid samples per tier
for candidate evidence when runtime permits. Do not set latency hard gates
until the baseline exists; exactness, bounded resources, source identity, and
retention of every valid sample are mandatory immediately.

The namespace rows remain outside `registered_total_ns`.
If a tier fails, add the smallest fixed-byte or fixed-file-count diagnostic
needed to distinguish namespace work from payload work; do not pre-register a
four-by-four Cartesian matrix.

The full release admission checklist is in
[0.1.1/README.md](0.1.1/README.md).

## Planned later benchmark families

- [v0.1.2](0.1.2/README.md) owns prepend/range-copy and online Workspace
  capture. Initialization's directory scan remains a v0.1.1 measurement even
  when the two paths share internals.
- [v0.1.3](0.1.3/README.md) indexes one document per filesystem workload family
  for one genesis Layer and one Branch. It owns deterministic tiered load,
  positional randomness, same-count and count-changing work, exact CAS/CDC
  deduplication, namespace mutations, links, and mixed workloads; it does not
  own Add, multi-Layer Diff, conflicts, or repeated history scaling.
- [v0.1.4](0.1.4/README.md) drafts multi-Layer and multi-Branch Commit history,
  Fork, Add, Diff, paged Query, conflict, resolution, head movement, historical
  reads, reopen, and storage-reuse coverage.

Their exact scenario tables are intentionally not preregistered here. Each
release must first freeze the smallest matrix that answers its product
questions without a Cartesian workload explosion.

### Draft v0.1.3 namespace deduplication extension

The [v0.1.3 namespace family](0.1.3/namespace-initialization-scale.md) adds
separately identified CAS/CDC rows through the existing namespace command and
runner. They never change or replace the v0.1.1 unique-content rows.

| Scenario ID | Role | Status |
| --- | --- | --- |
| `namespace-dedup-locality-1` | One 5 MB common-base file; nested-prefix anchor | Draft timed admission |
| `namespace-dedup-locality-10` | Ten 5 MB files with localized differences | Draft timed admission |
| `namespace-dedup-locality-100` | One hundred 5 MB files with localized differences | Draft timed admission |
| `namespace-dedup-mechanisms-proof` | Unique, exact-copy, shifted, common-body, scattered, and CDC-boundary controls | Draft proof |
| `namespace-dedup-preexisting-proof` | Reuse of a durable base through real-FUSE Workspace Commit | Draft proof |

Every file is independently materialized and fully scanned. The result schema
reports logical ingest, unique payload and canonical insertion, and SQLite
growth separately. Dedup-friendly logical throughput never satisfies the
unique-content initialization throughput gate or enters a pooled total.
