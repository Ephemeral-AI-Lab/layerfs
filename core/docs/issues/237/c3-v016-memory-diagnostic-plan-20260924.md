# #237: prospective matched Init memory diagnostic

> **Status: Research; informative and not a product contract.** Frozen before
> the additional Core operation. This is a memory-cause diagnostic, not a
> second performance arm or a #236 SDK benchmark row.

## Arms and claim

Reuse the retained v0.1.6 release `Client::initialize_layerstack` receipt
from the exact seed-1 SHAKE 100k/500-MB manifest (SHA-256
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`).
Do not call the old public operation again. Its driver already recorded
`getrusage(RUSAGE_SELF).ru_maxrss` and `proc_pid_rusage` current RSS and
physical footprint at **t0 after Store/client setup, immediately before
the public call**, and **t1 immediately after the call**. It also recorded
SQLite connection cache usage, source opens/reads and bounded importer
buffers. The old `ru_maxrss` grew from t0 to a new lifetime high at t1,
so its 87,932,928-B increase is a valid incremental high-water reading.

Run **one** new count-driven diagnostic on the promoted C3 Core SDK route
in the isolated worktree with Cargo `--release --locked`. Its temporary
benchmark-only driver will use the same two t0/t1 events and the same
`getrusage` and `proc_pid_rusage` APIs, reporting `ru_maxrss`, current RSS,
physical footprint, user/system CPU and swaps in bytes/nanoseconds.
Core's t0 follows `Host::create`; t1 immediately follows the one
`Client::init_project` result. The primary comparison is t1 process
high-water RSS and the increase above t0 **only if both runs set a new
high-water**. These are comparable observation windows around different
public APIs, not matched work or a speed comparison.

## Cause probes and input custody

- Use the same prepared manifest, a fresh independently verified byte
  copy and fresh content/History Stores. Invalidate source payload pages,
  then require 0/126,206 resident pages before launch. Directory/inode
  metadata residency remains unqualified. Read source payload residency
  again after the child exits; it is host page cache, outside process RSS.
- Record `wait4` per-child lifecycle CPU and peak RSS for the new Core
  diagnostic as a secondary scope. Do not compare that figure to old t1
  RSS as if the windows matched.
- Inside the temporary C3 source, read the live Save connection's
  `SQLITE_DBSTATUS_CACHE_USED` at the end of each of the file,
  prerequisite and tree Saves, before its connection is dropped. State
  that these are point readings, not a simultaneous process-cache peak.
  Do not change any SQLite policy or build flag.
- Count source file opens/read calls/read bytes, handoff batches and
  objects, inserted objects, pack creates/appends, and acknowledged
  transactions. Record the file import's retained entry/job vector
  capacities and path/name payload lengths. These explain work and
  explicit reservation; they are not a complete heap census.
- Independently reopen and verify all 101,001 paths, portable metadata,
  sizes and SHA-256 of 500,000,000 bytes. Retain the fresh Store/History,
  resource probes, build/source/instrument seals and every failed attempt
  in append-only output. Keep all reported times diagnostic only.

The comparison will show exact raw and MiB values, the remaining
unattributed RSS, and the differences in source/Store/History route.
SQLite caches, vector reservations, allocator overhead and host page
cache are different accounting domains and must not be added as though
they were disjoint parts of peak RSS. A stage with no matching probe is
`UNMEASURED`, not zero. The separate #229 sparse-history gate remains open.
