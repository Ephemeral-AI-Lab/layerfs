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
lifecycle rows use `fs-bench-pro-namespace-v3`, `commit-head-exact-reopen-v2`, and
`namespace-file-digest-tree-v2`. The v2 custody digest covers root, directory,
and file type/path/size plus deterministic mode (`0750` directories, `0640`
files), mtime (`1700000000.0`), and file-content digests during fixture
generation. Product rows run the bounded exact verifier through a fresh
real-FUSE Workspace after reconnect; that verification is outside
initialization timing.
The uniform deterministic mode/mtime values intentionally form a best-case
metadata-dedup profile. The edit uses the explicit
`content-only-normalized-mtime-v1` contract, which restores the fixed mtime
after changing content; a separate normal-overwrite mtime diagnostic is
required before extrapolating these results to real Workspace edits.

Namespace-admission rows remain outside the existing registered total. The
namespace runner is separate from `run.sh`, allowing one failing tier to be
iterated without running the registered payload campaign. Both runners are
LayerFS-only. `run-namespace.sh` supports one-case and `all` modes.

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
benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID all 4
```

The namespace runner creates fixtures outside product timing, starts one fresh
benchmark process per tier/sample, supervises whole-process CPU and peak RSS,
and retains immutable success or failure evidence under
`benchmark-results/fs-bench-pro/namespace/RUN_ID`. Namespace rows use their own
schema and never contribute to registered payload totals.

To compare isolated product-source variants against the exact same sealed
fixture without regenerating it, point later runs at the earlier campaign's
`scenarios` directory:

```sh
LAYERFS_NAMESPACE_FIXTURE_ROOT=/absolute/earlier-run/scenarios \
  benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID all 4
```

The runner validates and copies each compact manifest into the new immutable
evidence directory, uses the original fixture in place, and per sample checks
the manifest SHA plus fixture-root mode and mtime without rereading file bytes.
It records `generated-first-sample-uncontrolled`/`reused-first-sample-uncontrolled`
separately from later `*-subsequent-sample-uncontrolled` rows. These names are
sample ordinals, not cold/warm claims: the runner neither controls the host
page cache nor warms only the candidate. It also does not pretend the writable
host fixture is mounted read-only.
Manifests carrying the earlier `synthetic-small-heavy-v1` or
`namespace-file-digest-tree-v1` identities are rejected, never relabeled, and
cannot be pooled with v2 evidence; future runs must generate new manifests.

`run-status.json` and the corresponding `*-pass.txt` files report performance,
evidence, resource, correctness, cleanup, and quality independently. Missing
required counters are recorded as unavailable, so a performance hit cannot be
reported as a complete evidence pass.
The report enforces the frozen 100→1,000 and 1,000→10,000 adjacent ceilings of
1.30x and 1.70x. The 100,000-file result is independent: prospectively, with
the authorized 10-percent release tolerance, at most 3.235294118 seconds, at
least 153 MB/s, and at least 30,600 files/s. Its
preferred 200-MB/s / 2.5-second and stretch 250-MB/s / 2.0-second outcomes are
reported separately and are nonbinding. A faster 10,000-file result is never
delayed and never creates a stricter 100,000-file target. Historical rows keep
the target identity that applied when they were captured.

For active namespace-v3 rows, `reopen_verify_ns` and `complete_product_ns` are
exactly:

```text
reopen_verify_ns = reconnect_ns + reopen_workspace_create_ns +
                   reopen_content_verify_ns

complete_product_ns = layerstack_init_ns + branch_fork_ns +
                      workspace_create_ns + edit_ns + commit_ns +
                      workspace_end_ns + reopen_verify_ns
```

`product_lifecycle_ns` is an exact compatibility alias of
`complete_product_ns`. Reopened Workspace End is reported separately as
cleanup after T7. The reconnect phase drops the original Store/Client, opens a
fresh pair, and proves the Branch head equals the expected Commit. The reopened
real-FUSE Workspace runs the bounded exact namespace verifier; its digest,
scratch, worker count, compact plan/path/digest state, and read-ahead counters
are validated. After T7, a normal overwrite records whether the normalized
fixture mtime changes. That dirty diagnostic Workspace is discarded, and its
End time remains cleanup-only.
`whole_supervised_*` CPU/RSS fields cover the entire process and are never
described as product-only resources.

The harness records current RSS and the native lifetime high-water at T0, T1,
and T7. A phase high-water is exact only when the later snapshot establishes a
new lifetime maximum; otherwise its gate is unavailable. Per-connection
`SQLITE_DBSTATUS_CACHE_USED` binds the configured cache target and actual T0/T1
ownership without warming the Store before T0. Process-global memory and
allocation counters are separately marked `unavailable-disabled` when SQLite
returns impossible all-zero values; `SQLITE_STATUS_PAGECACHE_OVERFLOW` is
reported as overflow, not mislabeled as total page-cache ownership. The target
must be at most 64 MiB and remains 32 MiB; the ceiling is headroom, not a
request to allocate or fill 64 MiB.

The terminal resource gates are <=14.07 initialization CPU-seconds, <=10 MiB
recomputed explicit LayerFS ownership, <=128 MiB initialization incremental
HWM, <=256 MiB complete-lifecycle incremental HWM, zero new product/SQLite
workers, zero swap, and no OOM. RSS, physical footprint, CACHE_USED, explicit
ownership, CPU, and physical I/O remain separate fields; none is hidden by
subtracting another.

`initialization_disk_{read,write}_bytes` remain source-identified native
diagnostics. The runner does not compare them with logical bytes or Store
growth: those quantities are not physical-I/O ceilings. A binding physical-I/O
regression claim requires a source/platform/filesystem/cache-matched control;
the release gate retains the values as separately reported evidence rather
than inventing a logical-byte inequality. CPU uses the explicit 14.07-second
ceiling above. The
deterministic `logical_path_movement_{bytes,ratio}` fields instead combine the
exact source read, object-segment traffic, and Store growth equations.

Exact FUSE reads use at most four per-node two-MiB proxy read-ahead entries,
skip responses with no unread tail, and report the aggregate peak through
`maximum_product_read_ahead_bytes`. The retained
`issue9-v3-read4x2m-product-10k-r001-20260903` and
`issue9-v3-read4x2m-product-100k-r002-20260903` screens fetch exactly the
logical bytes served with zero unused bytes. Both bind source seal
`b082f9d06d0d7b052b8b238fa6bafc313ec5aecbd1dcb90a4385595c2c1f3043`;
they are supporting evidence, not a later-worktree terminal proof. The
normal-overwrite diagnostic reports `changed=false`; treat the namespace edit
profile as non-extrapolatable to automatic POSIX write-mtime semantics.

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

Terminal campaigns set `LAYERFS_NAMESPACE_RUN_COMPOSITE=1`. After all timed
samples, the runner itself executes the fixed warning-denying Clippy, bounded
full test, ignored large-spill/reconnect, live-FUSE, and live-Docker commands.
It writes a source-sealed `layerfs-namespace-runner-composite-proof-v2` receipt
containing each exact command, exit status, combined output, and output SHA-256,
then derives the seven focused-quality, large-spill/reconnect,
materialization/FUSE-equality, managed-Docker, post-attachment-failure,
exact-reconnect, and cleanup-census checks. External proof manifests are
rejected because a self-authored `true`/`ok` JSON document is not execution
evidence. Composite mode requires product `all` with at least four samples;
missing Docker, `/dev/fuse`, activation environment, or a test success marker
is a failure, never a successful skip.
