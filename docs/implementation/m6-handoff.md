# M6 completion handoff

Milestone 6 establishes credential-free Cloudflare Durable Object SQLite parity. The
normative status and checklist are in section 9 of `implementation-plan.md`; this note
records the accepted boundary for M7 and M8.

## Runtime and schema boundary

- `@ephemeralai/fs-sqlite-cloudflare` executes directly against
  `DurableObjectState.storage.sql` and maps core write and exclusive units of work to
  `transactionSync`. It has no Node SQLite import, mirror database, or process-local
  filesystem index.
- Node retains native SQLite `application_id` and `user_version` header identity.
  Durable Object storage uses the core-owned singleton `efs_schema_identity` table
  because its runtime authorizer rejects both header PRAGMAs and the corresponding
  table-valued PRAGMA functions. Initialization, migration, and identity advancement
  update the selected identity authority and `efs_meta.schema_version` in the same
  exclusive transaction. Reopen rejects missing, extra, malformed, or mismatched
  identity rows; neither adapter falls back to the other mechanism.
- Durable Object rows and BLOBs are detached into plain owned values, integers remain
  safe, callback-scoped transactions cannot escape or nest, callback-thrown errors
  retain object identity, and result/binding/elapsed limits remain enforced by the
  shared unit-of-work budgets. The adapter accepts exactly one SQL statement, including
  when a caller uses comments or a trailing semicolon, and normalizes genuine runtime
  constraint, busy, corruption, and `SQLITE_FULL` failures without rewriting callback
  errors.
- Runtime page counts and SQLite WAL/checkpoint controls are not exposed by the Durable
  Object API. The driver reports runtime-managed journal and memory policy, runtime
  physical-quota enforcement, runtime-size-only page metrics, and the finite reviewed
  BLOB, binding, database, and journal ceilings. An unknown account reports the decimal
  1,000,000,000-byte Free-plan ceiling for both runtime-owned database and journal
  capacity; an explicitly configured paid value is clamped at 10,000,000,000 decimal
  bytes. Node continues to report native SQLite page, WAL, checkpoint, cache, and mmap
  behavior.

## Faithful local fixture

- The exact preview Worker is `examples/durable-object-workspace/src/index.ts` with
  compatibility date `2026-08-10`, binding `FILESYSTEM` to class `FilesystemObject`, and
  migration `v1` declaring `new_sqlite_classes` for that class.
- `scripts/check-cloudflare-preview.mjs` performs a Wrangler deploy dry run only,
  verifies the reviewed configuration and emitted binding, and retains the emitted
  bundle long enough for the Vitest Workers pool to execute those exact bytes. The gate
  records the bundle digest and deletes its temporary output after the suite. It never
  deploys.
- The local gate pins `@cloudflare/vitest-pool-workers` `0.21.2`, Vitest `4.1.10`,
  Wrangler `4.122.0`, and workerd `1.20260810.1`. The pinned workerd source builds
  SQLite `3.47.0`; Durable Object SQL intentionally forbids querying `sqlite_version()`
  at runtime.
- `scripts/run-m6-local-gate.mjs` removes Cloudflare credential and proxy variables from
  every child, builds once for standalone `pnpm test:m6`, and runs preview, workerd,
  Node parity, and faithful Durable Object parity groups within one 600-second Durable
  Object-target deadline. `pnpm validate:m6:pre-evidence` first runs the separately
  timed accepted Node predecessor target and then reuses that build for the Durable
  Object target. Each target owns an executable 600,000-millisecond deadline; their
  sequential aggregate is not reported as one target runtime.

## Portable correctness boundary

- The shared testkit covers storage deduplication, a genuine 100,001-entry staged
  manifest closure, constant-row certificate validation, every certificate field,
  sealed-membership immutability, concurrent payload-quota admission, direct bounded
  usage comparison, authenticated range corruption, and staged-batch crash cleanup. It
  also covers namespace and path errors, ranges, links, rename/removal, metadata, stream
  abort/backpressure and immutable snapshots, branches, maintenance, resource limits,
  physical reopen, read-only Node reopen, concurrent adapter instances, and close
  lifecycle.
- The branch suite covers a frozen base, 50 independent writers, 50 same-inode
  conflicts, sibling ordering, ABA and alias conflicts, deterministic publication,
  terminal handle behavior, stream snapshots, replay after physical reopen, result
  expiry, and lifetime branch/operation reservation.
- Both adapters run the same twelve-family filesystem mutation fault matrix. Node closes
  and recreates its physical connection; the Durable Object harness calls
  `evictDurableObject` before verification. The exact matrix is 1,218 positions on each
  adapter. The publication matrix covers 95 direct and 91 prepared positions, and the
  maintenance matrix covers snapshot 110/42, collection 259/128, and abandoned cleanup
  61/33 statement/batch boundaries on each adapter. Every injected position is followed
  by physical driver destruction or runtime eviction and comparison with the complete
  old state; the first non-injected position must expose the complete new state.
- Released schemas 1, 2, and 3 run through 915 Durable Object migration-statement
  positions with eviction and recovery. Durable-table identity refusal covers wrong,
  newer, too-old, absent-with-user-objects, malformed, extra-row, and relational-version
  mismatch states without writes.
- Both adapters construct 100,000 distinct CAS objects, manifest roots, manifest nodes,
  and namespace rows under 256-row, 256-KiB query-result envelopes and a 4-MiB cache.
  Snapshot, verification, collection, durable marks, managed-memory comparison, and five
  real Durable Object evictions use bounded persisted cursors. The machine-readable exit
  artifact retains the exact fixture digests, cardinalities, memory peaks, maximum
  maintenance-call duration, and physical database size for each adapter.
- The faithful Durable Object smoke is the unchanged finite profile: one 16-MiB
  pseudorandom payload, 5,000 COW edits, 2,000 namespace/link operations, 16 readers and
  16 writers with 64 operations each, three runtime restarts, interrupted collection,
  and exact final digest, namespace, lease, reservation, usage, and verification checks
  within 60 seconds.
- Runtime eviction reconstructs committed bytes, active leases, branch state, and an
  interrupted collection from the same SQLite-backed Durable Object binding. A separate
  partial-staging scenario commits three bounded batches, evicts after each batch, and
  proves expired lease, certificate, entry, payload, and reservation cleanup is exact.
- The 100,000-row resource gate measures both the core-managed working set and absolute
  Workerd process RSS. A raw Durable Object SQLite control reproduces the runtime-owned
  row-count-dependent RSS effect without filesystem caches, while the filesystem's own
  managed high-water remains flat and bounded. The exit artifact records both windows,
  the unavailable exact isolate/process attribution, the platform isolate limit, and the
  conservative absolute Workerd process ceiling.

No Cloudflare account, credential, network deployment, hosted namespace, or other
external state is used or claimed by M6. Hosted preview execution remains an M9 gate
that requires new explicit user authorization. Existing user-owned performance artifacts
under `tests/performance/artifacts-m31*` remain outside this milestone.
