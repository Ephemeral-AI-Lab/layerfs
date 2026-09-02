# Payload create and read

## Status

Draft v0.1.3 family contract: 8 timed scenarios and 0 proof-only scenarios.
The two existing IDs retain their registered meanings; the six new IDs are not
registered.

## Problem statement

The registered payload campaign proves one 32 MiB cold create and one 32 MiB
sequential read. It does not show whether create cost scales with payload size
or whether repeated random reads remain bounded without changing filesystem
state.

## Goal

Keep `cold-create-32m` and `read-32m` byte-for-byte and boundary-for-boundary
unchanged, then add three nested create sizes and three nested random-read
loads. The family must distinguish sequential payload throughput from fixed
Workspace lifecycle cost.

## Files to read

- [Append-only benchmark contract](../benchmarking.md)
- [v0.1.3 parent plan](README.md)
- [`fs-bench-pro` campaign](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Shared workload helper](../../../../benchmark/fs-bench-pro/workload.rs)
- [LayerFS-only runner](../../../../benchmark/fs-bench-pro/run.sh)
- [Released v0.1.0 benchmark evidence](../../../../release-notes/0.1.0/benchmark-results.md)

## Fixed topology and lifecycle boundary

- Each row owns a fresh Store, SDK `Client`, LayerStack, genesis Layer, Branch,
  real-FUSE Workspace, process, and evidence directory.
- Fixture generation, Store/Client/container preparation, source sealing, and
  report writing are excluded and recorded separately.
- Create rows start with an empty genesis Layer. Their complete timed boundary
  is Workspace Create, fresh-process file creation and `sync_all`, Commit and
  Store visibility, then clean End.
- Read rows start from a Branch whose payload is prepared before the timer.
  Their complete timed boundary is Workspace Create, fresh-process reads,
  `UpToDate` Commit and visibility, then clean End.
- Fresh Store reconnect, exact filesystem verification, and cleanup are
  mandatory after each row. Their wall cost participates in the family budget
  but does not change the frozen `complete_lifecycle_ns` field.
- All filesystem operations use the production real-FUSE path. No materialized
  projection is a timed substitute.

## Timed scenarios

| Scenario ID | Status | Load | Exact timed operation | Required oracle |
| --- | --- | --- | --- | --- |
| `cold-create-32m` | Registered; frozen | Existing 32 MiB fixture | Existing empty-Branch create lifecycle, unchanged | Existing final size, digest, Commit, visibility, and reopen proof |
| `read-32m` | Registered; frozen | Existing 32 MiB file | Existing complete sequential-read lifecycle, unchanged | Exactly 32 MiB read; root and payload unchanged after reopen |
| `payload-create-1m` | Draft | First 1 MiB of the new 100 MiB fixture | Create one file, sync, Commit, visibility, End | Exact 1 MiB size, digest, canonical root, and reopen result |
| `payload-create-10m` | Draft | First 10 MiB of the same fixture | Same create lifecycle | Exact 10 MiB size, digest, canonical root, and reopen result |
| `payload-create-100m` | Draft | Complete 100 MiB fixture | Same create lifecycle | Exact 100 MiB size, digest, canonical root, and reopen result |
| `payload-random-read-1` | Draft | First 1 deterministic 4 KiB request | Read requests in one fresh process; no mutation | Transcript digest matches; Commit is `UpToDate`; root unchanged |
| `payload-random-read-10` | Draft | First 10 requests from the same schedule | Same random-read lifecycle | Same oracle for all 10 completed requests |
| `payload-random-read-100` | Draft | First 100 requests from the same schedule | Same random-read lifecycle | Same oracle for all 100 completed requests |

## Proof-only scenarios

| Count | Scenario IDs |
| ---: | --- |
| 0 | None; all required proofs are attached to the eight timed rows. |

## Tier/load rule and deterministic schedule

The shared multiplier is `a = 10`, applied to this family's declared load
unit:

- create load unit: 1 MiB, producing 1, 10, and 100 MiB;
- random-read load unit: one 4 KiB request, producing 1, 10, and 100 requests.

The new 1 MiB and 10 MiB fixtures are byte prefixes of the new 100 MiB fixture.
The lower random-read tiers are request prefixes of the 100-request schedule.
The existing 32 MiB rows are frozen anchors, not resized members of the new
tier schedule.

Candidate evidence uses exactly these three UTF-8 seed labels, one fresh family
sample per seed:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

New fixture blocks and request positions come from a domain-separated SHA-256
counter stream:

```text
SHA256(seed_label || 0x00 || domain || 0x00 || index_le_u64)
```

The create fixture concatenates digest blocks and truncates at 100 MiB. For
random reads, interpret the first eight digest bytes as little-endian `u64` and
reduce modulo `100 MiB - 4096 + 1`; the request is exactly 4 KiB. The transcript
oracle hashes each little-endian offset followed by its returned bytes, in
request order. Existing frozen rows do not consume this new seed stream.

## Required metrics and oracles

Record per row:

- complete wall and Workspace Create, execution, Commit, visibility, End, and
  reconnect/verification wall;
- inner workload wall, bytes requested/completed, and sequential MiB/s where
  applicable;
- random request count, size, ordered-offset digest, and transcript digest;
- process user/system CPU, peak RSS, cgroup peak, swap, and FUSE operation and
  transferred-byte counts;
- candidate, inserted, and reused objects/bytes, transaction maxima, Store
  semantic/allocation growth, Commit result, and canonical root;
- exact size and SHA-256 before and after fresh Store reconnect; and
- mount, process, output-reader, spool, Workspace, and lease cleanup state.

Missing metrics are evidence errors, not zeros. A read row must create no new
payload state solely because bytes were read.

## Expected-rate assumptions and family budget

The planning model for a complete scenario wall is:

```text
0.5 s
+ sequential_payload_MiB / 100
+ paths / 10,000
+ same_count_edits / 100
+ count_changing_edits / 50
```

The 0.5 s fixed component covers Create, Commit/`UpToDate`, End, fresh reopen,
verification, and cleanup, excluding workload terms. Sequential create/read
throughput is expected to be at least 100 MiB/s; namespace capacity is expected
to be at least 10,000 paths/s. The edit-rate terms are not governing loads for
this family.

One candidate campaign is three fresh samples of every timed row, one per seed.
The family wall is the sum of those 24 complete scenario walls. It excludes
fixture and environment preparation but includes mandatory reopen verification.

- Target family wall: **20 seconds**.
- Hard family wall: **40 seconds**.

## Acceptance criteria

- [ ] Run exactly the 8 timed IDs and no proof-only IDs.
- [ ] Preserve both existing scenario meanings and result fields.
- [ ] Prove 1/10/100 MiB create fixtures are nested deterministic prefixes.
- [ ] Prove random-read 1/10/100 loads are nested prefixes of one exact
  seed-bound request schedule.
- [ ] Use real FUSE, a fresh process, and fresh Store/Workspace state per row.
- [ ] Verify exact bytes, root, Commit outcome, fresh reopen, and cleanup.
- [ ] Retain all three candidate samples and every required metric.
- [ ] Meet the 20 s target and never exceed the 40 s hard family wall.
- [ ] Register new IDs only after source, fixture, seed, runner, schema, and
  evidence identities are frozen together.
