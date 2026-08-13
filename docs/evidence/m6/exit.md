# Milestone 6 exit

- Candidate commit: `082f4e98711035c2be2bd7d2f668f6c23e7a5b16`
- Date: 2026-08-13
- Sequential predecessor: accepted M5 candidate
  `710ddf3f05d85ad3264b4f2046c631a6fded9c14`
- Checklist complete: yes
- Primary environment: Windows `win32` x64, Node `v24.11.1`, pnpm `10.32.1`, AMD Ryzen
  Threadripper 7960X, 128 GiB RAM, Samsung 980 PRO / Crucial T705 SSD, faithful-local
  Workerd `1.20260810.1`, Workerd SQLite `3.47.0`, Node SQLite `3.50.4`, Wrangler
  `4.122.0`, Vitest `4.1.10`, and `@cloudflare/vitest-pool-workers` `0.21.2`
- Exact target commands: `node scripts/run-accepted-node-gate.mjs` followed by
  `node scripts/run-m6-local-gate.mjs --skip-build`, both from the clean detached
  candidate worktree
- Target results: the accepted Node predecessor passed in 534,363 ms and the
  credential-free faithful-local Durable Object selection passed in 486,086 ms. Each
  command enforced its own 600,000 ms target deadline. The M6 topology contains 345
  cumulative logical checks with zero failures.
- Correctness artifact: [`correctness.json`](./correctness.json)
- Exact preview bundle: Wrangler dry-run emitted 970,065 bytes with SHA-256
  `b0a125fc143e2a5dff23eac008a8a035ecf99b82324db30852424b24c45ad122`; the faithful
  Workers pool exercised those exact bytes without deploying them.
- Schema identity: Node retains `application_id` plus `user_version`. Durable Object
  SQLite uses the authorized singleton `efs_schema_identity` table because Workerd's
  runtime authorizer rejects the header PRAGMAs and table-valued equivalents. Identity
  and relational schema version advance atomically. The gate faults before and after all
  12 Node header-identity writes and all 13 Durable Object table-identity writes, then
  physically reopens or evicts and proves no partial initialization remains. Malformed
  definitions, including missing identity columns, return `ESCHEMA` without mutation.
- Migration evidence: populated released schemas 1–3 retain namespace, content,
  branches, revisions, and accounting. Node covers 922 migration statements and Durable
  Object covers all 996 statements, with physical reopen or runtime eviction after each
  injected boundary.
- Capability boundary: an unknown Cloudflare plan reports the conservative decimal
  1,000,000,000-byte database and runtime-owned journal ceiling; explicitly configured
  paid values are clamped at decimal 10,000,000,000 bytes. The M6 scale fixture selects
  a stricter 536,870,912-byte local ceiling. Page metrics are runtime-size-only, and
  WAL/checkpoint controls remain runtime managed.
- Fault/restart evidence: both adapters cover all twelve filesystem mutation families at
  1,218 positions each. Publication covers 95 direct plus 91 prepared positions.
  Maintenance covers snapshot 110/42, collection 259/128, and abandoned cleanup 61/33
  statement/batch positions, or 633 positions per adapter. Durable Object verification
  uses actual runtime eviction at restart boundaries.
- Scale evidence: fixture digest
  `e472eed749c34849f2bf86c8be12b17d8b82954b77e5911de126c90daaf39104`; exactly 100,000
  objects, namespace rows, manifest roots, and manifest nodes; 300,000 peak snapshot and
  GC marks; 1,000,006 verified entities; five real Durable Object evictions; and an
  88,379,392-byte database. Managed high-water remained exactly 4,481,396 bytes at both
  10,240 and 100,000 rows, below 16 MiB. The slowest maintenance call was 411 ms.
- Resource evidence: faithful scale execution peaked at 374,624,256 bytes of absolute
  Workerd process RSS below the conservative 805,306,368-byte process ceiling. A raw
  SQLite Durable Object control grew by 107,327,488 bytes and reproduced the
  runtime-owned row-count RSS effect without instantiating filesystem caches. Exact
  isolate attribution is unavailable and is not claimed.
- Smoke evidence: the faithful-local profile completed all 9,056 operations and three
  real evictions in 30,248 ms, retaining fixture digest
  `488a3edec4c7a4c4648fc4e3517bf99774efda366ff54d70b7fd9be6076571d8` and exact final
  payload, namespace, lease, reservation, usage, and verification checks.
- Known deviations: none. Hosted Cloudflare execution and replication are later
  milestone scopes, not M6 deviations; the machine-readable artifact records those
  boundaries as notes.
- Timing composition: after the measured scale/resource phase, the complete maintenance
  matrix runs in one process lane while the portable, migration, filesystem-fault, and
  publication-fault selections run serially in a second lane. All fixtures, fault
  positions, and result assertions remain unchanged.
- Independent audit: the combined PR #3 and PR #4 delta, exact target logs, owned-tree
  digest, and evidence topology were reviewed against the accepted M6 baseline
- Approved to begin M7: yes
