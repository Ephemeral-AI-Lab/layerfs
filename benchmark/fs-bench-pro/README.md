# fs-bench-pro

## v0.1.3 compact Docker workflow

The active `run-*.sh` entrypoints and `families/<family>/setup.sh`, `perf.sh`,
and `verify.sh` share the Docker-only runner. Build the Linux image with
`python3 shared/runner.py --build-image`, then supply the printed image tag
through `--image` or `LAYERFS_BENCH_IMAGE`. No project/Store data mounts or
Docker socket are used.

`perf.sh --smoke` selects the smallest real fixture within 50,000,000 bytes
and 1,000 files, not merely the smallest operation tier. `--perf-fast` is
one complete sample; `--perf-samples N` repeats the same selected input N
times. Post-initialization cases support `--setup fresh|clone`; initialization
always uses a fresh output Store. `verify.sh` requires an explicit case,
seed/repetition, source and input identity from the performance header and
finishes with PASS/FAIL/TIMEOUT/INCOMPLETE under the 59-second deadline.

Compact-v2 ordinary/reuse low tiers and compact-v3 namespace profiles replace
the old oversized lower-tier fixtures. Higher tiers remain unchanged. The
specialized 500 MiB capped replacements are large-only and are never chosen
by `--smoke`. Old low-tier results are historical and not comparable with
the compact profiles; use their frozen source revision for reproduction.

Normal output is one `perf.jsonl` or `verification.json`; failures may add a
bounded `failure.log`. Prepared data is a finite disposable Docker cache;
sample Stores and containers are removed. The legacy documentation below
describes historical contracts where it conflicts with this section. See
the [current infrastructure specification](../../docs/roadmap/0.1/0.1.3/benchmark-infrastructure-optimization-spec.md).

This benchmark follows the current
[0.1.x benchmark contract](../../docs/roadmap/0.1/benchmarking.md). Its
historical architecture is retained in
[`docs/research/history/v2-replacement/spec.md`](../../docs/research/history/v2-replacement/spec.md),
sections 17–19.

## Active SDK-only edit admission

Issue #20 is governed by the [SDK-only edit specification](../../docs/roadmap/0.1/0.1.2/sdk-only-edit-benchmark-rebuild.md)
and [benchmark rules](../../docs/general/benchmark_rules.md). Exactly three
families own 12/32/12 IDs at 1/10/100/500 MiB:

```text
families/edit_length_preserving.rs    + run-edit-length-preserving.sh
families/edit_length_changing.rs      + run-edit-length-changing.sh
families/edit_canonical_chunk_count.rs + run-edit-canonical-chunk-count.sh
```

Each performance row invokes one singular public SDK edit, Commit, End, and
post-End Branch query. Verification is separate, with prequalified exact roots,
independent streamed bytes, fresh Client/Store reconnect, read-only pre/post
FUSE inode checks, materialized equality, and payload-retention checks.

```bash
cargo build --release -p fs-benchmark-pro
benchmark/fs-bench-pro/run-edit-length-preserving.sh --self-check
benchmark/fs-bench-pro/run-edit-length-preserving.sh RUN_ID IMAGE --case overwrite-head-4k-on-1mib-ops-1 --repetition 1 --mode performance --source candidate
benchmark/fs-bench-pro/run-edit-length-preserving.sh RUN_ID IMAGE --case overwrite-head-4k-on-1mib-ops-1 --mode verify --source candidate
python3 benchmark/fs-bench-pro/sdk-edit-custody.py cache-self-check target/release/fs-benchmark-pro
```

Selected runs are non-admission diagnostics; a valid numerical miss is a no-go,
not a generic pass. Preparation uses the shared immutable
`sdk-edit-prepared-store-cache-v1` cache. Override its location with
`LAYERFS_SDK_EDIT_PREPARED_ROOT`. Cache compatibility excludes unrelated
Workspace presentation changes; every sample and qualifier still receives an
independent writable clone. Cached/hashed/cloned inputs are not called cold.

For each authentic clean source arm, build retained assets with:

```bash
python3 benchmark/fs-bench-pro/sdk-edit-custody.py build OUTPUT_DIR IMAGE
```

This retains the host binary plus sealed build/conformance/image evidence.
Admission requires `LAYERFS_SDK_EDIT_BASELINE_BIN`, `..._CANDIDATE_BIN`,
`..._BASELINE_IMAGE`, `..._CANDIDATE_IMAGE`, `..._BASELINE_REVISION`,
`..._CANDIDATE_REVISION`, `..._BASELINE_BUILD`, and `..._CANDIDATE_BUILD` (the two
build evidence directories), then `RUN_ID IMAGE --all --mode admission` on each
family wrapper. The exact product-only treatment must leave the harness,
workload, report generators, contract, and input-preparation compatibility
unchanged.

The release generator consumes a manifest naming all three final family
directories and hashes plus the sealed repository-gate directory. It fails
closed until all 560 performance rows, 56 aggregate receipts, 112 subproofs,
common source/environment/input identities, and repository gates pass. It
generates unpublished evidence documents only; it never tags or publishes.

## Campaign inventory

- The existing registered payload campaign is implemented: 32 MiB cold create,
  small edit, EDIT16, prepend, and read.
- The namespace-v3 lifecycle/admission campaign is implemented separately through the
  existing family:
  `namespace-100`, `namespace-1000`, `namespace-10000`, and
  `namespace-100000`.
- The earlier same-count edit family has 14 timed IDs and a separate
  fragmentation verifier; it is archival, not active SDK admission.
- The earlier count-changing family has 25 timed IDs and paired controls;
  it is archival, not active SDK admission.
- The v0.1.2 Store-footprint family is implemented with three controls,
  durable-census/dbstat custody, and separate exact verification.
- The canonical scenario and status table is the
  [0.1.x benchmark matrix](../../docs/roadmap/0.1/benchmarking.md).

## Historical and supporting family formats

The [v0.1.2 harness contract](../../docs/roadmap/0.1/0.1.2/fs-bench-pro-format.md)
keeps all new work in this crate. These earlier definitions/runners remain
available for reproducibility and supporting evidence, not active SDK edit
admission:

```text
families/init_namespace.rs           + run-namespace.sh
families/edit_same_count.rs      + run-edit-same-count.sh
families/edit_count_changing.rs  + run-edit-count-changing.sh
families/store_footprint.rs       + run-store-footprint.sh
```

Each runner defaults to one explicit case/seed in performance-only mode. Full
digest/root/reopen and adversarial proofs are separate `verify`/`admission`
modes and never enter performance timing. `run-namespace.sh` is the v0.1.1
`init_namespace` compatibility runner and the v0.1.2 family runner. Its pure
definitions live in `families/init_namespace.rs`; raw IDs and descriptive
aliases resolve to the same frozen scenarios. Namespace performance now invokes
only native host initialization. The historical positional lifecycle route
requires `LAYERFS_NAMESPACE_LEGACY_LIFECYCLE=1`; its raw schema is retained for
reproduction, and it must not be used for initialization-only performance.

The benchmark contract carries the canonical problem statements, goals, files
to read, and acceptance criteria; this README records harness usage and final
implementation status. See the
[v0.1.2 evidence index](../../release-notes/0.1.2/benchmark-results.md).

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
benchmark/fs-bench-pro/run-namespace.sh RUN_ID \
  --case namespace-10000 --seed 1 --source candidate --mode performance
benchmark/fs-bench-pro/run-namespace.sh RUN_ID \
  --all --source candidate --mode performance
```

The v0.1.2 family interface keeps performance and exact verification separate:

```sh
benchmark/fs-bench-pro/run-namespace.sh RUN_ID \
  --case namespace-100-files-125mb --seed 1 --source candidate
benchmark/fs-bench-pro/run-namespace.sh RUN_ID CONTAINER_ID \
  --case namespace-100 --source candidate --mode verify
```

`performance` is the default. Select one case/seed/source or explicit `--all`.
The full matrix retains four samples and reports the median of samples 2–4.
The binary, fixture reads, and fresh SQLite Store all run on the native host.
No Docker command, Workspace creation, edit, Commit, or reopen occurs in this
mode. Its primary metric is `layerstack_init_ns`, including final root and
LayerStack publication. The existing initializer-only raw resource/counter
receipt is reused; `performance/summary.json` describes the performance result.
`verify` requires a container and writes no performance row. Combined
`admission` is rejected; select verification separately through
`verify-selected.py` when required, with its 59-second limit.

The namespace runner creates fixtures outside product timing, starts one fresh
benchmark process per tier/sample, supervises whole-process CPU and peak RSS,
and retains immutable success or failure evidence. Legacy rows remain under
`benchmark-results/fs-bench-pro/namespace/RUN_ID`; family-mode rows use
`benchmark-results/fs-bench-pro/init_namespace/RUN_ID` with separate
`performance/` and `verification/` streams. Namespace rows never contribute to
registered payload totals.

Run a selected same-count performance row, an exact verifier, or the full
admission with:

```sh
benchmark/fs-bench-pro/run-edit-same-count.sh RUN_ID CONTAINER_ID \
  --case overwrite-middle-4k-ops-100 --seed 1 --source candidate
benchmark/fs-bench-pro/run-edit-same-count.sh RUN_ID CONTAINER_ID \
  --case overwrite-middle-4k-ops-100 --seed 1 --source candidate --mode verify
LAYERFS_SAME_COUNT_ANCHOR_FIXTURE=/absolute/registered-32m-directory \
  benchmark/fs-bench-pro/run-edit-same-count.sh RUN_ID CONTAINER \
  --all --source a-a-repeatability --mode admission
```

Run `run-edit-same-count.sh --prepare CONTAINER_ID` once before selected rows.
Prepared assets are keyed by source identity outside selected-run wall timing.
Terminal identical-source admission uses one sealed daemon container with
`--source a-a-repeatability`, alternates A/A labels per seed, and reports
repeatability rather than an improvement claim. Each label still receives a
fresh Store, Branch, Workspace, and workload process. Distinct sealed
containers remain required for directional baseline/candidate admission. The
v3 identical-source classifier gates the symmetric aggregate arm lifecycle
ratio; per-scenario ratios remain diagnostics because short A/A lifecycles were
not stable admission statistics. Directional comparisons still gate each
scenario independently.

The runner's `--self-check` performs no Docker command and must finish within
two seconds. Performance mode runs no digest/root/reopen verifier. Admission
retains 42 performance samples per arm, then runs only the separate 1,000-edit
fragmentation proof for each arm.

Run a selected count-changing row, one verifier, or a directional admission:

```sh
benchmark/fs-bench-pro/run-edit-count-changing.sh RUN_ID CONTAINER \
  --case insert-middle-4k-ops-100 --seed 1 --source candidate
benchmark/fs-bench-pro/run-edit-count-changing.sh RUN_ID CONTAINER \
  --case insert-middle-4k-on-8m-proof --seed 1 --source candidate --mode verify
LAYERFS_COUNT_CHANGING_ANCHOR_FIXTURE=/absolute/registered-32m-directory \
  benchmark/fs-bench-pro/run-edit-count-changing.sh RUN_ID CANDIDATE_CONTAINER \
  --all --source baseline-candidate --mode admission --paired-container BASELINE_CONTAINER
```

Directional baseline/candidate admission gates every primary scenario's ratio
of medians. Identical-source A/A is diagnostic repeatability only and is
rejected as count-changing terminal admission.
Both directional images must carry the same final product seal and revision;
only their workload/source seals differ. Expanded count-changing release receipts use
performance schema `v3`, whose `inner_edit_ns` excludes the post-operation
final-length validity check while still requiring that check to pass.
The frozen 32 MiB anchor's absolute path, size, digest, environment variable,
and exact command are part of custody.

Run a selected Store sample/verifier or the complete baseline admission:

```sh
benchmark/fs-bench-pro/run-store-footprint.sh RUN_ID CONTAINER \
  --case store-footprint-unique-100000 --seed 1 --source baseline --mode performance --tier 100000
benchmark/fs-bench-pro/run-store-footprint.sh RUN_ID CONTAINER \
  --case store-footprint-metadata-cardinality-100000 --seed 1 --source baseline --mode verify --tier 100000
benchmark/fs-bench-pro/run-store-footprint.sh RUN_ID CONTAINER \
  --all --source baseline --mode admission
```

Store verification checks content, file and directory metadata, authenticated
roots, fresh reconnect, resources, durable census, and cleanup in a separate
stream. Store controls never enter edit-family walls or `registered_total_ns`.

To compare isolated product-source variants against the exact same sealed
fixture without regenerating it, point later runs at the earlier campaign's
`scenarios` directory:

```sh
LAYERFS_NAMESPACE_FIXTURE_ROOT=/absolute/earlier-run/scenarios \
  benchmark/fs-bench-pro/run-namespace.sh RUN_ID --all --source candidate --mode performance
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
