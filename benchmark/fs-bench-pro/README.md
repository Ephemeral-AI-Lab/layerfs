# fs-bench-pro

This benchmark follows the current
[0.1.x benchmark contract](../../docs/roadmap/0.1/benchmarking.md). Its
historical architecture is retained in
[`docs/research/history/v2-replacement/spec.md`](../../docs/research/history/v2-replacement/spec.md),
sections 17–19.

## Campaign inventory

- The existing registered payload campaign is implemented: 32 MiB cold create,
  small edit, EDIT16, prepend, and read.
- The 0.1.1 namespace-admission campaign is specified but not implemented yet:
  `namespace-100`, `namespace-1000`, `namespace-10000`, and
  `namespace-100000`.
- The canonical scenario and status table is the
  [0.1.x benchmark matrix](../../docs/roadmap/0.1/benchmarking.md).

The benchmark contract also carries the canonical **Problem statement**,
**Goal**, **Files to read**, and **Acceptance criteria** for the v0.1.1
namespace work; this README records harness usage and implementation status.

Each planned namespace fixture has 2,500 unique deterministic bytes per regular
file and 100 regular files per directory. All timed namespace rows use real
FUSE. Materialization is reserved for one untimed equality proof at 10,000
files / 25 MB.

Namespace-admission rows remain outside the existing registered total and the
paired LayerFS-versus-Computer campaign. The planned namespace runner is
LayerFS-only and separate from both `run.sh` and `run-paired.sh`, allowing one
failing tier to be iterated without running the full or comparative campaigns.
It will support one-case and `all` modes; `run-namespace.sh` does not exist yet.

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
planned namespace matrix.

The container must run the current sealed image with TCP port `41273` published
only on `127.0.0.1`; its fixture must match `HOST_FIXTURE` byte-for-byte. The
script reads the protected daemon capability during untimed preparation,
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
