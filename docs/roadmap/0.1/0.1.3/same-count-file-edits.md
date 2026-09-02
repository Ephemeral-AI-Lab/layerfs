# Same-count file edits

## Status

Draft v0.1.3 family contract: 5 timed scenarios and 0 proof-only scenarios.
`small-edit` and `edit16` retain their registered meanings; the three random-edit
IDs are not registered.

## Problem statement

The registered rows cover one fixed ten-byte overwrite and sixteen repeated
overwrite/Commit cycles. They do not measure a single Commit after a nested
batch of variable-size writes at deterministic random positions while file
length and path count remain unchanged.

## Goal

Preserve the two registered edit rows, then add 1-, 10-, and 100-write same-count
batches that isolate dirty-range capture and one Commit from file creation,
deletion, append, prepend, and truncate work.

## Files to read

- [Append-only benchmark contract](../benchmarking.md)
- [v0.1.3 parent plan](README.md)
- [`fs-bench-pro` campaign](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Existing edit workload](../../../../benchmark/fs-bench-pro/workload.rs)
- [Workspace file I/O](../../../../crates/layerfs-workspace/src/file_io.rs)
- [Workspace Commit planner](../../../../crates/layerfs-workspace/src/changes.rs)
- [Released v0.1.0 benchmark evidence](../../../../release-notes/0.1.0/benchmark-results.md)

## Fixed topology and lifecycle boundary

- Each row owns a fresh Store, Client, imported 32 MiB `payload.bin`, genesis
  Layer, Branch, real-FUSE Workspace, and evidence directory.
- Fixture import, Store/Client/container preparation, source sealing, and
  report writing stay outside the row timer and are recorded separately.
- `small-edit` remains one fresh process, its existing deterministic ten-byte
  overwrite, one Commit, visibility, and clean End.
- `edit16` remains one Workspace with sixteen existing fresh-process
  overwrite/Commit cycles followed by clean End.
- Each new random row uses one Workspace and one fresh process that performs
  all declared writes, calls `sync_all` once, then performs one Commit,
  visibility check, and clean End.
- New writes never change target length or path count. Fresh reconnect, exact
  verification, and cleanup are mandatory and included in the family wall.

## Timed scenarios

| Scenario ID | Status | Load | Exact timed operation | Required oracle |
| --- | --- | --- | --- | --- |
| `small-edit` | Registered; frozen | Existing one ten-byte overwrite | Existing complete lifecycle, unchanged | Existing final digest, Commit, visibility, and reopen proof |
| `edit16` | Registered; frozen | Existing 16 overwrite/Commit cycles | Existing one-Workspace lifecycle, unchanged | Sixteen created Commits and exact final digest/root |
| `edit-same-random-1` | Draft | First 1 deterministic write | One process, 1 write, one sync, one Commit, visibility, End | Sequential oracle application yields exact 32 MiB digest/root |
| `edit-same-random-10` | Draft | First 10 deterministic writes | One process, 10 writes, one sync, one Commit, visibility, End | Sequential oracle application yields exact 32 MiB digest/root |
| `edit-same-random-100` | Draft | First 100 writes from the same schedule | Same lifecycle with 100 writes | Sequential oracle application yields exact 32 MiB digest/root |

## Proof-only scenarios

| Count | Scenario IDs |
| ---: | --- |
| 0 | None; all required proofs are attached to the four timed rows. |

## Tier/load rule and deterministic schedule

The shared multiplier is `a = 10`; this family's new primary load unit is one
same-count write. The 1- and 10-write rows are exact prefixes of the 100-write
row for the same seed.

Candidate evidence uses one fresh sample for each exact seed label:

```text
layerfs-v0.1.3-seed-1
layerfs-v0.1.3-seed-2
layerfs-v0.1.3-seed-3
```

For zero-based operation index `i`, compute:

```text
D = SHA256(seed_label || 0x00 || "edit-same" || 0x00 || i_le_u64)
length = 1 + little_endian_u16(D[0..2]) mod 4096
offset = little_endian_u64(D[2..10]) mod (32 MiB - length + 1)
```

Write bytes are the domain-separated SHA-256 counter stream for
`"edit-same-bytes"` and `i`, truncated to `length`. Overlaps are allowed and
later operations win. The exact oracle applies the same ordered schedule to a
copy of the source fixture. Existing `small-edit` and `edit16` schedules remain
unchanged and do not consume this stream.

## Required metrics and oracles

Record per row:

- complete wall and Workspace Create, execution, each Commit where applicable,
  visibility, End, reconnect/verification, and cleanup wall;
- attempted/completed write operations and bytes, unique changed bytes,
  overlapping bytes, dirty-range count, and captured/charged spool bytes;
- capture, candidate planning, content, namespace, admission, publication, and
  rebase phase walls;
- process user/system CPU, peak RSS, cgroup peak, swap, FUSE operation counts,
  and FUSE payload bytes;
- candidate, inserted, and reused objects/bytes, transaction maxima, Store
  semantic/allocation growth, and Commit IDs/results; and
- unchanged file/path count, exact 32 MiB length, SHA-256, canonical root,
  fresh reopen result, and complete resource cleanup.

Missing receipt fields are evidence errors. The oracle must detect an incorrect
write order even when the final length is correct.

## Expected-rate assumptions and family budget

Use the shared planning model:

```text
0.5 s
+ sequential_payload_MiB / 100
+ paths / 10,000
+ same_count_edits / 100
+ count_changing_edits / 50
```

The fixed 0.5 s component covers Create, Commit/`UpToDate`, End, fresh reopen,
verification, and cleanup. Same-count edit handling is expected to sustain at
least 100 operations/s. The sequential, namespace, and count-changing terms are
not governing loads for the new rows.

Candidate evidence is three fresh samples per timed row, one per seed. The
family wall is the sum of all 15 complete scenario walls; preparation is
excluded and mandatory reopen verification is included.

- Target family wall: **10 seconds**.
- Hard family wall: **20 seconds**.

## Acceptance criteria

- [ ] Run exactly the 5 timed IDs and no proof-only IDs.
- [ ] Preserve `small-edit` and `edit16` meanings, timing, and schemas.
- [ ] Keep file length and path count unchanged in all three new rows.
- [ ] Prove the 1- and 10-write schedules are exact prefixes of the 100-write
  schedule for each seed.
- [ ] Use one fresh process, one sync, and one Commit for each new row.
- [ ] Verify write order, exact bytes, canonical root, reopen, and cleanup.
- [ ] Retain three fresh samples and all operation, range, resource, Store, and
  Commit receipts.
- [ ] Meet the 10 s target and never exceed the 20 s hard family wall.
- [ ] Register new IDs only with frozen source, seed, fixture, runner, schema,
  and evidence identities.
