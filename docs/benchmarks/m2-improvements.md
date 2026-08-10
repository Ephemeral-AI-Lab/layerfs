# M2 improvements spec

Measured baseline and ranked optimization targets for the accepted M2 SQLite engine,
executed against the [`m2-minibench.md`](./m2-minibench.md) matrix before M3 starts. All
changes must preserve the M2 acceptance contracts: bounded memory, bounded statements,
exact usage accounting, workerd parity, and the host-neutral core.

## Measured baseline (2026-08-11, Node 24.11.1, file-backed WAL)

Baseline numbers from the mini-bench matrix before the M2 improvements (candidate
`2e06a44`, pure-JS SHA-256). After-measurement values appear in the outcomes table.

| Workload            | Measured (before)                                                                | Dominant cost                                              |
| ------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Sequential write    | 17.9 MiB/s (100 MiB streamed); 7.19% storage overhead                            | pure-JS SHA-256 ~40%, SQLite/statements ~45%, FastCDC ~15% |
| Sequential read     | 43.8 MiB/s cold, 44.4 MiB/s warm (100 MiB streamed)                              | digest re-verification ~95%                                |
| Small random read   | 2.85 ms/op (4 KiB, cold cache)                                                   | per-op transaction + root-to-leaf descend + verify         |
| One-byte edit       | ~6.2 s/edit on the 100 MiB file (streamed fallback, O(file))                     | full-file re-chunk + re-persist                            |
| Materialization     | 33.8 MiB/s (100 MiB reopen-and-read)                                             | digest re-verification + descend                           |
| Pure-JS SHA-256     | 66 MiB/s                                                                         | -                                                          |
| node:crypto SHA-256 | 2,218 MiB/s (34x)                                                                | -                                                          |
| FastCDC chunking    | 179 MiB/s                                                                        | -                                                          |
| Storage overhead    | 7.19% fresh data (single file); ~52% on 100 x 1 MiB; +4.7 MiB rewrite of 100 MiB | metadata + changed chunks + page rounding                  |

## Ranked improvements

| #   | Change                                                                                                                                                  | Expected effect                                                                                                      | Evidence                                                                                                     |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| R3  | Host-injected native hashing (WebCrypto/`node:crypto`) for write hashing and read verification; pure-JS remains the shared fallback                     | write 26 -> 42-50 MiB/s (~1.6-1.9x); read 61 -> 150-300 MiB/s (2.5-5x)                                               | 34x hashing headroom; hashing is ~40% of write and ~95% of read                                              |
| R7  | Carry the authenticated cursor across read pulls under a pinned lease; raise pull window to the query-batch limit; batch object reads (`hash IN (...)`) | small reads 2.85 ms/op -> ~1 ms/op; fewer transactions on sequential reads; warm >= 250 MiB/s                        | per-window transaction + re-descend + per-object SELECTs today (402 read txs / 1,787 statements per 100 MiB) |
| R5  | Batch reconciliation/insertion edges (one `hash IN (...)` per leaf, batched queue inserts)                                                              | write-path statements 4.3x fewer (A1 12,472 -> 2,880); reconciliation 0.0507 -> 0.0465 stmts/entry                   | statement counts in the 100,001-entry closure test and the mini-bench write cells                            |
| R1  | Wire bounded local reconnection into the small-edit fallback; stream the leaf through path-copy instead of the flat `maxManagedResidentBytes/9` window  | one-byte edit O(file) -> O(changed window); A5 3 edits on 100 MiB < 1 s total (re-based from "sub-10 ms"; see audit) | durable-edit tests; M3 acceptance depends on it                                                              |
| R6  | Derive WAL checkpoint/backpressure thresholds from a journal target instead of the 1 GiB ceiling                                                        | no multi-hundred-MiB WAL hysteresis on small filesystems                                                             | node-driver WAL tests                                                                                        |

## Measured outcomes (after R3 + R5 + FastCDC copy reduction)

Measured on the mini-bench matrix after the M2 improvement commit; storage behavior
(dedup, fresh-data overhead, exact quotas) is unchanged. Values are transcribed from the
checked-in artifacts (`tests/performance/artifacts/`); see
[`m2-minibench.md`](./m2-minibench.md) for per-cell detail and measurement caveats.

| Workload            | Today (before)               | After R3+R5+copy (measured)    | After R7+R1 (M3, planned)           |
| ------------------- | ---------------------------- | ------------------------------ | ----------------------------------- |
| Small edit (1 byte) | fallback ~6.2 s (O(file))    | 9.4 s for 3 edits (mixed path) | <1 s for 3 edits; O(changed window) |
| Big write           | 17.9 MiB/s                   | 44.2 MiB/s (2.5x)              | ~same                               |
| Big read            | 43.8 MiB/s                   | 118.1 MiB/s (2.7x)             | >=250 MiB/s warm (read batching)    |
| Small random read   | 2.85 ms/op                   | 1.17 ms/op (2.4x)              | <=1.0 ms/op                         |
| Many small ops      | ~89 ms/edit (B4)             | ~44 ms/edit (B4)               | M5 lazy branch pages target ~10x    |
| Storage             | 7.19% overhead, exact quotas | same                           | dedup on changed windows            |
| Write statements    | 12,472 (A1)                  | 2,880 (A1, 4.3x fewer)         | ~same                               |

## Audit record (2026-08-11, three independent review passes)

A skeptical three-pass audit reviewed the accepted M2 improvements against the code, the
measurement harness, and the planned follow-up work. All gates were re-run green (M1 35,
workerd 11, M2 99, architecture, style, docs, evidence).

- **Implementation audit (R3/R5/copy reduction/contracts/evidence): GO.** The hashing
  seam is wired into every algorithm path with the pure-JS fallback intact; the R5
  batching preserves unit/sequence/metadata accounting exactly; the copy reduction is
  sound; the evidence chain reproduces live (4,655 reconciliation statements,
  0.0465/entry; elapsed within variance). Two hardening notes are logged to the M3
  backlog: (a) intra-batch duplicate leaf edges skip the declared-length re-check the
  old per-edge path performed (strictness regression on forged digest-consistent
  manifests only), and (b) the reconciliation batch size is implicitly bounded by leaf
  size rather than an explicit binding cap.
- **Harness review: sound relative benchmark; anchors now transcribed from artifacts.**
  The managed-resident peak for stream-read cells is a construction-time snapshot (the
  observer fires at `readStream()` creation, not consumption) and is
  stale/mis-attributed in the A3/A4/A7/B cells; single-trial runs carry 10-20% variance;
  read cells grow +53.6 KiB from read-lease bookkeeping; A5's per-edit times mix the
  fallback and path-copy paths. All caveats are recorded in
  [`m2-minibench.md`](./m2-minibench.md); per-cell peak-at-stream- close is M3
  housekeeping before the R7 gate.
- **Sequencing review: targets re-based.** Sub-10 ms edits and GB/s-class warm reads are
  not achievable in M3: the durable-edit persistence floor is ~9 write transactions
  (~30-40 ms under `synchronous=FULL`), and the Node driver copies each byte 2-3x on
  read (GB/s needs the M9 mmap/zero-copy profile). The local-rebuild machinery also
  hard-caps files at 16 MiB (`MAX_DIAGNOSTIC_CONTENT_BYTES`), so the 100 MiB workloads
  always fall back today. The re-based targets and the revised sequencing (R7 read
  batching first, then R1, then async write hashing, then M5 lazy branch pages) are
  specified in [`m3-improvements.md`](./m3-improvements.md).

## Safety constraints

- Memory: all new paths remain admission-bounded; cursor leases release on
  cancellation/failure/close; no O(file) buffers.
- CPU: per-unit-of-work statement/elapsed budgets stay; no O(n^2).
- Host neutrality: the hashing seam is a capability injected by the host adapter
  (`node:crypto` on the Node adapter; WebCrypto exists on Node and workerd but is async
  and is not used inside read transactions); pure-JS stays the shared fallback so M1
  golden vectors remain byte-identical on both runtimes. No node-only module may enter
  `packages/fs/src` algorithm paths (architecture gate enforces).
- Storage: exact `efs_usage` accounting and quota ceilings are untouched.

## Milestone mapping

- M2 scope (this spec): R3, R5, FastCDC copy reduction; R7's verification half. Accepted
  with evidence at HEAD `93a6a1f`.
- M3 scope: the sequenced plan in [`m3-improvements.md`](./m3-improvements.md) — M3.1 R7
  read-path batching (cursor carry, widened pull windows, `hash IN (...)` object reads),
  M3.2 R1 bounded local reconnection in the durable-edit path, M3.3 async WebCrypto
  write hashing for workerd parity, plus mini-bench harness housekeeping and the two R5
  hardening items.
- M5 scope: lazy branch edits (COW pages + patches with publish-time local CDC), built
  on the M3.2 splice.
- M9 scope: DOFS comparisons, 80% bounded-range gate, 1.10x materialization gate,
  mmap/zero-copy profile, pack-file index evaluation.
