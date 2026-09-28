# Issue 273 checkpoint 5: paired mounted-checkpoint execution specification

> **Status:** Prospective execution specification and registry. Committed before
> the checkpoint-5 harness was implemented and before any checkpoint-5 sample
> was taken. It freezes the question, the twelve performance selections, the
> five correctness/space selections, the fixtures, schedules, cache procedure,
> timer boundaries, limits, metrics, and result layout. It records no result.

## 1. Question and claim

Does the #273 active backing (v2 fences, tagged child targets, selected hot
directory, charged EOF/Base/Zero cursors, balanced carries, slot epoch reuse,
selecting-revision retirement, precharged G1/G2 reconcile) reduce the work of
mounted one-byte mutations and quick Commits against the frozen #271 control
`48b51e874a41b3e1e6c6661e145316df8b408f07`, at the same fixture, workload,
oracle, cache procedure and limits?

Permitted claims from this campaign are exactly:

- per matched row, an Exec-phase, Commit-phase, complete-command and
  charged-backing ratio between the control and the candidate, each reported
  separately and each with its own eligibility status;
- count, structural and physical-backing statements per row;
- an explicit statement that a row's numeric ratio is `INELIGIBLE` where the
  declared cache state could not be enforced equally.

Not permitted: an "every row is 2x" claim, a general 2x claim without complete
cache-qualified matched evidence, reusing historical #271 or #265/`issue261`
timings as a denominator, or presenting a count bound as a measured latency.
The 3 MiB all-charged-backing target applies **only** to the named one-file,
one-generation 4,096-separated-WRITE checkpoint (selection 12, the original
#248 gate). No other row inherits it.

## 2. Operation contract

| Field | Value |
| --- | --- |
| `operation_contract_id` | `issue273-mounted-checkpoint-v1` |
| `operation_surface` | public `WorkspaceApi` mount / exec / commit |
| `operation_entrypoint` | `layerfs_sdk::WorkspaceApi::{mount,exec,commit}` |
| `orchestration_executor` | `/bin/sh -c` inside the sandbox daemon, launched once per Exec |
| `mutation_executor` | ordinary POSIX `write`/`pwrite` from `/fixtures/bin/write-separated` through FUSE |
| `implementation_route` | daemon FUSE adapter → host Workspace service → C1 → C2 |
| `projection` | `WorkspaceApi::status` projection counts after Commit |
| `acknowledgement_boundary` | one explicit `WorkspaceApi::commit` per command |

The driver is `benchmark_shell` (unmodified, byte-identical source in both
arms). It makes no internal Workspace mutation, no direct FUSE callback, no
Bridge/service/C1 call, and no special edit API. Every measured command is
exactly one Mount, one generic Exec, one explicit Commit, one status, one
unmount and one sandbox delete.

## 3. Performance registry: twelve selections, one sample per arm

Execution order is row-major, control before candidate for every case. One
sample per case per arm. No n3, no best-of, no unchanged-arm rerun.

| # | Selection ID | Fixture | Executed shell command | Writes | Complete-command limit |
| ---: | --- | --- | --- | ---: | ---: |
| 1 | `issue273-append-100-10m-v1` | 10 MiB master | `/fixtures/bin/write-separated append data.bin 100` | 100 | 15 s |
| 2 | `issue273-append-512-10m-v1` | 10 MiB master | `... append data.bin 512` | 512 | 15 s |
| 3 | `issue273-append-4097-10m-v1` | 10 MiB master | `... append data.bin 4097` | 4,097 | 25 s |
| 4 | `issue273-dispersed-100-10m-v1` | 10 MiB master | `... dispersed data.bin 100` | 100 | 15 s |
| 5 | `issue273-dispersed-512-10m-v1` | 10 MiB master | `... dispersed data.bin 512` | 512 | 15 s |
| 6 | `issue273-dispersed-4097-10m-v1` | 10 MiB master | `... dispersed data.bin 4097` | 4,097 | 25 s |
| 7 | `issue273-repeated-100-10m-v1` | 10 MiB master | `... repeated data.bin 100` | 100 | 15 s |
| 8 | `issue273-repeated-512-10m-v1` | 10 MiB master | `... repeated data.bin 512` | 512 | 15 s |
| 9 | `issue273-repeated-4097-10m-v1` | 10 MiB master | `... repeated data.bin 4097` | 4,097 | 25 s |
| 10 | `issue273-clean-commit-v1` | retained 4,097-record base | `true` (zero mutations) | 0 | 15 s |
| 11 | `issue273-one-edit-commit-v1` | retained 4,097-record base | `... separated data.bin 1` | 1 | 15 s |
| 12 | `issue248-separated-4097-v1` (original #248 gate) | 8,194-byte master | `... separated data.bin 4097` | 4,097 | 25 s |

Byte equation for selections 1-11: `data.bin` starts as 10,485,760 bytes of
`A`; syscall `i` (zero-based) writes `B + (i % 24)`.
Append uses `O_APPEND`; dispersed uses
`pwrite(fd, &byte, 1, (104729 + i*2654435761) % 10485760)`; repeated uses
`pwrite(fd, &byte, 1, 5242880)`. Selection 12 starts as 8,194 bytes of `A` and
writes `X` at `pwrite(fd, &byte, 1, 2*i)`. `gcd(2654435761, 10485760) = 1`, so
the dispersed positions are a distinct, interior, nonadjacent set; the
harness self-check re-derives that property. One fd, one mounted syscall per
byte, one writer process, and only four `PROGRESS` lines on stdout.

The matrix schedule is the same one registered by
`core/docs/issues/273/ACTIVE-FORMAT-AND-EVALUATION-v1.md`; the three 4,097
matrix rows and the #248 gate are the prospectively named 25-second
exceptions. Every other performance row has a 15-second complete-command
limit. The independent verifier has its own 9-second limit per invocation and
never enters a speed comparison.

### Fixtures and preparation

- **10 MiB master**: the closed, validated #271 fourhop patterns master
  (`issue271/fourhop-patterns-prepared-v1/master`, Store/history sealed by
  SHA-256, verified `PASS` at preparation). Reused, never regenerated.
- **8,194-byte master**: the closed #271 gate master
  (`issue271/combined-baseline-prepared-v1/master`) selected by the published
  gate prepared record. Reused, never regenerated.
- **Retained 4,097-record base** (selections 10 and 11 only): one preparation
  command on a fresh clone of the 10 MiB master that mounts once, executes
  `... dispersed data.bin 4097`, commits once, and closes; its wall is recorded
  as preparation, and it is produced **once** and reused byte-identically by
  both arms, so both arms read the same fixture bytes. Its own head is
  re-proved by each arm's own verifier before the measured command runs, and
  that proof is outside the measured command.
- Every measured arm takes an independent writable `shutil.copyfile` copy of
  the sealed Store/history. A clone is setup reuse and never a cold claim.
- Diagnostics are enabled identically in both arms:
  `LAYERFS_COMPLEXITY_DIAGNOSTIC=1` on host and daemon, and a declared
  `LAYERFS_FUSE_WRITE_SAMPLE_INTERVAL` per count (25 for 100-write and
  quick-Commit rows, 128 for 512-write rows, 1024 for 4,097-write rows). The
  observer overhead is therefore part of both arms and is disclosed.
- `LAYERFS_CONSTRUCTION_WORKERS=1` in every arm; no row raises it, and no row
  raises a timeout, a quota or a worker count.

## 4. Declared cache procedure

Applied identically to both arms, immediately before each measured command:

1. The cloned Store and history are mapped `PROT_READ|MAP_SHARED` and
   invalidated with `msync(MS_SYNC|MS_INVALIDATE)`, the established
   `darwin-shared-mmap-invalidate-mincore-v1` method from
   `benchmark/fs-bench-pro/shared/cold.py`. No fixture byte is written.
2. Whole-input residency is then re-checked with `mincore`; the page count and
   the resident page count are recorded for both files.
3. The launch gap between the last invalidation and process launch is recorded
   and must be at most 1 second for the row to be cache-`PASS`.
4. A positive control (`cold.py self_check`) may be run outside the measured
   command to prove the mechanism on this machine; its result is recorded.

Known, declared limitation: the daemon's private backing is created inside the
sandbox container (Linux), and the Exec of the same measured command writes the
pages that Commit later reads. The container's page cache cannot be invalidated
between Exec and Commit without modifying the frozen product or adding a
container-side helper between two product calls. The campaign therefore
declares the intra-command state instead of pretending to enforce it:
host-side source pages are cold-invalidated and verified, while the private
backing pages read by Commit are those written by this same command's Exec.
The Commit-phase and complete-command ratios consequently inherit that state
and are reported as `INELIGIBLE` for a numeric speed claim; the Exec-phase
ratio, the counts and the charged backing are not credited by that state and
are reported under their own status. No priming, no warm expected ranges and
no pooling warm and cold rows.

## 5. Timing boundaries and metrics

| Metric | Start | End | Includes |
| --- | --- | --- | --- |
| `driver_mount_ns` | before `WorkspaceApi::mount` | after it returns | mount only |
| `driver_exec_ns` | before `WorkspaceApi::exec` | after it returns | writer process, FUSE callbacks, product admission |
| `driver_commit_ns` | before `WorkspaceApi::commit` | after it returns | C1 lowering, C2 Store/history write |
| `complete_command_wall_ns` | before driver launch | after the driver exits | server open, container create, mount, exec, commit, status, unmount, container delete |
| `verifier_wall_ns` | before oracle launch | after it exits | separate scope, never in a speed comparison |
| `preparation_wall_ns` | before a preparation command | after it exits | fixture preparation only, reported separately |

`complete_command_wall_ns` is the external command wall of the whole measured
command and is labelled as such; it is never substituted for an inner metric.
Raw durations end in `_ns`, quantities in `_bytes`, counts in `_count`. All
clocks are monotonic (`time.monotonic_ns`, `Instant`).

## 6. Reported counters and resources

Per row: driver receipt (`mount_ns`, `exec_ns`, `commit_ns`, `operation_ns`,
`cleanup_ns`, `projection_counts`, `upstream_calls`,
`consumer_accounted_bytes`, `unmount_ok`, `sandbox_delete_ok`); writer
`PROGRESS` quartiles; `LFS_WRITE_SAMPLE v=3` class checkpoints with acquisition
and publication walls plus backing/metadata `allocated_bytes`,
`reserved_bytes`, `retained_payloads`, `ledger_reads/writes`,
`metadata_reads`, `routine_scans`; `LFS_EXTENT_SPLICE`/`LFS_EXTENT_EDGE`/
`LFS_METADATA_OWNER` extent-tree and ownership counters; `LFS_C1_EDIT_LOAD`,
`LFS_FILE_INPUT`, `LFS_C1_SAVE_COUNT`, `LFS_PIECE_LOWER` commit-source
counters; `LFT1` telemetry including `sampled_max_rss`, `cpu_shared_ns`,
`largest_gap_ns` and sample counts, scoped as process-shared samples of the
driver process. `st_blocks * 512` of the clone Store/history. Container cgroup
anonymous/file-cache memory and swap are **unavailable** in this driver
version and are recorded as unavailable rather than as zero. A lifetime or
process-shared sample is never presented as a phase peak.

## 7. Correctness and space selections

Each execution is capped at 60 seconds. These are correctness/space rows, not
speed rows; they carry `performance_claim=false`.

| Selection ID | Required scope |
| --- | --- |
| `issue273-many-file-128-v1` | 128 one-byte files; exact bytes/identities, shared backing and refunds |
| `issue273-multi-exec-v1` | three sequential 100-write Execs before one Commit; incremental backing across commands |
| `issue273-g1-g2-v1` | three consecutive edited generations; old reader held across successors |
| `issue273-retained-32-v1` | one edit and pin in each of 32 generations; retention and physical release |
| `issue273-mutations-v1` | truncate/hole, rename/unlink, aliases, open-unlinked handles, quota refusal, failed/uncertain backing and clean close |

Reuse is permitted only where exact identity and coverage satisfy the current
contract. Reused proof keeps its original row name and status; omissions are
recorded explicitly and are never renamed into a passing registered ID.

## 8. Oracle and verification

The independent oracle is one binary, built once from
`core/crates/layerfs-server/examples/verify_checkpoint5.rs` and reused
byte-identically by both arms. It reopens the arm's cloned Store/history
read-only, checks the genesis and branch identity, the old and new Commit
parentage, inventories every path and inode kind, checks directory and file
metadata, and hashes every byte of the old and the new head against manifests
derived by the harness directly from the schedules in §3. It re-derives the
declared write schedule independently of the writer's progress output,
including final length and changed-run count. Every performance row is also
verified by that arm's own `verify_shell` where the frozen arm verifier
supports the row's schedule (selections 1, 4, 7, 10, 11, 12 and both master
re-proofs); for selections 2, 3, 5, 6, 8 and 9 the arm-native verifier
hard-codes the 100-write schedule and cannot check the row, which is recorded
as an explicit omission and covered by the shared oracle instead. No sampled
byte oracle is used.

## 9. Result layout and statuses

```
benchmark-results/fs-bench-pro/issue273/checkpoint5/
  prepared-<arm>/            builds, archived binaries, sealed masters, oracle manifests, image IDs
  retained-base/             the one retained 4,097-record base and its preparation receipt
  <arm>/<NN>-<selection>/    one attempt: case files, driver stdout/stderr, receipt.json, SHA256SUMS
  campaign.json              registry, order, per-row status
  report/                    derived tables + SHA256SUMS
```

Every attempt uses a fresh, non-overwriting path. Statuses are `PASS`, `FAIL`
(started and missed a bound, or a correctness/cleanup failure), `INCOMPLETE`
(instrumentation or counters incomplete), `INELIGIBLE` (cache state not
enforceable, or a numeric claim the contract does not support) and `NOT_RUN`
(missing prerequisite, with the measured preparation/attempt wall and the
exact missing scope). Every failed, incomplete, ineligible or unrun cell is
preserved. No unchanged arm is repeated for a better number; a diagnosis uses
the retained receipts or a separately labelled count-driven diagnostic.

## 10. Production LOC

Harness, oracle example, specification and receipts add **zero** production
source lines: `core/benchmark/` is not product source, and
`core/crates/layerfs-server/examples/` is an example binary outside product
`src/`. Each commit records
`Production LOC: <before> -> <after> (delta 0)` with the counting command.

## Revision 1 (pre-sample): counter contract is arm-declared

No checkpoint-5 sample existed when this revision was written. The v2 active
backing retired the version-1 private ordered index, and with it the
`LFS_EXTENT_SPLICE` extent-tree counter that the v1 attachment emitted. The
counter contract is therefore declared **per arm** instead of being forced
identical:

- Arm-neutral and required from both arms: the `benchmark_shell` receipt, its
  `projection_counts` write class, the writer's four `PROGRESS` quartiles, the
  `LFS_WRITE_SAMPLE` class checkpoints declared in §3, `LFS_C1_EDIT_LOAD`,
  `LFS_FILE_INPUT` and the `LFT1` telemetry.
- Arm-declared: `LFS_EXTENT_SPLICE`/`LFS_EXTENT_EDGE`/`LFS_METADATA_OWNER`.
  When an arm emits the extent counters they must be complete (one splice per
  declared write, matching quartiles); when it does not, the row records
  `extent_counters_emitted_by_this_arm=false` and the extent-bound statement is
  reported as not measured for that arm rather than as zero.
- A row is `INCOMPLETE` when a required arm-neutral counter is missing or
  inconsistent, or when an emitted arm-declared counter is incomplete.

The retained 4,097-record base is proved by the shared oracle (which re-derives
arbitrary counts) and additionally by the arm's own frozen verifier as a plain
old/new tree identity without pattern keys, because that frozen verifier
hard-codes the 100-write pattern schedule. Both proofs are recorded.

## Prospective optimization correction (new identities; prior receipts unchanged)

The original `darwin-shared-mmap-invalidate-mincore-v1` procedure hashed each
input *after* its eviction and whole-input check, faulting it back in. New
attempt-v3/prepared-v2 receipts use `darwin-shared-mmap-invalidate-mincore-v2`:
validate the SHA-256 and size of **every** cloned input first; evict **every**
input next; perform the final whole-input residency checks next; then launch
without reading any input between that last check and launch. Record both
per-file hash/size/residency and the final-check-to-launch gap (at most 1 s).
The prior procedure and its receipts retain their historical, unqualified status.
Neither this host check nor private Linux O_DIRECT by itself proves the entire
Commit cache domain. A labelled `run --diagnostic` skips the separately bounded
verifiers, records `SKIPPED`, and can never be used for admission. Ordinary
`run` still invokes both applicable verifiers. These changes do not alter the
registered workload/limits; retained-control construction still requires a
live private journal, so existing retained fixture rows cannot be promoted.

### Prospective C1 observer v4 correction

`attempt-v4` accepts literal zero C1 work only from one complete emitted v1
C1 edit-load row and one complete emitted v1 file-input row with all required
nonnegative fields. A missing, malformed or interleaved row is
`INCOMPLETE`/`missing_or_malformed_or_interleaved`, not an inferred zero even
when the public Commit returns `UpToDate`. This correction applies to both
arms through the common harness and is not a retrospective revision of any
`attempt-v2/v3` receipt. The old retained controls remain invalid because
their committed-and-reattached Store cannot produce a live pinned private
journal in the same Workspace through the current SDK mount/Exec/Commit-only
interface. They must use a prospectively distinct scenario or remain
`NOT_RUN/INELIGIBLE` rather than being promoted.

### Prospective invalid-control and report-v2 handling

Do **not** run selections 10/11 against the old committed-and-reattached
`retained-v1/v2` fixture. The public SDK exposes mount, sequential Exec,
Commit, status and unmount, but no pin/lease capable of keeping a live private
journal across the preparation Commit in the same Workspace. Background or
concurrent Exec/command leases are outside #273. `retained` now refuses, and
`run`/`campaign` retain a `NOT_RUN` blocker receipt for these selections
without acquiring or timing a false control; the optional campaign `--retained`
argument is historical only. They still occupy registered row slots with the
same 15-second limits, and need a separately authorized v2 control design to
become runnable. `report-v2` emits a ratio only for two admitted, matched,
cache-qualified `PASS` rows; retained campaign-3 and earlier reports remain
unchanged. O_DIRECT is a requested private data-cache bypass in both arms,
not categorical proof that private/VM/backend/host caches are cold or equal.
