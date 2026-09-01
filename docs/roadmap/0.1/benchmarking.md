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

`fs-bench-pro` implements the registered 32 MiB payload campaign, but it cannot
yet measure the v0.1.1 existing-directory lifecycle across growing namespaces.
The namespace command and its LayerFS-only runner do not exist, so the earlier
interrupted 9,000-empty-file probe is not reproducible release evidence.

## Goal

Add a separate LayerFS-only namespace campaign that measures initialization,
real-FUSE Workspace creation, one localized edit, Commit, End, and exact reopen
verification at the four canonical tiers below. Keep the implemented payload
and paired Cloudflare Computer campaigns unchanged and independently runnable.

## Files to read

- [`benchmark/fs-bench-pro/src/main.rs`](../../../benchmark/fs-bench-pro/src/main.rs)
  for the implemented payload lifecycle, receipts, and summary.
- [`benchmark/fs-bench-pro/workload.rs`](../../../benchmark/fs-bench-pro/workload.rs)
  for the existing ten-byte edit workload.
- [`benchmark/fs-bench-pro/run.sh`](../../../benchmark/fs-bench-pro/run.sh) for
  the LayerFS payload runner and evidence-custody pattern.
- [`benchmark/fs-bench-pro/run-paired.sh`](../../../benchmark/fs-bench-pro/run-paired.sh)
  for the separate Cloudflare Computer comparison boundary.
- [The v0.1.1 checklist](0.1.1/README.md) for release-level admission and
  compatibility gates.

## Acceptance criteria

- The planned namespace matrix has exactly `namespace-100`, `namespace-1000`,
  `namespace-10000`, and `namespace-100000`.
- Every regular file has 2,500 unique deterministic bytes derived from its
  path, with 100 regular files per directory.
- Every timed namespace row uses real FUSE; materialization is used only for
  the untimed 10,000-file equality proof.
- A separate LayerFS-only `run-namespace.sh` supports one-case and `all` modes
  with immutable evidence and a fresh process per tier.
- Namespace rows never enter `registered_total_ns` or `run-paired.sh`.
- `run.sh` continues to own the implemented payload campaign, while the
  Cloudflare Computer campaign runs separately after the LayerFS candidate is
  stable.
- Documentation continues to say the namespace campaign is unimplemented until
  both its command and runner exist.

## Test matrix authority

The tables in this file are the canonical `fs-bench-pro` scenario inventory.
Every scenario has a stable ID, fixture, projection, timed boundary, and
status. Adding, removing, or changing a scenario requires updating this file,
the harness self-check, and any affected report parser together.

Results do not belong in the matrix. Immutable results remain under
`benchmark-results/` and the applicable release record.

Status meanings:

- **Registered:** implemented and included in the existing LayerFS hard gates.
- **Paired:** also included in the matched LayerFS-versus-Computer campaign.
- **Admission:** planned or implemented release-admission evidence, excluded
  from registered totals until an explicit gate is accepted.
- **Proof:** correctness/resource evidence, not a latency score.

Only the registered payload campaign is implemented today. The namespace
command and `run-namespace.sh` described below are the v0.1.1 implementation
plan; neither exists yet.

## Execution lanes

Keep the campaigns separate so LayerFS iteration never waits for an unrelated
comparison product:

| Lane | Runner | Purpose | When to run |
| --- | --- | --- | --- |
| LayerFS payload | `run.sh` | Existing registered 32 MiB LayerFS campaign | Focused regression and candidate evidence |
| LayerFS namespace | `run-namespace.sh` (not implemented) | 0.1.1 namespace admission and ceiling | Every relevant LayerFS iteration; one selected tier or the full matrix |
| Paired comparison | `run-paired.sh` | Existing matched LayerFS-versus-Computer payload campaign | After the LayerFS candidate is stable |

The 0.1.1 namespace matrix is LayerFS-only. It is not compared with Cloudflare
Computer because `layerstack init` has no established matched Computer
operation and the release question is LayerFS correctness, scaling, and
resource bounds. The existing paired payload campaign still runs once on the
candidate to catch comparative regressions.

A future namespace comparison requires its own preregistered matched boundary,
fixture custody, and separate runner. It must not be added to 0.1.1 by default.

The planned LayerFS-only runner must support one fast case and the full matrix
without calling either existing runner:

```text
benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID namespace-10000 1
benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID all 3
```

Both modes retain source/container seals and refuse to overwrite evidence. The
one-case mode is iteration evidence, not a release claim.

### Existing registered payload matrix

| Scenario ID | Fixture | Operation | Projection | Status |
| --- | --- | --- | --- | --- |
| `cold-create-32m` | Empty Layer | Create and commit one 32 MiB file | Real FUSE | Registered, paired |
| `small-edit` | One 32 MiB file | One ten-byte overwrite and Commit | Real FUSE | Registered |
| `edit16` | One 32 MiB file | Sixteen ten-byte overwrite/Commit cycles | Real FUSE | Registered, paired |
| `prepend-temp-copy-rename` | One 32 MiB file | Prepend ten bytes through temp-copy-rename | Real FUSE | Registered, paired |
| `read-32m` | One 32 MiB file | Sequentially read the complete file | Real FUSE | Registered, paired |

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
to improve a measurement. Do not run `run-paired.sh` in the inner optimization
loop; run the smallest failing LayerFS-only namespace case first.

## Planned 0.1.1 namespace admission matrix

The namespace campaign grows regular-file count and logical bytes together by
10x while holding each file at exactly 2,500 unique deterministic bytes derived
from its path. Use 100 regular files per directory.

| Scenario ID | Regular files | Directories | Logical bytes | Projection | Status |
| --- | ---: | ---: | ---: | --- | --- |
| `namespace-100` | 100 | 1 | 250,000 (0.25 MB) | Real FUSE | Planned admission |
| `namespace-1000` | 1,000 | 10 | 2,500,000 (2.5 MB) | Real FUSE | Planned admission |
| `namespace-10000` | 10,000 | 100 | 25,000,000 (25 MB) | Real FUSE | Planned admission |
| `namespace-100000` | 100,000 | 1,000 | 250,000,000 (250 MB) | Real FUSE | Planned admission ceiling |

`MB` is decimal; the exact byte count is authoritative.

`regular_files` excludes directories. The fixture manifest records regular
files, directories, logical bytes, and a deterministic digest separately.
Content must be unique across files so Store deduplication cannot collapse the
fixture into one repeated payload.

Use one existing 2,500-byte file as the edit target and reuse the current
ten-byte positional `edit` workload. The edit does not change file length.

Materialization is not a timed `fs-bench-pro` row. It is used only for one
untimed equality proof at 10,000 files / 25 MB: materialization and real FUSE
must produce the same logical state and canonical root. That proof must not be
mixed into the real-FUSE performance matrix.

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

Run every tier in a fresh process so process peak RSS belongs to that tier.
Start with one exploratory sample. Retain at least three valid samples per tier
for candidate evidence when runtime permits. Do not set latency hard gates
until the baseline exists; exactness, bounded resources, source identity, and
retention of every valid sample are mandatory immediately.

The namespace rows remain outside `registered_total_ns` and the paired campaign.
If a tier fails, add the smallest fixed-byte or fixed-file-count diagnostic
needed to distinguish namespace work from payload work; do not pre-register a
four-by-four Cartesian matrix.

The full release admission checklist is in
[0.1.1/README.md](0.1.1/README.md).
