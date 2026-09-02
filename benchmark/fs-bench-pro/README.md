# fs-bench-pro

This benchmark follows the current
[0.1.x benchmark contract](../../docs/roadmap/0.1/benchmarking.md). Its
historical architecture is retained in
[`docs/research/history/v2-replacement/spec.md`](../../docs/research/history/v2-replacement/spec.md),
sections 17–19.

## Campaign inventory

- The existing registered payload campaign is implemented: 32 MiB cold create,
  small edit, EDIT16, prepend, and read.
- The namespace-v3 lifecycle/admission campaign is implemented separately through the
  existing family:
  `namespace-100`, `namespace-1000`, `namespace-10000`, and
  `namespace-100000`.
- The canonical scenario and status table is the
  [0.1.x benchmark matrix](../../docs/roadmap/0.1/benchmarking.md).

The benchmark contract also carries the canonical **Problem statement**,
**Goal**, **Files to read**, and **Acceptance criteria** for the v0.1.1
namespace work; this README records harness usage and implementation status.

Namespace-v2 keeps 100 regular files per data directory and uses the frozen
`synthetic-small-heavy-v2` profile: exact Hamilton empty/tiny/small/medium
counts, one exact 100,000,000-byte anchor at the first three tiers, two anchors
at 100,000 files, and exact 125/200/300/500-million-byte tier budgets. Content
is unique, path-derived, fully materialized, and streamed with at most 1 MiB of
fixture scratch. Historical namespace-v1/v2 rows remain immutable; active
lifecycle rows use `fs-bench-pro-namespace-v3`, `commit-head-reopen-ready-v1`, and
`namespace-file-digest-tree-v2`. The v2 custody digest covers root, directory,
and file type/path/size plus deterministic mode (`0750` directories, `0640`
files), mtime (`1700000000.0`), and file-content digests during fixture
generation. Full namespace correctness remains a separate test-only check; the
active performance process contains no content verifier.
The uniform deterministic mode/mtime values intentionally form a best-case
metadata-dedup profile. The edit uses the explicit
`content-only-normalized-mtime-v1` contract, which restores the fixed mtime
after changing content; a separate normal-overwrite mtime diagnostic is
required before extrapolating these results to real Workspace edits.

Namespace-admission rows remain outside the existing registered total and the
paired LayerFS-versus-Computer campaign. The namespace runner is
LayerFS-only and separate from both `run.sh` and `run-paired.sh`, allowing one
failing tier to be iterated without running the full or comparative campaigns.
It supports one-case and `all` modes through `run-namespace.sh`.

The implemented payload LayerFS arm uses exactly one local `LayerStackStore`
and public SDK calls.
The benchmark process, SDK, Store, Workspace spool, and FUSE `ProxyHost` run
natively on macOS. Every measured mutation executes through a real FUSE
Workspace in one already prepared daemon container, starts a fresh process,
commits, and ends the Workspace. The container has no host bind; only its
capability-authenticated daemon port is published to host `127.0.0.1`. There is
no second Store or post-Commit publication operation.

For every lifecycle it records:

```text
T0 before Workspace Create
T1 Create returns
T2 fresh-process Exec/output returns
T3 Commit returns and is Store-visible
T4 End returns

workspace_create_ns   = T1 - T0
execution_ns          = T2 - T1
commit_api_ns         = T3 - T2
layerstack_visible_ns = T3 - T0
workspace_end_ns      = T4 - T3
complete_lifecycle_ns = T4 - T0
```

`workload.rs` is compiled into the prepared image as
`fs-benchmark-workload`. The create command reports its inner write interval on
stdout; the outer execution interval remains independently measured by the SDK.

Run the source/tooling checks:

```sh
benchmark/fs-bench-pro/run.sh --self-check
```

Run a sealed campaign against an already running prepared container:

```sh
benchmark/fs-bench-pro/run.sh RUN_ID CONTAINER_ID HOST_FIXTURE CONTAINER_FIXTURE [ITERATIONS]
```

This command runs only the implemented payload campaign. It does not run the
namespace matrix.

Run the namespace self-check and a sealed LayerFS-only tier or matrix with:

```sh
benchmark/fs-bench-pro/run-namespace.sh --self-check
benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID namespace-10000 1
benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID all 3
```

The namespace runner creates fixtures outside product timing, starts one fresh
benchmark process per tier/sample, supervises whole-process CPU and peak RSS,
and retains immutable success or failure evidence under
`benchmark-results/fs-bench-pro/namespace/RUN_ID`. Namespace rows use their own
schema and never contribute to payload totals or paired results.

To compare isolated product-source variants against the exact same sealed
fixture without regenerating it, point later runs at the earlier campaign's
`scenarios` directory:

```sh
LAYERFS_NAMESPACE_FIXTURE_ROOT=/absolute/earlier-run/scenarios \
  benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID all 3
```

The runner validates and copies each compact manifest into the new immutable
evidence directory, uses the original fixture in place, and per sample checks
the manifest SHA plus fixture-root mode and mtime without rereading file bytes.
It records `generated-first-use-uncontrolled`/`reused-first-use-uncontrolled`
separately from later
`*-post-first-use-uncontrolled` rows. It neither warms only the candidate nor
pretends the writable host fixture is mounted read-only; consequently it does
not label either state `controlled-warm`.
Manifests carrying the earlier `synthetic-small-heavy-v1` or
`namespace-file-digest-tree-v1` identities are rejected, never relabeled, and
cannot be pooled with v2 evidence; future runs must generate new manifests.

`run-status.json` and the corresponding `*-pass.txt` files report performance,
evidence, resource, correctness, cleanup, and quality independently. Missing
required counters are recorded as unavailable, so a performance hit cannot be
reported as a complete evidence pass.
The report also derives the 100,000-file adjacent-ratio ceiling from the actual
10,000-file median (`100k <= 2 * 10k`): for example, a measured 0.8-second 10k
median requires at most 1.6 seconds at 100k. The runner never delays a faster
tier to satisfy that ratio.

For active namespace-v3 rows, `product_lifecycle_ns` is exactly:

```text
layerstack_init_ns + branch_fork_ns + workspace_create_ns + edit_ns +
commit_ns + workspace_end_ns + reconnect_ns +
reopen_workspace_create_ns + reopen_workspace_end_ns
```

The reconnect phase drops the original Store/Client, opens a fresh pair, and
proves the Branch head equals the expected Commit. The reopened real-FUSE
Workspace is created, reaches ready, and is ended cleanly. No content digest,
full reopen correctness, verifier throughput, or read-ahead claim is made.
`whole_supervised_*` CPU/RSS fields cover the entire process and are never
described as product-only resources.

Set `LAYERFS_NAMESPACE_MODE=init-only-diagnostic` with the same four runner
arguments for a fresh-Store public `initialize_layerstack` diagnostic. It
creates no FUSE Workspace, retains the Store/canonical census and private
initialization frame, is explicitly nonterminal, and is excluded from all
binding medians and PASS decisions.

The container must run the current sealed image with TCP port `41273` published
only on `127.0.0.1`. The script reads the protected daemon capability during untimed preparation,
refuses host binds or a stale source seal, refuses to overwrite evidence, saves
source/host/container custody, writes raw JSONL, validates it, and appends a report to
`benchmark-results/fs-bench-pro/optimization-history.md`.

## Matched LayerFS versus Computer campaign

The cross-product profile uses the same isolated row histories, fresh
`/bin/sh -c` process shape, real FUSE, native-host authority SQLite, and
non-crash-durable acknowledgement on both products:

```text
journal_mode=MEMORY
synchronous=OFF
no checkpoint
no database or directory fsync
```

Computer still uses its unmodified public
`Workspace.runtime.exec(sync="wait")` push/FUSE/pull path. Four separately
prepared computerd containers keep cold-create, EDIT16, prepend, and read from
sharing executor state. Container and fixture preparation occurs before each
timed arm.

Build the thin Computer runtime image, which adds only the sealed shared
workload helper to the pinned product layer:

```sh
workload_hash=$(shasum -a 256 benchmark/fs-bench-pro/workload.rs | awk '{print $1}')
docker build \
  -f benchmark/fs-bench-pro/Dockerfile.computer-runtime \
  --build-arg WORKLOAD_SOURCE_SHA256="$workload_hash" \
  -t layerfs-fs-benchmark-pro-computer:fair-host-v3 .
```

After the LayerFS candidate is stable, run randomized adjacent pairs against
one already prepared LayerFS daemon container:

```sh
benchmark/fs-bench-pro/run-paired.sh \
  RUN_ID \
  LAYERFS_CONTAINER_ID \
  /absolute/host/fixture.bin \
  /fixture/payload.bin \
  /absolute/cloudflare-computer-root \
  layerfs-fs-benchmark-pro-computer:fair-host-v3 \
  7
```

The runner rejects source/image/fixture mismatch, host binds on the LayerFS
container, non-FUSE Computer executors, non-pinned Computer product files,
non-matching SQLite acknowledgement, missing storage census, and incomplete
pairs. It saves raw pair evidence and `report.md` under
`benchmark-results/fs-bench-pro/paired/RUN_ID`.

This Cloudflare Computer comparison covers only the existing matched payload
rows. Namespace rows are LayerFS-only and never contribute to paired results or
`registered_total_ns`.
