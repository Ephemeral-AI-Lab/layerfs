# Bounded streaming and accumulated work

> **Status: Current planning checklist; no release candidate exists.**
> Source, scope and evidence: [README](README.md).

## Proposed population and window model

Workspace size, file bytes and history may grow on storage within explicit
format/provider/quota limits. For a fixed admitted concurrency, resident memory
must depend on admitted windows rather than those populations. This is a design
target, not a measured capability or a promise of infinite storage.

| Growing population | Storage authority | Resident selection |
| --- | --- | --- |
| Inodes, names, extents, owners and locations | Daemon SQLite | Indexed cursor pages bounded by rows and encoded bytes |
| Captured generations and pending construction facts | Versioned SQL records / immutable source references | Selected roots, bounded drafts and progress cursors |
| Payload and historical chunks | Immutable backing and MinIO packs | Source, chunker, codec, pack, upload and read windows |
| Committed namespace, locators and history | Global SQLite / canonical immutable objects | Bounded verification and publication batches |
| Cleanup candidates | Indexed retirement records | Bounded claimed batch with retained pins/outcome custody |

Symbolic accounting to freeze after interface selection:

```text
admitted memory = fixed service state + engine/cache allowance
                + sum(active admitted pipeline windows)
                + handle/pin/control/completion allowances
                + observed kernel/socket/file-cache allowance
```

Count simultaneous allocations, including overlapping capture, encoding,
upload, reads, verification, installation and cleanup. Idle Workspaces must not
reserve full Commit buffers. Bound connection pools and active operations;
apply backpressure before acquiring the next window. Refuse new effects exactly
when admission fails and preserve writes already accepted.

Metadata scratch size is a separate disk quota. Configurability can permit
larger workloads, but does not fix a proportional RAM allocation or an inefficient
algorithm. Derive the total-record/byte profile from actual records and lifetimes;
do not inherit the Phase 5 scratch ceilings as a new Phase 6 format limit.

## Work laws to preserve

- Use indexed keyset traversal and bounded records/values. Avoid whole-frontier
  vectors, offset rescans and loading a complete file's chunk map before reading.
- Maintain final-state interval overwrite, shared immutable subtrees and opaque
  predecessor spans. Repeated writes must not create recursive WRITE history
  that every later Commit replays.
- Change construction visits the selected changes and required graph/order work;
  it must not copy the entire Workspace on each Commit. Freeze complexity in
  terms of changed entries, path depth, chunks and required certification work.
- Incremental directory/root maintenance must address
  [#289](https://github.com/Ephemeral-AI-Lab/layerfs/issues/289). Putting a quadratic
  scan into SQL is not an accepted scaling fix.
- Immutable application generations permit short database transactions. Avoid
  holding one SQLite read transaction across a long upload/Commit; long WAL
  readers can prevent checkpoint progress and grow retained WAL storage.
- Retire selected batches only after leases, pins, delta bases and uncertain
  outcomes permit it. Cleanup must make healthy progress under the supported load.

## Open — required physical proof

SQLite's [cache size](https://sqlite.org/pragma.html#pragma_cache_size) is a page
cache suggestion, not a whole-process memory limit.
[Temporary indexes/tables](https://sqlite.org/tempfiles.html) also use page
caches; selecting file-backed temporary storage does not remove their RAM cost.
[WAL](https://sqlite.org/wal.html) has one writer and checkpoint/read-lifetime
constraints. Define supported engine settings and query plans, temp/WAL/disk
admission and the actual provider capability.

Prove resident application, engine, kernel and file-cache behavior under
deterministically established overlap. A heap-only measurement, lifetime peak,
or containment kill does not establish bounded healthy progress. Define the
physical observation/enforcement available on Linux and Darwin and record gaps.
Keep byte correctness, algorithmic counts and speed qualification separate.
