# Release benchmark plan

| Field                    | Value                                           |
| ------------------------ | ----------------------------------------------- |
| Status                   | Normative version 0.1 release gate              |
| Comparison               | Prior accepted result and optional DOFS control |
| Correctness prerequisite | All mandatory correctness tests pass            |

## 1. Benchmark principles

Benchmarks measure the complete boundary claimed by their name. Setup, fixture creation,
cache preparation, checkpointing requested only for measurement, and teardown remain
outside the timed region and are reported separately.

Every run MUST use a fresh isolated SQLite database. A paired DOFS run uses a different
database and no shared warm engine state. Branch-only workloads may report DOFS as
unsupported; they may not change the fixture to make it pass.

Five measured whole-workload trials are required for large I/O cases. Small operations
MUST execute at least 100 iterations inside each measured trial so their p50, p95, and
p99 use a high-volume sample without a long-running soak. Reports also include minimum,
maximum, and arithmetic mean. A result is invalid unless the final bytes or separately
computed digest are correct.

No release gate waits merely to accumulate elapsed time. The mandatory smoke profile in
section 3 has a 60-second hard limit per target. Larger benchmarks are finite
iteration-based jobs. The default correctness and benchmark selection SHOULD finish
within 10 minutes per target on the reference runner. Cases that cannot fit are labeled
extended and do not block initial integration.

## 2. Required environments

The release matrix includes:

1. file-backed raw Node.js SQLite on the reference Computer runner;
2. Cloudflare Durable Object SQLite in a production-like preview deployment;
3. privileged Linux with a real FUSE mount;
4. the Computer development shim as a non-gating diagnostic; and
5. explicitly selected DOFS on the same Computer runner for common workloads.

The Cloudflare entry above means a stable, dedicated, non-production environment in a
user-controlled Cloudflare account. It MUST use the exact Worker bundle, compatibility
date, limits, and SQLite Durable Object migration accepted by the credential-free M6
local gate. A Node-backed mock, local Miniflare run, production namespace, or unclaimed
temporary Wrangler deployment does not satisfy this release environment.

Hosted preview execution is an external mutation and begins only in M9. Its command MUST
fail closed unless all three conditions hold:

1. the user has explicitly authorized the hosted run;
2. `EFS_ALLOW_CLOUDFLARE_PREVIEW=1` is set for that command; and
3. Wrangler reports authenticated access to the intended non-production account and
   environment.

Credentials alone are not authorization. Test code MUST NOT print, persist, or commit
OAuth tokens, API tokens, account secrets, or secret-bearing environment dumps. The M6
local conformance and smoke commands MUST neither inspect this opt-in nor contact
Cloudflare.

Reports MUST identify commit, runtime, operating system, CPU, memory, storage, SQLite
version and journal mode, cache target, mmap limit, database and journal ceilings, page
size, FastCDC parameters, manifest format, and every resource limit. They MUST state
whether OS cache dropping succeeded.

## 3. Fast pre-integration smoke profile

Before running the detailed benchmarks, Node SQLite, hosted-preview Durable Object
SQLite, and real FUSE MUST each complete this profile within 60 seconds on its reference
runner. The hosted profile is in addition to, not a replacement for, the identical
faithful-local Durable Object smoke accepted at M6:

1. initialize, write, reopen, and verify one 16 MiB pseudorandom file;
2. perform 5,000 one-byte COW edits distributed across same-page, clustered, and
   scattered positions;
3. execute 2,000 create, stat, rename, link, unlink, and directory operations;
4. run 16 readers and 16 writers for 64 bounded operations each;
5. force three close and reopen or runtime restart cycles;
6. interrupt and resume one bounded garbage-collection run; and
7. finish with exact digest, namespace, lease, reservation, and `efs_usage`
   verification.

The smoke profile is operation-count based. It MUST NOT sleep, wait for a soak duration,
create a 24-hour cursor, or repeat idle work. If a target cannot finish in 60 seconds,
the report identifies the slow operation; the gate is not made longer to hide it.

An optional `load-10m` profile MAY run after smoke. It has a hard 10-minute deadline and
continuously repeats real operations: small COW edits, namespace mutations, bounded
reads and writes, close and reopen, branch publication, collection, and verification. It
never sleeps to consume time. Its report records completed iterations, throughput,
errors, memory high-water, restarts, and final integrity. This profile is useful rollout
evidence but is not a version 0.1 integration blocker.

## 4. Common fixtures

Each size is generated from a recorded seed and has a checked digest:

| Fixture            | Sizes                                     | Purpose                      |
| ------------------ | ----------------------------------------- | ---------------------------- |
| Pseudorandom       | 100 MiB, 1 GiB                            | Worst-case CAS reuse         |
| Duplicate-heavy    | 100 MiB, 1 GiB                            | Deduplication and reuse      |
| Partially repeated | 100 MiB, 1 GiB                            | Mixed source-code-like reuse |
| Large logical      | 10 GiB from seeded repeated regions       | Manifest scaling             |
| Small-file tree    | 100,000 files                             | Namespace and metadata scale |
| Revision chain     | 1,000 and 100,000 changes                 | Replay and catch-up          |
| Branch fan-out     | 50 independent and 50 conflicting writers | Publication                  |

The 10 GiB fixture uses deterministic shared content to remain within the configured
durable-payload cap, but its logical manifest and range positions genuinely span 10 GiB.

## 5. Required metrics

Every applicable workload reports:

- logical bytes requested, returned, written, and materialized;
- CAS object bytes read, hashed, inserted, reused, and retained;
- COW page size, versions read, created, pinned, and reclaimed;
- manifest roots and nodes read, verified, created, reused, and transferred;
- manifest depth, leaf entries scanned, and full-rebuild fallback bytes;
- SQLite query, statement, transaction, busy-retry, and checkpoint counts;
- database main-file, WAL or journal, free-list, and charged metadata bytes;
- write and storage amplification at each documented boundary;
- current and high-water cache, query, prefetch, pending-write, replication,
  prepared-result, and total managed bytes;
- process peak resident memory and configured SQLite native-memory policy;
- backpressure count and duration;
- time to first byte or first durable progress;
- throughput and p50, p95, and p99 latency; and
- final fixture digest and integrity-verification result.

## 6. B01: Small-edit benchmark

For each 4, 8, and 16 KiB COW page size:

1. create a 100 MiB file;
2. overwrite one byte in the middle;
3. overwrite that byte 1,000 times;
4. perform 1,000 clustered one-byte edits; and
5. perform 1,000 scattered one-byte edits.

Before materialization, a one-byte edit MUST create zero CAS objects, zero manifest
roots or nodes, and exactly one current COW page. It MUST not read or hash the complete
file or complete manifest. It may read no more than the intersecting CAS objects and one
COW page. One thousand same-page edits retain one current page unless an active test
stream deliberately pins a predecessor.

Report page amplification, retained payload, bytes read and hashed, manifest nodes
traversed, queries, transactions, memory high-water, and latency.

## 7. B02: Cold manifest lookup

Using an empty manifest cache after physical reopen, read small ranges from the start,
middle, and end of 100 MiB and 1 GiB logical files.

Each lookup MUST validate no more than `maxManifestDepth + 1` manifest values and scan
no more than one leaf's 256 entries. Time to first byte must occur without enumerating
the complete manifest. A corrupted disposable derived index must change neither bytes
nor integrity outcomes.

Report root and node BLOB bytes, node depth, entries scanned, CAS objects read, queries,
time to first byte, and p50, p95, and p99.

## 8. B03: Large sequential reads

Read duplicate-heavy and pseudorandom 100 MiB fixtures by:

1. cold stream;
2. reopen stream;
3. warm stream;
4. bounded range loop;
5. pinned Node VFS `readIntoSync`; and
6. real FUSE.

Repeat the core stream and Node session cases at 1 GiB. Reads perform zero durable
content mutation except required bounded snapshot-lease records.

The 1 GiB managed-memory high-water may exceed the 100 MiB result by no more than one
preferred output chunk, one maximum CAS object, and one manifest node. Query count must
follow bounded object batches rather than FUSE callbacks.

## 9. B04: Sequential writes and materialization

Run with pseudorandom, partially repeated, and duplicate-heavy bytes:

- one 100 MiB sequential creation;
- 100 files of 1 MiB;
- one complete 100 MiB rewrite;
- 1,000 small callbacks followed by one visible commit; and
- interleaved sequential writes across 1, 16, and 64 sessions.

The implementation MUST retain no whole-file buffer. Transactions follow bounded staging
batches, not host callbacks. Hidden staging does not satisfy fsync; the measured visible
commit does. Close, restart, unmount, and remount must preserve the exact digest.

Report callback sizes, contiguous-run lengths, staging reasons, visible commit count,
CAS reuse, manifest nodes created, write amplification, queries, transactions, latency,
throughput, and memory.

## 10. B05: Materialization and storage efficiency

Materialize the B01 edit and structural insertions at the start, middle, and end of the
large fixtures. Measure local FastCDC reconnection and manifest-tree reconnection.

Unchanged CAS objects and authenticated manifest subtrees after reconnection must retain
their identities. Report changed-region bytes, bytes rechunked and hashed, new and
reused objects, new and reused manifest nodes, fallback count, database growth before
and after checkpoint, and reclaimable staging bytes.

Also write identical 100 MiB files under different paths and branches. The second copy
must add no duplicate CAS payload. Namespace, manifest, and revision overhead are
reported separately rather than described as payload deduplication.

## 11. B06: Branch publication

Measure:

- 50 independent paths published in both deterministic and randomized order;
- 50 writers to one inode;
- 100,000 changed paths at the result-byte limit;
- publication after 1,000 scattered edits; and
- replay after a lost response and physical restart.

Report preparation and final-transaction time separately, conflict count, changed paths,
rows and bytes in the final transaction, staged bytes, manifest reuse, and result replay
latency. Conflicts and replays must not change main.

## 12. B07: Replication

Measure:

- one-byte edit in 100 MiB;
- sequential 100 MiB transfer;
- transfer of an already-present 100 MiB file;
- catch-up across 1,000 revisions;
- authenticated empty-replica provisioning and restart;
- active-branch transfer plus generation-guarded publication;
- dropped response and resume in every phase; and
- abandoned staging followed by bounded collection.

The one-byte edit transfers only the root envelope, changed manifest nodes, missing CAS
objects, bounded revision metadata, and protocol overhead. It must not retransmit
unchanged objects or subtrees.

Peak replication buffers must remain at or below the negotiated limit. Envelope decoding
must not retain a second complete copy. Report first durable progress, transferred and
reused bytes, batches, receipts, retries, staging, memory, and physical growth.

For the Computer carrier profile, additionally report raw and decompressed frame bytes,
decoded envelope bytes, JSON/base64 expansion, transport high-water memory, live RPC
stubs after disconnect, and combined process RSS. Run maximum-sized and one-byte-over
maximum frames through the actual pinned Cap'n Web carrier; a custom binary loopback is
not a substitute.

## 13. B08: Concurrency and bounded resources

Under deliberately small budgets, run:

- 64 slow readers;
- 64 sequential writers;
- mixed readers, writers, replication, and garbage collection;
- cancellation and close under backpressure;
- SQLite busy and commit failure injection; and
- 100,000 CAS, manifest, namespace, and mark rows.

Tracked memory MUST remain within the configured ceiling. Backpressure must replace
per-handle allocation multiplication. Total row count and logical file size may increase
total work but not managed-memory high-water beyond one admitted bounded value.

An extended non-gating scale job MAY repeat the cursor and accounting cases with
millions of rows. It is diagnostic evidence, not a reason to delay initial Computer
integration or lengthen the smoke profile.

## 14. B09: Computer and DOFS comparison

Run the same engine-neutral fixtures through:

```text
workspace.fs
  -> authenticated bounded Cap'n Web carrier
  -> shared-runtime replication
  -> computerd
  -> exact branch through real FUSE
  -> shell or Git
  -> generation-guarded pull and publication
  -> restart and reconnect
```

Ephemeral AI FS is the omitted-configuration default. DOFS requires explicit selection,
uses an isolated schema and database, and is never an automatic fallback.

On the reference Computer runner:

- Ephemeral AI FS bounded-range median throughput MUST reach at least 80% of the DOFS
  median on the common fixture; and
- Ephemeral AI FS 100 MiB materialization median MUST take no more than 1.10 times the
  DOFS median.

Correctness, durability, no-materialization, and memory gates remain mandatory even if
the DOFS control does not satisfy them.

The Ephemeral AI FS trial MUST start with a genuinely empty persistent Node SQLite
replica, adopt the authority's exact genesis, and derive replication plus branch Node
VFS from one runtime budget. It MUST include replica-main read-only enforcement, branch
isolation, same-branch remount, dropped-message resume, pinned-reader activation, a
dirty writer conflict, guarded publication replay, and zero live sessions, leases,
reservations, or RPC stubs after cleanup.

## 15. Regression policy

After version 0.1 establishes a baseline, a candidate MUST NOT regress p50 or p95
latency by more than 10% on a gate workload. Replication additionally may not regress
p95, transferred bytes, peak buffers, or submitted SQLite BLOB bytes by more than 10%.

An exception requires a checked-in benchmark record identifying the exact correctness or
resource-safety improvement, before and after results, and the new accepted baseline. No
exception can waive correctness or a configured resource ceiling.

## 16. Result artifact

Each benchmark execution writes a machine-readable artifact equivalent to:

```json
{
  "schema": "efs-benchmark-result-v1",
  "benchmark": "B01-small-edit",
  "commit": "...",
  "engine": "ephemeral-ai-fs",
  "driver": "sqlite-node",
  "fixture": { "name": "random-100m", "sha256": "..." },
  "configuration": {},
  "trials": 5,
  "latencyMs": { "p50": 0, "p95": 0, "p99": 0 },
  "counters": {},
  "pass": true
}
```

Raw trial data is retained with the summary. Human reports must link the raw artifact
rather than transcribe only the fastest trial.

## 17. Go-live condition

Before making Ephemeral AI FS the Computer default:

1. all correctness suites pass;
2. every mandatory benchmark gate passes;
3. the 60-second smoke profile passes on Node SQLite, production-like Durable Object
   SQLite, and real FUSE;
4. there are zero digest mismatches, partial commits, lost updates, integrity failures,
   unreconciled usage counters, leaked reservations, or memory-limit violations; and
5. the accepted result artifacts and environment definitions are checked in.

There is no mandatory hour- or day-long soak for version 0.1 integration. The optional
load profile is capped at 10 minutes and remains iteration-heavy.
