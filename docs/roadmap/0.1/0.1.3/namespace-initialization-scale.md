# Namespace initialization scale

## Status

Frozen implemented admission family: 4 timed scenarios and 0 proof-only
scenarios. v0.1.3 reruns the existing IDs without changing their fixture,
lifecycle, phase fields, or oracles.

## Problem statement

Large-namespace evidence is meaningful to v0.1.3 only if the existing v0.1.1
initialization/lifecycle rows remain unchanged. Redefining them as init-only,
changing their file sizes, or replacing FUSE with materialization would break
the accumulated benchmark contract.

## Goal

Rerun the four frozen namespace tiers as the initialization-scale family,
retaining the complete existing-directory lifecycle and its separately emitted
phase measurements through exact reopen verification.

## Files to read

- [Append-only benchmark contract](../benchmarking.md)
- [v0.1.3 parent plan](README.md)
- [v0.1.1 lifecycle checklist](../0.1.1/README.md)
- [`fs-bench-pro` namespace command](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Namespace runner](../../../../benchmark/fs-bench-pro/run-namespace.sh)
- [Existing-directory importer](../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- [Workspace Commit planner](../../../../crates/layerfs-workspace/src/changes.rs)

## Fixed topology and lifecycle boundary

- Fixture generation, Store creation, Client/container preparation, source
  sealing, and report writing remain outside product timing and are recorded
  separately.
- Each tier/sample owns a fresh deterministic native fixture, Store, Client,
  LayerStack, genesis Layer, Branch, real-FUSE Workspace, fresh edit process,
  and evidence directory.
- `complete_product_ns` starts immediately before
  `Client::initialize_layerstack` with Store and Client ready.
- The timed sequence remains initialize LayerStack, fork Branch, create
  real-FUSE Workspace, perform the existing ten-byte overwrite, Commit, clean
  End, reconnect the Store, and complete exact namespace verification.
- Existing phase fields remain separately emitted. Materialization is not a
  timed replacement, and no init-only scenario is substituted.

## Timed scenarios

| Scenario ID | Status | Regular files / directories / bytes | Exact timed boundary | Required oracle |
| --- | --- | ---: | --- | --- |
| `namespace-100` | Implemented admission; frozen | 100 / 1 / 250,000 | Existing complete product lifecycle | Manifest counts/digest and edited reopen digest match |
| `namespace-1000` | Implemented admission; frozen | 1,000 / 10 / 2,500,000 | Existing complete product lifecycle | Same exact oracle at the frozen tier |
| `namespace-10000` | Implemented admission; frozen | 10,000 / 100 / 25,000,000 | Existing complete product lifecycle | Same exact oracle at the frozen tier |
| `namespace-100000` | Implemented admission ceiling; frozen | 100,000 / 1,000 / 250,000,000 | Existing complete product lifecycle | Same exact oracle at the frozen ceiling |

## Proof-only scenarios

| Count | Scenario IDs |
| ---: | --- |
| 0 | None; exact reconnect verification is already inside every timed row. |

## Tier/load rule and deterministic schedule

This family is the exception to newly generated `a = 10` schedules. Its
already-frozen tiers remain exactly 100, 1,000, 10,000, and 100,000 regular
files. Each file has exactly 2,500 unique deterministic bytes derived from its
path, with 100 regular files per data directory. `MB` remains decimal and exact
byte counts are authoritative.

Fixture content, paths, edit target, ten-byte marker, manifest digest, and
namespace oracle remain those implemented by the existing namespace command.
No new randomness or seed changes their meaning. Candidate custody uses three
fresh samples per tier labeled:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

The labels distinguish fresh evidence only; they do not alter fixture bytes or
the frozen edit schedule.

## Required metrics and oracles

Retain every existing field, including:

- `layerstack_init_ns`, `branch_fork_ns`, `workspace_create_ns`, `edit_ns`,
  `commit_ns`, `workspace_end_ns`, `reopen_verify_ns`, and
  `complete_product_ns`;
- process user/system CPU, peak RSS, cgroup peak, swap, scanned files/bytes,
  FUSE operations, and transferred bytes;
- fixture regular files, data directories, logical bytes, manifest digest, and
  reopened digest;
- initialization and Commit candidate, inserted, and reused objects/bytes,
  transaction maxima, Store semantic/allocation growth, and phase receipts;
  and
- exact Branch head, Commit result, edit target bytes, canonical root, fresh
  reconnect, and mount/process/spool/Workspace/lease cleanup.

An unavailable field is an evidence error. A correct digest without exact
counts and byte totals is insufficient.

## Expected-rate assumptions and family budget

Use the shared planning model:

```text
0.5 s
+ sequential_payload_MiB / 100
+ paths / 10,000
+ same_count_edits / 100
+ count_changing_edits / 50
```

The fixed 0.5 s component covers Create, Commit, End, fresh reopen,
verification, and cleanup. Initialization is expected to sustain at least
10,000 paths/s and sequential payload handling at least 100 MiB/s. The one
ten-byte overwrite is a negligible same-count term; no count-changing edit is
present.

Candidate evidence is three fresh samples of each of the four timed rows. The
family wall is the sum of all 12 complete scenario walls. Fixture/environment
preparation is excluded and recorded separately; reopen verification remains
inside the scenario boundary.

- Target family wall: **30 seconds**.
- Hard family wall: **60 seconds**.

## Acceptance criteria

- [ ] Run exactly the four existing namespace IDs and no proof-only IDs.
- [ ] Preserve every fixture count, byte count, lifecycle phase, projection,
  edit, result field, and oracle.
- [ ] Use fresh fixture, Store, Workspace, process, and evidence state for each
  tier/sample.
- [ ] Retain three fresh samples per tier without changing content by seed.
- [ ] Complete all rows through exact real-FUSE Commit, End, reconnect,
  verification, and cleanup.
- [ ] Report all phase, path, byte, object, transaction, resource, root, and
  digest evidence without silent zeros.
- [ ] Meet the 30 s target and never exceed the 60 s hard family wall.
- [ ] Introduce a new ID rather than silently changing a frozen scenario.
