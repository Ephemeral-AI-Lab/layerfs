# `fs-bench-pro` v0.1.2 family format

> **Status:** Complete for all four family definitions and runners in v0.1.2.
> All files remain under
> `benchmark/fs-bench-pro`. Tracked by
> [GitHub issue #17](https://github.com/Ephemeral-AI-Lab/layerfs/issues/17).

## Goal

Make one selected performance case the default development loop while keeping
family membership, workload generation, result schemas, verification, custody,
and full admission deterministic.

The benchmark measures performance. Exact bytes/root/reopen and adversarial
proofs are separate verifier modes and never enter performance timing.

## Minimal repository layout

```text
benchmark/fs-bench-pro/
├── Cargo.toml
├── Dockerfile.layerfs
├── workload.rs                         # shared container workload dispatcher
├── families/
│   ├── init_namespace.rs               # frozen v0.1.1 family definitions
│   ├── edit_same_count.rs              # pure family IDs/schedules/oracles
│   ├── edit_count_changing.rs          # pure family IDs/schedules/oracles
│   └── store_footprint.rs              # pure Store control definitions
├── src/
│   └── main.rs                         # existing shared host harness + dispatch
├── run.sh                              # frozen v0.1.0 payload runner
├── run-namespace.sh                    # frozen v0.1.1 namespace family runner
├── run-edit-same-count.sh              # v0.1.2 family runner
├── run-edit-count-changing.sh          # v0.1.2 family runner
└── run-store-footprint.sh              # v0.1.2 Store runner
```

Do not create another crate. Issue 0 extracts the already coherent v0.1.1
namespace definitions into `families/init_namespace.rs` while preserving
`run-namespace.sh`, every registered ID, fixture, timing boundary, schema, and
retained result. Do not otherwise perform a speculative big-bang split of the
working payload code. Extract a shared helper from `main.rs` only when two
family paths actually need it.

Each `families/*.rs` file is a pure, standard-library-only definition module so
both the host benchmark binary and the standalone `workload.rs` binary can
include the exact same IDs, schedules, length equations, and expected manifests.
Update the Dockerfile to copy those files before compiling `workload.rs`. Do not
duplicate schedule logic between host and container.

The released namespace family receives a meaningful family identity and
descriptive aliases without rewriting frozen raw IDs:

| `family_id` | Frozen `scenario_id` | CLI/display alias |
| --- | --- | --- |
| `init_namespace` | `namespace-100` | `namespace-100-files-125mb` |
| `init_namespace` | `namespace-1000` | `namespace-1000-files-200mb` |
| `init_namespace` | `namespace-10000` | `namespace-10000-files-300mb` |
| `init_namespace` | `namespace-100000` | `namespace-100000-files-500mb` |

`run-namespace.sh --case` accepts either value and every new wrapper record
emits both. Historical JSONL remains untouched.

## Family module contract

Each family file owns:

```text
FAMILY_ID
all scenario IDs in stable display order
registered/frozen versus proposed status
fixture profile and exact size
seed labels
operation count
operation manifest generation
pre/post length equations
supplied/inserted/deleted/zero byte equations
paired control ID where applicable
performance receipt requirements
verification manifest generation
self-checks for prefix and bounds
```

It does not own:

- Store/Client/container creation;
- FUSE attachment;
- Commit/End orchestration;
- process supervision;
- source sealing;
- evidence directory creation;
- generic JSONL/report emission; or
- Git/build/environment custody.

Those remain shared harness responsibilities.

## Command surface

Thin family scripts translate a stable user-facing CLI into the host binary.
They require an explicit selected case unless `--all` is present:

```text
run-edit-same-count.sh RUN_ID CONTAINER_ID \
  --case overwrite-middle-4k-ops-100 \
  --seed 1 \
  --source candidate \
  --mode performance

run-edit-count-changing.sh RUN_ID CONTAINER_ID \
  --case append-tail-4k-ops-100 \
  --seed 1 \
  --source baseline \
  --mode performance
```

Required options:

| Option | Values | Rule |
| --- | --- | --- |
| `--case` | exact family ID | Required unless `--all`; rejects cross-family IDs |
| `--seed` | `1`, `2`, `3` | Required for a selected timed case |
| `--source` | `baseline`, `candidate` | Never inferred from Git state |
| `--mode` | `performance`, `verify`, `admission` | Defaults to `performance` |
| `--all` | flag | Required to run a complete family; rejected in ordinary performance development unless explicitly supplied |

No command named only `run` may silently execute every family. Existing
`run.sh` and `run-namespace.sh` retain their frozen meanings.

## Execution modes

### `performance`

- Runs one selected family/case/seed/source arm by default.
- Uses the fixed MacBook/Docker/Linux-FUSE product lifecycle.
- Times Create, fresh-process workload, Commit/visibility, and End.
- Emits latency, throughput, I/O, candidate, CPU, memory, and cleanup validity.
- Does not hash the complete result, compare canonical roots, reopen the Store,
  inject failures, run materialization, or execute proof groups.
- Rejects failed process status, incomplete operation count, wrong reported
  final length, timeout, OOM, swap, resource breach, or cleanup failure.

### `verify`

- Runs only the selected scenario or named proof group.
- Performs independent exact byte/length/zero-range oracle, canonical/root and
  inode checks, fresh reconnect/reopen, resource assertions, and cleanup.
- Produces no performance distribution and never updates performance summaries.

### `admission`

- Requires `--all`.
- Runs all three seeds for every timed family member.
- Retains baseline and candidate arms in deterministic alternating order.
- Runs the complete separate verifier/proof set afterward.
- Produces independent performance, verification, resource, cleanup, custody,
  and overall admission statuses.

For the expanded count-changing family, admission owns 25 paired primary IDs
plus six final-candidate scaling IDs. The scaling IDs remain in
`edit_count_changing.rs` and `run-edit-count-changing.sh`; they write separate
`scaling/` streams and do not acquire fictitious 256 KiB same-count controls.
The six exact IDs, fixture algebra, gates, and verifier coverage are frozen in
[the count-changing family specification](count-changing-file-edits.md).

## Timing schema

Retain the existing lifecycle equations:

```text
T0 before Workspace Create
T1 Create returns
T2 fresh-process workload/output completes
T3 Commit returns and Branch head is visible
T4 End returns

workspace_create_ns   = T1 - T0
execution_ns          = T2 - T1
commit_api_ns         = T3 - T2
layerstack_visible_ns = T3 - T0
workspace_end_ns      = T4 - T3
complete_lifecycle_ns = T4 - T0
```

Add operation throughput from the mutation-only sub-interval without moving the
public lifecycle boundaries:

```text
operations_per_second = completed_operations * 1e9 / inner_edit_ns
supplied_bytes_per_second = supplied_bytes * 1e9 / inner_edit_ns
rewrite_bytes_per_second = copied_payload_bytes * 1e9 / inner_edit_ns
```

Use `null` plus a status for an inapplicable or unavailable rate; never emit a
misleading numeric zero.

## Performance JSONL schema

Create an append-only schema rather than changing `fs-bench-pro-v4` in place:

```json
{
  "schema": "fs-bench-pro-edit-performance-v3",
  "family_id": "edit_count_changing",
  "scenario_id": "delete-middle-2k-on-1mib-ops-1-scale",
  "display_name": "Delete middle 2 KiB on 1 MiB",
  "cohort": "delete-shrink-scaling",
  "mode": "performance",
  "source_arm": "candidate",
  "seed": 1,
  "seed_label": "layerfs-v0.1.2-seed-1",
  "execution_profile": "macbook-docker-desktop-linux-fuse-v1",
  "fixture_profile": "count-changing-scaling-1048576-bytes-v1",
  "fixture_digest": "...",
  "operation": "delete",
  "position": "middle",
  "implementation": "temp-copy-fsync-rename",
  "operation_count": 1,
  "attempted_operations": 1,
  "completed_operations": 1,
  "paired_same_count_control_id": "not-applicable-scaling-file-size-cohort",
  "fixture_bytes": 1048576,
  "initial_file_bytes": 1048576,
  "edit_offset": 523264,
  "final_file_bytes": 1046528,
  "supplied_bytes": 0,
  "inserted_bytes": 0,
  "deleted_bytes": 2048,
  "copied_payload_bytes": 1046528,
  "read_payload_bytes": 1048576,
  "logical_zero_bytes": 0,
  "workspace_create_ns": 14550000,
  "execution_ns": 15000000,
  "inner_edit_ns": 12000000,
  "mutation_ns_per_operation": 12000000,
  "mutation_metric_role": "diagnostic-scaling",
  "commit_api_ns": 5000000,
  "layerstack_visible_ns": 34550000,
  "workspace_end_ns": 4000000,
  "complete_lifecycle_ns": 38550000,
  "operations_per_second": 83,
  "copied_payload_bytes_per_second": 87210666,
  "verification_status": "not-run-performance-mode",
  "cleanup_status": "pass"
}
```

The actual schema additionally requires FUSE/spool counts, piece/tree work,
candidate/inserted/reused object and byte counts, transaction maxima, CPU,
process/cgroup RSS, swap, OOM, timeout, cache profile, source seal, image digest,
and receipt-availability statuses. Keep fields flat unless a repeated structure
has independently justified nesting; existing parsers are line-oriented.

`fs-bench-pro-edit-performance-v1` remains the same-count schema and identifies
the earliest superseded count-changing diagnostic runs. Count-changing `v2`
corrected `inner_edit_ns` so it contains only the selected filesystem mutation
calls, while the cheap final-length validity check runs immediately afterward
and still invalidates a wrong result. Expanded-family release evidence uses
append-only `v3`, adds an explicit cohort/fixture/edit contract, and makes
average mutation latency the primary 256 KiB temp-copy gate. Copied MiB/s is a
secondary diagnostic.

Frozen v0.1.0 rows continue to emit their original schema plus a v0.1.2 wrapper
record containing `family_id`, descriptive `display_name`, source arm, and
campaign identity. Do not rewrite historical raw lines.

## Verification JSONL schema

Keep verifier results separate:

```json
{
  "schema": "fs-bench-pro-edit-verification-v3",
  "family_id": "edit_count_changing",
  "scenario_id": "delete-middle-2k-on-1mib-ops-1-scale",
  "verification_id": "exact-result",
  "mode": "verify",
  "source_arm": "candidate",
  "seed": 1,
  "status": "pass",
  "cohort": "delete-shrink-scaling",
  "fixture_profile": "count-changing-scaling-1048576-bytes-v1",
  "fixture_digest": "...",
  "fixture_bytes": 1048576,
  "edit_offset": 523264,
  "deleted_bytes": 2048,
  "inserted_bytes": 0,
  "expected_file_bytes": 1046528,
  "observed_file_bytes": 1046528,
  "expected_sha256": "...",
  "observed_sha256": "...",
  "expected_canonical_file_root": "...",
  "observed_canonical_file_root": "...",
  "expected_canonical_root": "...",
  "observed_canonical_root": "...",
  "root_status": "pass",
  "independent_oracle": true,
  "fresh_reconnect": true,
  "fresh_fuse_reopen": true,
  "fresh_reopen_status": "pass",
  "resource_status": "pass",
  "cleanup_status": "pass",
  "setup_ns": 10000000,
  "verification_ns": 25000000
}
```

Failure injection and conformance groups use the same verifier stream with a
descriptive `verification_id`. They do not masquerade as timed scenarios.
The `v2` verifier schema introduced separate Store/init/Workspace preparation in
`setup_ns` from edit, Commit, oracle, reconnect, reopen, digest, resource, and
cleanup proof work in `verification_ns`; the runner also retains each
verifier's external command wall separately. Count-changing `v3` additionally
binds the scaling cohort's fixture length, edit offset, byte algebra, expected
and observed result, and exact reopen proof.

## Evidence layout

```text
benchmark-results/fs-bench-pro/<family>/<run-id>/
├── environment/
│   ├── source-seal.json
│   ├── host.json
│   ├── docker.json
│   ├── image-digest.txt
│   └── runner-arguments.txt
├── performance/
│   ├── raw.jsonl
│   └── summary.json
├── verification/
│   ├── raw.jsonl
│   └── summary.json
├── scaling/
│   ├── raw.jsonl
│   ├── summary.json
│   └── verification.jsonl
├── scenarios/<scenario>/<source>/<seed>/
│   └── raw receipts and logs
├── run-status.json
└── report.md
```

Performance, verification, resource, cleanup, custody, and overall admission
statuses remain independent in `run-status.json`. A development performance run
may have overall status `performance-complete-verification-not-run`; it must not
be labeled release evidence.

## Pairing and summaries

Admission alternates source order:

```text
seed 1: baseline -> candidate
seed 2: candidate -> baseline
seed 3: baseline -> candidate
```

Summary rows report:

- median candidate and baseline phase walls;
- candidate-median/baseline-median ratios from three paired seeds;
- average mutation latency per operation as the primary 256 KiB temp-copy
  metric, plus operations/s and payload throughput as appropriate;
- absolute frozen gates where applicable;
- target/hard outcomes;
- CPU/RSS/swap and FUSE/spool/object receipts; and
- verifier status as a separate column, never folded into elapsed time.

If one pair is invalid, rerun the complete pair. Never rerun only the slower arm
or silently discard a valid outlier.

## Self-checks

Each family module and runner must leave one fast check behind:

```text
family IDs unique and in stable order
every 1/10 prefix equals the 100-operation schedule prefix
every count-changing row changes length on every operation
every same-count row preserves length on every operation
every primary count-changing row resolves a valid same-count control
every count-changing scaling row is explicitly unpaired and satisfies its
fixture/edit/final/copied/read byte algebra
all offsets and checked length equations remain valid through 100 operations
performance mode cannot invoke verifier code
--all is required for complete-family execution
performance and verification schemas reject unknown/missing required fields
```

Each `run-edit-*.sh --self-check` and `run-store-footprint.sh --self-check`
performs only parser, registry, schedule, and
schema checks. It starts no container and runs no product benchmark.

## Acceptance criteria

- [x] All new family code and runners live under `benchmark/fs-bench-pro`.
- [x] Exactly one pure definition file and one runner exist per new family;
  shared host/workload/lifecycle/evidence code is not copied.
- [x] The Docker workload and host harness consume the same family definitions.
- [x] One selected performance case/seed is the default development path;
  complete family execution requires explicit `--all`.
- [x] Performance and verification use different schemas, files, summaries, and
  status fields, and verification never enters performance time.
- [x] Existing payload/namespace commands and raw schemas remain compatible.
- [x] Paired order, invalid-run handling, receipts, custody, and evidence layout
  are deterministic and self-checked.
